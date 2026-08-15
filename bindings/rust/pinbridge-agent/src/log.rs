//! Lifecycle logging for the agent. Written from the tool main / internal
//! threads only — NEVER from analysis callbacks (hot path stays silent).
//! One file, truncated at startup, append after; path override via
//! PINBRIDGE_AGENT_LOG.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static PATH: Mutex<String> = Mutex::new(String::new());
/// The open log file, opened lazily once and reused: per-call open/close
/// churned the process heap hundreds of times per second on busy internal
/// threads (and a wedged heap turns every open into a crash roll of the
/// dice). Internal threads are never suspended by the breaker, so holding
/// this plain std mutex across a write is safe.
static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

fn path() -> String {
    let guard = PATH.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_empty() {
        std::env::var("PINBRIDGE_AGENT_LOG").unwrap_or_else(|_| "pinbridge-agent.log".to_string())
    } else {
        guard.clone()
    }
}

fn timestamp() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{}.{:03}", millis / 1000, millis % 1000)
}

/// Truncates the log and writes the startup header. Call once from the tool
/// main before anything else that logs.
pub fn init(child_control_port: Option<u16>) {
    let configured = std::env::var("PINBRIDGE_AGENT_LOG")
        .unwrap_or_else(|_| "pinbridge-agent.log".to_string());
    let selected = match child_control_port {
        Some(port) => {
            let original = std::path::Path::new(&configured);
            let parent = original.parent().unwrap_or_else(|| std::path::Path::new(""));
            let stem = original
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("pinbridge-agent");
            let extension = original
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("log");
            parent
                .join(format!(
                    "{stem}.child-{}-{port}.{extension}",
                    std::process::id()
                ))
                .to_string_lossy()
                .into_owned()
        }
        None => configured,
    };
    if let Ok(mut guard) = PATH.lock() {
        *guard = selected;
    }
    let _ = std::fs::write(path(), format!("{} agent starting\n", timestamp()));
}

/// Appends one line. Best-effort: logging must never break the host.
pub fn line(message: &str) {
    use std::io::Write;
    let mut guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path())
            .ok();
    }
    if let Some(file) = guard.as_mut() {
        let _ = writeln!(file, "{} {}", timestamp(), message);
    }
}

/// Appends a multi-line block (e.g. the fini summary) verbatim.
pub fn append_block(text: &str) {
    use std::io::Write;
    let mut guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path())
            .ok();
    }
    if let Some(file) = guard.as_mut() {
        let _ = file.write_all(text.as_bytes());
        if !text.ends_with('\n') {
            let _ = file.write_all(b"\n");
        }
    }
}
