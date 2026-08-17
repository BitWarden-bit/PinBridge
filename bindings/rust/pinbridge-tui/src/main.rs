//! pinbridge-tui: terminal dashboard and launcher for pinbridge-agent.
//!
//! Usage:
//!   pinbridge-tui [options] -- <target.exe> [args...]   launch backend + dashboard
//!   pinbridge-tui [options]                             attach to a running agent
//!   pinbridge-tui --probe [-- target ...]               one-shot counters check
//!
//! Options: --port N (default 9001)  --pin <pin.exe>  --agent <pinbridge_agent.dll>
//!          --entry-bp (default) | --no-entry-bp

mod ui;

use pinbridge_client::client::{Client, ScriptListEntry, Snapshot};
use pinbridge_client::launch;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use pinbridge_proto as proto;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::collections::VecDeque;
use std::process::Child;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const RATE_WINDOW: usize = 240;
const COUNTERS_PERIOD: Duration = Duration::from_millis(250);
const EVENTS_PERIOD: Duration = Duration::from_millis(500);
const NEWEST_LIMIT: u64 = 24;
const OUTPUT_PERIOD: Duration = Duration::from_millis(500);
const OUTPUT_LIMIT: u32 = 512;
const LIST_PERIOD: Duration = Duration::from_secs(2);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

struct Config {
    port: u16,
    probe: bool,
    pin: Option<String>,
    agent: Option<String>,
    target: Vec<String>,
    entry_bp: bool,
    pin_probe: bool,
}

fn parse_args() -> Config {
    let mut config = Config {
        port: proto::DEFAULT_PORT,
        probe: false,
        pin: None,
        agent: None,
        target: Vec::new(),
        entry_bp: true,
        pin_probe: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--probe" => config.probe = true,
            "--port" => {
                if let Some(value) = args.next() {
                    config.port = value.parse().unwrap_or(proto::DEFAULT_PORT);
                }
            }
            "--pin" => config.pin = args.next(),
            "--agent" => config.agent = args.next(),
            "--entry-bp" => config.entry_bp = true,
            "--no-entry-bp" => config.entry_bp = false,
            "--pin-probe" => config.pin_probe = true,
            "--" => config.target.extend(args.by_ref()),
            other if other.starts_with("--") => {
                eprintln!("unknown option: {other}");
            }
            other => config.target.push(other.to_string()),
        }
    }
    config
}

fn probe(port: u16) -> i32 {
    match Client::connect(port) {
        Ok(mut client) => {
            let ping = client.ping();
            let counters = client.counters();
            let newest = client.ring_newest(3);
            match (ping, counters, newest) {
                (Ok((major, minor, pid, _)), Ok((_, dropped, capacity, kinds)), Ok((_, events))) => {
                    println!("probe ok: abi={major}.{minor} pid={pid} dropped={dropped} capacity={capacity}");
                    println!(
                        "hook_regs={} memory={} exec={} branch_edge={}",
                        kinds[0], kinds[1], kinds[2], kinds[3]
                    );
                    for event in &events {
                        println!(
                            "  event #{:<9} kind={} tid={} ip=0x{:x} arg0=0x{:x} arg1=0x{:x}",
                            event.sequence, event.kind, event.thread_id, event.address,
                            event.arg0, event.arg1
                        );
                    }
                    0
                }
                _ => {
                    eprintln!("probe failed: bad response");
                    2
                }
            }
        }
        Err(error) => {
            eprintln!("probe failed: {error}");
            2
        }
    }
}

/// If the config names a target, spawn pin + agent for it and wait for the
/// query port. Returns the child (owned by the caller, killed on exit).
fn maybe_spawn_backend(config: &Config) -> Option<Child> {
    if config.target.is_empty() {
        return None;
    }
    let options = launch::LaunchOptions {
        pin: config.pin.clone(),
        agent: config.agent.clone(),
        arch: None, // auto-detect from the target's PE headers
        port: config.port,
        entry_bp: config.entry_bp,
        probe_mode: config.pin_probe,
    };
    match launch::launch_for_target(&options, &config.target, STARTUP_TIMEOUT) {
        Ok((child, _port)) => Some(child),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

/// One poller tick: the counters snapshot plus whatever plugin data was
/// fetched on this tick's own cadences.
struct Poll {
    snapshot: Snapshot,
    /// plugin print lines ("plugin: line") fetched since the previous tick.
    output: Vec<String>,
    /// script_list result, Some only on ticks where it was refreshed and
    /// answered (old servers stay None forever -> the panel shows no summary).
    plugins: Option<Vec<ScriptListEntry>>,
}

fn poller(port: u16, tx: std::sync::mpsc::Sender<Poll>) {
    // Ring cursor, kept across reconnects so a transient TCP drop doesn't
    // re-fetch (and duplicate) lines already shown.
    let mut after_seq = 0u64;
    loop {
        match Client::connect(port) {
            Ok(mut client) => {
                let mut last_events = Instant::now() - EVENTS_PERIOD;
                let mut last_output = Instant::now() - OUTPUT_PERIOD;
                let mut last_list = Instant::now() - LIST_PERIOD;
                loop {
                    let mut poll = Poll {
                        snapshot: Snapshot { connected: true, ..Snapshot::default() },
                        output: Vec::new(),
                        plugins: None,
                    };
                    let alive = (|| {
                        let (major, minor, pid, _) = client.ping().ok()?;
                        let (total, dropped, capacity, kinds) = client.counters().ok()?;
                        poll.snapshot.abi = (major, minor);
                        poll.snapshot.pid = pid;
                        poll.snapshot.total = total;
                        poll.snapshot.dropped = dropped;
                        poll.snapshot.capacity = capacity;
                        poll.snapshot.kind_counts = kinds;
                        if last_events.elapsed() >= EVENTS_PERIOD {
                            if let Ok((_, events)) = client.ring_newest(NEWEST_LIMIT) {
                                poll.snapshot.newest = events;
                            }
                            last_events = Instant::now();
                        }
                        if last_output.elapsed() >= OUTPUT_PERIOD {
                            // Scripting unavailable / old server: keep the
                            // lines already shown and stay quiet.
                            if let Ok((next, lines)) = client.script_output(after_seq, OUTPUT_LIMIT)
                            {
                                after_seq = next;
                                poll.output = lines
                                    .into_iter()
                                    .map(|line| format!("{}: {}", line.plugin, line.line))
                                    .collect();
                            }
                            last_output = Instant::now();
                        }
                        if last_list.elapsed() >= LIST_PERIOD {
                            if let Ok(entries) = client.script_list() {
                                poll.plugins = Some(entries);
                            }
                            last_list = Instant::now();
                        }
                        Some(())
                    })();
                    if alive.is_none() {
                        break; // reconnect
                    }
                    if tx.send(poll).is_err() {
                        return;
                    }
                    std::thread::sleep(COUNTERS_PERIOD);
                }
            }
            Err(_) => {
                let _ = tx.send(Poll {
                    snapshot: Snapshot::default(),
                    output: Vec::new(),
                    plugins: None,
                });
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

fn run_dashboard(port: u16) {
    let (tx, rx): (_, Receiver<Poll>) = channel();
    std::thread::spawn(move || poller(port, tx));

    enable_raw_mode().expect("raw mode");
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("alternate screen");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("terminal");

    let mut state = ui::UiState {
        snapshot: Snapshot::default(),
        rate_history: Vec::new(),
        plugin_lines: VecDeque::new(),
        plugins_summary: None,
    };
    let mut last_total: Option<(u64, Instant)> = None;

    loop {
        let mut dirty = false;
        while let Ok(poll) = rx.try_recv() {
            dirty = true;
            let now = Instant::now();
            if let Some((prev_total, prev_at)) = last_total {
                let secs = now.duration_since(prev_at).as_secs_f64();
                if secs > 0.0 && poll.snapshot.total >= prev_total {
                    let rate = ((poll.snapshot.total - prev_total) as f64 / secs) as u64;
                    state.rate_history.push(rate);
                    if state.rate_history.len() > RATE_WINDOW {
                        state.rate_history.remove(0);
                    }
                }
            }
            last_total = Some((poll.snapshot.total, now));
            state.push_plugin_lines(poll.output);
            if let Some(entries) = poll.plugins {
                state.plugins_summary = Some(ui::plugin_summary(&entries));
            }
            state.snapshot = poll.snapshot;
        }

        // Only redraw when something changed: full-screen repaints on a fixed
        // timer visibly flicker on the Windows console host.
        if dirty {
            terminal.draw(|frame| ui::draw(frame, &state)).expect("draw");
        }

        if crossterm::event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = crossterm::event::read() {
                if key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                {
                    break;
                }
            }
        }
    }

    disable_raw_mode().expect("leave raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen).expect("leave alternate screen");
}

fn main() {
    let config = parse_args();
    let mut backend = maybe_spawn_backend(&config);

    let exit_code = if config.probe {
        probe(config.port)
    } else {
        run_dashboard(config.port);
        0
    };

    if let Some(child) = backend.as_mut() {
        launch::kill_backend(child);
    }
    std::process::exit(exit_code);
}
