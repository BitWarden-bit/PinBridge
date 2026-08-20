//! No-Python build of the in-agent scripting host.
//!
//! Compiled in place of `scripting` when the `scripting` feature is explicitly
//! disabled with `--no-default-features` on either x64 or x86. The query
//! server, breaker, and trace paths keep calling `crate::scripting::*`
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

pub unsafe fn instrument_memory_translation(_ins: pinbridge_sys::PbInsHandle, _address: u64) {}

/// One SCRIPT_LIST row (shape-compatible with `scripting::PluginInfo`).
#[derive(Clone)]
pub struct PluginInfo {
    pub name: String,
    pub state: u8,
    pub delivered: u64,
    pub dropped: u64,
    pub breakpoints: Vec<BreakpointBindingInfo>,
    pub decisions: Vec<DecisionBindingInfo>,
}

#[derive(Clone)]
pub struct BreakpointBindingInfo {
    pub id: u32,
    pub callback_name: String,
    pub description: String,
    pub once: bool,
    pub thread_id: Option<u32>,
    pub last_stop_generation: u64,
    pub last_action: Option<String>,
    pub last_return: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct DecisionBindingInfo {
    pub id: u64,
    pub selector: String,
    pub callback_name: String,
    pub description: String,
    pub once: bool,
    pub address: Option<u64>,
    pub thread_id: Option<u32>,
    pub codes: Option<Vec<u32>>,
    pub last_generation: u64,
    pub last_return: Option<String>,
    pub last_error: Option<String>,
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

pub fn initialize_before_application() {}

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

pub fn note_external_breakpoint_set(_id: u32, _address: u64) {}

pub fn output_page(after: u64, _limit: usize) -> (u64, Vec<output::OutputEntry>) {
    (after, Vec::new())
}
