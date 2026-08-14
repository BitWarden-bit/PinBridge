//! Bounded plugin output ring (4096 lines) behind the SCRIPT_OUTPUT op.
//!
//! Writers: the scripting thread only (pb.print, plugin errors, lifecycle
//! events). Reader: the query-server thread assembling SCRIPT_OUTPUT
//! replies. A plain std Mutex is safe here — neither side is a Pin analysis
//! callback, and both hold it for microseconds.

use std::collections::VecDeque;
use std::sync::Mutex;

const CAPACITY: usize = 4096;

#[derive(Clone)]
pub struct OutputEntry {
    pub seq: u64,
    pub plugin: String,
    pub line: String,
}

struct Ring {
    entries: VecDeque<OutputEntry>,
    next_seq: u64,
}

static RING: Mutex<Ring> = Mutex::new(Ring {
    entries: VecDeque::new(),
    next_seq: 1,
});

/// Appends one line (a lifecycle event, a pb.print, or a plugin error).
pub fn push(plugin: &str, line: &str) {
    let mut ring = RING.lock().unwrap_or_else(|e| e.into_inner());
    let seq = ring.next_seq;
    ring.next_seq += 1;
    if ring.entries.len() >= CAPACITY {
        ring.entries.pop_front();
    }
    ring.entries.push_back(OutputEntry {
        seq,
        plugin: plugin.to_string(),
        line: line.to_string(),
    });
}

/// Lines with seq > `after` (oldest first), capped at `limit`.
/// Returns (next_seq_for_followup, entries).
pub fn page(after: u64, limit: usize) -> (u64, Vec<OutputEntry>) {
    let ring = RING.lock().unwrap_or_else(|e| e.into_inner());
    let out: Vec<OutputEntry> = ring
        .entries
        .iter()
        .filter(|entry| entry.seq > after)
        .take(limit)
        .cloned()
        .collect();
    let next = out.last().map(|entry| entry.seq).unwrap_or(after);
    (next, out)
}
