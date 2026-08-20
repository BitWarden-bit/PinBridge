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
use super::{
    current_plugin_display_name, current_plugin_name, with_current_plugin_mut, Watch, RPC_PORT,
};
use core::sync::atomic::Ordering;
use pinbridge_client::client::Client;
use pinbridge_proto::{ARCH_X64, HOOK_LOG_FLAG_FUNCTION, HOOK_LOG_FLAG_SIGNATURE};
use pinbridge_sys::{pb_pin_sleep, PbRegId, PB_INVALID_THREAD_ID, PB_OK, PB_REG_INVALID_};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

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
    let plugin = current_plugin_display_name().unwrap_or_else(|| "?".to_string());
    output::push(&plugin, msg);
    crate::log::line(&format!("[py:{plugin}] {msg}"));
}

/// Alias of print (old scripts used pb.log).
#[pyfunction(name = "log")]
fn pb_log(msg: &str) {
    let plugin = current_plugin_display_name().unwrap_or_else(|| "?".to_string());
    output::push(&plugin, msg);
    crate::log::line(&format!("[py:{plugin}] {msg}"));
}

// ---- target actions (one loopback RPC each) ----

#[pyfunction(name = "read_mem")]
fn pb_read_mem(py: Python<'_>, addr: u64, len: u64) -> Option<Py<PyBytes>> {
    if len == 0 || len > (1 << 20) {
        return None;
    }
    let data = crate::control::read_memory(addr, len);
    Some(PyBytes::new_bound(py, &data).unbind())
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
#[pyfunction(name = "breakpoint", signature = (address, callback, *, description, once=false, thread_id=None))]
fn pb_breakpoint(
    py: Python<'_>,
    address: u64,
    callback: Py<PyAny>,
    description: String,
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
    let description = description.trim();
    if description.is_empty() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "breakpoint description must not be empty",
        ));
    }
    if description.len() > 512 {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "breakpoint description exceeds 512 bytes",
        ));
    }
    if description.chars().any(char::is_control) {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "breakpoint description must be one printable line",
        ));
    }
    if thread_id == Some(PB_INVALID_THREAD_ID) {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "thread_id uses the reserved invalid-thread sentinel",
        ));
    }

    let callback_name = callback
        .bind(py)
        .getattr("__qualname__")
        .or_else(|_| callback.bind(py).getattr("__name__"))
        .and_then(|value| value.extract::<String>())
        .unwrap_or_else(|_| "<callable>".to_string());

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
    let id = subscriptions::with_script_native_set(address, || rpc(|c| c.bp_set(address)))
        .ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("failed to create native breakpoint")
        })?;

    let previous = with_current_plugin_mut(|plugin| {
        plugin.breakpoints.insert(
            id,
            BreakpointSubscription::new(
                callback,
                callback_name,
                description.to_string(),
                once,
                thread_id,
            ),
        )
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
                &current_plugin_display_name().unwrap_or_else(|| "?".to_string()),
                &format!("breakpoint {id} handler removed; native removal failed"),
            );
            return Ok(false);
        }
    }
    Ok(true)
}

/// Arms a generic half-open execution range. The native engine stops before
/// the first matching instruction and publishes `execution.trap` only after
/// all application contexts are stable. Protector/OEP policy belongs in the
/// Python plugin; this primitive has no unpacking semantics.
#[pyfunction(
    name = "execution_trap",
    signature = (start, end, *, once=true, thread_id=None)
)]
fn pb_execution_trap(start: u64, end: u64, once: bool, thread_id: Option<u32>) -> PyResult<u32> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    if start == 0 || start >= end {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "execution trap must be a non-empty half-open range",
        ));
    }
    if thread_id == Some(PB_INVALID_THREAD_ID) {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "thread_id uses the reserved invalid-thread sentinel",
        ));
    }
    let id = crate::execution_trap::set(start, end, once, thread_id).map_err(|status| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
            "failed to arm native execution trap: status {status}"
        ))
    })?;
    with_current_plugin_mut(|plugin| plugin.execution_traps.insert(id)).ok_or_else(no_plugin)?;
    Ok(id)
}

/// Removes only an execution trap owned by the current plugin.
#[pyfunction(name = "execution_trap_remove")]
fn pb_execution_trap_remove(id: u32) -> PyResult<bool> {
    let owned = with_current_plugin_mut(|plugin| plugin.execution_traps.remove(&id))
        .ok_or_else(no_plugin)?;
    if !owned {
        return Ok(false);
    }
    Ok(crate::execution_trap::remove(id))
}

/// Lists active native execution traps as
/// `(id,start,end,thread_id|None,once,hits)` tuples.
#[pyfunction(name = "execution_traps")]
fn pb_execution_traps() -> Vec<(u32, u64, u64, Option<u32>, bool, u64)> {
    crate::execution_trap::list()
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

#[pyfunction(name = "pin_state")]
fn pb_pin_state() -> (String, i32) {
    (
        crate::pin_session::state_name().to_string(),
        crate::pin_session::last_registration_status(),
    )
}

#[pyfunction(name = "pin_attach_supported")]
fn pb_pin_attach_supported() -> bool {
    crate::pin_session::attach_supported()
}

#[pyfunction(name = "pin_detach")]
fn pb_pin_detach() -> PyResult<bool> {
    let status = crate::pin_session::request_detach();
    if status == PB_OK {
        Ok(true)
    } else {
        Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
            "Pin detach request failed with status {status}; state={}",
            crate::pin_session::state_name()
        )))
    }
}

#[pyfunction(name = "pin_attach")]
fn pb_pin_attach() -> PyResult<bool> {
    match crate::pin_session::request_attach() {
        Ok(status) => Ok(status == pinbridge_sys::PB_ATTACH_INITIATED),
        Err(status) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
            "Pin attach request failed with status {status}; state={}",
            crate::pin_session::state_name()
        ))),
    }
}

#[pyfunction(name = "resolve")]
fn pb_resolve(addr: u64) -> Option<String> {
    rpc(|c| c.resolve(&[addr]))?.pop()?.display()
}

#[pyfunction(name = "resolve_name")]
fn pb_resolve_name(spec: &str) -> Option<u64> {
    crate::resolve::resolve_name(spec).filter(|addr| *addr != 0)
}

#[pyfunction(name = "disasm")]
fn pb_disasm(addr: u64, count: u64) -> Option<Vec<(u64, u32, u32, u64, String)>> {
    let count = count.clamp(1, 128);
    let rows = crate::disasm::disassemble_local(addr, count).ok()?;
    Some(
        rows.into_iter()
            .map(|row| (row.address, row.size, row.kind, row.target, row.text))
            .collect(),
    )
}

#[pyfunction(name = "modules")]
fn pb_modules() -> Vec<(u64, u64, bool, String)> {
    crate::control::modules()
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

/// Control-plane topology for parent/child Pin sessions. A root session has
/// no parent; followed children receive both ports before their PIN_Init.
#[pyfunction(name = "control_port")]
fn pb_control_port() -> u16 {
    RPC_PORT.load(core::sync::atomic::Ordering::Acquire)
}

#[pyfunction(name = "parent_control_port")]
fn pb_parent_control_port() -> Option<u16> {
    crate::child_process::parent_control_port()
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

/// Reads the dedicated native Hook lane without involving the generic event
/// ring. The returned dictionary uses Python integers and contains an events
/// list ordered as requested. This query is intended for script mainlines and
/// asynchronous observers; a synchronous intercept callback must use its
/// supplied event instead of starting a nested control-plane RPC.
#[pyfunction(
    name = "hook_events_query",
    signature = (*, limit=1024, before=0, after=0, order="desc", hook_types=None, phases=None, modules=None, symbols=None, thread_ids=None, addresses=None)
)]
fn pb_hook_events_query(
    py: Python<'_>,
    limit: usize,
    before: u64,
    after: u64,
    order: &str,
    hook_types: Option<Vec<String>>,
    phases: Option<Vec<String>>,
    modules: Option<Vec<String>>,
    symbols: Option<Vec<String>>,
    thread_ids: Option<Vec<u32>>,
    addresses: Option<Vec<u64>>,
) -> PyResult<PyObject> {
    if limit == 0 || limit > 4096 {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "hook_events_query limit must be 1..4096",
        ));
    }
    if !matches!(order, "asc" | "desc") {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "hook_events_query order must be 'asc' or 'desc'",
        ));
    }
    validate_python_filter("hook_types", hook_types.as_deref(), &["api", "instruction"])?;
    validate_python_filter("phases", phases.as_deref(), &["hit", "entry", "return"])?;

    let (total, dropped, next, mut records, resolutions) = rpc(|client| {
        let (total, dropped, next, records) = client.hook_events_window(4096, before)?;
        let mut unique = records.iter().map(|record| record.address).collect::<Vec<_>>();
        unique.sort_unstable();
        unique.dedup();
        let resolved = client.resolve(&unique)?;
        let resolutions = unique
            .into_iter()
            .zip(resolved)
            .map(|(address, resolved)| {
                let display = resolved.display();
                (
                    address,
                    resolved.module,
                    resolved.symbol,
                    display,
                )
            })
            .collect::<Vec<_>>();
        Ok((total, dropped, next, records, resolutions))
    })
    .ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            "hook_events_query is unavailable during a synchronous callback or disconnected session",
        )
    })?;

    records.sort_by_key(|record| record.sequence);
    if order == "desc" {
        records.reverse();
    }
    let output = PyList::empty_bound(py);
    for record in records {
        if record.sequence <= after {
            continue;
        }
        let function_hook = record.flags & HOOK_LOG_FLAG_FUNCTION != 0;
        let hook_type = if function_hook { "api" } else { "instruction" };
        let phase = if !function_hook {
            "hit"
        } else if record.kind == crate::event::EVENT_HOOK_RETURN {
            "return"
        } else {
            "entry"
        };
        if !python_filter_matches(hook_types.as_deref(), hook_type)
            || !python_filter_matches(phases.as_deref(), phase)
            || thread_ids
                .as_ref()
                .is_some_and(|values| !values.contains(&record.thread_id))
            || addresses
                .as_ref()
                .is_some_and(|values| !values.contains(&record.address))
        {
            continue;
        }
        let resolution = resolutions
            .iter()
            .find(|(address, _, _, _)| *address == record.address);
        let module = resolution
            .map(|(_, module, _, _)| module.as_str())
            .filter(|value| !value.is_empty());
        let symbol = resolution
            .map(|(_, _, symbol, _)| symbol.as_str())
            .filter(|value| !value.is_empty());
        if !python_patterns_match(modules.as_deref(), module)
            || !python_patterns_match(symbols.as_deref(), symbol)
        {
            continue;
        }
        let event = PyDict::new_bound(py);
        event.set_item("sequence", record.sequence)?;
        event.set_item("timestamp_unix_ns", record.timestamp_unix_ns)?;
        event.set_item("kind", phase)?;
        event.set_item("hook_type", hook_type)?;
        event.set_item("thread_id", record.thread_id)?;
        event.set_item("address", record.address)?;
        event.set_item("module", module)?;
        event.set_item("symbol", symbol)?;
        event.set_item(
            "display",
            resolution.and_then(|(_, _, _, display)| display.as_deref()),
        )?;
        event.set_item(
            "signature_capture",
            record.flags & HOOK_LOG_FLAG_SIGNATURE != 0,
        )?;
        let argument_count = record.argument_count.min(16) as usize;
        event.set_item("arguments", record.arguments[..argument_count].to_vec())?;
        event.set_item(
            "return_value",
            (function_hook && phase == "return").then_some(record.arguments[0]),
        )?;
        output.append(event)?;
        if output.len() >= limit {
            break;
        }
    }
    let result = PyDict::new_bound(py);
    result.set_item("lane_total", total)?;
    result.set_item("lane_dropped", dropped)?;
    result.set_item("history_overwritten", total.saturating_sub(32768))?;
    result.set_item("next_cursor", next)?;
    result.set_item("window_before", before)?;
    result.set_item("returned", output.len())?;
    result.set_item("events", output)?;
    Ok(result.into_py(py))
}

fn validate_python_filter(name: &str, values: Option<&[String]>, allowed: &[&str]) -> PyResult<()> {
    if let Some(values) = values {
        for value in values {
            if !allowed.contains(&value.as_str()) {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "unsupported {name} value: {value}"
                )));
            }
        }
    }
    Ok(())
}

fn python_filter_matches(values: Option<&[String]>, value: &str) -> bool {
    values
        .map(|values| values.iter().any(|candidate| candidate == value))
        .unwrap_or(true)
}

fn python_patterns_match(patterns: Option<&[String]>, value: Option<&str>) -> bool {
    patterns
        .map(|patterns| {
            patterns.is_empty()
                || value.is_some_and(|value| {
                    patterns
                        .iter()
                        .any(|pattern| python_wildcard_match(pattern, value))
                })
        })
        .unwrap_or(true)
}

fn python_wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase().into_bytes();
    let value = value.to_ascii_lowercase().into_bytes();
    let (mut p, mut v, mut star, mut retry) = (0usize, 0usize, None, 0usize);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = v;
        } else if let Some(star_at) = star {
            p = star_at + 1;
            retry += 1;
            v = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
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
/// bounded and a wide range can overflow) into a .pbtr file. False when a
/// session is already recording/draining or the arguments are rejected.
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
    crate::control::memory_region(address).map(|region| {
        (
            region.base,
            region.size,
            region.allocation_base,
            region.protect,
            region.state,
            region.kind,
        )
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

/// `(state, active, recorded, dropped)` where state is one of idle,
/// recording, draining, complete, or failed.
#[pyfunction(name = "trace_status_detail")]
fn pb_trace_status_detail() -> Option<(String, bool, u64, u64)> {
    rpc(|c| c.trace_status_detail()).map(|status| {
        (
            status.state_name().to_string(),
            status.active,
            status.recorded,
            status.dropped,
        )
    })
}

// ---- registrations (mutate the current plugin's filters) ----

fn parse_syscall_numbers(numbers: Option<Vec<u64>>) -> PyResult<Option<crate::TlsFreeSet<u32>>> {
    numbers
        .map(|numbers| {
            let mut filter = crate::new_set();
            for number in numbers {
                if number >= crate::syscall_engine::SYSCALL_NUMBER_LIMIT as u64 {
                    return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        "syscall number must be in the native 0..0xfff range",
                    ));
                }
                filter.insert(number as u32);
            }
            Ok(filter)
        })
        .transpose()
}

/// Registers a named asynchronous event handler owned by the current
/// plugin.  Breakpoints use pb.breakpoint because they are synchronous stop
/// events with a return action; pb.on handles notification events.
#[pyfunction(name = "on", signature = (event, callback, *, once=false, address=None, numbers=None))]
fn pb_on_event(
    py: Python<'_>,
    event: &str,
    callback: Py<PyAny>,
    once: bool,
    address: Option<u64>,
    numbers: Option<Vec<u64>>,
) -> PyResult<u64> {
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
            "breakpoint is a synchronous stop event; use pb.breakpoint(address, callback, description=...)",
        ));
    }
    let selector = EventSelector::parse(event).ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "unknown event {event:?}; use pb.event_names() to list supported names"
        ))
    })?;
    let is_hook = matches!(
        selector,
        EventSelector::Kind(crate::event::EVENT_HOOK_REGS)
            | EventSelector::Kind(crate::event::EVENT_HOOK_RETURN)
    );
    if address.is_some() && !is_hook {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "address is valid only for hook.entry and hook.return",
        ));
    }
    if is_hook && address == Some(0) {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "Hook observer address must be non-zero",
        ));
    }
    if numbers.is_some() && selector != EventSelector::Kind(crate::event::EVENT_SYSCALL) {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "numbers is valid only for the syscall event",
        ));
    }
    let syscall_numbers = parse_syscall_numbers(numbers)?;
    let hook_lease = if let Some(address) = address {
        let existing = rpc(|client| client.hook_list()).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Hook observer could not query native Hook points",
            )
        })?;
        Some((address, !existing.contains(&address)))
    } else {
        None
    };
    if selector.requires_smc_registration() && crate::high_priority::enable_smc() != PB_OK {
        return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            "Pin rejected SMC callback registration",
        ));
    }
    if is_hook {
        // Existing native Hook points may already be executing, so publish
        // observation interest before installing the Python subscription.
        crate::hooks::set_observation_enabled(true);
    }
    let (id, subscription) =
        EventSubscription::new(selector, callback, once, address, syscall_numbers);
    with_current_plugin_mut(|plugin| plugin.events.insert(id, subscription))
        .ok_or_else(no_plugin)?;
    if let Some((address, created_by_scripts)) = hook_lease {
        if !rpc(|client| client.hook_set(address)).unwrap_or(false) {
            let _ = with_current_plugin_mut(|plugin| plugin.events.remove(&id));
            mark_native_dirty();
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Hook observer could not arm the native Hook point",
            ));
        }
        decisions::acquire_hook(address, created_by_scripts);
    }
    mark_native_dirty();
    Ok(id)
}

/// Removes one named event handler from the current plugin.
#[pyfunction(name = "off")]
fn pb_off_event(subscription_id: u64) -> PyResult<bool> {
    let removed = with_current_plugin_mut(|plugin| plugin.events.remove(&subscription_id))
        .ok_or_else(no_plugin)?;
    if let Some(subscription) = &removed {
        if let Some(address) = subscription.hook_address {
            if decisions::release_hook(address) {
                decisions::queue_hook_removal(address);
            }
        }
        mark_native_dirty();
    }
    Ok(removed.is_some())
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
    signature = (event, callback, *, description="", once=false, address=None, thread_id=None, numbers=None, codes=None)
)]
fn pb_intercept(
    py: Python<'_>,
    event: &str,
    callback: Py<PyAny>,
    description: &str,
    once: bool,
    address: Option<u64>,
    thread_id: Option<u32>,
    numbers: Option<Vec<u64>>,
    codes: Option<Vec<u64>>,
) -> PyResult<u64> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    if !callback.bind(py).is_callable() {
        return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "interceptor callback must be callable",
        ));
    }
    let description = description.trim();
    if description.len() > 512 {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "interceptor description exceeds 512 bytes",
        ));
    }
    let selector = DecisionSelector::parse(event).ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "unknown interceptor {event:?}; use pb.decision_names() to list supported names"
        ))
    })?;
    if selector.is_hook() && description.is_empty() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "Hook interceptor requires a non-empty description",
        ));
    }
    let callback_name = callback
        .bind(py)
        .getattr("__qualname__")
        .or_else(|_| callback.bind(py).getattr("__name__"))
        .and_then(|value| value.extract::<String>())
        .unwrap_or_else(|_| "<callable>".to_string());
    let number_filter = numbers
        .map(|numbers| -> PyResult<_> {
            let mut filter = crate::new_set();
            for number in numbers {
                let number = u32::try_from(number).map_err(|_| {
                    PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        "syscall number must fit in 32 bits",
                    )
                })?;
                filter.insert(number);
            }
            Ok(filter)
        })
        .transpose()?;
    let code_filter = codes
        .map(|codes| -> PyResult<_> {
            let mut filter = crate::new_set();
            for code in codes {
                let code = u32::try_from(code).map_err(|_| {
                    PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        "exception code must fit in 32 bits",
                    )
                })?;
                filter.insert(code);
            }
            Ok(filter)
        })
        .transpose()?;
    let created_hook = if selector.is_hook() {
        if number_filter.is_some() || code_filter.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Hook interceptors do not accept numbers or codes",
            ));
        }
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
    } else if selector == DecisionSelector::ChildFollow {
        if address.is_some()
            || thread_id.is_some()
            || number_filter.is_some()
            || code_filter.is_some()
        {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "child.follow does not accept address, thread_id, numbers, or codes",
            ));
        }
        None
    } else if matches!(
        selector,
        DecisionSelector::SyscallEntry | DecisionSelector::SyscallExit
    ) {
        if address.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "syscall interceptors do not accept address",
            ));
        }
        if code_filter.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "syscall interceptors do not accept codes",
            ));
        }
        None
    } else if selector.is_debugger() {
        if address.is_some() || number_filter.is_some() || code_filter.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "debugger interceptors accept only the optional thread_id filter",
            ));
        }
        None
    } else {
        if address.is_some() || number_filter.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "exception.handle does not accept address or numbers",
            ));
        }
        None
    };
    let (id, subscription) = DecisionSubscription::new(
        selector,
        callback,
        callback_name,
        description.to_string(),
        once,
        address,
        thread_id,
        number_filter,
        code_filter,
    );
    with_current_plugin_mut(|plugin| plugin.decisions.insert(id, subscription))
        .ok_or_else(no_plugin)?;
    super::interceptors::publish_interests();
    if let Some((address, created_by_scripts)) = created_hook {
        if !rpc(|client| client.hook_set(address)).unwrap_or(false) {
            let _ = with_current_plugin_mut(|plugin| plugin.decisions.remove(&id));
            super::interceptors::publish_interests();
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
    let removed =
        with_current_plugin_mut(|plugin| plugin.decisions.remove(&id)).ok_or_else(no_plugin)?;
    let Some(subscription) = removed else {
        return Ok(false);
    };
    if let Some(address) = subscription.address {
        if decisions::release_hook(address) {
            decisions::queue_hook_removal(address);
        }
    }
    super::interceptors::publish_interests();
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
    let numbers = parse_syscall_numbers(numbers)?;
    with_current_plugin_mut(|p| {
        p.filters.syscall_numbers = numbers;
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

fn instrumentation_kind(name: &str) -> Option<u32> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "instruction" | "instruction.exec" | "exec" => crate::engines::INSTRUMENT_EXEC,
        "memory" | "mem" => crate::engines::INSTRUMENT_MEMORY,
        "branch" | "branch.edge" | "branch_edge" => crate::engines::INSTRUMENT_BRANCH,
        "instruction.decode" | "instruction_decode" | "decode" => crate::engines::INSTRUMENT_DECODE,
        "trace.instrument" | "trace_instrument" | "trace" => crate::engines::INSTRUMENT_TRACE,
        "routine.instrument" | "routine_instrument" | "routine" | "function" => {
            crate::engines::INSTRUMENT_ROUTINE
        }
        "basic_block.instrument" | "basic_block_instrument" | "bbl.instrument" | "bbl" => {
            crate::engines::INSTRUMENT_BBL
        }
        _ => return None,
    })
}

/// Compiles this plugin's high-frequency capture rules into the native Pin
/// instrumentation path. Python never runs at instrumentation or analysis
/// time; only immutable range/thread/kind filters are read there.
#[pyfunction(
    name = "instrumentation_set",
    signature = (*, kinds=None, ranges=None, threads=None)
)]
fn pb_instrumentation_set(
    kinds: Option<Vec<String>>,
    ranges: Option<Vec<(u64, u64)>>,
    threads: Option<Vec<u32>>,
) -> PyResult<u64> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    let mut kind_mask = 0;
    for name in kinds.unwrap_or_else(|| {
        vec![
            "instruction".to_string(),
            "memory".to_string(),
            "branch.edge".to_string(),
        ]
    }) {
        kind_mask |= instrumentation_kind(&name).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "unknown instrumentation kind {name:?}; expected instruction, instruction.decode, memory, branch.edge, trace.instrument, routine.instrument, or basic_block.instrument"
            ))
        })?;
    }
    if kind_mask == 0 {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "instrumentation_set needs at least one kind; use instrumentation_clear to disable",
        ));
    }

    let mut ranges = ranges.unwrap_or_else(|| vec![crate::engines::trace_range()]);
    if ranges.is_empty() || ranges.len() > crate::engines::MAX_INSTRUMENTATION_RANGES {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "instrumentation ranges must contain 1..={} entries",
            crate::engines::MAX_INSTRUMENTATION_RANGES
        )));
    }
    if ranges.iter().any(|(start, end)| start >= end) {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "every instrumentation range must satisfy start < end",
        ));
    }
    ranges.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }

    let mut threads = threads.unwrap_or_default();
    if threads.len() > crate::engines::MAX_INSTRUMENTATION_THREADS
        || threads.contains(&PB_INVALID_THREAD_ID)
    {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "instrumentation threads must contain at most {} valid thread ids",
            crate::engines::MAX_INSTRUMENTATION_THREADS
        )));
    }
    threads.sort_unstable();
    threads.dedup();
    let spec = super::instrumentation::Spec {
        kinds: kind_mask,
        ranges: merged,
        threads,
    };
    let previous = with_current_plugin_mut(|plugin| plugin.instrumentation.replace(spec))
        .ok_or_else(no_plugin)?;
    match super::instrumentation::publish() {
        Ok(generation) => Ok(generation),
        Err(status) => {
            with_current_plugin_mut(|plugin| plugin.instrumentation = previous);
            super::native_policies::refresh_best_effort("rollback failed configuration");
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "native instrumentation update failed with status {status}"
            )))
        }
    }
}

/// Removes this plugin's capture policy without affecting other plugins.
#[pyfunction(name = "instrumentation_clear")]
fn pb_instrumentation_clear() -> PyResult<bool> {
    let previous =
        with_current_plugin_mut(|plugin| plugin.instrumentation.take()).ok_or_else(no_plugin)?;
    if previous.is_none() {
        return Ok(false);
    }
    match super::instrumentation::publish() {
        Ok(_) => Ok(true),
        Err(status) => {
            with_current_plugin_mut(|plugin| plugin.instrumentation = previous);
            super::native_policies::refresh_best_effort("rollback failed clear");
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "native instrumentation clear failed with status {status}"
            )))
        }
    }
}

/// Returns this plugin's declarative policy, not another plugin's rules.
#[pyfunction(name = "instrumentation_policy")]
fn pb_instrumentation_policy() -> PyResult<Option<(Vec<String>, Vec<(u64, u64)>, Vec<u32>)>> {
    with_current_plugin_mut(|plugin| {
        plugin.instrumentation.as_ref().map(|spec| {
            let mut kinds = Vec::new();
            if spec.kinds & crate::engines::INSTRUMENT_EXEC != 0 {
                kinds.push("instruction".to_string());
            }
            if spec.kinds & crate::engines::INSTRUMENT_MEMORY != 0 {
                kinds.push("memory".to_string());
            }
            if spec.kinds & crate::engines::INSTRUMENT_BRANCH != 0 {
                kinds.push("branch.edge".to_string());
            }
            if spec.kinds & crate::engines::INSTRUMENT_DECODE != 0 {
                kinds.push("instruction.decode".to_string());
            }
            if spec.kinds & crate::engines::INSTRUMENT_TRACE != 0 {
                kinds.push("trace.instrument".to_string());
            }
            if spec.kinds & crate::engines::INSTRUMENT_ROUTINE != 0 {
                kinds.push("routine.instrument".to_string());
            }
            if spec.kinds & crate::engines::INSTRUMENT_BBL != 0 {
                kinds.push("basic_block.instrument".to_string());
            }
            (kinds, spec.ranges.clone(), spec.threads.clone())
        })
    })
    .ok_or_else(no_plugin)
}

fn translation_operation(name: &str) -> Option<u32> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "load" | "read" => super::memory_translation::OP_LOAD,
        "store" | "write" => super::memory_translation::OP_STORE,
        _ => return None,
    })
}

/// Replaces this plugin's address mappings. A mapping tuple is
/// (source_start, source_end, target_start); the offset inside the source
/// range is preserved at the target.
#[pyfunction(
    name = "memory_translation_set",
    signature = (mappings, *, threads=None, instruction_ranges=None, operations=None, include_pin=false)
)]
fn pb_memory_translation_set(
    mappings: Vec<(u64, u64, u64)>,
    threads: Option<Vec<u32>>,
    instruction_ranges: Option<Vec<(u64, u64)>>,
    operations: Option<Vec<String>>,
    include_pin: bool,
) -> PyResult<u64> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    if mappings.is_empty() || mappings.len() > super::memory_translation::MAX_MAPPINGS {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "memory mappings must contain 1..={} entries",
            super::memory_translation::MAX_MAPPINGS
        )));
    }
    let mut mappings = mappings;
    mappings.sort_unstable_by_key(|mapping| mapping.0);
    let mut native_mappings = Vec::with_capacity(mappings.len());
    let mut previous_end = 0;
    for (index, (source_start, source_end, target_start)) in mappings.into_iter().enumerate() {
        if source_start >= source_end
            || target_start
                .checked_add(source_end - source_start)
                .is_none()
            || (index != 0 && source_start < previous_end)
        {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "memory mappings must be non-overlapping valid ranges and target_start + length must not overflow",
            ));
        }
        previous_end = source_end;
        native_mappings.push(super::memory_translation::Mapping {
            source_start,
            source_end,
            target_start,
        });
    }

    let mut threads = threads.unwrap_or_default();
    if threads.len() > super::memory_translation::MAX_THREADS
        || threads.contains(&PB_INVALID_THREAD_ID)
    {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "memory translation threads must contain at most {} valid thread ids",
            super::memory_translation::MAX_THREADS
        )));
    }
    threads.sort_unstable();
    threads.dedup();

    let mut instruction_ranges = instruction_ranges.unwrap_or_default();
    if instruction_ranges.len() > super::memory_translation::MAX_INSTRUCTION_RANGES
        || instruction_ranges.iter().any(|(start, end)| start >= end)
    {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "instruction_ranges must contain at most {} valid half-open ranges",
            super::memory_translation::MAX_INSTRUCTION_RANGES
        )));
    }
    instruction_ranges.sort_unstable();
    let mut merged_instruction_ranges: Vec<(u64, u64)> = Vec::new();
    for (start, end) in instruction_ranges {
        if let Some(last) = merged_instruction_ranges.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged_instruction_ranges.push((start, end));
    }

    let mut operation_mask = 0;
    for operation in operations.unwrap_or_else(|| vec!["load".to_string(), "store".to_string()]) {
        operation_mask |= translation_operation(&operation).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "unknown memory operation {operation:?}; expected load or store"
            ))
        })?;
    }
    if operation_mask == 0 {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "memory_translation_set needs at least one operation",
        ));
    }

    let spec = super::memory_translation::Spec {
        mappings: native_mappings,
        threads,
        instruction_ranges: merged_instruction_ranges,
        operations: operation_mask,
        include_pin,
    };
    let previous = with_current_plugin_mut(|plugin| plugin.memory_translation.replace(spec))
        .ok_or_else(no_plugin)?;
    match super::memory_translation::publish() {
        Ok(generation) => Ok(generation),
        Err(status) => {
            with_current_plugin_mut(|plugin| plugin.memory_translation = previous);
            super::native_policies::refresh_best_effort("rollback failed memory translation");
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "native memory translation update failed with status {status}; active plugin mappings must not overlap"
            )))
        }
    }
}

#[pyfunction(name = "memory_translation_clear")]
fn pb_memory_translation_clear() -> PyResult<bool> {
    let previous =
        with_current_plugin_mut(|plugin| plugin.memory_translation.take()).ok_or_else(no_plugin)?;
    if previous.is_none() {
        return Ok(false);
    }
    match super::memory_translation::publish() {
        Ok(_) => Ok(true),
        Err(status) => {
            with_current_plugin_mut(|plugin| plugin.memory_translation = previous);
            super::native_policies::refresh_best_effort("rollback failed translation clear");
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "native memory translation clear failed with status {status}"
            )))
        }
    }
}

type MemoryTranslationPolicy = (
    Vec<(u64, u64, u64)>,
    Vec<u32>,
    Vec<(u64, u64)>,
    Vec<String>,
    bool,
);

#[pyfunction(name = "memory_translation_policy")]
fn pb_memory_translation_policy() -> PyResult<Option<MemoryTranslationPolicy>> {
    with_current_plugin_mut(|plugin| {
        plugin.memory_translation.as_ref().map(|spec| {
            let mappings = spec
                .mappings
                .iter()
                .map(|mapping| {
                    (
                        mapping.source_start,
                        mapping.source_end,
                        mapping.target_start,
                    )
                })
                .collect();
            let mut operations = Vec::new();
            if spec.operations & super::memory_translation::OP_LOAD != 0 {
                operations.push("load".to_string());
            }
            if spec.operations & super::memory_translation::OP_STORE != 0 {
                operations.push("store".to_string());
            }
            (
                mappings,
                spec.threads.clone(),
                spec.instruction_ranges.clone(),
                operations,
                spec.include_pin,
            )
        })
    })
    .ok_or_else(no_plugin)
}

/// Replaces this plugin's native instruction-byte overlays. Each tuple is
/// (virtual_address, bytes). Active plugins may not own overlapping ranges.
#[pyfunction(name = "code_fetch_set")]
fn pb_code_fetch_set(segments: Vec<(u64, Vec<u8>)>) -> PyResult<u64> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    if segments.is_empty() || segments.len() > super::code_fetch::MAX_SEGMENTS {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "code fetch segments must contain 1..={} entries",
            super::code_fetch::MAX_SEGMENTS
        )));
    }
    let total_bytes = segments
        .iter()
        .try_fold(0usize, |total, (_, bytes)| total.checked_add(bytes.len()))
        .ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>("code fetch byte count overflow")
        })?;
    if total_bytes == 0 || total_bytes > super::code_fetch::MAX_TOTAL_BYTES {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "code fetch policy must contain 1..={} bytes",
            super::code_fetch::MAX_TOTAL_BYTES
        )));
    }

    let mut segments = segments;
    segments.sort_unstable_by_key(|segment| segment.0);
    let mut native_segments = Vec::with_capacity(segments.len());
    let mut previous_end = 0u64;
    for (index, (start, bytes)) in segments.into_iter().enumerate() {
        let end = start.checked_add(bytes.len() as u64).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "code fetch segment end address overflow",
            )
        })?;
        if bytes.is_empty() || (index != 0 && start < previous_end) {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "code fetch segments must be non-empty and non-overlapping",
            ));
        }
        previous_end = end;
        native_segments.push(super::code_fetch::Segment { start, bytes });
    }

    let spec = super::code_fetch::Spec {
        segments: native_segments,
    };
    let previous =
        with_current_plugin_mut(|plugin| plugin.code_fetch.replace(spec)).ok_or_else(no_plugin)?;
    match super::code_fetch::publish() {
        Ok(generation) => Ok(generation),
        Err(status) => {
            with_current_plugin_mut(|plugin| plugin.code_fetch = previous);
            super::native_policies::refresh_best_effort("rollback failed code fetch update");
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "native code fetch update failed with status {status}; active plugin segments must not overlap"
            )))
        }
    }
}

#[pyfunction(name = "code_fetch_clear")]
fn pb_code_fetch_clear() -> PyResult<bool> {
    let previous =
        with_current_plugin_mut(|plugin| plugin.code_fetch.take()).ok_or_else(no_plugin)?;
    if previous.is_none() {
        return Ok(false);
    }
    match super::code_fetch::publish() {
        Ok(_) => Ok(true),
        Err(status) => {
            with_current_plugin_mut(|plugin| plugin.code_fetch = previous);
            super::native_policies::refresh_best_effort("rollback failed code fetch clear");
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "native code fetch clear failed with status {status}"
            )))
        }
    }
}

#[pyfunction(name = "code_fetch_policy")]
fn pb_code_fetch_policy(py: Python<'_>) -> PyResult<Option<Vec<(u64, Py<PyBytes>)>>> {
    with_current_plugin_mut(|plugin| {
        plugin.code_fetch.as_ref().map(|spec| {
            spec.segments
                .iter()
                .map(|segment| {
                    (
                        segment.start,
                        PyBytes::new_bound(py, &segment.bytes).unbind(),
                    )
                })
                .collect()
        })
    })
    .ok_or_else(no_plugin)
}

/// Configures inputs consumed by Pin's pre-XED-decode callback. These are
/// global decoding semantics, so active plugins must agree on every feature
/// they explicitly select. None leaves a feature unspecified by this plugin.
#[pyfunction(
    name = "xed_decode_set",
    signature = (*, cet=None, cldemote=None, mpx=None)
)]
fn pb_xed_decode_set(
    cet: Option<bool>,
    cldemote: Option<bool>,
    mpx: Option<bool>,
) -> PyResult<u64> {
    if current_plugin_name().is_none() {
        return Err(no_plugin());
    }
    if cet.is_none() && cldemote.is_none() && mpx.is_none() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "xed_decode_set needs cet, cldemote, or mpx; use xed_decode_clear to disable",
        ));
    }
    let spec = super::xed_decode::Spec { cet, cldemote, mpx };
    let previous =
        with_current_plugin_mut(|plugin| plugin.xed_decode.replace(spec)).ok_or_else(no_plugin)?;
    match super::xed_decode::publish() {
        Ok(generation) => Ok(generation),
        Err(status) => {
            with_current_plugin_mut(|plugin| plugin.xed_decode = previous);
            super::native_policies::refresh_best_effort("rollback failed XED decode update");
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "native XED decode update failed with status {status}; active plugins must agree on explicit feature values"
            )))
        }
    }
}

#[pyfunction(name = "xed_decode_clear")]
fn pb_xed_decode_clear() -> PyResult<bool> {
    let previous =
        with_current_plugin_mut(|plugin| plugin.xed_decode.take()).ok_or_else(no_plugin)?;
    if previous.is_none() {
        return Ok(false);
    }
    match super::xed_decode::publish() {
        Ok(_) => Ok(true),
        Err(status) => {
            with_current_plugin_mut(|plugin| plugin.xed_decode = previous);
            super::native_policies::refresh_best_effort("rollback failed XED decode clear");
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "native XED decode clear failed with status {status}"
            )))
        }
    }
}

#[pyfunction(name = "xed_decode_policy")]
fn pb_xed_decode_policy() -> PyResult<Option<(Option<bool>, Option<bool>, Option<bool>)>> {
    with_current_plugin_mut(|plugin| {
        plugin
            .xed_decode
            .map(|spec| (spec.cet, spec.cldemote, spec.mpx))
    })
    .ok_or_else(no_plugin)
}

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
        "instruction.decode" | "instruction_decode" | "decode" => {
            1 << crate::event::EVENT_INSTRUCTION_DECODE
        }
        "trace.instrument" | "trace_instrument" => 1 << crate::event::EVENT_TRACE_INSTRUMENT,
        "routine.instrument" | "routine_instrument" | "function.instrument" => {
            1 << crate::event::EVENT_ROUTINE_INSTRUMENT
        }
        "basic_block.instrument" | "basic_block_instrument" | "bbl.instrument" => {
            1 << crate::event::EVENT_BBL_INSTRUMENT
        }
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
    m.add_function(wrap_pyfunction!(pb_execution_trap, m)?)?;
    m.add_function(wrap_pyfunction!(pb_execution_trap_remove, m)?)?;
    m.add_function(wrap_pyfunction!(pb_execution_traps, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hit, m)?)?;
    m.add_function(wrap_pyfunction!(pb_is_stopped, m)?)?;
    m.add_function(wrap_pyfunction!(pb_stop, m)?)?;
    m.add_function(wrap_pyfunction!(pb_resume, m)?)?;
    m.add_function(wrap_pyfunction!(pb_step, m)?)?;
    m.add_function(wrap_pyfunction!(pb_wait_stop, m)?)?;
    m.add_function(wrap_pyfunction!(pb_sleep, m)?)?;
    m.add_function(wrap_pyfunction!(pb_pin_state, m)?)?;
    m.add_function(wrap_pyfunction!(pb_pin_attach_supported, m)?)?;
    m.add_function(wrap_pyfunction!(pb_pin_detach, m)?)?;
    m.add_function(wrap_pyfunction!(pb_pin_attach, m)?)?;
    m.add_function(wrap_pyfunction!(pb_resolve, m)?)?;
    m.add_function(wrap_pyfunction!(pb_resolve_name, m)?)?;
    m.add_function(wrap_pyfunction!(pb_disasm, m)?)?;
    m.add_function(wrap_pyfunction!(pb_modules, m)?)?;
    m.add_function(wrap_pyfunction!(pb_threads, m)?)?;
    m.add_function(wrap_pyfunction!(pb_counters, m)?)?;
    m.add_function(wrap_pyfunction!(pb_control_port, m)?)?;
    m.add_function(wrap_pyfunction!(pb_parent_control_port, m)?)?;
    m.add_function(wrap_pyfunction!(pb_exports, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_set, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_rule, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_rules_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_remove, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_hook_events_query, m)?)?;
    m.add_function(wrap_pyfunction!(pb_exc_policy, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_start, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_start_spec, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_extend, m)?)?;
    m.add_function(wrap_pyfunction!(pb_memory_region, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_stop, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_status, m)?)?;
    m.add_function(wrap_pyfunction!(pb_trace_status_detail, m)?)?;
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
    m.add_function(wrap_pyfunction!(pb_instrumentation_set, m)?)?;
    m.add_function(wrap_pyfunction!(pb_instrumentation_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_instrumentation_policy, m)?)?;
    m.add_function(wrap_pyfunction!(pb_memory_translation_set, m)?)?;
    m.add_function(wrap_pyfunction!(pb_memory_translation_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_memory_translation_policy, m)?)?;
    m.add_function(wrap_pyfunction!(pb_code_fetch_set, m)?)?;
    m.add_function(wrap_pyfunction!(pb_code_fetch_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_code_fetch_policy, m)?)?;
    m.add_function(wrap_pyfunction!(pb_xed_decode_set, m)?)?;
    m.add_function(wrap_pyfunction!(pb_xed_decode_clear, m)?)?;
    m.add_function(wrap_pyfunction!(pb_xed_decode_policy, m)?)?;
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
