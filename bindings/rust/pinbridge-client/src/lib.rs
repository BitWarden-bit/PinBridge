//! Shared client-side pieces for pinbridge UI frontends (TUI, Tauri, MCP...).

pub mod client;
pub mod launch;

pub use client::{Client, OutputLine, ScriptListEntry, Snapshot, KIND_NAMES};
pub use launch::{kill_backend, resolve_pin, spawn_backend, wait_for_port, BackendConfig};
