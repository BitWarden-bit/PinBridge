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

use super::decisions::{self, DecisionSelector, DecisionSubscription, PUBLIC_DECISION_NAMES};
use super::events::{EventSelector, EventSubscription, PUBLIC_EVENT_NAMES};
use super::host::{connect, mark_native_dirty, BATCH_MAX};
use super::output;
use super::subscriptions::{self, BreakpointSubscription};
use super::{current_plugin_name, with_current_plugin_mut, Watch, RPC_PORT};
use core::sync::atomic::Ordering;
use pinbridge_client::client::Client;
use pinbridge_proto::ARCH_X64;
use pinbridge_sys::{pb_pin_sleep, PbRegId, PB_INVALID_THREAD_ID, PB_OK, PB_REG_INVALID_};
use pyo3::prelude::*;

/// Runtime architecture id from a PING reply (x64 fallback against an agent
/// that predates the arch extension).
fn ping_arch(client: &mut Client) -> std::io::Result<u32> {
    Ok(client.ping_full()?.arch.unwrap_or(ARCH_X64))
}

fn rpc<R>(f: impl FnOnce(&mut Client) -> std::io::Result<R>) -> Option<R> {
    // A mailbox command parked in send_command owns the single-threaded
    // query server until the scripting thread drains it — and it cannot
    // drain while this callback is blocked on a loopback read. Fail fast
    // instead of riding the full client timeout (the script run/off
    // wedge); event-driven plugins see the same None a failed RPC returns
    // today and re-fire on the next event.
    if super::send_waiting() || decisions::python_decision_active() {
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
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("registration outside of a plugin context")
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
    rpc(|c| {
        let arch = ping_arch(c)?;
        let pairs = c.context_get(tid)?;
        Ok(pinbridge_client::registers::reg_id(arch, name)
            .and_then(|reg| pairs.into_iter().find(|(r, _)| *r == reg).map(|(_, v)| v)))
    })
    .flatten()
}

#[pyfunction(name = "set_reg")]
fn pb_set_reg(tid: u32, name: &str, value: u64) -> bool {
    rpc(|c| {
        let arch = ping_arch(c)?;
        let Some(reg) = pinbridge_client::registers::reg_id(arch, name) else {
            return Ok(false);
        };
        c.context_set(tid, reg, value)?;
        Ok(true)
    })
    .unwrap_or(false)
}

#[pyfunction(name = "bp_set")]
fn pb_bp_set(addr: u64) -> Option<u32> {
    rpc(|c| c.bp_set(addr))
}

#[pyfunction(name = "bp_remove")]
fn pb_bp_remove(id: u32) -> bool {
    rpc(|c| c.bp_remove(id)).is_some()
}

/// Binds one exact native breakpoint to a callback owned by the current
/// plugin.  Unlike legacy bp_set/on_bp_hit, the callback is stored with this
/// id and may return stay/resume/step_into/step_over.
#[pyfunction(name = "breakpoint", signature = (address, callback, *, once=false, thread_id=None))]
fn pb_breakpoint(
    py: Python<'_>,
    address: u64,
    callback: Py<PyAny>,
    once: bool,
    thread_id: Option<u32>,
) -> PyResult<u32> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    if address == 0 {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "breakpoint address must be non-zero",
        ));
    }
    if !callback.bind(py).is_callable() {
        return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "breakpoint callback must be callable",
        ));
    }
    if thread_id == Some(PB_INVALID_THREAD_ID) {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "thread_id uses the reserved invalid-thread sentinel",
        ));
    }

    // Determine whether this script layer created the native breakpoint.
    // Existing CLI/legacy breakpoints remain externally owned and are not
    // removed when the Python subscription is released.
    let existed = rpc(|c| c.bp_list())
        .ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "failed to inspect native breakpoint table",
            )
        })?
        .4
        .iter()
        .any(|(_, existing_address, _)| *existing_address == address);
    let id = rpc(|c| c.bp_set(address)).ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("failed to create native breakpoint")
    })?;

    let previous = with_current_plugin_mut(|plugin| {
        plugin
            .breakpoints
            .insert(id, BreakpointSubscription::new(callback, once, thread_id))
    })
    .ok_or_else(no_plugin)?;
    if previous.is_none() {
        subscriptions::acquire_native(id, !existed);
    }
    Ok(id)
}

/// Removes only the current plugin's bound handler.  The native breakpoint
/// remains while another plugin or a legacy owner still uses it.
#[pyfunction(name = "breakpoint_remove")]
fn pb_breakpoint_remove(id: u32) -> PyResult<bool> {
    let removed =
        with_current_plugin_mut(|plugin| plugin.breakpoints.remove(&id)).ok_or_else(no_plugin)?;
    if removed.is_none() {
        return Ok(false);
    }
    if subscriptions::release_native(id) {
        if rpc(|c| c.bp_remove(id)).is_none() {
            subscriptions::queue_native_removal(id);
            output::push(
                &current_plugin_name().unwrap_or_else(|| "?".to_string()),
                &format!("breakpoint {id} handler removed; native removal failed"),
            );
            return Ok(false);
        }
    }
    Ok(true)
}

#[pyfunction(name = "hit")]
fn pb_hit() -> (Option<u32>, u64) {
    match rpc(|c| c.bp_list()) {
        Some((_, hit_tid, hit_addr, _, _)) => ((hit_tid != u32::MAX).then_some(hit_tid), hit_addr),
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
            .map(|(address, size, kind, text, _bytes, target)| (address, size, kind, target, text))
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

/// Installs a synchronous register action for an already armed hook point.
/// The action runs on the application thread before the hooked instruction:
/// set `set_reg` to `set_value` when the optional match register satisfies
/// `(value & match_mask) == (match_value & match_mask)`. `thread_id=None`
/// applies to every thread. `stack0`, `stack1`, ... select ABI-aware stack
/// arguments (x86 `[ESP+4]` onward; x64 `[RSP+0x28]` onward). This is a
/// native rule, not a Python callback, so it is safe to use on hot hooks and
/// the write reaches the live context.
#[pyfunction(
    name = "hook_rule",
    signature = (addr, set_reg, set_value, match_reg=None, match_mask=0, match_value=0, thread_id=None)
)]
fn pb_hook_rule(
    addr: u64,
    set_reg: &str,
    set_value: u64,
    match_reg: Option<&str>,
    match_mask: u64,
    match_value: u64,
    thread_id: Option<u32>,
) -> PyResult<bool> {
    let arch = crate::arch::wire_id();
    let Some(set_reg) = pinbridge_client::registers::reg_id(arch, set_reg) else {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "hook_rule: unknown set_reg for target architecture",
        ));
    };
    let match_reg = match match_reg {
        Some(name) => pinbridge_client::registers::reg_id(arch, name).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "hook_rule: unknown match_reg for target architecture",
            )
        })?,
        None => PB_REG_INVALID_,
    };
    Ok(crate::hooks::set_rule(crate::hooks::HookRule {
        address: addr,
        thread_id: thread_id.unwrap_or(PB_INVALID_THREAD_ID),
        match_reg: match_reg as PbRegId,
        match_mask,
        match_value,
        set_reg: set_reg as PbRegId,
        set_value,
    }))
}

/// Removes all synchronous hook action rules without disarming hook points.
#[pyfunction(name = "hook_rules_clear")]
fn pb_hook_rules_clear() {
    crate::hooks::clear_rules();
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
        "syscall" | "syscalls" => 5,
        "exception" | "exceptions" | "context_change" => 6,
        "registers" | "regs" | "context" | "reg_snapshot" => 13,
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
    let names = kinds.unwrap_or_else(|| vec!["exec".to_string(), "memory".to_string()]);
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

/// Structured recorder start: ranges are `(lo, hi)` pairs and `threads` is
/// an optional list of Pin thread ids. Empty threads means all threads.
#[pyfunction(name = "trace_start_spec", signature = (path, kinds=None, ranges=None, threads=None))]
fn pb_trace_start_spec(
    path: &str,
    kinds: Option<Vec<String>>,
    ranges: Option<Vec<(u64, u64)>>,
    threads: Option<Vec<u32>>,
) -> bool {
    let names = kinds.unwrap_or_else(|| vec!["exec".to_string(), "memory".to_string()]);
    let mut kind_ids = Vec::with_capacity(names.len());
    for name in &names {
        let Some(kind) = record_kind(name) else {
            return false;
        };
        kind_ids.push(kind);
    }
    let Some(ranges) = ranges else {
        return false;
    };
    rpc(|c| c.trace_start_spec(&kind_ids, &ranges, threads.as_deref().unwrap_or(&[]), path))
        .is_some()
}

/// Extends the active native trace with additional address ranges. Existing
/// kind/thread filters remain in force; range additions are marker-tagged in
/// the PBTR stream.
#[pyfunction(name = "trace_extend")]
fn pb_trace_extend(ranges: Vec<(u64, u64)>) -> bool {
    rpc(|c| c.trace_extend(&ranges)).is_some()
}

/// Returns `(base, size, allocation_base, protect, state, type)` for the
/// virtual region containing an address, or None when it is unmapped.
#[pyfunction(name = "memory_region")]
fn pb_memory_region(address: u64) -> Option<(u64, u64, u64, u32, u32, u32)> {
    rpc(|c| c.memory_region(address)).and_then(|region| {
        region.map(|r| {
            (
                r.base,
                r.size,
                r.allocation_base,
                r.protect,
                r.state,
                r.kind,
            )
        })
    })
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

/// Registers a named asynchronous event handler owned by the current
/// plugin.  Breakpoints use pb.breakpoint because they are synchronous stop
/// events with a return action; pb.on handles notification events.
#[pyfunction(name = "on", signature = (event, callback, *, once=false))]
fn pb_on_event(py: Python<'_>, event: &str, callback: Py<PyAny>, once: bool) -> PyResult<u64> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    if !callback.bind(py).is_callable() {
        return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "event callback must be callable",
        ));
    }
    if event.trim().eq_ignore_ascii_case("breakpoint") {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "breakpoint is a synchronous stop event; use pb.breakpoint(address, callback)",
        ));
    }
    let selector = EventSelector::parse(event).ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "unknown event {event:?}; use pb.event_names() to list supported names"
        ))
    })?;
    if selector.requires_smc_registration() && crate::high_priority::enable_smc() != PB_OK {
        return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            "Pin rejected SMC callback registration",
        ));
    }
    let (id, subscription) = EventSubscription::new(selector, callback, once);
    with_current_plugin_mut(|plugin| plugin.events.insert(id, subscription))
        .ok_or_else(no_plugin)?;
    mark_native_dirty();
    Ok(id)
}

/// Removes one named event handler from the current plugin.
#[pyfunction(name = "off")]
fn pb_off_event(subscription_id: u64) -> PyResult<bool> {
    let removed =
        with_current_plugin_mut(|plugin| plugin.events.remove(&subscription_id).is_some())
            .ok_or_else(no_plugin)?;
    if removed {
        mark_native_dirty();
    }
    Ok(removed)
}

/// Canonical names accepted by pb.on. Breakpoints use pb.breakpoint because
/// their handlers return a synchronous stop decision.
#[pyfunction(name = "event_names")]
fn pb_event_names() -> Vec<&'static str> {
    PUBLIC_EVENT_NAMES.to_vec()
}

/// Registers a return-valued synchronous interceptor. Unlike pb.on(), the
/// native event waits for a bounded time and consumes the callback result.
#[pyfunction(
    name = "intercept",
    signature = (event, callback, *, once=false, address=None, thread_id=None)
)]
fn pb_intercept(
    py: Python<'_>,
    event: &str,
    callback: Py<PyAny>,
    once: bool,
    address: Option<u64>,
    thread_id: Option<u32>,
) -> PyResult<u64> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    if !callback.bind(py).is_callable() {
        return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "interceptor callback must be callable",
        ));
    }
    let selector = DecisionSelector::parse(event).ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "unknown interceptor {event:?}; use pb.decision_names() to list supported names"
        ))
    })?;
    let created_hook = if selector.is_hook() {
        let address = address.filter(|address| *address != 0).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "hook interceptor requires a non-zero address",
            )
        })?;
        let existing = rpc(|client| client.hook_list()).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "hook interceptor could not query native Hook points",
            )
        })?;
        Some((address, !existing.contains(&address)))
    } else {
        if address.is_some() || thread_id.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "child.follow does not accept address or thread_id",
            ));
        }
        None
    };
    let (id, subscription) =
        DecisionSubscription::new(selector, callback, once, address, thread_id);
    with_current_plugin_mut(|plugin| plugin.decisions.insert(id, subscription))
        .ok_or_else(no_plugin)?;
    super::host::publish_decision_interests();
    if let Some((address, created_by_scripts)) = created_hook {
        if !rpc(|client| client.hook_set(address)).unwrap_or(false) {
            let _ = with_current_plugin_mut(|plugin| plugin.decisions.remove(&id));
            super::host::publish_decision_interests();
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "hook interceptor could not arm the native Hook point",
            ));
        }
        decisions::acquire_hook(address, created_by_scripts);
    }
    Ok(id)
}

#[pyfunction(name = "unintercept")]
fn pb_unintercept(id: u64) -> PyResult<bool> {
    let removed = with_current_plugin_mut(|plugin| plugin.decisions.remove(&id))
        .ok_or_else(no_plugin)?;
    let Some(subscription) = removed else {
        return Ok(false);
    };
    if let Some(address) = subscription.address {
        if decisions::release_hook(address) {
            decisions::queue_hook_removal(address);
        }
    }
    super::host::publish_decision_interests();
    Ok(true)
}

#[pyfunction(name = "decision_names")]
fn pb_decision_names() -> Vec<&'static str> {
    PUBLIC_DECISION_NAMES.to_vec()
}

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
fn pb_watch(kinds: Vec<String>, range: Option<(u64, u64)>, batch: Option<u64>) -> PyResult<()> {
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
    mark_native_dirty();
    Ok(())
}

/// Clears the watch subscription (dedicated callbacks keep flowing).
#[pyfunction(name = "unsubscribe")]
fn pb_unsubscribe() {
    with_current_plugin_mut(|p| p.filters.watch = None);
    mark_native_dirty();
}

/// Old name of watch (kept so existing examples keep working).
#[pyfunction(name = "subscribe", signature = (kinds, range=None, batch=None))]
fn pb_subscribe(kinds: Vec<String>, range: Option<(u64, u64)>, batch: Option<u64>) -> PyResult<()> {
    pb_watch(kinds, range, batch)
}

fn kind_bit(name: &str) -> Option<u32> {
    Some(match name {
        "hook" => (1 << 1) | (1 << crate::event::EVENT_HOOK_RETURN),
        "hook_regs" => 1 << 1,
        "hook_return" => 1 << crate::event::EVENT_HOOK_RETURN,
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
    m.add_function(wrap_pyfunction!(pb_breakpoint, m)?)?;
    m.add_function(wrap_pyfunction!(pb_breakpoint_remove, m)?)?;
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
    m.add_function(wrap_pyfunction!(pb_hook_rule, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_rules_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_remove, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_exc_policy, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_start, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_start_spec, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_extend, m)?)?;
    m.add_function(wrap_pyfunction!(pb_memory_region, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_stop, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_status, m)?)?;
    m.add_function(wrap_pyfunction!(pb_on_event, m)?)?;
    m.add_function(wrap_pyfunction!(pb_off_event, m)?)?;
    m.add_function(wrap_pyfunction!(pb_event_names, m)?)?;
    m.add_function(wrap_pyfunction!(pb_intercept, m)?)?;
    m.add_function(wrap_pyfunction!(pb_unintercept, m)?)?;
    m.add_function(wrap_pyfunction!(pb_decision_names, m)?)?;
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
