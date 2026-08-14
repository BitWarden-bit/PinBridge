//! Out-of-process UI query service: a Pin internal thread serving a binary
//! loopback protocol (see pinbridge-proto). When no UI is connected the only
//! cost is a blocked accept; ring reads are try-lock copies into reserved
//! buffers and serialize after unlocking, so analysis callbacks never wait
//! on a UI and the accept loop never parks on a contended Pin mutex.

use crate::event::{Event, EVENT_KIND_COUNT};
use crate::ring;
use crate::{bp, context, control, disasm, engines, exception, hooks, stepper, syscall_engine};
use core::ffi::c_void;
use pinbridge_proto as proto;
use pinbridge_sys::*;
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};

/// The loopback port the query server actually bound (0 = not bound).
/// The in-process script host dials this port for all its debugger work.
pub static BOUND_PORT: core::sync::atomic::AtomicU16 =
    core::sync::atomic::AtomicU16::new(0);

fn to_record(event: &Event) -> proto::EventRecord {
    proto::EventRecord {
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

fn handle_ping() -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    proto::put_u32(&mut out, PB_ABI_VERSION_MAJOR);
    proto::put_u32(&mut out, PB_ABI_VERSION_MINOR);
    let mut pid: i32 = 0;
    unsafe {
        pb_pin_get_pid(&mut pid);
    }
    proto::put_u32(&mut out, pid as u32);
    proto::put_u64(&mut out, ring::ring_total()); // lock-free content edge: never parks the accept loop
    out
}

/// COUNTERS -> [u64 total][u64 dropped][u64 capacity][8 × u64 per-kind]
/// (kinds 1..=8; the array width follows EVENT_KIND_COUNT). All fields are
/// lock-free reads, so this handler can never block behind the ring mutex.
/// total is the content edge (pageable events); dropped counts submissions
/// that did not survive to a pageable slot (try-lock drops + window loss).
fn handle_counters() -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    let total = ring::ring_total();
    let retained = total.min(ring::RING_CAPACITY as u64);
    proto::put_u64(&mut out, total);
    proto::put_u64(&mut out, ring::total_seq().saturating_sub(retained));
    proto::put_u64(&mut out, ring::RING_CAPACITY as u64);
    for kind in 1..EVENT_KIND_COUNT {
        proto::put_u64(&mut out, ring::kind_count(kind));
    }
    out
}

const RING_PAGE_MAX_LIMIT: u64 = 2048;

/// RING_PAGE: cursor-paged ring read with try-lock semantics. The Vec is
/// reserved BEFORE the lock attempt so the critical section is a plain
/// memcpy — allocating under a Pin mutex deadlocks against threads that hold
/// the process-heap lock while blocking on Pin locks (the wedge this fix
/// addresses). A busy mutex answers with an empty page carrying the live
/// total (count=0, next=after): clients retry, and the single-threaded
/// accept loop never parks behind the hot path.
fn handle_ring_page(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let after = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let limit = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?.min(RING_PAGE_MAX_LIMIT);

    let mut events = Vec::with_capacity(limit as usize); // allocated OUTSIDE the lock
    let (missed, total) = match ring::try_page(after, limit as usize, &mut events) {
        Some(pair) => pair,
        None => (0, ring::ring_total()), // busy: empty page, cursor unchanged
    };

    let mut out = Vec::with_capacity(24 + events.len() * proto::EVENT_WIRE_LEN);
    proto::put_u64(&mut out, total);
    proto::put_u64(&mut out, missed);
    proto::put_u64(&mut out, events.last().map(|e| e.sequence).unwrap_or(after));
    proto::put_u64(&mut out, events.len() as u64);
    for event in &events {
        to_record(event).encode(&mut out);
    }
    Ok(out)
}

fn handle_bp_set(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    match bp::set(address) {
        Ok(id) => {
            crate::log::line(&format!("breakpoint set id={id} at 0x{address:x}"));
            let mut out = Vec::with_capacity(4);
            proto::put_u32(&mut out, id);
            Ok(out)
        }
        Err(_) => Err(proto::STATUS_INTERNAL),
    }
}

/// Response: [stopped:u8][hit_tid:u32][hit_addr:u64][stop_gen:u64][count:u32]
///           then count × [id:u32][address:u64][hits:u64].
/// hit_tid/hit_addr describe the breakpoint hit that caused the current
/// stop (hit_tid == u32::MAX for a manual pause or after resume; tid 0 is
/// a real thread, so 0 must never be the none-sentinel). stop_gen bumps on
/// every completed stop — poll-based UIs key refreshes off it.
fn handle_bp_list() -> Vec<u8> {
    let entries = bp::list();
    let (hit_tid, hit_addr) = bp::last_hit();
    let mut out = Vec::with_capacity(21 + entries.len() * 20);
    out.push(control::is_stopped() as u8);
    proto::put_u32(&mut out, hit_tid);
    proto::put_u64(&mut out, hit_addr);
    proto::put_u64(&mut out, bp::stop_gen());
    proto::put_u32(&mut out, entries.len() as u32);
    for (id, address, hits) in entries {
        proto::put_u32(&mut out, id);
        proto::put_u64(&mut out, address);
        proto::put_u64(&mut out, hits);
    }
    out
}

fn handle_bp_remove(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let id = reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?;
    if !bp::remove(id) {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    crate::log::line(&format!("breakpoint removed id={id}"));
    let mut out = Vec::with_capacity(4);
    proto::put_u32(&mut out, id);
    Ok(out)
}

fn handle_engine_set(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let kind = reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?;
    let on = reader.u8().ok_or(proto::STATUS_BAD_REQUEST)?;
    if !engines::set_engine_enabled(kind, on != 0) {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    crate::log::line(&format!("engine {kind} -> {on}"));
    Ok(Vec::new())
}

fn handle_exc_policy_set(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let enabled = reader.u8().ok_or(proto::STATUS_BAD_REQUEST)? != 0;
    let code = reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?;
    exception::set_policy(enabled, code);
    crate::log::line(&format!("exception policy enabled={enabled} code=0x{code:x}"));
    Ok(Vec::new())
}

fn handle_exc_policy_get() -> Vec<u8> {
    let (enabled, code, pending) = exception::policy();
    let mut out = Vec::with_capacity(6);
    out.push(enabled as u8);
    proto::put_u32(&mut out, code);
    out.push(pending as u8);
    out
}

/// SYSCALL_FILTER: [u8 mode (0=all, 1=only listed)][u16 count][count × u32 number]
/// -> empty. The filter snapshot swaps in atomically; syscall entry/exit
/// callbacks read it lock-free.
fn handle_syscall_filter(payload: &[u8]) -> Result<Vec<u8>, u8> {
    const MAX_FILTER_NUMBERS: usize = 4096;
    let mut reader = proto::Reader::new(payload);
    let mode = reader.u8().ok_or(proto::STATUS_BAD_REQUEST)?;
    if mode > 1 {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let count = reader.u16().ok_or(proto::STATUS_BAD_REQUEST)? as usize;
    if count > MAX_FILTER_NUMBERS {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let mut numbers = Vec::with_capacity(count);
    for _ in 0..count {
        numbers.push(reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?);
    }
    syscall_engine::set_filter(mode, &numbers);
    crate::log::line(&format!("syscall filter mode={mode} count={count}"));
    Ok(Vec::new())
}

/// HOOK_SET: [u64 addr] -> [u32 ok] (0 = set full at 4096 points).
fn handle_hook_set(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let ok = hooks::set(address);
    crate::log::line(&format!("hook set 0x{address:x} -> {ok}"));
    let mut out = Vec::with_capacity(4);
    proto::put_u32(&mut out, ok as u32);
    Ok(out)
}

/// HOOK_REMOVE: [u64 addr] -> empty.
fn handle_hook_remove(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    hooks::remove(address);
    Ok(Vec::new())
}

/// HOOK_LIST: empty -> [u32 count][count × u64 addr].
fn handle_hook_list() -> Vec<u8> {
    let addresses = hooks::list();
    let mut out = Vec::with_capacity(4 + addresses.len() * 8);
    proto::put_u32(&mut out, addresses.len() as u32);
    for address in addresses {
        proto::put_u64(&mut out, address);
    }
    out
}

/// TRACE_START: [u32 kinds_mask][u64 lo][u64 hi][u16 path_len][path]
/// -> [u32 ok] on success; STATUS_INTERNAL carries the reason text
/// ("already recording", bad mask/range, alloc failure, ...). Arms the
/// record channel and flushes the window's JIT (see record.rs).
fn handle_trace_start(payload: &[u8]) -> (u8, Vec<u8>) {
    let mut reader = proto::Reader::new(payload);
    let bad = || (proto::STATUS_BAD_REQUEST, b"bad trace_start payload".to_vec());
    let Some(kinds_mask) = reader.u32() else {
        return bad();
    };
    let Some(lo) = reader.u64() else {
        return bad();
    };
    let Some(hi) = reader.u64() else {
        return bad();
    };
    let Some(path_len) = reader.u16() else {
        return bad();
    };
    let Some(path) = take_str(&mut reader, path_len as usize) else {
        return bad();
    };
    match crate::record::start(kinds_mask, lo, hi, path) {
        Ok(()) => {
            let mut out = Vec::with_capacity(4);
            proto::put_u32(&mut out, 1);
            (proto::STATUS_OK, out)
        }
        Err(message) => (proto::STATUS_INTERNAL, message.into_bytes()),
    }
}

/// TRACE_STOP: empty -> [u64 recorded][u64 dropped]. Bounded wait (~5s)
/// for the drainer to catch up; idempotent when not recording.
fn handle_trace_stop() -> Vec<u8> {
    let (recorded, dropped) = crate::record::stop();
    let mut out = Vec::with_capacity(16);
    proto::put_u64(&mut out, recorded);
    proto::put_u64(&mut out, dropped);
    out
}

/// TRACE_STATUS: empty -> [u8 active][u64 recorded][u64 dropped].
fn handle_trace_status() -> Vec<u8> {
    let (active, recorded, dropped) = crate::record::status();
    let mut out = Vec::with_capacity(17);
    out.push(active as u8);
    proto::put_u64(&mut out, recorded);
    proto::put_u64(&mut out, dropped);
    out
}

/// Single step: payload [u32 tid][u8 mode (0=into, 1=over)]. Requires the
/// application to be stopped. Exact stepping: decode the current
/// instruction's control flow (ABI v1.4) and plant one-shot breakpoints on
/// every possible successor, so the landing stops through the exact
/// redirect path instead of drifting past it. Falls back to the
/// exec-capture stepper when the instruction cannot be decoded.
fn take_str(reader: &mut proto::Reader, len: usize) -> Option<String> {
    let rest = reader.remaining();
    if rest.len() < len {
        return None;
    }
    let text = String::from_utf8_lossy(&rest[..len]).into_owned();
    reader.skip(len)?;
    Some(text)
}

/// SCRIPT_LOAD: [u16 name_len][name][u32 src_len][source] -> [u32 script_id].
/// Upsert by name: a same-name load replaces the old plugin (its on_unload
/// runs first). Failure carries the compile error text in the payload so
/// clients can show *why* a plugin was rejected. The reply fires after
/// COMPILE only; the top level and pb_init() run on the next host tick.
fn handle_script_load(payload: &[u8]) -> (u8, Vec<u8>) {
    let mut reader = proto::Reader::new(payload);
    let bad = || (proto::STATUS_BAD_REQUEST, b"bad script_load payload".to_vec());
    let Some(name_len) = reader.u16() else {
        return bad();
    };
    let Some(name) = take_str(&mut reader, name_len as usize) else {
        return bad();
    };
    let Some(src_len) = reader.u32() else {
        return bad();
    };
    let Some(source) = take_str(&mut reader, src_len as usize) else {
        return bad();
    };
    match crate::scripting::load(name, source) {
        Ok(id) => {
            let mut out = Vec::with_capacity(4);
            proto::put_u32(&mut out, id);
            (proto::STATUS_OK, out)
        }
        Err(message) => (proto::STATUS_INTERNAL, message.into_bytes()),
    }
}

/// SCRIPT_UNLOAD: [u16 name_len][name] -> empty. An empty name unloads all
/// plugins.
fn handle_script_unload(payload: &[u8]) -> (u8, Vec<u8>) {
    let mut reader = proto::Reader::new(payload);
    let bad = || (proto::STATUS_BAD_REQUEST, b"bad script_unload payload".to_vec());
    let Some(name_len) = reader.u16() else {
        return bad();
    };
    let Some(name) = take_str(&mut reader, name_len as usize) else {
        return bad();
    };
    match crate::scripting::unload(&name) {
        Ok(()) => (proto::STATUS_OK, Vec::new()),
        Err(message) => (proto::STATUS_INTERNAL, message.into_bytes()),
    }
}

/// SCRIPT_LIST: empty -> [u32 count][count × (u16 nlen, name, u8 state,
/// u64 delivered, u64 dropped)]. state: 1 = running, 2 = error.
/// An unavailable scripting host answers with an empty list.
fn handle_script_list() -> Vec<u8> {
    let entries = crate::scripting::list().unwrap_or_default();
    let mut out = Vec::with_capacity(4 + entries.len() * 32);
    proto::put_u32(&mut out, entries.len() as u32);
    for entry in entries {
        out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
        out.extend_from_slice(entry.name.as_bytes());
        out.push(entry.state);
        proto::put_u64(&mut out, entry.delivered);
        proto::put_u64(&mut out, entry.dropped);
    }
    out
}

/// SCRIPT_OUTPUT: [u64 after][u32 limit] -> [u64 next][u32 count]
/// [count × (u64 seq, u16 nlen, name, u16 llen, line)]. Serves the plugin
/// output ring regardless of python availability.
fn handle_script_output(payload: &[u8]) -> Result<Vec<u8>, u8> {
    const OUTPUT_PAGE_MAX: u32 = 1024;
    let mut reader = proto::Reader::new(payload);
    let after = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let limit = reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?.min(OUTPUT_PAGE_MAX) as usize;
    let (next, entries) = crate::scripting::output_page(after, limit);
    let mut out = Vec::with_capacity(12 + entries.len() * 48);
    proto::put_u64(&mut out, next);
    proto::put_u32(&mut out, entries.len() as u32);
    for entry in entries {
        // keep the wire frame well-formed even for pathological lines
        let line: String = entry.line.chars().take(u16::MAX as usize).collect();
        let name: String = entry.plugin.chars().take(u16::MAX as usize).collect();
        proto::put_u64(&mut out, entry.seq);
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(line.len() as u16).to_le_bytes());
        out.extend_from_slice(line.as_bytes());
    }
    Ok(out)
}

pub fn handle_step(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let tid = reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?;
    let over = reader.u8().ok_or(proto::STATUS_BAD_REQUEST)? != 0;
    if !control::is_stopped() {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let context = unsafe {
        let mut context: PbConstContextHandle = core::ptr::null();
        if pb_pin_get_stopped_thread_context(tid, &mut context) != PB_OK || context.is_null() {
            return Err(proto::STATUS_BAD_REQUEST);
        }
        context
    };
    let read_reg = |reg: i32| -> Option<u64> {
        if reg < 0 {
            return None;
        }
        let mut value: u64 = 0;
        unsafe {
            if pb_pin_get_context_reg(context, reg as u32, &mut value) != PB_OK {
                return None;
            }
        }
        Some(value)
    };
    let read_mem_u64 = |address: u64| -> Option<u64> {
        let mut buffer = [0u8; 8];
        let mut copied: u64 = 0;
        unsafe {
            pb_pin_safe_copy(buffer.as_mut_ptr() as *mut c_void, address, 8, &mut copied);
        }
        if copied == 8 {
            Some(u64::from_le_bytes(buffer))
        } else {
            None
        }
    };
    let rip = read_reg(PB_REG_RIP as i32).ok_or(proto::STATUS_BAD_REQUEST)?;

    let mut flow: PbFlowInsn = unsafe { core::mem::zeroed() };
    let decoded = unsafe {
        let mut bytes = [0u8; 16];
        let mut copied: u64 = 0;
        pb_pin_safe_copy(bytes.as_mut_ptr() as *mut c_void, rip, 16, &mut copied);
        copied > 0
            && pb_disassemble_flow(bytes.as_ptr(), copied, rip, &mut flow) == PB_OK
            && flow.size > 0
    };

    // step over a call: a single one-shot on the fallthrough
    if decoded && over && flow.kind == 2 {
        let fallthrough = rip + flow.size as u64;
        let id = bp::set_oneshot(fallthrough).map_err(|_| proto::STATUS_INTERNAL)?;
        bp::note_step_bp(id, fallthrough);
        bp::arm_resume_skip(); // swallow the replay if we sit on a breakpoint
        bp::arm_step_watchdog(100); // ~5s: auto-pause if the call never returns
        if !bp::control_command(bp::CMD_RESUME) {
            return Err(proto::STATUS_INTERNAL);
        }
        crate::log::line(&format!("step over tid={tid} from 0x{rip:x} bp=0x{fallthrough:x}"));
        return Ok(vec![1]);
    }

    if decoded {
        // enumerate every possible successor of the current instruction
        let fallthrough = rip + flow.size as u64;
        let mut candidates: Vec<u64> = Vec::with_capacity(3);
        if flow.kind == 0 || flow.conditional != 0 {
            candidates.push(fallthrough);
        }
        if flow.has_target != 0 {
            candidates.push(flow.target);
        }
        if flow.ind_reg != 0 {
            if let Some(value) = read_reg(flow.base_reg) {
                candidates.push(value);
            }
        }
        if flow.ind_mem != 0 {
            let base = if flow.base_reg == PB_REG_RIP as i32 {
                Some(fallthrough) // RIP-relative: base is the next instruction
            } else {
                read_reg(flow.base_reg)
            };
            if let Some(base) = base {
                let ea = base
                    .wrapping_add(read_reg(flow.index_reg).unwrap_or(0).wrapping_mul(flow.scale))
                    .wrapping_add(flow.disp as u64);
                if let Some(value) = read_mem_u64(ea) {
                    candidates.push(value);
                }
            }
        }
        if flow.kind == 3 {
            // ret: successor is the qword at [rsp]
            if let Some(value) = read_reg(PB_REG_RSP as i32).and_then(read_mem_u64) {
                candidates.push(value);
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut planted = 0u32;
        for address in &candidates {
            if let Ok(id) = bp::set_oneshot(*address) {
                bp::note_step_bp(id, *address);
                planted += 1;
            }
        }
        if planted > 0 {
            bp::arm_resume_skip(); // swallow the replay if we sit on a breakpoint
            bp::arm_step_watchdog(100); // ~5s: auto-pause if no successor fires
            if !bp::control_command(bp::CMD_RESUME) {
                return Err(proto::STATUS_INTERNAL);
            }
            crate::log::line(&format!(
                "step tid={tid} from 0x{rip:x} -> {planted} candidate bp(s)"
            ));
            return Ok(vec![1]);
        }
    }

    // fallback: exec-capture stepper (imprecise on huge traces, but works
    // without a decodable instruction)
    stepper::arm(tid, rip, false);
    bp::arm_step_watchdog(100);
    if !bp::control_command(bp::CMD_RESUME) {
        return Err(proto::STATUS_INTERNAL);
    }
    crate::log::line(&format!("step(fallback) tid={tid} from 0x{rip:x}"));
    Ok(vec![1])
}

/// RESOLVE_NAME: [u16 len]["module!Export"] -> [u64 address] (0 = unknown).
fn handle_resolve_name(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let len = reader.u16().ok_or(proto::STATUS_BAD_REQUEST)? as usize;
    let spec = take_str(&mut reader, len).ok_or(proto::STATUS_BAD_REQUEST)?;
    let address = crate::resolve::resolve_name(&spec).unwrap_or(0);
    let mut out = Vec::with_capacity(8);
    proto::put_u64(&mut out, address);
    Ok(out)
}

/// TEMP wedge-hunt trace: one agent-log line per served op, only with
/// PINBRIDGE_QS_TRACE=1. Atomic tri-state gate (no OnceLock: a contended
/// get_or_init parks through std TLS, unassigned in this module).
fn qs_trace(op_code: u8, entry: bool) {
    static ON: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
    match ON.load(core::sync::atomic::Ordering::Relaxed) {
        2 => {}
        1 => return,
        _ => {
            let on =
                std::env::var("PINBRIDGE_QS_TRACE").ok().as_deref() == Some("1");
            ON.store(
                if on { 2 } else { 1 },
                core::sync::atomic::Ordering::Relaxed,
            );
            if !on {
                return;
            }
        }
    }
    crate::log::line(&format!(
        "qs {} op=0x{op_code:02x}",
        if entry { "begin" } else { "end" }
    ));
}

fn serve_client(stream: &mut TcpStream) {
    let _ = stream.set_nodelay(true);
    loop {
        let frame = proto::read_frame(stream);
        let (op_code, _status, payload) = match frame {
            Ok(frame) => frame,
            Err(_) => return, // client went away or sent garbage: drop it
        };
        qs_trace(op_code, true);
        let (status, body) = match op_code {
            proto::op::PING => (proto::STATUS_OK, handle_ping()),
            proto::op::COUNTERS => (proto::STATUS_OK, handle_counters()),
            proto::op::RING_PAGE => match handle_ring_page(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::STOP => (proto::STATUS_OK, control::handle_stop()),
            proto::op::RESUME => (proto::STATUS_OK, control::handle_resume()),
            proto::op::READ_MEM => match control::handle_read_mem(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::WRITE_MEM => match control::handle_write_mem(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::MODULES => (proto::STATUS_OK, control::handle_modules()),
            proto::op::BP_SET => match handle_bp_set(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::BP_LIST => (proto::STATUS_OK, handle_bp_list()),
            proto::op::BP_REMOVE => match handle_bp_remove(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::THREADS => (proto::STATUS_OK, context::handle_threads()),
            proto::op::CONTEXT_GET => match context::handle_context_get(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::CONTEXT_SET => match context::handle_context_set(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::ENGINE_SET => match handle_engine_set(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::EXC_POLICY_SET => match handle_exc_policy_set(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::EXC_POLICY_GET => (proto::STATUS_OK, handle_exc_policy_get()),
            proto::op::STEP => match handle_step(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::DISASM => match disasm::handle_disasm(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::RESOLVE => match crate::resolve::handle_resolve(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::RESOLVE_NAME => match handle_resolve_name(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::EXPORTS => match crate::resolve::handle_exports(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::SYSCALL_FILTER => match handle_syscall_filter(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::TRACE_START => handle_trace_start(&payload),
            proto::op::TRACE_STOP => (proto::STATUS_OK, handle_trace_stop()),
            proto::op::TRACE_STATUS => (proto::STATUS_OK, handle_trace_status()),
            proto::op::HOOK_SET => match handle_hook_set(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::HOOK_REMOVE => match handle_hook_remove(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            proto::op::HOOK_CLEAR => {
                hooks::clear();
                (proto::STATUS_OK, Vec::new())
            }
            proto::op::HOOK_LIST => (proto::STATUS_OK, handle_hook_list()),
            proto::op::SCRIPT_LOAD => {
                let reply = handle_script_load(&payload);
                crate::diag::heap_check("qs script_load");
                reply
            }
            proto::op::SCRIPT_UNLOAD => {
                let reply = handle_script_unload(&payload);
                crate::diag::heap_check("qs script_unload");
                reply
            }
            proto::op::SCRIPT_LIST => (proto::STATUS_OK, handle_script_list()),
            proto::op::SCRIPT_OUTPUT => match handle_script_output(&payload) {
                Ok(body) => (proto::STATUS_OK, body),
                Err(code) => (code, Vec::new()),
            },
            _ => (proto::STATUS_BAD_REQUEST, Vec::new()),
        };
        qs_trace(op_code, false);
        // TEMP hunt aid: per-op heap validation (PINBRIDGE_HEAP_CHECK_FAST=1).
        if crate::diag::heap_check_fast_enabled() {
            crate::diag::heap_check("qs op");
        }
        if proto::write_frame(stream, op_code, status, &body).is_err() {
            return;
        }
    }
}

unsafe extern "C" fn server_main(argument: *mut c_void) {
    let listener = Box::from_raw(argument as *mut TcpListener);
    let mut tid: PbThreadId = 0;
    pb_pin_thread_id(&mut tid);
    crate::log::line(&format!(
        "query server thread up (pin tid {tid}, os tid {})",
        crate::diag::os_tid()
    ));
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                serve_client(&mut stream);
            }
            Err(ref error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => continue,
        }
    }
}

/// Binds the loopback port NOW (tool thread) and spawns the accept loop on a
/// Pin internal thread. Binding here matters: with PINBRIDGE_ENTRY_BP the app
/// stops at its first instruction, and a not-yet-started internal thread
/// apparently never gets scheduled once all app threads are suspended — but
/// an already-bound listener completes client handshakes in the kernel, so
/// the launcher's wait_for_port succeeds regardless.
pub fn spawn() -> PbStatus {
    let port = std::env::var("PINBRIDGE_AGENT_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(proto::DEFAULT_PORT);
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        Err(error) => {
            // headless-tolerant: log and run without a control plane
            crate::log::line(&format!("query server bind 127.0.0.1:{port} failed: {error}"));
            return PB_OK;
        }
    };
    let bound = listener
        .local_addr()
        .map(|address| address.port())
        .unwrap_or(port);
    BOUND_PORT.store(bound, core::sync::atomic::Ordering::Release);
    crate::log::line(&format!("query server bound 127.0.0.1:{bound}"));
    let boxed = Box::into_raw(Box::new(listener));
    let mut thread_id: PbThreadId = 0;
    let mut thread_uid: PbPinThreadUid = 0;
    let status = unsafe {
        pb_pin_spawn_internal_thread(
            Some(server_main),
            boxed as *mut c_void,
            0,
            &mut thread_id,
            &mut thread_uid,
        )
    };
    if status != PB_OK {
        drop(unsafe { Box::from_raw(boxed) }); // no thread will claim it
    }
    status
}
