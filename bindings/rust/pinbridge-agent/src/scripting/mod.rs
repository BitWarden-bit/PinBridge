//! In-agent CPython scripting host: multi-plugin, event-driven.
//!
//! One Pin internal thread owns the interpreter and the whole host (Python
//! never runs in Pin analysis callbacks). Every debugger operation a plugin
//! performs is a loopback TCP client call to the query server — the same
//! discipline the old separate script-host cdylib followed, now compiled
//! into the agent directly. Deployment: pinbridge_agent.dll + python310.dll
//! (+ optional python310.zip embedded stdlib) side by side; build.rs
//! delay-loads the python import so Pin's tool loader never sees it.
//!
//! Plugins are Python modules registered by name (upsert: a same-name load
//! replaces after calling the old plugin's `on_unload`). Each plugin's fixed
//! callbacks are auto-discovered from its module namespace:
//!   pb_init()                on_exception(evt)   on_syscall(evt)
//!   on_bp_hit(evt)           on_module_load(evt) on_module_unload(evt)
//!   on_event_batch(evts,missed)  on_stop(tid,addr)  on_unload()
//! A defined callback without an explicit pb.on_*/pb.watch registration is
//! subscribed unfiltered (see host::default_subscribe).

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use pinbridge_sys::*;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use std::sync::mpsc::Sender;

use crate::{new_map, TlsFreeMap, TlsFreeSet};

pub mod api;
mod code_fetch;
mod decisions;
mod events;
mod host;
mod instrumentation;
mod interceptors;
mod memory_translation;
mod native_policies;
pub mod output;
mod python_data;
mod subscriptions;
mod xed_decode;

pub const STATE_RUNNING: u8 = 1;
pub const STATE_ERROR: u8 = 2;

/// Loopback port of the query server; plugins dial it for every operation.
pub static RPC_PORT: AtomicU16 = AtomicU16::new(0);
/// True once python310.dll preloaded AND the interpreter initialized. When
/// false the command loop still answers every op cleanly (load errors,
/// list empty, output keeps serving).
static PYTHON_READY: AtomicBool = AtomicBool::new(false);

pub fn python_ready() -> bool {
    PYTHON_READY.load(Ordering::Acquire)
}

pub fn initialize_native_policies() -> PbStatus {
    native_policies::initialize()
}

pub fn reregister_after_attach() -> PbStatus {
    let status = native_policies::reregister_after_attach();
    if status != PB_OK {
        return status;
    }
    code_fetch::reregister_after_attach()
}

pub unsafe fn instrument_memory_translation(ins: PbInsHandle, address: u64) {
    memory_translation::instrument(ins, address);
}

pub fn set_python_ready(ready: bool) {
    PYTHON_READY.store(ready, Ordering::Release);
}

/// True while the scripting thread is inside the python310.dll
/// LoadLibraryExW / interpreter-init sequence. The breaker's stop paths wait
/// this out (bounded): a stop landing mid-load wedges the process — the
/// loader holds the OS loader lock and pinvm's module-load integration
/// stalls while the application is stopped, so the load never finishes; the
/// query server then blocks behind the loader lock on its next first-called
/// delay-loaded synch import, and the whole control plane freezes (the
/// observed stress_control "卡死": scripting stuck in LoadLibraryExW, query
/// server stuck in __tailMerge_api_ms_win_core_synch, app left stopped).
static PY_LOAD_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

pub fn py_load_in_flight() -> bool {
    PY_LOAD_IN_FLIGHT.load(Ordering::Acquire)
}

pub fn set_py_load_in_flight(loading: bool) {
    PY_LOAD_IN_FLIGHT.store(loading, Ordering::Release);
}

// ---- plugin registry (scripting thread only — RefCell, never a lock) ----

/// Native watch subscription (replaces the old global subscribe): which
/// event kinds in which address range flow to `on_event_batch`, and the per
/// tick page cap.
pub struct Watch {
    pub kinds_mask: u32,
    pub lo: u64,
    pub hi: u64, // 0 = no range limit
    pub batch: u64,
}

pub struct Filters {
    /// None = all exception codes.
    pub exc_codes: Option<TlsFreeSet<u32>>,
    /// None = all syscall numbers (native filter falls back to mode all).
    pub syscall_numbers: Option<TlsFreeSet<u32>>,
    pub watch: Option<Watch>,
    pub want_bp: bool,
}

impl Default for Filters {
    fn default() -> Self {
        Filters {
            exc_codes: None,
            syscall_numbers: None,
            watch: None,
            want_bp: false,
        }
    }
}

pub struct Plugin {
    #[allow(dead_code)]
    pub name: String,
    /// Owned so the module (and its functions) stays alive; never read —
    /// callbacks are invoked through the grabbed Py<PyAny> handles below.
    #[allow(dead_code)]
    pub module: Py<PyModule>,
    pub state: u8, // STATE_RUNNING | STATE_ERROR
    pub on_exception: Option<Py<PyAny>>,
    pub on_syscall: Option<Py<PyAny>>,
    pub on_bp_hit: Option<Py<PyAny>>,
    pub on_module_load: Option<Py<PyAny>>,
    pub on_module_unload: Option<Py<PyAny>>,
    pub on_event_batch: Option<Py<PyAny>>,
    pub on_stop: Option<Py<PyAny>>,
    pub on_unload: Option<Py<PyAny>>,
    /// Named `pb.on(...)` subscriptions. Multiple handlers may observe the
    /// same event without creating more native Pin callbacks.
    pub events: TlsFreeMap<u64, events::EventSubscription>,
    /// Return-valued synchronous interceptors (`pb.intercept`).
    pub decisions: TlsFreeMap<u64, decisions::DecisionSubscription>,
    /// This plugin's high-frequency capture rules. They are compiled into
    /// one immutable native policy shared by the Pin analysis callbacks.
    pub instrumentation: Option<instrumentation::Spec>,
    /// Virtual-to-backing memory mappings compiled for Pin's process-global
    /// memory-address translation callback.
    pub memory_translation: Option<memory_translation::Spec>,
    /// Python-provided instruction bytes served directly by the native Pin
    /// code-fetch callback.
    pub code_fetch: Option<code_fetch::Spec>,
    /// Inputs applied by Pin's process-global pre-XED-decode callback.
    pub xed_decode: Option<xed_decode::Spec>,
    /// New-style callbacks bound to exact native breakpoint ids.  Legacy
    /// `on_bp_hit` remains separate and receives every stop notification.
    pub breakpoints: TlsFreeMap<u32, subscriptions::BreakpointSubscription>,
    pub filters: Filters,
    /// Ring cursor: events with sequence <= cursor are consumed already.
    pub cursor: u64,
    /// Independent cursor in the rare/high-priority event queue.
    pub priority_cursor: u64,
    pub last_stop_gen: u64,
    /// Separate edge cursor for bound breakpoint callbacks.  It must not
    /// share `last_stop_gen`: a plugin may use both the new handler and the
    /// legacy on_stop/on_bp_hit notifications for the same stop.
    pub last_breakpoint_gen: u64,
    pub delivered: u64,
    pub dropped: u64,
}

// ---- plugin registry (scripting thread only — static, never a lock) ----

/// The registry is touched ONLY from the scripting thread: mailbox handlers,
/// the tick, and Python callbacks all run there; the query-server thread
/// only talks to the mailbox. It is a plain static, not a thread_local:
/// Pin internal threads tear down TLS in ways std cannot track — a
/// thread_local registry died here with `AccessError` (cannot access a TLS
/// value during/after destruction) on a live thread. No std locks either:
/// analysis callbacks never come near it.
static mut REGISTRY: Option<TlsFreeMap<String, Plugin>> = None;
/// Name of the plugin whose top level / pb_init / callback is running right
/// now; pb.on_*/pb.watch registrations mutate THAT plugin.
static mut CURRENT_PLUGIN: Option<String> = None;

/// Immutable access to the registry (scripting thread only). Never hold the
/// reference across a Python call — callbacks re-enter mutably.
pub fn with_registry<R>(f: impl FnOnce(&TlsFreeMap<String, Plugin>) -> R) -> R {
    unsafe {
        let map = &mut *core::ptr::addr_of_mut!(REGISTRY);
        f(map.get_or_insert_with(new_map))
    }
}

/// Mutable access to the registry (scripting thread only). Same discipline.
pub fn with_registry_mut<R>(f: impl FnOnce(&mut TlsFreeMap<String, Plugin>) -> R) -> R {
    unsafe {
        let map = &mut *core::ptr::addr_of_mut!(REGISTRY);
        f(map.get_or_insert_with(new_map))
    }
}

/// Name of the plugin currently executing Python code (None outside any
/// plugin context). Scripting thread only.
pub fn current_plugin_name() -> Option<String> {
    unsafe { (*core::ptr::addr_of!(CURRENT_PLUGIN)).clone() }
}

/// Mutates the plugin currently executing Python code, if any (scripting
/// thread only; called from pb.on_*/pb.watch inside callbacks/top levels).
pub fn with_current_plugin_mut<R>(f: impl FnOnce(&mut Plugin) -> R) -> Option<R> {
    let name = current_plugin_name()?;
    with_registry_mut(|r| r.get_mut(&name).map(f))
}

/// Sets the current-plugin context around a piece of Python execution
/// (scripting thread only).
pub fn with_plugin_context<R>(name: &str, f: impl FnOnce() -> R) -> R {
    unsafe {
        *core::ptr::addr_of_mut!(CURRENT_PLUGIN) = Some(name.to_string());
    }
    let result = f();
    unsafe {
        *core::ptr::addr_of_mut!(CURRENT_PLUGIN) = None;
    }
    result
}

// ---- mailbox command channel (query-server thread sends, host answers) ----

pub enum ScriptCmd {
    Load {
        name: String,
        source: String,
    },
    /// Empty name = unload all.
    Unload {
        name: String,
    },
}

pub enum ScriptReply {
    Id(u32),
    Ok,
}

/// One SCRIPT_LIST row.
#[derive(Clone)]
pub struct PluginInfo {
    pub name: String,
    pub state: u8,
    pub delivered: u64,
    pub dropped: u64,
}

/// Published after every registry mutation; lets SCRIPT_LIST answer without
/// a mailbox round trip. The old host's status op was lock-free for the
/// same reason: a single-threaded query server blocked in send_command
/// collides with the scripting thread's own loopback tick calls, and every
/// poll would ride that stall window (observed as flaky 10060s on routine
/// `script list` from a persistent shell). Both threads here are ordinary
/// (no Pin analysis callbacks), so a briefly-held std Mutex is fine.
static LIST_SNAPSHOT: std::sync::Mutex<Vec<PluginInfo>> = std::sync::Mutex::new(Vec::new());

/// Rebuilds the snapshot from the registry (scripting thread only).
pub fn publish_list_snapshot() {
    let snapshot = with_registry(|r| {
        let mut entries: Vec<PluginInfo> = r
            .values()
            .map(|p| PluginInfo {
                name: p.name.clone(),
                state: p.state,
                delivered: p.delivered,
                dropped: p.dropped,
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    });
    *LIST_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner()) = snapshot;
}

static MAILBOX: std::sync::OnceLock<Sender<ScriptCmd>> = std::sync::OnceLock::new();
static REPLY_SLOT: std::sync::Mutex<Option<Result<ScriptReply, String>>> =
    std::sync::Mutex::new(None);
static REPLY_READY: AtomicBool = AtomicBool::new(false);
static SEND_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// True while the query-server thread is parked in send_command waiting for
/// the scripting host. The host MUST NOT start a blocking loopback RPC then:
/// the single-threaded server cannot serve it until the mailbox is drained,
/// and the host cannot drain the mailbox until its RPC returns — the
/// observed 5-15s `script run/off` wedge. The tick checks this before every
/// connected phase and skips straight back to the mailbox instead.
static SEND_WAITING: AtomicBool = AtomicBool::new(false);

/// True while a mailbox command is waiting for the host (see SEND_WAITING).
pub fn send_waiting() -> bool {
    SEND_WAITING.load(Ordering::Acquire)
}

pub fn set_mailbox(tx: Sender<ScriptCmd>) {
    let _ = MAILBOX.set(tx); // set exactly once (spawn is called once)
}

pub fn send_command(cmd: ScriptCmd) -> Result<ScriptReply, String> {
    let _guard = SEND_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tx = MAILBOX.get().ok_or("scripting host not running")?;
    {
        *REPLY_SLOT.lock().unwrap_or_else(|e| e.into_inner()) = None;
        REPLY_READY.store(false, Ordering::Release);
    }
    // Publish BEFORE the send lands in the queue: the host skips its
    // loopback RPCs from this point on, so it reaches the mailbox fast.
    SEND_WAITING.store(true, Ordering::Release);
    crate::diag::heap_check("send_command enter");
    let sent = tx.send(cmd);
    if sent.is_err() {
        SEND_WAITING.store(false, Ordering::Release);
        return Err("scripting host gone".to_string());
    }
    if std::env::var("PINBRIDGE_SCRIPT_DIAG").ok().as_deref() == Some("1") {
        crate::log::line("diag send_command sent, waiting");
    }
    for _ in 0..12_000 {
        if REPLY_READY.load(Ordering::Acquire) {
            SEND_WAITING.store(false, Ordering::Release);
            crate::diag::heap_check("send_command replied");
            return REPLY_SLOT
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
                .unwrap_or(Err("host produced no result".to_string()));
        }
        unsafe {
            pb_pin_sleep(5);
        }
    }
    SEND_WAITING.store(false, Ordering::Release);
    crate::diag::heap_check("send_command timeout");
    Err("script command timed out".to_string())
}

pub fn reply(result: Result<ScriptReply, String>) {
    {
        *REPLY_SLOT.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        REPLY_READY.store(true, Ordering::Release);
    }
}

// ---- public ops (query-server thread) ----

/// Loads (or replaces) a plugin. Blocks until the host has COMPILED the
/// source; syntax errors come back in the Err text. The module's top level
/// and pb_init() run on the host's first tick after this reply (the query
/// server must be free for the top level's pb.* calls).
pub fn load(name: String, source: String) -> Result<u32, String> {
    if !python_ready() {
        return Err("python unavailable: python310.dll not loaded".to_string());
    }
    match send_command(ScriptCmd::Load { name, source })? {
        ScriptReply::Id(id) => Ok(id),
        _ => Err("unexpected host reply".to_string()),
    }
}

pub fn unload(name: &str) -> Result<(), String> {
    send_command(ScriptCmd::Unload {
        name: name.to_string(),
    })?;
    Ok(())
}

/// Current plugin listing from the published snapshot (mailbox-free).
pub fn list() -> Result<Vec<PluginInfo>, String> {
    Ok(LIST_SNAPSHOT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}

/// SCRIPT_OUTPUT paging: lines with seq > `after`, oldest first.
/// (next_seq, entries).
pub fn output_page(after: u64, limit: usize) -> (u64, Vec<output::OutputEntry>) {
    output::page(after, limit)
}

// ---- spawn (from lib.rs, on Pin's tool-load path) ----

/// Spawns ONE Pin internal thread running the whole scripting host. The
/// OS-loader work (python310.dll preload, Py_Initialize) happens on THAT
/// thread — never here, because agent_main runs inside Pin's tool-load
/// sequence where entering the OS loader deadlocks the process.
///
/// Threading constraints that shaped this design (all observed in the
/// field, see third_party/pyo3/src/gil.rs for the linked one):
///   * a plain std::thread is NOT an option — Rust std's own thread-start
///     bookkeeping uses thread-locals, and this module's TLS index is never
///     assigned (Pin maps it privately), so a spawned std thread faults at
///     startup;
///   * std thread_local is equally unusable on the Pin internal thread, so
///     the registry is a plain static and pyo3 is vendored with its GIL
///     refcount made TLS-free — with both in place the Pin internal thread
///     runs the host cleanly (mpsc/Mutex/atomics were always fine there:
///     the query server predates them).
pub fn spawn(port: u16) -> PbStatus {
    if port == 0 {
        crate::log::line("scripting disabled: no control plane port");
        return PB_OK;
    }
    RPC_PORT.store(port, Ordering::Release);
    // Arm the load gate BEFORE the thread exists: a stop must never land
    // while the scripting thread is inside the python310.dll load (see
    // py_load_in_flight). Cleared at the end of host::run()'s init attempt.
    set_py_load_in_flight(true);
    let mut thread_id: PbThreadId = 0;
    let mut thread_uid: PbPinThreadUid = 0;
    let status = unsafe {
        pb_pin_spawn_internal_thread(
            Some(scripting_entry),
            core::ptr::null_mut(),
            0,
            &mut thread_id,
            &mut thread_uid,
        )
    };
    if status != PB_OK {
        set_py_load_in_flight(false); // no thread will clear it
    }
    status
}

unsafe extern "C" fn scripting_entry(_argument: *mut c_void) {
    host::run(); // never returns
}

// ---- agent directory resolution (GetMappedFileName + QueryDosDevice) ----

extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn K32GetMappedFileNameW(
        process: *mut c_void,
        address: *const c_void,
        buffer: *mut u16,
        size: u32,
    ) -> u32;
    fn QueryDosDeviceW(name: *const u16, buffer: *mut u16, size: u32) -> u32;
}

static MODULE_ANCHOR: u8 = 0;

/// Directory containing the agent DLL. Pin's tool loader maps the agent
/// privately (no PEB module entry), so GetModuleHandleEx(FROM_ADDRESS)
/// finds nothing — GetMappedFileName works on the raw mapping instead and
/// hands back a device path we convert to a drive path.
pub fn agent_dir() -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 512];
        let len = K32GetMappedFileNameW(
            GetCurrentProcess(),
            &MODULE_ANCHOR as *const u8 as *const c_void,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        );
        if len == 0 {
            return None;
        }
        let device = String::from_utf16_lossy(&buffer[..len as usize]);
        let dos = device_to_dos(&device)?;
        Some(
            dos.rfind(['\\', '/'])
                .map(|i| dos[..i].to_string())
                .unwrap_or(dos),
        )
    }
}

fn device_to_dos(device: &str) -> Option<String> {
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:", letter as char);
        let mut wide: Vec<u16> = drive.encode_utf16().collect();
        wide.push(0);
        let mut buffer = [0u16; 64];
        let len =
            unsafe { QueryDosDeviceW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
        if len == 0 {
            continue;
        }
        let target = String::from_utf16_lossy(&buffer[..len as usize]);
        let target = target.trim_end_matches('\0');
        if !target.is_empty() && device.starts_with(target) {
            return Some(format!("{}{}", drive, &device[target.len()..]));
        }
    }
    None
}
