//! Renders one dashboard frame with sample data to a text snapshot.
//! Dev aid: `cargo run -p pinbridge-tui --example render_demo` (no TTY needed).

#![allow(dead_code)]

#[path = "../src/ui.rs"]
mod ui;

use pinbridge_client::client::Snapshot;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn main() {
    let mut snapshot = Snapshot::default();
    snapshot.connected = true;
    snapshot.abi = (1, 1);
    snapshot.pid = 36128;
    snapshot.total = 18_407_694;
    snapshot.capacity = 65536;
    snapshot.dropped = 18_342_158;
    snapshot.kind_counts = [0, 5_034_169, 11_097_245, 2_276_280, 0, 0, 0, 0];
    for (i, (kind, addr, a0, a1)) in [
        (3u32, 0x7ffbcfa8acbcu64, 0x2u64, 0x0u64),
        (4, 0x7ffbcfa8acbe, 0x7ffbcfa8acc2, 0x1),
        (2, 0x7ffbcfa8acc0, 0x42c61cf480, 0x4),
    ]
    .iter()
    .enumerate()
    {
        snapshot.newest.push(pinbridge_proto::EventRecord {
            sequence: 18_407_692 + i as u64,
            kind: *kind,
            thread_id: 0,
            address: *addr,
            arg0: *a0,
            arg1: *a1,
            ..Default::default()
        });
    }
    let state = ui::UiState {
        snapshot,
        rate_history: vec![
            2_100_000, 3_400_000, 4_800_000, 5_400_000, 5_100_000, 5_500_000, 5_400_000,
            5_600_000, 5_450_000, 5_500_000,
        ],
        plugin_lines: std::collections::VecDeque::from(vec![
            "oep.py: iat scan complete, 312 thunks fixed".to_string(),
            "trace.py: page 47 delivered (4096 events)".to_string(),
            "oep.py: candidate OEP 0x1400012c0".to_string(),
        ]),
        plugins_summary: Some("oep.py(running,deliv 312) trace.py(running,deliv 47)".to_string()),
    };

    let backend = TestBackend::new(110, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| ui::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let area = *buffer.area();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        println!("{}", line.trim_end());
    }
}
