//! Shared client-side pieces for pinbridge UI frontends (TUI, Tauri, MCP...).

pub mod arch;
pub mod client;
pub mod launch;
pub mod registers;

pub use arch::{detect_pe_arch, parse_pe, Arch, PeInfo};
pub use client::{Client, OutputLine, ScriptListEntry, Snapshot, KIND_NAMES};
pub use launch::{
    kill_backend, resolve_backend, resolve_pin, resolve_target_arch, spawn_backend,
    wait_for_entry_stop, wait_for_port, BackendConfig, LaunchMetadata, ResolvedBackend,
};
