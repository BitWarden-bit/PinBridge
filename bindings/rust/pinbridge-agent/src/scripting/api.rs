//! The `pb` module exposed to Python plugins.
//!
//! Every action function is a thin wrapper over the loopback query protocol
//! — one short-lived connection per call (the query server serves one client
//! at a time; a persistent connection would starve the UI). Registration
//! functions (pb.on_*/pb.watch) mutate the CURRENT plugin's filters — the
//! plugin whose top level / pb_init / callback is running right now.
//!
//! All Python runs on the scripting thread; pyo3 calls return Result and are
//! handled explicitly (panic="abort" would turn any panic into a process
//! abort inside the target).

use super::host::{connect, mark_native_dirty, BATCH_MAX};
use super::output;
use super::{current_plugin_name, with_current_plugin_mut, Watch, RPC_PORT};
use core::sync::atomic::Ordering;
use pinbridge_client::client::Client;
use pinbridge_sys::pb_pin_sleep;
use pyo3::prelude::*;

/// Canonical GP register table (id values mirror the ABI's PB_REG_*).
const GP_REGS: [(&str, u32); 18] = [
    ("rax", 10), ("rbx", 7), ("rcx", 9), ("rdx", 8),
    ("rsi", 4), ("rdi", 3), ("rbp", 5), ("rsp", 6),
    ("r8", 11), ("r9", 12), ("r10", 13), ("r11", 14),
    ("r12", 15), ("r13", 16), ("r14", 17), ("r15", 18),
    ("rip", 26), ("rflags", 25),
];

fn reg_id(name: &str) -> Option<u32> {
    GP_REGS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, id)| *id)
}

fn rpc<R>(f: impl FnOnce(&mut Client) -> std::io::Result<R>) -> Option<R> {
    // A mailbox command parked in send_command owns the single-threaded
    // query server until the scripting thread drains it — and it cannot
    // drain while this callback is blocked on a loopback read. Fail fast
    // instead of riding the full client timeout (the script run/off
    // wedge); event-driven plugins see the same None a failed RPC returns
    // today and re-fire on the next event.
    if super::send_waiting() {
        return None;
    }
    let port = RPC_PORT.load(Ordering::Acquire);
    let mut client = connect(port)?;
    f(&mut client).ok()
}

fn pin_sleep(ms: u32) {
    unsafe {
        pb_pin_sleep(ms);
    }
}

fn no_plugin() -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
        "registration outside of a plugin context",
    )
}

// ---- output ----

#[pyfunction(name = "print")]
fn pb_print(msg: &str) {
    let plugin = current_plugin_name().unwrap_or_else(|| "?".to_string());
    output::push(&plugin, msg);
    crate::log::line(&format!("[py:{plugin}] {msg}"));
}

/// Alias of print (old scripts used pb.log).
#[pyfunction(name = "log")]
fn pb_log(msg: &str) {
    let plugin = current_plugin_name().unwrap_or_else(|| "?".to_string());
    output::push(&plugin, msg);
    crate::log::line(&format!("[py:{plugin}] {msg}"));
}

// ---- target actions (one loopback RPC each) ----

#[pyfunction(name = "read_mem")]
fn pb_read_mem(addr: u64, len: u64) -> Option<Vec<u8>> {
    if len == 0 || len > (1 << 20) {
        return None;
    }
    rpc(|c| c.read_memory(addr, len))
}

#[pyfunction(name = "write_mem")]
fn pb_write_mem(addr: u64, data: &[u8]) -> u64 {
    rpc(|c| c.write_memory(addr, data)).unwrap_or(0)
}

#[pyfunction(name = "get_reg")]
fn pb_get_reg(tid: u32, name: &str) -> Option<u64> {
    let reg = reg_id(name)?;
    let pairs = rpc(|c| c.context_get(tid))?;
    pairs.into_iter().find(|(r, _)| *r == reg).map(|(_, v)| v)
}

#[pyfunction(name = "set_reg")]
fn pb_set_reg(tid: u32, name: &str, value: u64) -> bool {
    let Some(reg) = reg_id(name) else {
        return false;
    };
    rpc(|c| c.context_set(tid, reg, value)).is_some()
}

#[pyfunction(name = "bp_set")]
fn pb_bp_set(addr: u64) -> Option<u32> {
    rpc(|c| c.bp_set(addr))
}

#[pyfunction(name = "bp_remove")]
fn pb_bp_remove(id: u32) -> bool {
    rpc(|c| c.bp_remove(id)).is_some()
}

#[pyfunction(name = "hit")]
fn pb_hit() -> (Option<u32>, u64) {
    match rpc(|c| c.bp_list()) {
        Some((_, hit_tid, hit_addr, _, _)) => {
            ((hit_tid != u32::MAX).then_some(hit_tid), hit_addr)
        }
        None => (None, 0),
    }
}

#[pyfunction(name = "is_stopped")]
fn pb_is_stopped() -> bool {
    rpc(|c| c.bp_list())
        .map(|(stopped, _, _, _, _)| stopped)
        .unwrap_or(false)
}

#[pyfunction(name = "stop")]
fn pb_stop() -> bool {
    rpc(|c| c.stop()).unwrap_or(false)
}

#[pyfunction(name = "resume")]
fn pb_resume() -> bool {
    rpc(|c| c.resume()).unwrap_or(false)
}

#[pyfunction(name = "step")]
fn pb_step(tid: u32, over: bool) -> bool {
    rpc(|c| c.step(tid, over)).unwrap_or(false)
}

#[pyfunction(name = "wait_stop")]
fn pb_wait_stop(timeout_ms: u64) -> bool {
    let mut remaining = timeout_ms;
    loop {
        if pb_is_stopped() {
            return true;
        }
        if remaining == 0 {
            return false;
        }
        pin_sleep(5);
        remaining = remaining.saturating_sub(5);
    }
}

#[pyfunction(name = "sleep")]
fn pb_sleep(ms: u64) {
    pin_sleep(ms.min(3_600_000) as u32);
}

#[pyfunction(name = "resolve")]
fn pb_resolve(addr: u64) -> Option<String> {
    rpc(|c| c.resolve(&[addr]))?.pop()?.display()
}

#[pyfunction(name = "resolve_name")]
fn pb_resolve_name(spec: &str) -> Option<u64> {
    rpc(|c| c.resolve_name(spec)).filter(|addr| *addr != 0)
}

#[pyfunction(name = "disasm")]
fn pb_disasm(addr: u64, count: u64) -> Option<Vec<(u64, u32, u32, u64, String)>> {
    let count = count.clamp(1, 128);
    let rows = rpc(|c| c.disasm(addr, count))?;
    Some(
        rows.into_iter()
            .map(|(address, size, kind, text, _bytes, target)| {
                (address, size, kind, target, text)
            })
            .collect(),
    )
}

#[pyfunction(name = "modules")]
fn pb_modules() -> Vec<(u64, u64, bool, String)> {
    rpc(|c| c.modules()).unwrap_or_default()
}

#[pyfunction(name = "threads")]
fn pb_threads() -> Vec<u32> {
    rpc(|c| c.threads()).unwrap_or_default()
}

#[pyfunction(name = "counters")]
fn pb_counters() -> Option<(u64, u64, u64, Vec<u64>)> {
    let (total, dropped, capacity, kinds) = rpc(|c| c.counters())?;
    Some((total, dropped, capacity, kinds.to_vec()))
}

// ---- new actions (prior phases added the wire ops) ----

/// Named exports of a loaded module: (address, name) pairs.
#[pyfunction(name = "exports")]
fn pb_exports(module: &str) -> Vec<(u64, String)> {
    rpc(|c| c.exports(module)).unwrap_or_default()
}

/// Adds a hook point (kind-1 hook_regs event on hit). False when the 4096
/// point cap is hit.
#[pyfunction(name = "hook_set")]
fn pb_hook_set(addr: u64) -> bool {
    rpc(|c| c.hook_set(addr)).unwrap_or(false)
}

#[pyfunction(name = "hook_remove")]
fn pb_hook_remove(addr: u64) -> bool {
    rpc(|c| c.hook_remove(addr)).is_some()
}

#[pyfunction(name = "hook_clear")]
fn pb_hook_clear() -> bool {
    rpc(|c| c.hook_clear()).is_some()
}

/// Exception pause policy passthrough (EXC_POLICY_SET): enabled + code
/// (0 = any). Note the stop window is imprecise by design.
#[pyfunction(name = "exc_policy", signature = (enabled, code=0))]
fn pb_exc_policy(enabled: bool, code: u64) -> bool {
    rpc(|c| c.exc_policy_set(enabled, code as u32)).is_some()
}

// ---- trace recording channel (.pbtr file) ----

/// Maps a pb kind name to the record-channel kind: replay wants bytes and
/// values, so exec records as exec_bytes(9) and memory as mem_value(10).
fn record_kind(name: &str) -> Option<u32> {
    Some(match name {
        "exec" | "exec_bytes" => 9,
        "memory" | "mem" | "mem_value" => 10,
        "branch" | "branch_edge" => 4,
        "exec_plain" => 3,
        "mem_plain" => 2,
        _ => return None,
    })
}

/// Starts recording the given kinds (default exec+memory, at value tier)
/// for instructions in range (default: everywhere — narrow it, the tape is
/// lossless but the flood is real) into a .pbtr file. False when a session
/// is already recording or the arguments are rejected.
#[pyfunction(name = "trace_start", signature = (path, kinds=None, range=None))]
fn pb_trace_start(path: &str, kinds: Option<Vec<String>>, range: Option<(u64, u64)>) -> bool {
    let names =
        kinds.unwrap_or_else(|| vec!["exec".to_string(), "memory".to_string()]);
    let mut kind_ids = Vec::with_capacity(names.len());
    for name in &names {
        let Some(kind) = record_kind(name) else {
            return false;
        };
        kind_ids.push(kind);
    }
    let (lo, hi) = range.unwrap_or((0, u64::MAX));
    rpc(|c| c.trace_start(&kind_ids, lo, hi, path)).is_some()
}

/// Stops the recording session and returns (recorded, dropped).
#[pyfunction(name = "trace_stop")]
fn pb_trace_stop() -> Option<(u64, u64)> {
    rpc(|c| c.trace_stop())
}

/// (active, recorded, dropped) snapshot of the recording channel.
#[pyfunction(name = "trace_status")]
fn pb_trace_status() -> Option<(bool, u64, u64)> {
    rpc(|c| c.trace_status())
}

// ---- registrations (mutate the current plugin's filters) ----

/// Restrict on_exception to the given codes (None = all).
#[pyfunction(name = "on_exception", signature = (codes=None))]
fn pb_on_exception(codes: Option<Vec<u64>>) -> PyResult<()> {
    with_current_plugin_mut(|p| {
        p.filters.exc_codes = codes.map(|v| {
            let mut set = crate::new_set();
            set.extend(v.into_iter().map(|c| c as u32));
            set
        });
    })
    .ok_or_else(no_plugin)?;
    Ok(())
}

/// Restrict on_syscall to the given numbers (None = all). The native filter
/// is the union across plugins, recomputed on the next host tick.
#[pyfunction(name = "on_syscall", signature = (numbers=None))]
fn pb_on_syscall(numbers: Option<Vec<u64>>) -> PyResult<()> {
    with_current_plugin_mut(|p| {
        p.filters.syscall_numbers = numbers.map(|v| {
            let mut set = crate::new_set();
            set.extend(v.into_iter().map(|n| n as u32));
            set
        });
    })
    .ok_or_else(no_plugin)?;
    mark_native_dirty();
    Ok(())
}

/// Subscribe to breakpoint hits (on_bp_hit). Defined callback = subscribed
/// by default; this exists for explicitness.
#[pyfunction(name = "on_bp")]
fn pb_on_bp() -> PyResult<()> {
    with_current_plugin_mut(|p| p.filters.want_bp = true).ok_or_else(no_plugin)?;
    Ok(())
}

/// Subscribe to module load/unload (on_module_load / on_module_unload).
/// Delivery is callback-driven; this exists for explicitness.
#[pyfunction(name = "on_modules")]
fn pb_on_modules() {}

/// Subscribes on_event_batch to the given event kinds (hook, mem, exec,
/// branch, syscall, ctx, module_load, module_unload), optionally limited to
/// an address range; batch paces the per-tick page size (default 512).
#[pyfunction(name = "watch", signature = (kinds, range=None, batch=None))]
fn pb_watch(
    kinds: Vec<String>,
    range: Option<(u64, u64)>,
    batch: Option<u64>,
) -> PyResult<()> {
    let mut mask: u32 = 0;
    for name in &kinds {
        mask |= kind_bit(name).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("bad kind: {name}"))
        })?;
    }
    if mask == 0 {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "watch: no kinds given",
        ));
    }
    let (lo, hi) = range.unwrap_or((0, 0));
    let batch = batch.unwrap_or(512).clamp(1, BATCH_MAX);
    with_current_plugin_mut(|p| {
        p.filters.watch = Some(Watch {
            kinds_mask: mask,
            lo,
            hi,
            batch,
        });
    })
    .ok_or_else(no_plugin)?;
    Ok(())
}

/// Clears the watch subscription (dedicated callbacks keep flowing).
#[pyfunction(name = "unsubscribe")]
fn pb_unsubscribe() {
    with_current_plugin_mut(|p| p.filters.watch = None);
}

/// Old name of watch (kept so existing examples keep working).
#[pyfunction(name = "subscribe", signature = (kinds, range=None, batch=None))]
fn pb_subscribe(
    kinds: Vec<String>,
    range: Option<(u64, u64)>,
    batch: Option<u64>,
) -> PyResult<()> {
    pb_watch(kinds, range, batch)
}

fn kind_bit(name: &str) -> Option<u32> {
    Some(match name {
        "hook" | "hook_regs" => 1 << 1,
        "mem" | "memory" => 1 << 2,
        "exec" => 1 << 3,
        "branch" | "branch_edge" => 1 << 4,
        "syscall" => 1 << 5,
        "ctx" | "context_change" => 1 << 6,
        "module_load" => 1 << 7,
        "module_unload" => 1 << 8,
        _ => return None,
    })
}

#[pymodule]
fn pb(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pb_print, m)?)?;
    m.add_function(wrap_pyfunction!(pb_log, m)?)?;
    m.add_function(wrap_pyfunction!(pb_read_mem, m)?)?;
    m.add_function(wrap_pyfunction!(pb_write_mem, m)?)?;
    m.add_function(wrap_pyfunction!(pb_get_reg, m)?)?;
    m.add_function(wrap_pyfunction!(pb_set_reg, m)?)?;
    m.add_function(wrap_pyfunction!(pb_bp_set, m)?)?;
    m.add_function(wrap_pyfunction!(pb_bp_remove, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hit, m)?)?;
    m.add_function(wrap_pyfunction!(pb_is_stopped, m)?)?;
    m.add_function(wrap_pyfunction!(pb_stop, m)?)?;
    m.add_function(wrap_pyfunction!(pb_resume, m)?)?;
    m.add_function(wrap_pyfunction!(pb_step, m)?)?;
    m.add_function(wrap_pyfunction!(pb_wait_stop, m)?)?;
    m.add_function(wrap_pyfunction!(pb_sleep, m)?)?;
    m.add_function(wrap_pyfunction!(pb_resolve, m)?)?;
    m.add_function(wrap_pyfunction!(pb_resolve_name, m)?)?;
    m.add_function(wrap_pyfunction!(pb_disasm, m)?)?;
    m.add_function(wrap_pyfunction!(pb_modules, m)?)?;
    m.add_function(wrap_pyfunction!(pb_threads, m)?)?;
    m.add_function(wrap_pyfunction!(pb_counters, m)?)?;
    m.add_function(wrap_pyfunction!(pb_exports, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_set, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_remove, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_exc_policy, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_start, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_stop, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_status, m)?)?;
    m.add_function(wrap_pyfunction!(pb_on_exception, m)?)?;
    m.add_function(wrap_pyfunction!(pb_on_syscall, m)?)?;
    m.add_function(wrap_pyfunction!(pb_on_bp, m)?)?;
    m.add_function(wrap_pyfunction!(pb_on_modules, m)?)?;
    m.add_function(wrap_pyfunction!(pb_watch, m)?)?;
    m.add_function(wrap_pyfunction!(pb_unsubscribe, m)?)?;
    m.add_function(wrap_pyfunction!(pb_subscribe, m)?)?;
    Ok(())
}

/// Registers the pb module in Python's inittab. MUST run before
/// Py_Initialize (the macro panics otherwise — call exactly once, from the
/// scripting thread, before the interpreter starts).
pub fn append_pb_to_inittab() {
    pyo3::append_to_inittab!(pb);
}
