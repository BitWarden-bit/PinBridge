//! The scripting host loop: preload python310.dll, init the interpreter,
//! drain the mailbox, and tick every ~5ms — ONE loopback round trip per
//! tick, with the connection DROPPED before any Python callback runs (the
//! query server is single-threaded; callbacks issue their own RPCs).
//!
//! Sacred ordering (from the old script host, do not regress):
//!   1. SCRIPT_LOAD replies after COMPILE only;
//!   2. the top level and pb_init() run on the NEXT tick, after the query
//!      server answered the load;
//!   3. the tick's client connection is closed before Python runs.

use super::events::{self, EventSelector};
use super::output;
use super::subscriptions::{self, merge_action, StopAction};
use super::{
    agent_dir, python_ready, reply, set_mailbox, set_python_ready, with_plugin_context,
    with_registry, with_registry_mut, Plugin, ScriptCmd, ScriptReply, Watch, RPC_PORT, STATE_ERROR,
    STATE_RUNNING,
};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use pinbridge_client::client::Client;
use pinbridge_sys::pb_pin_sleep;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};

/// Event kind ids (wire values, mirror of the agent's event.rs).
const EVENT_HOOK_REGS: u32 = 1;
const EVENT_MEMORY: u32 = 2;
const EVENT_EXEC: u32 = 3;
const EVENT_BRANCH_EDGE: u32 = 4;
const EVENT_SYSCALL: u32 = 5;
const EVENT_CONTEXT_CHANGE: u32 = 6;
const EVENT_MODULE_LOAD: u32 = 7;
const EVENT_MODULE_UNLOAD: u32 = 8;

const CONTEXT_CHANGE_EXCEPTION: u64 = 4; // PB_CONTEXT_CHANGE_REASON_EXCEPTION

/// pb.watch default when on_event_batch exists but never subscribed.
const DEFAULT_WATCH_KINDS: u32 =
    (1 << EVENT_HOOK_REGS) | (1 << EVENT_MEMORY) | (1 << EVENT_EXEC) | (1 << EVENT_BRANCH_EDGE);
const DEFAULT_WATCH_BATCH: u64 = 512;

const RING_PAGE_CAP: u64 = 2048;
const PRIORITY_PAGE_CAP: usize = 512;
pub const BATCH_MAX: u64 = 4096;

// Debugger launches request an entry stop before plugin code is allowed to
// run.  Compilation may happen earlier so SCRIPT_LOAD remains responsive;
// execution opens permanently after the first real stop.  Raw runs without
// PINBRIDGE_ENTRY_BP retain the historical immediate-script behavior.
static STARTUP_GATE_OPEN: AtomicBool = AtomicBool::new(false);

fn startup_gate_open() -> bool {
    if std::env::var("PINBRIDGE_ENTRY_BP").ok().as_deref() != Some("1") {
        return true;
    }
    if STARTUP_GATE_OPEN.load(Ordering::Acquire) {
        return true;
    }
    let entry = crate::entry_bp_address();
    let (_tid, hit) = crate::bp::last_hit();
    if crate::control::is_stopped() && entry != 0 && hit == entry {
        STARTUP_GATE_OPEN.store(true, Ordering::Release);
        crate::log::line("scripting startup gate opened at entry stop");
        true
    } else {
        false
    }
}

/// Adaptive ceiling for the tick's ring pull (single writer: the host
/// thread; atomic for plain visibility). The throttle tracks ACTUAL Python
/// cost — events routed to callbacks last tick — not page fullness: sparse
/// consumers keep full pages (catching up under flood costs a cheap Rust
/// loop), while Python-heavy watchers shrink so a tick's encode+dispatch
/// stays inside the ~5ms budget.
static ADAPT_PAGE: AtomicU64 = AtomicU64::new(RING_PAGE_CAP);
const ADAPT_PAGE_MIN: u64 = 128;
/// Events routed to Python callbacks during the current/last tick.
static TICK_ROUTED: AtomicU64 = AtomicU64::new(0);

/// Adaptive tick cadence (ms between ticks). 5ms when the host is keeping
/// up; backs off toward 40ms while the ring is flooding (pages returning
/// with missed events = we are hopelessly behind, and every tick is then
/// pure churn: one loopback round trip + encode + dispatch against a
/// cursor that can never catch up). Drops are by design; the UI stays free.
static TICK_SLEEP_MS: AtomicU64 = AtomicU64::new(5);
const TICK_SLEEP_MAX: u64 = 40;

fn adapt_tick_sleep(behind: bool) {
    let cur = TICK_SLEEP_MS.load(Ordering::Relaxed);
    let next = if behind {
        (cur * 2).min(TICK_SLEEP_MAX)
    } else {
        (cur / 2).max(5)
    };
    TICK_SLEEP_MS.store(next, Ordering::Relaxed);
}

fn adapt_page_limit() {
    let routed = TICK_ROUTED.swap(0, Ordering::Relaxed);
    let cur = ADAPT_PAGE.load(Ordering::Relaxed);
    let next = if routed > 512 {
        (cur / 2).max(ADAPT_PAGE_MIN) // Python-heavy: back off hard
    } else if routed < 64 {
        (cur * 2).min(RING_PAGE_CAP) // cheap tick: allow full pages again
    } else {
        cur
    };
    ADAPT_PAGE.store(next, Ordering::Relaxed);
}

/// Set whenever the union of plugin syscall interests may have changed
/// (load / unload / pb.on_syscall); applied on the next tick's connection.
static NATIVE_DIRTY: AtomicBool = AtomicBool::new(false);

pub fn mark_native_dirty() {
    NATIVE_DIRTY.store(true, Ordering::Release);
}

pub fn connect(port: u16) -> Option<Client> {
    Client::connect(port).ok()
}

// ---- loopback tick client (short timeouts; see SEND_WAITING in mod.rs) ----

/// The host's own loopback client. pinbridge_client::Client rides 5s read
/// timeouts — fine for plugin pb.* calls, poisonous for the tick: when the
/// query server is parked in send_command waiting for THIS host, a tick RPC
/// issued into that window can only return via its read timeout, and three
/// stacked 5s stalls were the observed 5-15s `script run/off` wedge. Tick
/// RPCs therefore fail fast and the host loops back to the mailbox. Any
/// error poisons the connection (a timed-out read may have eaten half a
/// frame): the caller drops it and reconnects on the next tick.
struct TickClient {
    stream: std::net::TcpStream,
}

/// Every tick op is a millisecond-class loopback round trip when the server
/// is free; 250ms only ever fires when the server is parked (send_command)
/// or momentarily busy, and bounds the wedge either way.
const TICK_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);
/// resolve can hit a cold PE export parse server-side (thousands of safe
/// copies for a first-seen module); give it room, still bounded.
const RESOLVE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(2000);

/// One resolved address (wire shape of the RESOLVE op).
struct TickResolution {
    kind: u8,
    offset: u64,
    module: String,
    symbol: String,
}

impl TickResolution {
    /// Same rendering as pinbridge_client's Resolution::display.
    fn display(&self) -> Option<String> {
        match self.kind {
            2 if self.offset > 0 => Some(format!(
                "{}!{}+0x{:x}",
                self.module, self.symbol, self.offset
            )),
            2 => Some(format!("{}!{}", self.module, self.symbol)),
            1 => Some(format!("{}+0x{:x}", self.module, self.offset)),
            _ => None,
        }
    }
}

fn read_short_str(r: &mut pinbridge_proto::Reader) -> Option<String> {
    let len = r.u16()? as usize;
    let rest = r.remaining();
    if rest.len() < len {
        return None;
    }
    let text = String::from_utf8_lossy(&rest[..len]).into_owned();
    r.skip(len)?;
    Some(text)
}

impl TickClient {
    fn connect(port: u16) -> Option<TickClient> {
        let stream = std::net::TcpStream::connect(("127.0.0.1", port)).ok()?;
        stream.set_nodelay(true).ok()?;
        stream.set_read_timeout(Some(TICK_READ_TIMEOUT)).ok()?;
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs(1)))
            .ok()?;
        Some(TickClient { stream })
    }

    /// One request/response. None on any transport/server failure.
    fn request(&mut self, op_code: u8, payload: &[u8]) -> Option<Vec<u8>> {
        pinbridge_proto::write_frame(
            &mut self.stream,
            op_code,
            pinbridge_proto::STATUS_OK,
            payload,
        )
        .ok()?;
        let (resp_op, status, body) = pinbridge_proto::read_frame(&mut self.stream).ok()?;
        if resp_op != op_code || status != pinbridge_proto::STATUS_OK {
            return None;
        }
        Some(body)
    }

    /// Content edge (total retained events); 0 is the caller's fallback.
    fn counters_total(&mut self) -> Option<u64> {
        let body = self.request(pinbridge_proto::op::COUNTERS, &[])?;
        pinbridge_proto::Reader::new(&body).u64()
    }

    /// (stopped, hit_tid, hit_addr, stop_gen, entries) — same as bp_list.
    fn bp_list(&mut self) -> Option<(bool, u32, u64, u64, Vec<(u32, u64, u64)>)> {
        let body = self.request(pinbridge_proto::op::BP_LIST, &[])?;
        if body.len() < 21 {
            return None;
        }
        let stopped = body[0] != 0;
        let mut r = pinbridge_proto::Reader::new(&body[1..]);
        let hit_tid = r.u32()?;
        let hit_addr = r.u64()?;
        let stop_gen = r.u64()?;
        let count = r.u32()?;
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            entries.push((r.u32()?, r.u64()?, r.u64()?));
        }
        Some((stopped, hit_tid, hit_addr, stop_gen, entries))
    }

    fn bp_remove(&mut self, id: u32) -> Option<()> {
        let mut request = Vec::with_capacity(4);
        pinbridge_proto::put_u32(&mut request, id);
        self.request(pinbridge_proto::op::BP_REMOVE, &request)?;
        Some(())
    }

    fn context_get(&mut self, thread_id: u32) -> Option<Vec<(u32, u64)>> {
        let mut request = Vec::with_capacity(4);
        pinbridge_proto::put_u32(&mut request, thread_id);
        let body = self.request(pinbridge_proto::op::CONTEXT_GET, &request)?;
        let mut reader = pinbridge_proto::Reader::new(&body);
        let count = reader.u32()?;
        let mut registers = Vec::with_capacity(count as usize);
        for _ in 0..count {
            registers.push((reader.u32()?, reader.u64()?));
        }
        Some(registers)
    }

    /// (missed, next, events) for sequences after `after`.
    fn ring_page(
        &mut self,
        after: u64,
        limit: u64,
    ) -> Option<(u64, u64, Vec<pinbridge_proto::EventRecord>)> {
        let mut request = Vec::with_capacity(16);
        pinbridge_proto::put_u64(&mut request, after);
        pinbridge_proto::put_u64(&mut request, limit);
        let body = self.request(pinbridge_proto::op::RING_PAGE, &request)?;
        let mut r = pinbridge_proto::Reader::new(&body);
        let _total = r.u64()?;
        let missed = r.u64()?;
        let next = r.u64()?;
        let count = r.u64()?;
        let mut events = Vec::with_capacity(count as usize);
        let mut rest = r.remaining();
        for _ in 0..count {
            events.push(pinbridge_proto::EventRecord::decode(rest)?);
            rest = &rest[pinbridge_proto::EVENT_WIRE_LEN..];
        }
        Some((missed, next, events))
    }

    fn resolve(&mut self, addresses: &[u64]) -> Option<Vec<TickResolution>> {
        self.stream
            .set_read_timeout(Some(RESOLVE_READ_TIMEOUT))
            .ok()?;
        let mut request = Vec::with_capacity(4 + addresses.len() * 8);
        pinbridge_proto::put_u32(&mut request, addresses.len() as u32);
        for address in addresses {
            pinbridge_proto::put_u64(&mut request, *address);
        }
        let body = self.request(pinbridge_proto::op::RESOLVE, &request)?;
        let _ = self.stream.set_read_timeout(Some(TICK_READ_TIMEOUT));
        let mut r = pinbridge_proto::Reader::new(&body);
        let count = r.u32()?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let kind = r.u8()?;
            let _base = r.u64()?;
            let offset = r.u64()?;
            let module = read_short_str(&mut r)?;
            let symbol = read_short_str(&mut r)?;
            out.push(TickResolution {
                kind,
                offset,
                module,
                symbol,
            });
        }
        Some(out)
    }

    fn engine_set(&mut self, kind: u32, on: bool) -> Option<()> {
        let mut request = Vec::with_capacity(5);
        pinbridge_proto::put_u32(&mut request, kind);
        request.push(on as u8);
        self.request(pinbridge_proto::op::ENGINE_SET, &request)?;
        Some(())
    }

    fn syscall_filter(&mut self, mode: u8, numbers: &[u32]) -> Option<()> {
        let mut request = Vec::with_capacity(3 + numbers.len() * 4);
        request.push(mode);
        request.extend_from_slice(&(numbers.len() as u16).to_le_bytes());
        for number in numbers {
            pinbridge_proto::put_u32(&mut request, *number);
        }
        self.request(pinbridge_proto::op::SYSCALL_FILTER, &request)?;
        Some(())
    }
}

/// A waiting mailbox command owns the near future: the query server is a
/// single thread parked in send_command, so every connected phase below
/// would stall on it while it stalls on us. Checked before each blocking
/// step; on true the tick bails out and the loop drains the mailbox.
#[inline]
fn send_waiting() -> bool {
    super::send_waiting()
}

// ---- bootstrap ----

pub fn run() -> ! {
    let port = RPC_PORT.load(Ordering::Acquire);
    {
        let mut tid: pinbridge_sys::PbThreadId = 0;
        unsafe {
            pinbridge_sys::pb_pin_thread_id(&mut tid);
        }
        crate::log::line(&format!(
            "scripting thread up (pin tid {tid}, os tid {})",
            unsafe { GetCurrentThreadId() }
        ));
    }
    // py_load_in_flight was armed in spawn() (before this thread could load
    // anything); it is cleared only after the whole load+init attempt ends.
    let python_module = preload_python();
    let ready = match python_module {
        Some(module) => match super::python_data::init_cells(module) {
            Ok(()) => init_interpreter(),
            Err(export) => {
                crate::log::line(&format!(
                    "scripting disabled: python310.dll export missing: {export}"
                ));
                false
            }
        },
        None => false,
    };
    super::set_py_load_in_flight(false);
    let (tx, rx) = std::sync::mpsc::channel();
    set_mailbox(tx);
    set_python_ready(ready);
    if !ready {
        crate::log::line("scripting unavailable: script ops degrade cleanly");
    }
    let mut pending: Vec<(String, Py<PyAny>)> = Vec::new();
    if ready {
        autoload_plugins(&mut pending);
    }
    host_loop(&rx, &mut pending, port)
}

fn host_loop(
    rx: &std::sync::mpsc::Receiver<ScriptCmd>,
    pending: &mut Vec<(String, Py<PyAny>)>,
    port: u16,
) -> ! {
    let mut heartbeat: u64 = 0;
    loop {
        let mut handled = false;
        while let Ok(cmd) = rx.try_recv() {
            handled = true;
            let result = match cmd {
                ScriptCmd::Load { name, source } => {
                    let result = cmd_load(pending, &name, &source);
                    match &result {
                        Ok(_) => crate::log::line(&format!("plugin compiled: {name}")),
                        Err(error) => {
                            crate::log::line(&format!("plugin compile failed ({name}): {error}"))
                        }
                    }
                    result.map(ScriptReply::Id)
                }
                ScriptCmd::Unload { name } => {
                    cmd_unload(&name);
                    Ok(ScriptReply::Ok)
                }
            };
            reply(result);
        }
        if handled {
            crate::diag::heap_check("mailbox");
        }
        tick(pending, port);
        heartbeat += 1;
        // Background bracket only: full-heap HeapValidate holds the heap
        // lock for the whole scan, so a fast cadence stalls the query
        // server's big ring_page allocations (probe effect swamps signal).
        if heartbeat % 400 == 0 {
            crate::diag::heap_check("tick");
        }
        // TEMP hunt aid: PINBRIDGE_HEAP_CHECK_FAST=1 validates every tick.
        if crate::diag::heap_check_fast_enabled() && heartbeat % 4 == 0 {
            crate::diag::heap_check("tick-fast");
        }
        // The snapshot also republishes periodically: delivered/dropped
        // counters advance on every dispatch, not just on registry
        // mutations, and SCRIPT_LIST readers poll between them.
        if heartbeat % 200 == 0 {
            super::publish_list_snapshot();
        }
        if heartbeat % 1200 == 0 {
            crate::log::line(&format!("scripting heartbeat {heartbeat}"));
        }
        unsafe {
            pb_pin_sleep(TICK_SLEEP_MS.load(Ordering::Relaxed) as u32);
        }
    }
}

// ---- python310.dll preload + interpreter init (scripting thread only) ----

extern "system" {
    fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetCurrentThreadId() -> u32;
}

const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x8;

/// Preloads python310.dll with the REAL OS loader (Pin's tool loader never
/// saw it: the import is delay-loaded). Once loaded, the agent's own
/// delay-loaded imports resolve to this module on first use. Returns the
/// module handle so the data-symbol cells can be filled from it.
fn preload_python() -> Option<*mut c_void> {
    if let Some(dir) = agent_dir() {
        let path = format!("{dir}\\python310.dll");
        let wide: Vec<u16> = path.encode_utf16().chain(core::iter::once(0)).collect();
        let module = unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                core::ptr::null_mut(),
                LOAD_WITH_ALTERED_SEARCH_PATH,
            )
        };
        if !module.is_null() {
            crate::log::line(&format!("python310.dll preloaded from {path}"));
            return Some(module);
        }
        crate::log::line(&format!(
            "python310.dll not loadable from {path}; trying PATH"
        ));
    } else {
        crate::log::line("python310.dll preload: agent dir unresolved; trying PATH");
    }
    let wide: Vec<u16> = "python310.dll"
        .encode_utf16()
        .chain(core::iter::once(0))
        .collect();
    let module = unsafe { LoadLibraryW(wide.as_ptr()) };
    if !module.is_null() {
        crate::log::line("python310.dll preloaded via PATH");
        return Some(module);
    }
    crate::log::line("scripting disabled: python310.dll not found");
    None
}

fn status_message(status: &pyo3::ffi::PyStatus) -> String {
    if status.err_msg.is_null() {
        return "unknown error".to_string();
    }
    unsafe { std::ffi::CStr::from_ptr(status.err_msg) }
        .to_string_lossy()
        .into_owned()
}

fn status_failed(status: pyo3::ffi::PyStatus, what: &str) -> bool {
    let message = status_message(&status);
    if unsafe { pyo3::ffi::PyStatus_Exception(status) } != 0 {
        crate::log::line(&format!("{what} failed: {message}"));
        return true;
    }
    false
}

/// Manual interpreter start (no pyo3 auto-initialize): the default Python
/// config, switched to isolated + zip-based stdlib when the embedded distro
/// (python310.zip next to the agent) is present.
fn init_interpreter() -> bool {
    super::api::append_pb_to_inittab(); // must precede Py_Initialize
    unsafe {
        let mut preconfig: pyo3::ffi::PyPreConfig = core::mem::zeroed();
        pyo3::ffi::PyPreConfig_InitPythonConfig(&mut preconfig);
        if status_failed(pyo3::ffi::Py_PreInitialize(&preconfig), "python preinit") {
            return false;
        }
        let mut config: pyo3::ffi::PyConfig = core::mem::zeroed();
        pyo3::ffi::PyConfig_InitPythonConfig(&mut config);
        // The wide strings must outlive Py_InitializeFromConfig.
        let mut wide_paths: Vec<Vec<u16>> = Vec::new();
        let mut failed = false;
        if let Some(dir) = agent_dir() {
            let zip = format!("{dir}\\python310.zip");
            if std::path::Path::new(&zip).exists() {
                config.isolated = 1;
                config.module_search_paths_set = 1;
                for path in [&zip, &dir] {
                    let wide: Vec<u16> = path.encode_utf16().chain(core::iter::once(0)).collect();
                    let status = pyo3::ffi::PyWideStringList_Append(
                        &mut config.module_search_paths,
                        wide.as_ptr(),
                    );
                    if status_failed(status, "python module search path") {
                        failed = true;
                        break;
                    }
                    wide_paths.push(wide);
                }
                if !failed {
                    crate::log::line(&format!("python embedded distro: {zip}"));
                }
            }
        }
        if failed {
            pyo3::ffi::PyConfig_Clear(&mut config);
            return false;
        }
        let status = pyo3::ffi::Py_InitializeFromConfig(&config);
        pyo3::ffi::PyConfig_Clear(&mut config);
        if status_failed(status, "python init") {
            return false;
        }
        // Release the GIL; Python::with_gil re-acquires it per section.
        pyo3::ffi::PyEval_SaveThread();
    }
    crate::log::line("python interpreter initialized (in-agent)");
    true
}

/// PINBRIDGE_AGENT_PLUGINS=<dir>: load every *.py (sorted by name) through
/// the normal load path; failures land in the output ring and the agent log
/// and never stop the remaining loads.
fn autoload_plugins(pending: &mut Vec<(String, Py<PyAny>)>) {
    let Ok(dir) = std::env::var("PINBRIDGE_AGENT_PLUGINS") else {
        return;
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) => {
            crate::log::line(&format!("plugins dir unreadable ({dir}): {error}"));
            return;
        }
    };
    let mut files: Vec<(String, std::path::PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|e| e == "py").unwrap_or(false))
        .map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name, path)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, path) in files {
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                if let Err(error) = cmd_load(pending, &name, &source) {
                    output::push(&name, &format!("autoload failed: {error}"));
                    crate::log::line(&format!("plugin autoload failed ({name}): {error}"));
                }
            }
            Err(error) => {
                output::push(&name, &format!("autoload read failed: {error}"));
                crate::log::line(&format!("plugin autoload read failed ({name}): {error}"));
            }
        }
    }
}

// ---- mailbox command handlers (scripting thread) ----

/// Compile-only load (upsert by name). The module's top level runs on the
/// next tick, AFTER the load reply freed the query server.
fn cmd_load(
    pending: &mut Vec<(String, Py<PyAny>)>,
    name: &str,
    source: &str,
) -> Result<u32, String> {
    if !python_ready() {
        return Err("python unavailable: python310.dll not loaded".to_string());
    }
    crate::diag::heap_check("cmd_load pre");
    let replaced = with_registry(|r| r.contains_key(name));
    if replaced {
        retire_plugin(name, "replaced");
    }
    let c_source =
        std::ffi::CString::new(source).map_err(|_| "source contains a NUL byte".to_string())?;
    let c_name = std::ffi::CString::new(name).map_err(|_| "bad plugin name".to_string())?;
    Python::with_gil(|py| {
        let code = unsafe {
            pyo3::ffi::Py_CompileString(
                c_source.as_ptr(),
                c_name.as_ptr(),
                pyo3::ffi::Py_file_input,
            )
        };
        crate::diag::heap_check("cmd_load post-compile");
        if code.is_null() {
            let error = PyErr::fetch(py);
            let text = format!("{error}");
            crate::diag::heap_check("cmd_load post-errfmt");
            output::push(name, &format!("compile error: {text}"));
            return Err(text);
        }
        pending.push((name.to_string(), unsafe { Py::from_owned_ptr(py, code) }));
        Ok(1)
    })
}

fn cmd_unload(name: &str) {
    let names: Vec<String> = if name.is_empty() {
        with_registry(|r| r.keys().cloned().collect())
    } else {
        vec![name.to_string()]
    };
    for name in names {
        retire_plugin(&name, "unloaded");
    }
}

/// Removes a plugin: on_unload under the GIL with the plugin context set,
/// then drop (Py objects released under the GIL). Lifecycle is recorded in
/// the output ring and the agent log.
fn retire_plugin(name: &str, reason: &str) {
    let (existed, breakpoint_ids, hook_addresses) = Python::with_gil(|py| {
        let on_unload = with_registry(|r| {
            r.get(name)
                .and_then(|p| p.on_unload.as_ref().map(|cb| cb.clone_ref(py)))
        });
        if let Some(callback) = on_unload {
            with_plugin_context(name, || {
                if let Err(error) = callback.call0(py) {
                    output::push(name, &format!("on_unload failed: {error}"));
                    crate::log::line(&format!("plugin {name} on_unload failed: {error}"));
                }
            });
        }
        with_registry_mut(|r| {
            let Some(plugin) = r.remove(name) else {
                return (false, Vec::new(), Vec::new());
            };
            let ids = plugin.breakpoints.keys().copied().collect();
            let hook_addresses = plugin
                .decisions
                .values()
                .filter_map(|subscription| subscription.address)
                .collect();
            // Drop every Py callback while the GIL is held.
            drop(plugin);
            (true, ids, hook_addresses)
        })
    });
    if existed {
        for id in breakpoint_ids {
            if subscriptions::release_native(id) {
                subscriptions::queue_native_removal(id);
            }
        }
        for address in hook_addresses {
            if super::decisions::release_hook(address) {
                super::decisions::queue_hook_removal(address);
            }
        }
        super::interceptors::publish_interests();
        super::native_policies::refresh_best_effort("plugin retired");
        output::push(name, reason);
        crate::log::line(&format!("plugin {reason}: {name}"));
        mark_native_dirty();
        super::publish_list_snapshot();
    }
}

/// Runs a freshly compiled plugin's top level, then pb_init(), then grabs
/// the fixed callbacks and applies default subscriptions.
fn exec_pending(pending: &mut Vec<(String, Py<PyAny>)>, port: u16) {
    if pending.is_empty() {
        return;
    }
    let batch = std::mem::take(pending);
    for (name, code) in batch {
        exec_one(&name, code, port);
    }
}

fn exec_one(name: &str, code: Py<PyAny>, port: u16) {
    crate::diag::heap_check("exec_one pre");
    Python::with_gil(|py| {
        let module = match PyModule::new_bound(py, name) {
            Ok(module) => module,
            Err(error) => {
                output::push(name, &format!("module init failed: {error}"));
                crate::log::line(&format!("plugin {name} module init failed: {error}"));
                return;
            }
        };
        let globals = module.dict();
        // Register BEFORE exec so top-level pb.on_*/pb.watch calls mutate
        // this plugin's filters.
        let plugin = Plugin {
            name: name.to_string(),
            module: module.clone().unbind(),
            state: STATE_RUNNING,
            on_exception: None,
            on_syscall: None,
            on_bp_hit: None,
            on_module_load: None,
            on_module_unload: None,
            on_event_batch: None,
            on_stop: None,
            on_unload: None,
            events: crate::new_map(),
            decisions: crate::new_map(),
            instrumentation: None,
            memory_translation: None,
            code_fetch: None,
            breakpoints: crate::new_map(),
            filters: super::Filters::default(),
            cursor: 0,
            priority_cursor: 0,
            last_stop_gen: 0,
            last_breakpoint_gen: 0,
            delivered: 0,
            dropped: 0,
        };
        with_registry_mut(|r| r.insert(name.to_string(), plugin));
        let outcome: Result<(), PyErr> = with_plugin_context(name, || {
            let result = unsafe {
                pyo3::ffi::PyEval_EvalCode(code.as_ptr(), globals.as_ptr(), globals.as_ptr())
            };
            if result.is_null() {
                return Err(PyErr::fetch(py));
            }
            let init = module
                .getattr("pb_init")
                .ok()
                .filter(|value| value.is_callable());
            if let Some(init) = init {
                init.call0()?;
            }
            Ok(())
        });
        if let Err(error) = outcome {
            output::push(name, &format!("top-level failed: {error}"));
            crate::log::line(&format!("plugin {name} top-level failed: {error}"));
            with_registry_mut(|r| {
                if let Some(p) = r.get_mut(name) {
                    p.state = STATE_ERROR;
                }
            });
            super::interceptors::publish_interests();
            super::native_policies::refresh_best_effort("plugin initialization failed");
            super::publish_list_snapshot();
            return;
        }
        // Grab the fixed callbacks, then default-subscribe whatever the
        // plugin defined but did not explicitly register.
        with_registry_mut(|r| {
            if let Some(p) = r.get_mut(name) {
                let grab = |attr: &str| -> Option<Py<PyAny>> {
                    module
                        .getattr(attr)
                        .ok()
                        .filter(|value| value.is_callable())
                        .map(|value| value.unbind())
                };
                p.on_exception = grab("on_exception");
                p.on_syscall = grab("on_syscall");
                p.on_bp_hit = grab("on_bp_hit");
                p.on_module_load = grab("on_module_load");
                p.on_module_unload = grab("on_module_unload");
                p.on_event_batch = grab("on_event_batch");
                p.on_stop = grab("on_stop");
                p.on_unload = grab("on_unload");
                default_subscribe(p);
            }
        });
        // Start fresh: events/stops from before the load are not the plugin's.
        let (cursor, priority_cursor, gen) = match TickClient::connect(port) {
            Some(mut client) => (
                client.counters_total().unwrap_or(0),
                crate::priority::total(),
                client.bp_list().map(|b| b.3).unwrap_or(0),
            ),
            None => (0, crate::priority::total(), 0),
        };
        with_registry_mut(|r| {
            if let Some(p) = r.get_mut(name) {
                p.cursor = cursor;
                p.priority_cursor = priority_cursor;
                p.last_stop_gen = gen;
            }
        });
        output::push(name, "loaded");
        crate::log::line(&format!("plugin running: {name}"));
        mark_native_dirty();
        super::publish_list_snapshot();
    });
}

/// A defined callback without an explicit registration subscribes
/// unfiltered.
fn default_subscribe(plugin: &mut Plugin) {
    if plugin.on_event_batch.is_some() && plugin.filters.watch.is_none() {
        plugin.filters.watch = Some(Watch {
            kinds_mask: DEFAULT_WATCH_KINDS,
            lo: 0,
            hi: 0,
            batch: DEFAULT_WATCH_BATCH,
        });
    }
    if plugin.on_bp_hit.is_some() {
        plugin.filters.want_bp = true;
    }
}

// ---- the tick ----

/// What one connected phase fetched, shared by every plugin's dispatch.
struct TickShared {
    stop_gen: u64,
    stopped: bool,
    hit_tid: u32,
    hit_addr: u64,
    bp_entries: Vec<(u32, u64, u64)>,
    context_registers: Vec<(u32, u64)>,
    priority_events: Vec<pinbridge_proto::EventRecord>,
    events: Vec<pinbridge_proto::EventRecord>,
    module_names: crate::TlsFreeMap<u64, String>,
}

fn consumes_ring(p: &Plugin) -> bool {
    p.on_exception.is_some()
        || p.on_syscall.is_some()
        || p.on_module_load.is_some()
        || p.on_module_unload.is_some()
        || p.events
            .values()
            .any(|subscription| !subscription.selector.is_priority())
        || (p.on_event_batch.is_some() && p.filters.watch.is_some())
}

fn consumes_priority(p: &Plugin) -> bool {
    p.events
        .values()
        .any(|subscription| subscription.selector.is_priority())
}

fn event_record(event: &crate::event::Event) -> pinbridge_proto::EventRecord {
    pinbridge_proto::EventRecord {
        sequence: event.sequence,
        kind: event.kind,
        thread_id: event.thread_id,
        address: event.address,
        arg0: event.arg0,
        arg1: event.arg1,
        arg2: event.arg2,
        arg3: event.arg3,
        arg4: event.arg4,
        arg5: event.arg5,
        arg6: event.arg6,
        arg7: event.arg7,
    }
}

fn tick(pending: &mut Vec<(String, Py<PyAny>)>, port: u16) {
    if send_waiting() {
        // A script op is parked in send_command on the query server; any
        // loopback work here would deadlock-into-timeout against it. Back
        // to the mailbox immediately.
        return;
    }
    if startup_gate_open() {
        exec_pending(pending, port);
    }
    if !python_ready() {
        return;
    }
    super::interceptors::dispatch_pending();
    // adapt the pull size to last tick's measured Python cost (routed count)
    adapt_page_limit();

    let (min_cursor, min_priority_cursor, wants_stop, page_limit) = with_registry(|r| {
        let mut min_cursor: Option<u64> = None;
        let mut min_priority_cursor: Option<u64> = None;
        let mut wants_stop = false;
        let mut page_limit = 0u64;
        let mut any_watch = false;
        for p in r.values() {
            if p.state != STATE_RUNNING {
                continue;
            }
            if consumes_ring(p) {
                min_cursor = Some(min_cursor.map(|m: u64| m.min(p.cursor)).unwrap_or(p.cursor));
            }
            if consumes_priority(p) {
                min_priority_cursor = Some(
                    min_priority_cursor
                        .map(|m: u64| m.min(p.priority_cursor))
                        .unwrap_or(p.priority_cursor),
                );
            }
            if p.filters.want_bp || p.on_stop.is_some() || !p.breakpoints.is_empty() {
                wants_stop = true;
            }
            if p.on_event_batch.is_some() {
                if let Some(w) = &p.filters.watch {
                    any_watch = true;
                    page_limit = page_limit.max(w.batch);
                }
            }
        }
        let page_limit = if any_watch {
            page_limit.clamp(1, RING_PAGE_CAP)
        } else {
            RING_PAGE_CAP
        };
        // adaptive good-citizen ceiling under flood (see adapt_page_limit)
        let page_limit = page_limit.min(ADAPT_PAGE.load(Ordering::Relaxed));
        (min_cursor, min_priority_cursor, wants_stop, page_limit)
    });
    let knobs_dirty = NATIVE_DIRTY.swap(false, Ordering::AcqRel);
    let pending_native_removals = subscriptions::has_native_removals();
    let exit_delivery_pending = crate::lifecycle::exit_delivery_pending();
    if min_cursor.is_none()
        && min_priority_cursor.is_none()
        && !wants_stop
        && !knobs_dirty
        && !pending_native_removals
        && !exit_delivery_pending
    {
        return;
    }
    diag("tick conn begin");

    // Phase 1: one short server round trip; the connection MUST be dropped
    // before any Python callback runs (callbacks dial their own RPCs).
    let mut shared = TickShared {
        stop_gen: 0,
        stopped: false,
        hit_tid: u32::MAX,
        hit_addr: 0,
        bp_entries: Vec::new(),
        context_registers: Vec::new(),
        priority_events: Vec::new(),
        events: Vec::new(),
        module_names: crate::new_map(),
    };
    if let Some(after) = min_priority_cursor {
        let mut events = Vec::with_capacity(PRIORITY_PAGE_CAP);
        if crate::priority::try_page(after, PRIORITY_PAGE_CAP, &mut events).is_some() {
            shared
                .priority_events
                .extend(events.iter().map(event_record));
        }
    }
    {
        if send_waiting() {
            return;
        }
        let Some(mut client) = TickClient::connect(port) else {
            return; // control plane momentarily unreachable; retry next tick
        };
        diag("tick connected");
        if pending_native_removals && !send_waiting() {
            for id in subscriptions::take_native_removals() {
                if client.bp_remove(id).is_none() {
                    subscriptions::queue_native_removal(id);
                }
            }
        }
        if knobs_dirty && !send_waiting() {
            recompute_native_knobs(&mut client);
        }
        if wants_stop && !send_waiting() {
            if let Some((stopped, hit_tid, hit_addr, gen, entries)) = client.bp_list() {
                shared.stopped = stopped;
                shared.hit_tid = hit_tid;
                shared.hit_addr = hit_addr;
                shared.stop_gen = gen;
                shared.bp_entries = entries;
                if stopped && hit_tid != u32::MAX {
                    shared.context_registers = client.context_get(hit_tid).unwrap_or_default();
                }
            }
        }
        if let Some(after) = min_cursor {
            if send_waiting() {
                return;
            }
            if let Some((missed, _next, events)) = client.ring_page(after, page_limit) {
                // flood gauge: cursor fell behind, or the page came back full
                adapt_tick_sleep(missed > 0 || events.len() as u64 >= page_limit);
                // Resolve module names for kind 7/8 while still connected.
                let mut bases: Vec<u64> = events
                    .iter()
                    .filter(|e| e.kind == EVENT_MODULE_LOAD || e.kind == EVENT_MODULE_UNLOAD)
                    .map(|e| e.arg0)
                    .collect();
                bases.sort_unstable();
                bases.dedup();
                if !bases.is_empty() && !send_waiting() {
                    if let Some(resolutions) = client.resolve(&bases) {
                        for (base, resolution) in bases.iter().zip(resolutions.iter()) {
                            let display = resolution
                                .display()
                                .unwrap_or_else(|| format!("0x{base:x}"));
                            shared.module_names.insert(*base, display);
                        }
                    }
                }
                shared.events = events;
            }
        }
    } // <- the connection is closed here, before Python runs
    diag("tick conn closed, dispatch begin");

    if send_waiting() {
        return; // free the host for the parked script op
    }
    // Phase 2: per-plugin dispatch (their pb.* RPCs get a free server).
    let (stop_action, dispatch_completed) = Python::with_gil(|py| {
        let names = with_registry(|r| {
            let mut names: Vec<String> = r.keys().cloned().collect();
            names.sort();
            names
        });
        let action = dispatch_bound_breakpoints(py, &names, &shared);
        let mut completed = true;
        for name in &names {
            if send_waiting() {
                completed = false;
                break; // script op parked on the server: stop dialing it
            }
            dispatch_one(py, name, &shared);
        }
        (action, completed)
    });
    if dispatch_completed && exit_delivery_pending {
        crate::lifecycle::acknowledge_exit_delivery();
    }
    apply_stop_action(port, stop_action, &shared);
    diag("tick dispatch end");
}

/// Verbose tick tracing for wedge hunting; on only with
/// PINBRIDGE_SCRIPT_DIAG=1 (the tick is ~200 Hz when consumers exist).
fn diag(msg: &str) {
    static ONCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ONCE.get_or_init(|| std::env::var("PINBRIDGE_SCRIPT_DIAG").ok().as_deref() == Some("1")) {
        crate::log::line(&format!("diag {msg}"));
    }
}

/// Union native requirements across fixed callbacks, named handlers and
/// batch watches. Enabling is monotonic here: a script must not switch off
/// an engine that a CLI/UI session enabled independently. Syscall filters
/// are still recomputed as a union because they are script-owned policy.
/// Runs on the scripting thread, never in an analysis callback.
fn recompute_native_knobs(client: &mut TickClient) {
    let (any, all, union, telemetry) = with_registry(|r| {
        let mut any = false;
        let mut all = false;
        let mut union = crate::new_set();
        let mut telemetry = [false; 3]; // memory, exec, branch
        for p in r.values() {
            if p.state != STATE_RUNNING {
                continue;
            }
            for subscription in p.events.values() {
                match subscription.selector {
                    EventSelector::Kind(EVENT_MEMORY) => telemetry[0] = true,
                    EventSelector::Kind(EVENT_EXEC) => telemetry[1] = true,
                    EventSelector::Kind(EVENT_BRANCH_EDGE) => telemetry[2] = true,
                    _ => {}
                }
            }
            if let Some(watch) = &p.filters.watch {
                telemetry[0] |= watch.kinds_mask & (1u32 << EVENT_MEMORY) != 0;
                telemetry[1] |= watch.kinds_mask & (1u32 << EVENT_EXEC) != 0;
                telemetry[2] |= watch.kinds_mask & (1u32 << EVENT_BRANCH_EDGE) != 0;
            }
            let generic_syscall = p
                .events
                .values()
                .any(|subscription| subscription.selector == EventSelector::Kind(EVENT_SYSCALL));
            if p.on_syscall.is_none() && !generic_syscall {
                continue;
            }
            any = true;
            if generic_syscall {
                // Named event subscriptions currently have no number
                // filter, so their native requirement is the full stream.
                all = true;
            }
            match &p.filters.syscall_numbers {
                None => all = true,
                Some(numbers) => union.extend(numbers.iter().copied()),
            }
        }
        (any, all, union, telemetry)
    });
    for (kind, enabled) in [EVENT_MEMORY, EVENT_EXEC, EVENT_BRANCH_EDGE]
        .into_iter()
        .zip(telemetry)
    {
        if enabled {
            let _ = client.engine_set(kind, true);
        }
    }
    if any {
        let _ = client.engine_set(EVENT_SYSCALL, true);
    }
    if all || !any {
        let _ = client.syscall_filter(0, &[]);
    } else {
        let numbers: Vec<u32> = union.into_iter().collect();
        let _ = client.syscall_filter(1, &numbers);
    }
}

struct BoundBreakpointSnapshot {
    plugin: String,
    id: u32,
    callback: Py<PyAny>,
    once: bool,
    order: u64,
}

fn parse_stop_action(value: &Bound<'_, PyAny>) -> Result<StopAction, String> {
    if value.is_none() {
        return Ok(StopAction::Stay);
    }
    if let Ok(name) = value.extract::<String>() {
        return StopAction::from_name(&name)
            .ok_or_else(|| format!("unknown breakpoint action: {name}"));
    }
    if let Ok(dict) = value.downcast::<PyDict>() {
        let action = dict
            .get_item("action")
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "breakpoint result dictionary needs 'action'".to_string())?;
        let name = action
            .extract::<String>()
            .map_err(|_| "breakpoint result 'action' must be a string".to_string())?;
        return StopAction::from_name(&name)
            .ok_or_else(|| format!("unknown breakpoint action: {name}"));
    }
    Err("breakpoint callback must return None, an action string, or {'action': ...}".to_string())
}

/// Invokes callbacks explicitly bound with pb.breakpoint().  This is kept
/// separate from the legacy per-plugin dispatch: bound handlers own a stop
/// decision, while on_bp_hit/on_stop are compatibility notifications.
fn dispatch_bound_breakpoints(
    py: Python<'_>,
    names: &[String],
    shared: &TickShared,
) -> Option<StopAction> {
    if !shared.stopped || shared.hit_tid == u32::MAX || shared.stop_gen == 0 {
        return None;
    }
    let Some((hit_id, _, hits)) = shared
        .bp_entries
        .iter()
        .find(|(_, address, _)| *address == shared.hit_addr)
        .copied()
    else {
        return None;
    };

    let mut handlers = with_registry(|registry| {
        let mut handlers = Vec::new();
        for name in names {
            let Some(plugin) = registry.get(name) else {
                continue;
            };
            if plugin.state != STATE_RUNNING || plugin.last_breakpoint_gen >= shared.stop_gen {
                continue;
            }
            let Some(subscription) = plugin.breakpoints.get(&hit_id) else {
                continue;
            };
            if subscription
                .thread_id
                .map(|tid| tid != shared.hit_tid)
                .unwrap_or(false)
            {
                continue;
            }
            handlers.push(BoundBreakpointSnapshot {
                plugin: name.clone(),
                id: hit_id,
                callback: subscription.callback.clone_ref(py),
                once: subscription.once,
                order: subscription.order,
            });
        }
        handlers
    });
    handlers.sort_by(|left, right| {
        left.plugin
            .cmp(&right.plugin)
            .then(left.order.cmp(&right.order))
    });
    if handlers.is_empty() {
        return None;
    }

    let mut merged: Option<StopAction> = None;
    for handler in handlers {
        // Give every plugin its own dictionary.  A handler is free to annotate
        // its event, but those mutations must not leak into another plugin.
        let registers = PyDict::new_bound(py);
        for (register, value) in &shared.context_registers {
            let name = crate::arch::gp_name(*register).unwrap_or("unknown");
            let _ = registers.set_item(name, *value);
        }
        let event = PyDict::new_bound(py);
        let _ = event.set_item("type", "breakpoint");
        let _ = event.set_item("id", handler.id);
        let _ = event.set_item("address", shared.hit_addr);
        let _ = event.set_item("addr", shared.hit_addr); // legacy-friendly alias
        let _ = event.set_item("tid", shared.hit_tid);
        let _ = event.set_item("stop_generation", shared.stop_gen);
        let _ = event.set_item("hits", hits);
        let _ = event.set_item("arch", crate::arch::name());
        let _ = event.set_item("pointer_width", crate::arch::pointer_width());
        let _ = event.set_item(
            "context_complete",
            shared.context_registers.len() == crate::arch::gp_registers().len(),
        );
        let _ = event.set_item("registers", &registers);

        let result = with_plugin_context(&handler.plugin, || {
            handler
                .callback
                .call1(py, (event,))
                .map_err(|error| error.to_string())
                .and_then(|value| parse_stop_action(value.bind(py)))
        });
        let (action, error) = match result {
            Ok(action) => (action, None),
            Err(error) => (StopAction::Stay, Some(error)),
        };
        merged = Some(merge_action(merged, action));

        let mut released = false;
        with_registry_mut(|registry| {
            if let Some(plugin) = registry.get_mut(&handler.plugin) {
                plugin.last_breakpoint_gen = shared.stop_gen;
                plugin.delivered = plugin.delivered.saturating_add(1);
                if error.is_some() {
                    plugin.state = STATE_ERROR;
                }
                if handler.once && plugin.breakpoints.remove(&handler.id).is_some() {
                    released = true;
                }
            }
        });
        if released && subscriptions::release_native(handler.id) {
            subscriptions::queue_native_removal(handler.id);
        }
        if let Some(error) = error {
            output::push(
                &handler.plugin,
                &format!("breakpoint {} callback failed: {error}", handler.id),
            );
            crate::log::line(&format!(
                "plugin {} breakpoint {} callback failed: {error}",
                handler.plugin, handler.id
            ));
            super::publish_list_snapshot();
            super::native_policies::refresh_best_effort("breakpoint callback failed");
        }
    }
    merged
}

/// Executes the one merged stop action after every plugin callback has
/// returned.  The stop generation is checked again so a legacy callback
/// that already resumed or stepped cannot be followed by a second action.
fn apply_stop_action(port: u16, action: Option<StopAction>, shared: &TickShared) {
    let removals = subscriptions::take_native_removals();
    let hook_removals = super::decisions::take_hook_removals();
    if action.is_none() && removals.is_empty() && hook_removals.is_empty() {
        return;
    }
    let Some(mut client) = connect(port) else {
        for id in removals {
            subscriptions::queue_native_removal(id);
        }
        for address in hook_removals {
            super::decisions::queue_hook_removal(address);
        }
        return;
    };
    for id in removals {
        if client.bp_remove(id).is_err() {
            // It may already be gone (for example a one-shot step landing).
            crate::log::line(&format!("bound breakpoint native remove skipped id={id}"));
        }
    }
    for address in hook_removals {
        if client.hook_remove(address).is_err() {
            super::decisions::queue_hook_removal(address);
            crate::log::line(&format!(
                "synchronous Hook native remove deferred address=0x{address:x}"
            ));
        }
    }
    let Some(action) = action else {
        return;
    };
    if action == StopAction::Stay {
        return;
    }
    let Ok((stopped, hit_tid, hit_addr, generation, _)) = client.bp_list() else {
        return;
    };
    if !stopped
        || generation != shared.stop_gen
        || hit_tid != shared.hit_tid
        || hit_addr != shared.hit_addr
    {
        return;
    }
    let result = match action {
        StopAction::Stay => true,
        StopAction::Resume => client.resume().unwrap_or(false),
        StopAction::StepInto => client.step(shared.hit_tid, false).unwrap_or(false),
        StopAction::StepOver => client.step(shared.hit_tid, true).unwrap_or(false),
    };
    if !result {
        crate::log::line(&format!("bound breakpoint action failed: {action:?}"));
    }
}

/// Callbacks a dispatch may invoke, cloned out of the registry (under the
/// GIL) so no registry reference is ever held across a Python call.
struct EventHandlerSnapshot {
    id: u64,
    selector: EventSelector,
    callback: Py<PyAny>,
    once: bool,
    order: u64,
    sticky_delivered: bool,
}

struct DispatchSnapshot {
    cursor: u64,
    priority_cursor: u64,
    last_stop_gen: u64,
    on_exception: Option<Py<PyAny>>,
    on_syscall: Option<Py<PyAny>>,
    on_bp_hit: Option<Py<PyAny>>,
    on_module_load: Option<Py<PyAny>>,
    on_module_unload: Option<Py<PyAny>>,
    on_event_batch: Option<Py<PyAny>>,
    on_stop: Option<Py<PyAny>>,
    event_handlers: Vec<EventHandlerSnapshot>,
    exc_codes: Option<crate::TlsFreeSet<u32>>,
    syscall_numbers: Option<crate::TlsFreeSet<u32>>,
    watch: Option<(u32, u64, u64, u64)>, // mask, lo, hi, batch
    want_bp: bool,
}

fn dispatch_one(py: Python<'_>, name: &str, shared: &TickShared) {
    let snapshot = with_registry(|r| {
        let plugin = r.get(name)?;
        if plugin.state != STATE_RUNNING {
            return None;
        }
        let mut event_handlers: Vec<EventHandlerSnapshot> = plugin
            .events
            .iter()
            .map(|(id, subscription)| EventHandlerSnapshot {
                id: *id,
                selector: subscription.selector,
                callback: subscription.callback.clone_ref(py),
                once: subscription.once,
                order: subscription.order,
                sticky_delivered: subscription.sticky_delivered,
            })
            .collect();
        event_handlers.sort_by_key(|handler| handler.order);
        Some(DispatchSnapshot {
            cursor: plugin.cursor,
            priority_cursor: plugin.priority_cursor,
            last_stop_gen: plugin.last_stop_gen,
            on_exception: plugin.on_exception.as_ref().map(|c| c.clone_ref(py)),
            on_syscall: plugin.on_syscall.as_ref().map(|c| c.clone_ref(py)),
            on_bp_hit: plugin.on_bp_hit.as_ref().map(|c| c.clone_ref(py)),
            on_module_load: plugin.on_module_load.as_ref().map(|c| c.clone_ref(py)),
            on_module_unload: plugin.on_module_unload.as_ref().map(|c| c.clone_ref(py)),
            on_event_batch: plugin.on_event_batch.as_ref().map(|c| c.clone_ref(py)),
            on_stop: plugin.on_stop.as_ref().map(|c| c.clone_ref(py)),
            event_handlers,
            exc_codes: plugin.filters.exc_codes.clone(),
            syscall_numbers: plugin.filters.syscall_numbers.clone(),
            watch: plugin
                .filters
                .watch
                .as_ref()
                .map(|w| (w.kinds_mask, w.lo, w.hi, w.batch)),
            want_bp: plugin.filters.want_bp,
        })
    });
    let Some(mut s) = snapshot else {
        return;
    };

    let mut delivered: u64 = 0;
    let mut dropped: u64 = 0;
    let mut failed: Option<String> = None;
    let mut remove_event_handlers: Vec<u64> = Vec::new();
    let mut delivered_sticky_handlers: Vec<u64> = Vec::new();

    // Every callback runs with the plugin context set, so pb.print carries
    // the plugin name and pb.on_*/pb.watch inside callbacks mutate this
    // plugin's filters.
    with_plugin_context(name, || {
        // Lifecycle state is sticky: plugins usually load after Pin's
        // application-start edge, so each new subscription receives the
        // current state once instead of losing it behind a fresh cursor.
        if crate::lifecycle::process_started() {
            let event = events::synthetic_process_event(crate::event::EVENT_PROCESS_START);
            let _ = route_named_handlers(
                py,
                &mut s.event_handlers,
                &event,
                shared,
                &mut delivered,
                &mut failed,
                &mut remove_event_handlers,
                &mut delivered_sticky_handlers,
            );
        }
        if failed.is_none() && crate::lifecycle::process_exiting() {
            let mut event = events::synthetic_process_event(crate::event::EVENT_PROCESS_EXIT);
            event.arg0 = crate::lifecycle::process_exit_code() as i64 as u64;
            event.arg1 = crate::lifecycle::process_exit_source() as u64;
            let _ = route_named_handlers(
                py,
                &mut s.event_handlers,
                &event,
                shared,
                &mut delivered,
                &mut failed,
                &mut remove_event_handlers,
                &mut delivered_sticky_handlers,
            );
        }

        // Rare events have an independent queue/cursor, so an instruction
        // flood cannot evict them before Python observes them.
        let mut priority_missed = 0u64;
        if failed.is_none() {
            for event in &shared.priority_events {
                if event.sequence <= s.priority_cursor {
                    continue;
                }
                if priority_missed == 0 && event.sequence > s.priority_cursor + 1 {
                    priority_missed = event.sequence - s.priority_cursor - 1;
                }
                s.priority_cursor = event.sequence;
                if route_named_handlers(
                    py,
                    &mut s.event_handlers,
                    event,
                    shared,
                    &mut delivered,
                    &mut failed,
                    &mut remove_event_handlers,
                    &mut delivered_sticky_handlers,
                ) {
                    break;
                }
            }
        }
        dropped += priority_missed;

        // Stop-gen edge: on_bp_hit first, then on_stop (tid -1 = manual pause).
        if failed.is_none() && shared.stop_gen > s.last_stop_gen {
            s.last_stop_gen = shared.stop_gen;
            if shared.stopped {
                let tid: i64 = if shared.hit_tid == u32::MAX {
                    -1
                } else {
                    shared.hit_tid as i64
                };
                if s.want_bp {
                    if let Some(callback) = &s.on_bp_hit {
                        let id = shared
                            .bp_entries
                            .iter()
                            .find(|entry| entry.1 == shared.hit_addr)
                            .map(|entry| entry.0)
                            .unwrap_or(0);
                        let event = PyDict::new_bound(py);
                        let _ = event.set_item("tid", tid);
                        let _ = event.set_item("addr", shared.hit_addr);
                        let _ = event.set_item("id", id);
                        if let Err(error) = callback.call1(py, (event,)) {
                            failed = Some(format!("on_bp_hit: {error}"));
                        }
                    }
                }
                if failed.is_none() {
                    if let Some(callback) = &s.on_stop {
                        if let Err(error) = callback.call1(py, (tid, shared.hit_addr as i64)) {
                            failed = Some(format!("on_stop: {error}"));
                        }
                    }
                }
            }
        }

        // Ring events: dedicated callbacks first, the watch batch collected
        // alongside (independent subscriptions).
        let mut batch: Vec<pinbridge_proto::EventRecord> = Vec::new();
        let mut plugin_missed: u64 = 0;
        if failed.is_none() {
            for event in &shared.events {
                if event.sequence <= s.cursor {
                    continue;
                }
                if plugin_missed == 0 && event.sequence > s.cursor + 1 {
                    plugin_missed = event.sequence - s.cursor - 1;
                }
                s.cursor = event.sequence;
                if route_event(py, &s, event, shared, &mut delivered, &mut failed) {
                    break; // callback failed: plugin marked error below
                }
                if route_named_handlers(
                    py,
                    &mut s.event_handlers,
                    event,
                    shared,
                    &mut delivered,
                    &mut failed,
                    &mut remove_event_handlers,
                    &mut delivered_sticky_handlers,
                ) {
                    break;
                }
                if s.on_event_batch.is_some() {
                    if let Some((mask, lo, hi, _batch)) = &s.watch {
                        if (mask & (1u32 << event.kind)) != 0
                            && (*hi == 0 || (event.address >= *lo && event.address < *hi))
                        {
                            batch.push(*event);
                        }
                    }
                }
            }
        }

        if failed.is_none() {
            if let (Some(callback), Some(_)) = (&s.on_event_batch, &s.watch) {
                if !batch.is_empty() || plugin_missed > 0 {
                    match build_event_list(py, &batch) {
                        Ok(events) => {
                            delivered += batch.len() as u64;
                            if let Err(error) = callback.call1(py, (events, plugin_missed)) {
                                failed = Some(format!("on_event_batch: {error}"));
                            }
                        }
                        Err(error) => {
                            failed = Some(format!("on_event_batch(build): {error}"));
                        }
                    }
                }
            }
        }
        dropped += plugin_missed;
    });

    TICK_ROUTED.fetch_add(delivered, Ordering::Relaxed);

    // Write back consumption state (brief mutable access; the plugin may
    // have mutated its own filters from inside a callback — keep those).
    with_registry_mut(|r| {
        if let Some(p) = r.get_mut(name) {
            p.cursor = s.cursor;
            p.priority_cursor = s.priority_cursor;
            p.last_stop_gen = s.last_stop_gen;
            p.delivered += delivered;
            p.dropped += dropped;
            for id in &delivered_sticky_handlers {
                if let Some(subscription) = p.events.get_mut(id) {
                    subscription.sticky_delivered = true;
                }
            }
            for id in &remove_event_handlers {
                p.events.remove(id);
            }
            if failed.is_some() {
                p.state = STATE_ERROR;
            }
        }
    });
    if failed.is_some() {
        super::publish_list_snapshot();
        super::native_policies::refresh_best_effort("event callback failed");
    }
    if let Some(what) = failed {
        output::push(name, &format!("callback failed: {what}"));
        crate::log::line(&format!("plugin {name} callback failed: {what}"));
    }
}

/// Routes one event through the plugin's ordered `pb.on(...)` handlers.
/// The snapshot is mutable only to suppress a duplicate raw lifecycle edge
/// after a sticky replay during the same tick; registry state is written
/// back after every Python callback has returned.
#[allow(clippy::too_many_arguments)]
fn route_named_handlers(
    py: Python<'_>,
    handlers: &mut [EventHandlerSnapshot],
    event: &pinbridge_proto::EventRecord,
    shared: &TickShared,
    delivered: &mut u64,
    failed: &mut Option<String>,
    removals: &mut Vec<u64>,
    sticky_deliveries: &mut Vec<u64>,
) -> bool {
    for handler in handlers {
        if !handler.selector.matches(event) {
            continue;
        }
        if handler.selector.is_sticky() && handler.sticky_delivered {
            continue;
        }

        if handler.selector.is_sticky() {
            handler.sticky_delivered = true;
            if !sticky_deliveries.contains(&handler.id) {
                sticky_deliveries.push(handler.id);
            }
        }
        if handler.once && !removals.contains(&handler.id) {
            removals.push(handler.id);
        }

        let module_name = match event.kind {
            EVENT_MODULE_LOAD | EVENT_MODULE_UNLOAD => {
                shared.module_names.get(&event.arg0).map(String::as_str)
            }
            _ => None,
        };
        let event_dict = match events::build_event_dict(py, handler.selector, event, module_name) {
            Ok(event_dict) => event_dict,
            Err(error) => {
                *failed = Some(format!(
                    "pb.on({}): event build failed: {error}",
                    handler.selector.event_type()
                ));
                return true;
            }
        };
        match handler.callback.call1(py, (event_dict,)) {
            Ok(_) => *delivered += 1,
            Err(error) => {
                *failed = Some(format!("pb.on({}): {error}", handler.selector.event_type()));
                return true;
            }
        }
    }
    false
}

/// Routes one ring event to its dedicated callback. Returns true when a
/// callback failed (caller stops dispatching this plugin).
fn route_event(
    py: Python<'_>,
    s: &DispatchSnapshot,
    event: &pinbridge_proto::EventRecord,
    shared: &TickShared,
    delivered: &mut u64,
    failed: &mut Option<String>,
) -> bool {
    let result: Option<Result<(), String>> = match event.kind {
        EVENT_CONTEXT_CHANGE if event.arg0 == CONTEXT_CHANGE_EXCEPTION => {
            let code = event.arg1 as u32;
            match (&s.on_exception, &s.exc_codes) {
                (Some(callback), codes) if codes.as_ref().map_or(true, |c| c.contains(&code)) => {
                    let dict = PyDict::new_bound(py);
                    let _ = dict.set_item("tid", event.thread_id);
                    let _ = dict.set_item("code", event.arg1);
                    let _ = dict.set_item("rip", event.arg2);
                    let _ = dict.set_item("reason", event.arg0);
                    Some(
                        callback
                            .call1(py, (dict,))
                            .map(|_| ())
                            .map_err(|e| format!("on_exception: {e}")),
                    )
                }
                _ => None,
            }
        }
        EVENT_SYSCALL => {
            let number = event.arg0 as u32;
            match (&s.on_syscall, &s.syscall_numbers) {
                (Some(callback), numbers)
                    if numbers.as_ref().map_or(true, |n| n.contains(&number)) =>
                {
                    let dict = PyDict::new_bound(py);
                    let phase = event.arg1;
                    let _ = dict.set_item("number", event.arg0);
                    let _ = dict.set_item("phase", phase);
                    let _ = dict.set_item("tid", event.thread_id);
                    let args: Vec<u64> = if phase == 0 {
                        vec![
                            event.arg2, event.arg3, event.arg4, event.arg5, event.arg6, event.arg7,
                        ]
                    } else {
                        Vec::new()
                    };
                    let _ = dict.set_item("args", args);
                    let _ = dict.set_item("retval", if phase == 1 { event.arg3 } else { 0 });
                    Some(
                        callback
                            .call1(py, (dict,))
                            .map(|_| ())
                            .map_err(|e| format!("on_syscall: {e}")),
                    )
                }
                _ => None,
            }
        }
        EVENT_MODULE_LOAD | EVENT_MODULE_UNLOAD => {
            let is_load = event.kind == EVENT_MODULE_LOAD;
            let callback = if is_load {
                &s.on_module_load
            } else {
                &s.on_module_unload
            };
            let what = if is_load {
                "on_module_load"
            } else {
                "on_module_unload"
            };
            match callback {
                Some(callback) => {
                    let base = event.arg0;
                    let module_name = shared
                        .module_names
                        .get(&base)
                        .cloned()
                        .unwrap_or_else(|| format!("0x{base:x}"));
                    let dict = PyDict::new_bound(py);
                    let _ = dict.set_item("base", base);
                    let _ = dict.set_item("end", if is_load { event.arg1 } else { 0 });
                    let _ = dict.set_item("is_main", event.arg2 != 0);
                    let _ = dict.set_item("name", module_name);
                    Some(
                        callback
                            .call1(py, (dict,))
                            .map(|_| ())
                            .map_err(|e| format!("{what}: {e}")),
                    )
                }
                None => None,
            }
        }
        _ => None,
    };
    match result {
        Some(Ok(())) => {
            *delivered += 1;
            false
        }
        Some(Err(what)) => {
            *failed = Some(what);
            true
        }
        None => false,
    }
}

fn build_event_list(py: Python<'_>, batch: &[pinbridge_proto::EventRecord]) -> PyResult<Py<PyAny>> {
    let out = PyList::empty_bound(py);
    for event in batch {
        let row = PyDict::new_bound(py);
        row.set_item("seq", event.sequence)?;
        row.set_item("kind", event.kind)?;
        row.set_item("kind_name", kind_name(event.kind))?;
        row.set_item("tid", event.thread_id)?;
        row.set_item("addr", event.address)?;
        row.set_item("a0", event.arg0)?;
        row.set_item("a1", event.arg1)?;
        row.set_item("a2", event.arg2)?;
        row.set_item("a3", event.arg3)?;
        row.set_item("a4", event.arg4)?;
        row.set_item("a5", event.arg5)?;
        row.set_item("a6", event.arg6)?;
        row.set_item("a7", event.arg7)?;
        out.append(row)?;
    }
    Ok(out.into_any().unbind())
}

fn kind_name(kind: u32) -> &'static str {
    crate::event::kind_name(kind)
}
