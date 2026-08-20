//! Pure rendering: takes a UiState, draws a frame. No I/O here, so the
//! layout is testable headless via ratatui's TestBackend.

use pinbridge_client::client::{ScriptListEntry, Snapshot, KIND_NAMES};
use pinbridge_proto as proto;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Sparkline, Table, TableState};
use ratatui::Frame;
use std::collections::VecDeque;

/// Bound on retained plugin log lines (the server-side ring is 4096).
pub const PLUGIN_LINE_CAP: usize = 1000;

pub struct UiState {
    pub snapshot: Snapshot,
    /// events/sec samples, oldest first (rightmost is newest).
    pub rate_history: Vec<u64>,
    /// plugin print lines ("plugin: line"), oldest first; the panel shows the tail.
    pub plugin_lines: VecDeque<String>,
    /// one-line script_list summary; None when the server doesn't answer op 0x42.
    pub plugins_summary: Option<String>,
}

impl UiState {
    /// Append freshly fetched lines, evicting the oldest past PLUGIN_LINE_CAP.
    pub fn push_plugin_lines(&mut self, lines: Vec<String>) {
        for line in lines {
            if self.plugin_lines.len() >= PLUGIN_LINE_CAP {
                self.plugin_lines.pop_front();
            }
            self.plugin_lines.push_back(line);
        }
    }
}

/// Wire state byte -> display name (agent: 1 = running, 2 = error).
fn script_state_name(state: u8) -> &'static str {
    match state {
        1 => "running",
        2 => "error",
        _ => "?",
    }
}

/// "a.py(running,deliv 123) b.py(error)" — one line, fits in a panel title.
pub fn plugin_summary(entries: &[ScriptListEntry]) -> String {
    if entries.is_empty() {
        return "no plugins".to_string();
    }
    entries
        .iter()
        .map(|entry| {
            let mut text = format!(
                "{}({},deliv {}",
                entry.name,
                script_state_name(entry.state),
                entry.delivered
            );
            if entry.dropped > 0 {
                text.push_str(&format!(",drop {}", entry.dropped));
            }
            text.push(')');
            text
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn kind_name(kind: u32) -> &'static str {
    KIND_NAMES
        .get(kind.wrapping_sub(1) as usize)
        .copied()
        .unwrap_or("?")
}

fn event_line(event: &proto::EventRecord) -> String {
    match event.kind {
        1 => format!(
            "#{:<9} {:<11} tid={:<3} ip=0x{:x} rcx=0x{:x} rdx=0x{:x} r8=0x{:x} r9=0x{:x}",
            event.sequence,
            kind_name(event.kind),
            event.thread_id,
            event.address,
            event.arg0,
            event.arg1,
            event.arg2,
            event.arg3
        ),
        2 => format!(
            "#{:<9} {:<11} tid={:<3} ip=0x{:x} ea=0x{:x} size={} access={}",
            event.sequence,
            kind_name(event.kind),
            event.thread_id,
            event.address,
            event.arg0,
            event.arg1,
            event.arg2
        ),
        4 => format!(
            "#{:<9} {:<11} tid={:<3} ip=0x{:x} target=0x{:x} taken={}",
            event.sequence,
            kind_name(event.kind),
            event.thread_id,
            event.address,
            event.arg0,
            event.arg1
        ),
        _ => format!(
            "#{:<9} {:<11} tid={:<3} ip=0x{:x} size={}",
            event.sequence,
            kind_name(event.kind),
            event.thread_id,
            event.address,
            event.arg0
        ),
    }
}

pub fn draw(frame: &mut Frame, state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // status line
            Constraint::Length(3), // rate sparkline
            Constraint::Length(3), // ring gauge
            Constraint::Min(10),   // events + plugins
        ])
        .split(frame.area());

    // Bottom area: newest events on top, plugin log takes the lower half.
    let bottom = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[3]);

    let snap = &state.snapshot;
    let title = if snap.connected {
        format!(
            "pinbridge-agent | ABI {}.{} | pid {} | total {}",
            snap.abi.0, snap.abi.1, snap.pid, snap.total
        )
    } else {
        "pinbridge-agent | DISCONNECTED".to_string()
    };
    let style = if snap.connected {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(title)
            .style(style)
            .block(Block::default().borders(Borders::ALL)),
        chunks[0],
    );

    let rate: u64 = state.rate_history.last().copied().unwrap_or(0);
    let spark = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("events/s (now {rate})")),
        )
        .data(&state.rate_history)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(spark, chunks[1]);

    let used_ratio = if snap.capacity > 0 {
        (snap.total.min(snap.capacity)) as f64 / snap.capacity as f64
    } else {
        0.0
    };
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(format!(
            "ring {} / {} | dropped {}",
            snap.total.min(snap.capacity),
            snap.capacity,
            snap.dropped
        )))
        .ratio(used_ratio);
    frame.render_widget(gauge, chunks[2]);

    let counter_line = format!(
        "hook_regs={}  memory={}  exec={}  branch_edge={}",
        snap.kind_counts[0], snap.kind_counts[1], snap.kind_counts[2], snap.kind_counts[3]
    );
    let rows: Vec<Row> = std::iter::once(Row::new(vec![counter_line]))
        .chain(snap.newest.iter().map(|e| Row::new(vec![event_line(e)])))
        .collect();
    let table = Table::new(rows, [Constraint::Min(60)]).block(
        Block::default()
            .borders(Borders::ALL)
            .title("newest events"),
    );
    frame.render_stateful_widget(table, bottom[0], &mut TableState::default());

    let plugins_title = match &state.plugins_summary {
        Some(summary) => format!("Plugins | {summary}"),
        None => "Plugins".to_string(),
    };
    // Autoscroll: only the tail that fits inside the borders is rendered.
    let visible = bottom[1].height.saturating_sub(2) as usize;
    let skip = state.plugin_lines.len().saturating_sub(visible);
    let lines: Vec<Line> = state
        .plugin_lines
        .iter()
        .skip(skip)
        .map(|line| Line::from(line.clone()))
        .collect();
    let plugins =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(plugins_title));
    frame.render_widget(plugins, bottom[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn renders_dashboard() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.connected = true;
        snapshot.abi = (1, 1);
        snapshot.pid = 4242;
        snapshot.total = 13_639_398;
        snapshot.capacity = 65536;
        snapshot.dropped = 13_573_862;
        snapshot.kind_counts = [0, 2_976_481, 8_575_177, 2_087_740, 0, 0, 0, 0];
        snapshot.newest.push(proto::EventRecord {
            sequence: 13_639_398,
            kind: 4,
            thread_id: 0,
            address: 0x7ffbd135dd3c,
            arg0: 0x7ffbcff6e1a0,
            arg1: 1,
            ..Default::default()
        });
        let mut state = UiState {
            snapshot,
            rate_history: vec![10, 20, 40, 80],
            plugin_lines: VecDeque::new(),
            plugins_summary: None,
        };
        state.push_plugin_lines(vec![
            "oep.py: watching for OEP".to_string(),
            "trace.py: plugin-marker-7f3a captured".to_string(),
        ]);
        state.plugins_summary = Some("oep.py(running,deliv 123) trace.py(error)".to_string());
        terminal.draw(|frame| draw(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("pinbridge-agent"), "missing title: {text}");
        assert!(text.contains("pid 4242"), "missing pid");
        assert!(text.contains("events/s"), "missing rate panel");
        assert!(text.contains("branch_edge"), "missing event row");
        assert!(text.contains("0x7ffbd135dd3c"), "missing address");
        assert!(text.contains("Plugins"), "missing plugins panel");
        assert!(
            text.contains("oep.py(running,deliv 123)"),
            "missing summary"
        );
        assert!(text.contains("plugin-marker-7f3a"), "missing log line");
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.connected = true;
        let mut state = UiState {
            snapshot,
            rate_history: Vec::new(),
            plugin_lines: VecDeque::new(),
            plugins_summary: None,
        };
        state.push_plugin_lines(vec!["p.py: hello".to_string()]);
        terminal.draw(|frame| draw(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("newest events"), "events panel gone: {text}");
        assert!(text.contains("Plugins"), "plugins panel gone: {text}");
        assert!(text.contains("p.py: hello"), "log line gone: {text}");
    }

    #[test]
    fn plugin_lines_are_capped() {
        let mut state = UiState {
            snapshot: Snapshot::default(),
            rate_history: Vec::new(),
            plugin_lines: VecDeque::new(),
            plugins_summary: None,
        };
        state.push_plugin_lines(
            (0..PLUGIN_LINE_CAP + 10)
                .map(|i| format!("line {i}"))
                .collect(),
        );
        assert_eq!(state.plugin_lines.len(), PLUGIN_LINE_CAP);
        assert_eq!(state.plugin_lines.front().unwrap(), "line 10");
        state.push_plugin_lines(Vec::new());
        assert_eq!(state.plugin_lines.len(), PLUGIN_LINE_CAP);
    }

    #[test]
    fn summary_formats_entries() {
        let entries = vec![
            ScriptListEntry {
                name: "a.py".to_string(),
                state: 1,
                delivered: 123,
                dropped: 0,
            },
            ScriptListEntry {
                name: "b.py".to_string(),
                state: 2,
                delivered: 0,
                dropped: 5,
            },
        ];
        assert_eq!(
            plugin_summary(&entries),
            "a.py(running,deliv 123) b.py(error,deliv 0,drop 5)"
        );
        assert_eq!(plugin_summary(&[]), "no plugins");
    }
}
