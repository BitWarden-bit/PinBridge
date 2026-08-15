//! No-Python build of the in-agent scripting host.
//!
//! Compiled in place of `scripting` when the `scripting` feature is off
//! (the i686 x86 build, where pyo3 cannot match a 64-bit CPython). The
//! query server, breaker, and trace paths keep calling `crate::scripting::*`
//! through this module, which supplies the same names and shapes with fixed
//! answers: every scripting op reports "python scripting disabled in this
//! build", while the control plane and trace recording stay fully usable.

use pinbridge_sys::{PbStatus, PB_OK};

pub fn python_ready() -> bool {
    false
}

/// No Python-owned hot-path policies exist in the no-scripting build.
pub fn initialize_native_policies() -> PbStatus {
    PB_OK
}

pub fn reregister_after_attach() -> PbStatus {
    PB_OK
}

pub unsafe fn instrument_memory_translation(
    _ins: pinbridge_sys::PbInsHandle,
    _address: u64,
) {
}

/// One SCRIPT_LIST row (shape-compatible with `scripting::PluginInfo`).
#[derive(Clone)]
pub struct PluginInfo {
    pub name: String,
    pub state: u8,
    pub delivered: u64,
    pub dropped: u64,
}

pub mod output {
    /// Shape-compatible with `scripting::output::OutputEntry`.
    #[derive(Clone)]
    pub struct OutputEntry {
        pub seq: u64,
        pub plugin: String,
        pub line: String,
    }
}

/// The breaker waits this out before a stop. With no Python there is never a
/// load in flight, so stops never defer.
pub fn py_load_in_flight() -> bool {
    false
}

/// No thread to spawn: return success so the control plane still comes up.
pub fn spawn(_port: u16) -> PbStatus {
    crate::log::line("python scripting disabled in this build (no scripting feature)");
    PB_OK
}

pub fn load(_name: String, _source: String) -> Result<u32, String> {
    Err("python scripting disabled in this build".to_string())
}

pub fn unload(_name: &str) -> Result<(), String> {
    Err("python scripting disabled in this build".to_string())
}

pub fn list() -> Result<Vec<PluginInfo>, String> {
    Ok(Vec::new())
}

pub fn output_page(after: u64, _limit: usize) -> (u64, Vec<output::OutputEntry>) {
    (after, Vec::new())
}
