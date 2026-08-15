//! pinbridge-ui: Tauri (HTML/WebView) dashboard for pinbridge-agent.
//!
//! Same launcher role as pinbridge-tui:
//!   pinbridge-ui [--pin <pin.exe>] [--agent <dll>] [--port N] [-- target args...]
//! Without a target it attaches to an already-running agent.
//!
//! The agent query server is single-client, so the poller and all command
//! handlers share one connection through AppState.

use pinbridge_client::{client::Client, launch};
use pinbridge_proto as proto;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, RunEvent, State};

const POLL_PERIOD: Duration = Duration::from_millis(250);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

struct Config {
    port: u16,
    pin: Option<String>,
    agent: Option<String>,
    target: Vec<String>,
}

fn parse_args() -> Config {
    let mut config = Config {
        port: proto::DEFAULT_PORT,
        pin: None,
        agent: None,
        target: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                if let Some(value) = args.next() {
                    config.port = value.parse().unwrap_or(proto::DEFAULT_PORT);
                }
            }
            "--pin" => config.pin = args.next(),
            "--agent" => config.agent = args.next(),
            "--" => config.target.extend(args.by_ref()),
            other => config.target.push(other.to_string()),
        }
    }
    config
}

struct AppState {
    port: Mutex<u16>,
    pin: Option<String>,
    agent: Option<String>,
    client: Mutex<Option<Client>>,
    backend: Arc<Mutex<Option<Child>>>,
    target: Mutex<Option<String>>,
}

impl AppState {
    fn current_port(&self) -> u16 {
        *self.port.lock().unwrap()
    }

    /// Runs `f` with the shared client, reconnecting once on failure.
    fn with_client<R>(
        &self,
        f: impl FnOnce(&mut Client) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut guard = self.client.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            let client = Client::connect(self.current_port()).map_err(|e| e.to_string())?;
            *guard = Some(client);
        }
        let client = guard.as_mut().unwrap();
        match f(client) {
            Err(error) => {
                *guard = None; // reconnect next time
                Err(error)
            }
            ok => ok,
        }
    }
}

#[tauri::command]
fn cmd_session(state: State<'_, Arc<AppState>>) -> serde_json::Value {
    let running = state.backend.lock().unwrap().is_some();
    let target = state.target.lock().unwrap().clone();
    // Attach mode (no owned backend): a reachable agent is a live session too.
    let attached =
        running || state.with_client(|c| c.ping().map(|_| ()).map_err(|e| e.to_string())).is_ok();
    serde_json::json!({ "running": attached, "target": target })
}

#[tauri::command]
fn cmd_launch(state: State<'_, Arc<AppState>>, target: String, pin: Option<String>, entry_bp: Option<bool>) -> Result<(), String> {
    {
        let guard = state.backend.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("backend already running; kill it first".to_string());
        }
    }
    let options = launch::LaunchOptions {
        pin: pin.or_else(|| state.pin.clone()),
        agent: state.agent.clone(),
        arch: None, // auto-detect from the target's PE headers
        port: 0, // auto-pick a free port per session (avoids zombie collisions)
        entry_bp: entry_bp.unwrap_or(true), // debugger default: break at entry
    };
    let args = vec![target.clone()];
    let (child, port) = launch::launch_for_target(&options, &args, STARTUP_TIMEOUT)
        .map_err(|e| e.to_string())?;
    *state.backend.lock().map_err(|e| e.to_string())? = Some(child);
    *state.target.lock().map_err(|e| e.to_string())? = Some(target);
    *state.port.lock().map_err(|e| e.to_string())? = port;
    *state.client.lock().map_err(|e| e.to_string())? = None; // reconnect on new port
    Ok(())
}

#[tauri::command]
fn cmd_kill_backend(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut guard = state.backend.lock().map_err(|e| e.to_string())?;
    if let Some(child) = guard.as_mut() {
        launch::kill_backend(child);
    }
    *guard = None;
    *state.target.lock().map_err(|e| e.to_string())? = None;
    *state.client.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

// ---- tauri commands (addresses cross as 0x strings, JS numbers are f64) ----

fn parse_addr(text: &str) -> Result<u64, String> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|_| format!("bad address: {text}"))
    } else {
        trimmed.parse::<u64>().map_err(|_| format!("bad number: {text}"))
    }
}

#[tauri::command]
fn cmd_control(state: State<'_, Arc<AppState>>, action: String) -> Result<bool, String> {
    state.with_client(|c| {
        match action.as_str() {
            "stop" => c.stop(),
            "resume" => c.resume(),
            _ => return Err("bad action".to_string()),
        }
        .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn cmd_step(state: State<'_, Arc<AppState>>, tid: u32, over: bool) -> Result<bool, String> {
    state.with_client(|c| c.step(tid, over).map_err(|e| e.to_string()))
}

#[tauri::command]
fn cmd_bp_set(state: State<'_, Arc<AppState>>, address: String) -> Result<u32, String> {
    let address = parse_addr(&address)?;
    state.with_client(|c| c.bp_set(address).map_err(|e| e.to_string()))
}

#[tauri::command]
fn cmd_bp_remove(state: State<'_, Arc<AppState>>, id: u32) -> Result<u32, String> {
    state.with_client(|c| c.bp_remove(id).map_err(|e| e.to_string()))
}

#[tauri::command]
fn cmd_bp_list(
    state: State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    state.with_client(|c| {
        let (stopped, hit_tid, hit_addr, _gen, entries) = c.bp_list().map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "stopped": stopped,
            "hit_tid": hit_tid,
            "hit_addr": format!("0x{hit_addr:x}"),
            "breakpoints": entries.iter().map(|(id, address, hits)| serde_json::json!({
                "id": id, "address": format!("0x{address:x}"), "hits": hits,
            })).collect::<Vec<_>>(),
        }))
    })
}

#[tauri::command]
fn cmd_modules(state: State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    state.with_client(|c| {
        let entries = c.modules().map_err(|e| e.to_string())?;
        Ok(serde_json::json!(entries.iter().map(|(low, high, main, name)| {
            serde_json::json!({
                "low": format!("0x{low:x}"), "high": format!("0x{high:x}"),
                "main": main, "name": name,
            })
        }).collect::<Vec<_>>()))
    })
}

#[tauri::command]
fn cmd_threads(state: State<'_, Arc<AppState>>) -> Result<Vec<u32>, String> {
    state.with_client(|c| c.threads().map_err(|e| e.to_string()))
}

#[tauri::command]
fn cmd_context(state: State<'_, Arc<AppState>>, tid: u32) -> Result<serde_json::Value, String> {
    state.with_client(|c| {
        let pairs = c.context_get(tid).map_err(|e| e.to_string())?;
        Ok(serde_json::json!(pairs.iter().map(|(reg, value)| {
            serde_json::json!({ "reg": reg, "value": format!("0x{value:016x}") })
        }).collect::<Vec<_>>()))
    })
}

#[tauri::command]
fn cmd_setreg(state: State<'_, Arc<AppState>>, tid: u32, reg: u32, value: String) -> Result<(), String> {
    let value = parse_addr(&value)?;
    state.with_client(|c| c.context_set(tid, reg, value).map_err(|e| e.to_string()))
}

#[tauri::command]
fn cmd_read_mem(state: State<'_, Arc<AppState>>, address: String, size: u64) -> Result<String, String> {
    let address = parse_addr(&address)?;
    state.with_client(|c| {
        let data = c.read_memory(address, size).map_err(|e| e.to_string())?;
        Ok(data.iter().map(|b| format!("{b:02x}")).collect())
    })
}

#[tauri::command]
fn cmd_write_mem(state: State<'_, Arc<AppState>>, address: String, data: String) -> Result<u64, String> {
    let address = parse_addr(&address)?;
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() % 2 != 0 {
        return Err("hex string needs an even length".to_string());
    }
    let bytes: Result<Vec<u8>, _> = (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16))
        .collect();
    let bytes = bytes.map_err(|e| format!("bad hex: {e}"))?;
    state.with_client(|c| c.write_memory(address, &bytes).map_err(|e| e.to_string()))
}

#[tauri::command]
fn cmd_disasm(state: State<'_, Arc<AppState>>, address: String, count: u64) -> Result<serde_json::Value, String> {
    let address = parse_addr(&address)?;
    state.with_client(|c| {
        let rows = c.disasm(address, count).map_err(|e| e.to_string())?;
        Ok(serde_json::json!(rows.iter().map(|(addr, _size, kind, text, bytes, target)| {
            serde_json::json!({
                "address": format!("0x{addr:x}"),
                "bytes": bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                "kind": kind,
                "text": text,
                "target": format!("0x{target:x}"),
            })
        }).collect::<Vec<_>>()))
    })
}

/// Batch address resolution: hex addresses in, x64dbg-style display strings
/// out (module!export / module+0xoff / null). Symbolization lives in the
/// agent (RESOLVE opcode), so scripts get the same data.
#[tauri::command]
fn cmd_resolve(state: State<'_, Arc<AppState>>, addresses: Vec<String>) -> Result<serde_json::Value, String> {
    let mut parsed = Vec::with_capacity(addresses.len());
    for text in &addresses {
        parsed.push(parse_addr(text)?);
    }
    state.with_client(|c| {
        let resolved = c.resolve(&parsed).map_err(|e| e.to_string())?;
        Ok(serde_json::json!(resolved.iter().map(|r| r.display()).collect::<Vec<_>>()))
    })
}

/// Page-up disassembly: returns `count` rows ending right before `address`.
/// x86 has no backward decode, so scan candidate starts (max instruction is
/// 15 bytes) until the forward stream lands exactly on `address` — paging up
/// from a guessed start used to fabricate mid-instruction addresses, and
/// breakpoints set on those rows could never fire.
#[tauri::command]
fn cmd_disasm_up(state: State<'_, Arc<AppState>>, address: String, count: u64) -> Result<serde_json::Value, String> {
    let address = parse_addr(&address)?;
    let count = count.clamp(1, 512);
    state.with_client(|c| {
        let span = count * 15;
        let base = address.saturating_sub(span);
        let mut chosen: Option<Vec<_>> = None;
        for shift in 0..15u64 {
            let rows = c.disasm(base + shift, count + 16).map_err(|e| e.to_string())?;
            if let Some(pos) = rows.iter().position(|r| r.0 == address) {
                let keep = &rows[..pos];
                let from = keep.len().saturating_sub(count as usize);
                chosen = Some(keep[from..].to_vec());
                break;
            }
        }
        let rows = match chosen {
            Some(rows) => rows,
            None => c.disasm(base, count).map_err(|e| e.to_string())?, // unaligned region: rough page
        };
        Ok(serde_json::json!(rows.iter().map(|(addr, _size, kind, text, bytes, target)| {
            serde_json::json!({
                "address": format!("0x{addr:x}"),
                "bytes": bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                "kind": kind,
                "text": text,
                "target": format!("0x{target:x}"),
            })
        }).collect::<Vec<_>>()))
    })
}

// ---- poller: pushes snapshots to the webview over the shared connection ----

fn snapshot_json(
    ping: (u32, u32, u32, u64),
    counters: (u64, u64, u64, [u64; 8]),
    events: &[proto::EventRecord],
    rate: u64,
    bp_state: (bool, u32, u64, u64, Vec<(u32, u64, u64)>),
) -> serde_json::Value {
    let (stopped, hit_tid, hit_addr, stop_gen, bps) = bp_state;
    serde_json::json!({
        "connected": true,
        "abi": [ping.0, ping.1],
        "pid": ping.2,
        "total": counters.0,
        "dropped": counters.1,
        "capacity": counters.2,
        "kinds": counters.3,
        "rate": rate,
        "stopped": stopped,
        "hit_tid": hit_tid,
        "hit_addr": format!("0x{hit_addr:x}"),
        "stop_gen": stop_gen,
        "bps": bps.iter().map(|(id, address, hits)| serde_json::json!({
            "id": id, "address": format!("0x{address:x}"), "hits": hits,
        })).collect::<Vec<_>>(),
        "events": events.iter().map(|e| serde_json::json!({
            "sequence": e.sequence, "kind": e.kind, "thread_id": e.thread_id,
            "address": format!("0x{:x}", e.address),
            "arg0": format!("0x{:x}", e.arg0), "arg1": e.arg1, "arg2": e.arg2,
        })).collect::<Vec<_>>(),
    })
}

fn poller(handle: tauri::AppHandle, state: Arc<AppState>) {
    let mut last_total: Option<(u64, Instant)> = None;
    loop {
        let snapshot = state.with_client(|client| {
            let ping = client.ping().map_err(|e| e.to_string())?;
            let counters = client.counters().map_err(|e| e.to_string())?;
            let (_, events) = client.ring_newest(24).map_err(|e| e.to_string())?;
            let bp_state = client.bp_list().map_err(|e| e.to_string())?;
            let now = Instant::now();
            let mut rate = 0u64;
            if let Some((prev_total, prev_at)) = last_total {
                let secs = now.duration_since(prev_at).as_secs_f64();
                if secs > 0.0 && counters.0 >= prev_total {
                    rate = ((counters.0 - prev_total) as f64 / secs) as u64;
                }
            }
            last_total = Some((counters.0, now));
            Ok(snapshot_json(ping, counters, &events, rate, bp_state))
        });
        match snapshot {
            Ok(value) => {
                if handle.emit("snapshot", value).is_err() {
                    return;
                }
            }
            Err(_) => {
                let _ = handle.emit("snapshot", serde_json::json!({ "connected": false }));
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        }
        std::thread::sleep(POLL_PERIOD);
    }
}

// ---- test hook: scripted session driver (see main()) ----

fn run_script(state: Arc<AppState>, script: &str) {
    use std::io::Write;
    let exe = std::env::current_exe().unwrap_or_default();
    let log_path = exe
        .parent()
        .map(|p| p.join("ui_script_log.txt"))
        .unwrap_or_else(|| std::path::PathBuf::from("ui_script_log.txt"));
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).ok();
    macro_rules! slog {
        ($($a:tt)*) => {{
            if let Some(f) = log.as_mut() { let _ = writeln!(f, $($a)*); let _ = f.flush(); }
            eprintln!($($a)*);
        }};
    }
    for _ in 0..100 {
        if state.with_client(|c| c.ping().map(|_| ()).map_err(|e| e.to_string())).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    slog!("script start: {script}");
    let mut mark: Option<u64> = None;
    for cmd in script.split(';').map(str::trim).filter(|c| !c.is_empty()) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let result: Result<(), String> = (|| match parts.as_slice() {
            ["sleep", ms] => {
                let ms = ms.parse().map_err(|_| "bad ms".to_string())?;
                std::thread::sleep(Duration::from_millis(ms));
                Ok(())
            }
            ["resume"] => state.with_client(|c| c.resume().map(|_| ()).map_err(|e| e.to_string())),
            ["stop"] => state.with_client(|c| c.stop().map(|_| ()).map_err(|e| e.to_string())),
            ["mark"] => state.with_client(|c| {
                let tids = c.threads().map_err(|e| e.to_string())?;
                let tid = *tids.first().ok_or("no threads".to_string())?;
                let pairs = c.context_get(tid).map_err(|e| e.to_string())?;
                let rip = pairs
                    .iter()
                    .find(|(r, _)| *r == 26)
                    .map(|(_, v)| *v)
                    .ok_or("no rip".to_string())?;
                mark = Some(rip);
                Ok(())
            }),
            ["bp", "rip"] => state.with_client(|c| {
                let tids = c.threads().map_err(|e| e.to_string())?;
                let tid = *tids.first().ok_or("no threads".to_string())?;
                let pairs = c.context_get(tid).map_err(|e| e.to_string())?;
                let rip = pairs
                    .iter()
                    .find(|(r, _)| *r == 26)
                    .map(|(_, v)| *v)
                    .ok_or("no rip".to_string())?;
                slog!("script: bp at rip=0x{rip:x}");
                c.bp_set(rip).map(|_| ()).map_err(|e| e.to_string())
            }),
            ["bp", "mark"] => {
                let addr = mark.ok_or("no mark".to_string())?;
                state.with_client(|c| c.bp_set(addr).map(|_| ()).map_err(|e| e.to_string()))
            }
            ["si"] | ["so"] => {
                let over = parts[0] == "so";
                state.with_client(|c| {
                    let tids = c.threads().map_err(|e| e.to_string())?;
                    let tid = *tids.first().ok_or("no threads".to_string())?;
                    c.step(tid, over).map(|_| ()).map_err(|e| e.to_string())
                })
            }
            ["bp", arg] => {
                let addr = if let Some(off) = arg.strip_prefix("main+") {
                    let off =
                        u64::from_str_radix(off.trim_start_matches("0x"), 16).map_err(|e| e.to_string())?;
                    state.with_client(|c| {
                        let mods = c.modules().map_err(|e| e.to_string())?;
                        mods.iter()
                            .find(|m| m.2)
                            .map(|m| m.0 + off)
                            .ok_or_else(|| "no main module".to_string())
                    })?
                } else {
                    parse_addr(arg)?
                };
                state.with_client(|c| c.bp_set(addr).map(|_| ()).map_err(|e| e.to_string()))
            }
            ["watch", secs] => {
                let secs: u64 = secs.parse().map_err(|_| "bad secs".to_string())?;
                let deadline = Instant::now() + Duration::from_secs(secs);
                let mut last: Option<(bool, u64)> = None;
                while Instant::now() < deadline {
                    let info = state.with_client(|c| c.bp_list().map_err(|e| e.to_string()));
                    if let Ok((stopped, hit_tid, hit_addr, gen, _)) = info {
                        if last != Some((stopped, gen)) {
                            slog!("watch: stopped={stopped} gen={gen} hit_tid={hit_tid} hit=0x{hit_addr:x}");
                            last = Some((stopped, gen));
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Ok(())
            }
            _ => Err(format!("bad script cmd: {cmd}")),
        })();
        match result {
            Ok(()) => slog!("script ok: {cmd}"),
            Err(error) => slog!("script FAIL: {cmd}: {error}"),
        }
    }
    slog!("script done");
}

fn main() {
    let config = parse_args();
    let backend: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let initial_target = if config.target.is_empty() {
        None
    } else {
        Some(config.target.join(" "))
    };

    // Auto-pick the port for owned sessions too: a fixed default port can be
    // held by a leaked zombie backend, and wait_for_port would then happily
    // attach us to the WRONG session (it answers connects like a live agent).
    let mut session_port = config.port;
    if !config.target.is_empty() {
        let options = launch::LaunchOptions {
            pin: config.pin.clone(),
            agent: config.agent.clone(),
            arch: None, // auto-detect from the target's PE headers
            port: 0,
            entry_bp: true, // debugger default: stop at the entry point
        };
        match launch::launch_for_target(&options, &config.target, STARTUP_TIMEOUT) {
            Ok((child, port)) => {
                session_port = port;
                *backend.lock().unwrap() = Some(child);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }

    let state = Arc::new(AppState {
        port: Mutex::new(session_port),
        pin: config.pin.clone(),
        agent: config.agent.clone(),
        client: Mutex::new(None),
        backend: Arc::clone(&backend),
        target: Mutex::new(initial_target),
    });

    // Test hook (headless UI verification): PINBRIDGE_UI_SCRIPT drives the
    // session through the shared client like a scripted user, logging state
    // transitions to ui_script_log.txt next to the exe.
    //   commands: sleep <ms> | resume | stop | bp <addr|main+0xrva> | watch <s>
    if let Ok(script) = std::env::var("PINBRIDGE_UI_SCRIPT") {
        let script_state = Arc::clone(&state);
        std::thread::spawn(move || run_script(script_state, &script));
    }
    let poller_state = Arc::clone(&state);
    let backend_for_exit = Arc::clone(&backend);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || poller(handle, poller_state));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd_session,
            cmd_launch,
            cmd_kill_backend,
            cmd_control,
            cmd_step,
            cmd_bp_set,
            cmd_bp_remove,
            cmd_bp_list,
            cmd_modules,
            cmd_threads,
            cmd_context,
            cmd_setreg,
            cmd_read_mem,
            cmd_write_mem,
            cmd_disasm,
            cmd_disasm_up,
            cmd_resolve,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_handle, event| {
            if let RunEvent::Exit = event {
                if let Some(child) = backend_for_exit.lock().unwrap().as_mut() {
                    launch::kill_backend(child);
                }
            }
        });
}
