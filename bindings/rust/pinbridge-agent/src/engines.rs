//! Instrumentation-time engine dispatch and the analysis capture callbacks.
//!
//! Two-level filtering keeps the hot path cheap:
//!   1. instrumentation time — only in-range instructions get capture calls;
//!   2. analysis time — one atomic flag check before recording an event.

use crate::event::{
    Event, EVENT_BRANCH_EDGE, EVENT_EXEC, EVENT_HOOK_REGS, EVENT_HOOK_RETURN, EVENT_MEMORY,
    EVENT_SYSCALL,
};
use crate::ring::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

static ENGINE_ACTIVE: AtomicBool = AtomicBool::new(true);
static TRACE_START: AtomicU64 = AtomicU64::new(0x1000);
static TRACE_END: AtomicU64 = AtomicU64::new(u64::MAX);
static HOOK_START: AtomicU64 = AtomicU64::new(0);
static HOOK_END: AtomicU64 = AtomicU64::new(0); // empty range: hook engine off
static ENABLE_EXEC: AtomicBool = AtomicBool::new(true);
static ENABLE_MEMORY: AtomicBool = AtomicBool::new(true);
static ENABLE_BRANCH: AtomicBool = AtomicBool::new(true);

// Instruction metadata (kind + static size), recorded at instrumentation time
// for the step-over machinery. Instrumentation time may allocate; guarded by
// a Pin mutex like every shared structure in this process.
pub const KIND_LINEAR: u8 = 0;
pub const KIND_BRANCH: u8 = 1;
pub const KIND_CALL: u8 = 2;
pub const KIND_RETURN: u8 = 3;

static META_MUTEX: AtomicUsize = AtomicUsize::new(0);
static mut INS_META: Option<crate::TlsFreeMap<u64, (u8, u8)>> = None;
const META_MAX: usize = 1_000_000;

pub fn meta_init() -> PbStatus {
    let mut handle: PbMutexHandle = core::ptr::null_mut();
    let status = unsafe { pb_pin_mutex_init(&mut handle) };
    if status == PB_OK {
        META_MUTEX.store(handle as usize, Ordering::Release);
        unsafe {
            INS_META = Some(crate::new_map());
        }
    }
    status
}

fn meta_record(address: u64, kind: u8, size: u8) {
    let mutex = META_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if mutex.is_null() {
        return;
    }
    unsafe {
        if pb_pin_mutex_lock(mutex) != PB_OK {
            return;
        }
        let map = &mut *core::ptr::addr_of_mut!(INS_META);
        if let Some(map) = map.as_mut() {
            if map.len() < META_MAX {
                map.insert(address, (kind, size));
            }
        }
        pb_pin_mutex_unlock(mutex);
    }
}

/// (kind, size) for an instrumented instruction, if recorded.
pub fn meta_lookup(address: u64) -> Option<(u8, u8)> {
    let mutex = META_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if mutex.is_null() {
        return None;
    }
    let mut found = None;
    unsafe {
        if pb_pin_mutex_lock(mutex) == PB_OK {
            let map = &*core::ptr::addr_of!(INS_META);
            if let Some(map) = map.as_ref() {
                found = map.get(&address).copied();
            }
            pb_pin_mutex_unlock(mutex);
        }
    }
    found
}

fn parse_range(value: &str) -> Option<(u64, u64)> {
    let (start, end) = value.split_once('-')?;
    let start = u64::from_str_radix(start.trim().trim_start_matches("0x"), 16).ok()?;
    let end = u64::from_str_radix(end.trim().trim_start_matches("0x"), 16).ok()?;
    (start < end).then_some((start, end))
}

/// Reads the optional range knobs:
///   PINBRIDGE_AGENT_RANGE=0xSTART-0xEND        trace/memory coverage
///   PINBRIDGE_AGENT_HOOK_RANGE=0xSTART-0xEND   register-capture hook coverage
pub fn configure_from_env() {
    if let Ok(value) = std::env::var("PINBRIDGE_AGENT_RANGE") {
        if let Some((start, end)) = parse_range(&value) {
            TRACE_START.store(start, Ordering::Relaxed);
            TRACE_END.store(end, Ordering::Relaxed);
        }
    }
    if let Ok(value) = std::env::var("PINBRIDGE_AGENT_HOOK_RANGE") {
        if let Some((start, end)) = parse_range(&value) {
            HOOK_START.store(start, Ordering::Relaxed);
            HOOK_END.store(end, Ordering::Relaxed);
        }
    }
    // Debug/verification aid: comma list out of exec,memory,branch.
    if let Ok(value) = std::env::var("PINBRIDGE_AGENT_ENGINES") {
        ENABLE_EXEC.store(value.contains("exec"), Ordering::Relaxed);
        ENABLE_MEMORY.store(value.contains("memory"), Ordering::Relaxed);
        ENABLE_BRANCH.store(value.contains("branch"), Ordering::Relaxed);
    }
}

pub fn trace_range() -> (u64, u64) {
    (
        TRACE_START.load(Ordering::Relaxed),
        TRACE_END.load(Ordering::Relaxed),
    )
}

pub fn hook_range() -> (u64, u64) {
    (
        HOOK_START.load(Ordering::Relaxed),
        HOOK_END.load(Ordering::Relaxed),
    )
}

/// Runtime engine toggle (immediate: checked inside the analysis callback).
/// kind: 2=memory 3=exec 4=branch_edge 5=syscall. Returns false on bad kind.
pub fn set_engine_enabled(kind: u32, on: bool) -> bool {
    match kind {
        EVENT_MEMORY => ENABLE_MEMORY.store(on, Ordering::Relaxed),
        EVENT_EXEC => ENABLE_EXEC.store(on, Ordering::Relaxed),
        EVENT_BRANCH_EDGE => ENABLE_BRANCH.store(on, Ordering::Relaxed),
        EVENT_SYSCALL => crate::syscall_engine::set_enabled(on),
        _ => return false,
    }
    true
}

#[inline]
fn in_range(address: u64, start: u64, end: u64) -> bool {
    address >= start && address < end
}

#[inline]
unsafe fn query_bool(ins: PbInsHandle, query: unsafe extern "C" fn(PbInsHandle, *mut u8) -> PbStatus) -> bool {
    let mut value: u8 = 0;
    query(ins, &mut value) == PB_OK && value != 0
}

/// Instrumentation callback: runs once per discovered instruction.
pub unsafe extern "C" fn on_ins(ins: PbInsHandle, _user_data: *mut c_void) {
    if !ENGINE_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let mut address: u64 = 0;
    if pb_ins_address(ins, &mut address) != PB_OK {
        return;
    }

    // instrumentation breakpoints: any address, regardless of capture ranges
    if crate::bp::any_active() {
        if let Some(slot) = crate::bp::find(address) {
            crate::bp::instrument(ins, slot);
        }
    }

    let (hook_start, hook_end) = hook_range();
    if in_range(address, hook_start, hook_end) {
        pb_ins_insert_capture_regs(ins, Some(on_hook_regs), core::ptr::null_mut());
    }

    // Runtime hook points receive a borrowed Pin context. The callback still
    // emits the normal kind-1 event, then applies any precompiled action rules
    // directly to that context before the application instruction continues.
    if crate::hooks::any() && crate::hooks::contains(address) {
        crate::hooks::mark_return(address, query_bool(ins, pb_ins_is_ret));
        pb_ins_insert_capture_regs_ctx(ins, Some(on_hook_context), core::ptr::null_mut());
    }

    // record channel: independent of the main trace range and engine
    // enables; insertions are inert until a session arms (analysis-time
    // re-check inside the record callbacks)
    let (rec_lo, rec_hi) = crate::record::instrumentation_range();
    let recording = crate::record::instrumentation_enabled()
        && in_range(address, rec_lo, rec_hi);

    let (trace_start, trace_end) = trace_range();
    if !in_range(address, trace_start, trace_end) {
        if recording {
            let branchy = query_bool(ins, pb_ins_is_call)
                || query_bool(ins, pb_ins_is_ret)
                || query_bool(ins, pb_ins_is_branch);
            crate::record::instrument(ins, branchy);
        }
        return;
    }
    let mut size: u64 = 0;
    pb_ins_size(ins, &mut size);
    let is_call = query_bool(ins, pb_ins_is_call);
    let is_ret = !is_call && query_bool(ins, pb_ins_is_ret);
    let is_branch = !is_call && !is_ret && query_bool(ins, pb_ins_is_branch);
    let kind = if is_call {
        KIND_CALL
    } else if is_ret {
        KIND_RETURN
    } else if is_branch {
        KIND_BRANCH
    } else {
        KIND_LINEAR
    };
    meta_record(address, kind, size.min(255) as u8);

    if ENABLE_EXEC.load(Ordering::Relaxed) {
        pb_ins_insert_exec(ins, Some(on_exec), core::ptr::null_mut());
    }
    if ENABLE_MEMORY.load(Ordering::Relaxed) {
        pb_ins_insert_memory_operands(ins, Some(on_memory), core::ptr::null_mut());
    }
    if ENABLE_BRANCH.load(Ordering::Relaxed) && (is_branch || is_call || is_ret) {
        pb_ins_insert_branch_edge(ins, Some(on_branch_edge), core::ptr::null_mut());
    }
    if recording {
        crate::record::instrument(ins, is_branch || is_call || is_ret);
    }
}

unsafe extern "C" fn on_hook_regs(
    address: u64,
    thread_id: u32,
    rcx: u64,
    rdx: u64,
    r8: u64,
    r9: u64,
    _user_data: *mut c_void,
) {
    submit(Event {
        kind: EVENT_HOOK_REGS,
        thread_id,
        address,
        arg0: rcx,
        arg1: rdx,
        arg2: r8,
        arg3: r9,
        ..Event::EMPTY
    });
}

unsafe extern "C" fn on_hook_context(
    address: u64,
    thread_id: u32,
    context: PbContextHandle,
    rcx: u64,
    rdx: u64,
    r8: u64,
    r9: u64,
    _user_data: *mut c_void,
) {
    if crate::hooks::take_replay_guard(thread_id, address) {
        return;
    }
    // Keep the pre-action ABI stack arguments in the event as a4..a7. The
    // fixed a0..a3 slots remain the captured register values for compatibility
    // with existing hook scripts.
    let mut stack_args = [0u64; 4];
    let is_return = crate::hooks::is_return(address);
    if !is_return {
        for (index, value) in stack_args.iter_mut().enumerate() {
            let _ = pb_pin_get_context_stack_arg(
                context as PbConstContextHandle,
                index as u32,
                value,
            );
        }
    }
    if is_return {
        let mut return_value = 0;
        let _ = pb_pin_get_context_reg(
            context as PbConstContextHandle,
            crate::arch::return_reg(),
            &mut return_value,
        );
        submit(Event {
            kind: EVENT_HOOK_RETURN,
            thread_id,
            address,
            // Return hooks use a0 for the value visible immediately before
            // the native action; the remaining slots retain the normal
            // register/stack snapshot for diagnostics.
            arg0: return_value,
            arg1: rcx,
            arg2: rdx,
            arg3: r8,
            arg4: r9,
            arg5: stack_args[0],
            arg6: stack_args[1],
            arg7: stack_args[2],
            ..Event::EMPTY
        });
    } else {
        submit(Event {
            kind: EVENT_HOOK_REGS,
            thread_id,
            address,
            arg0: rcx,
            arg1: rdx,
            arg2: r8,
            arg3: r9,
            arg4: stack_args[0],
            arg5: stack_args[1],
            arg6: stack_args[2],
            arg7: stack_args[3],
            ..Event::EMPTY
        });
    }
    let mut changed =
        crate::hooks::apply_rules(address, thread_id, context, [rcx, rdx, r8, r9]) > 0;
    if let Some(response) = crate::sync_intercept::decide_hook(
        address,
        thread_id,
        is_return,
        context,
        stack_args,
    ) {
        changed |= crate::sync_intercept::apply_hook_response(context, &response, is_return);
        if response.action == crate::sync_intercept::HOOK_ACTION_RETURN
            && !is_return
            && crate::sync_intercept::return_from_hook(context)
        {
            let _ = pb_pin_execute_at(context as PbConstContextHandle);
            return;
        }
        if crate::sync_intercept::response_changes_instruction_pointer(&response) && changed {
            let _ = pb_pin_execute_at(context as PbConstContextHandle);
            return;
        }
    }
    if changed {
        let _ = crate::hooks::execute_modified_context(thread_id, address, context);
    }
}

unsafe extern "C" fn on_memory(
    instruction_address: u64,
    thread_id: u32,
    memory_address: u64,
    size: u32,
    access: u32,
    _user_data: *mut c_void,
) {
    if !ENABLE_MEMORY.load(Ordering::Relaxed) {
        return;
    }
    submit(Event {
        kind: EVENT_MEMORY,
        thread_id,
        address: instruction_address,
        arg0: memory_address,
        arg1: size as u64,
        arg2: access as u64,
        ..Event::EMPTY
    });
}

unsafe extern "C" fn on_exec(
    address: u64,
    thread_id: u32,
    size: u32,
    _user_data: *mut c_void,
) {
    if crate::stepper::on_step_event(thread_id, address) {
        return;
    }
    if !ENABLE_EXEC.load(Ordering::Relaxed) {
        return;
    }
    submit(Event {
        kind: EVENT_EXEC,
        thread_id,
        address,
        arg0: size as u64,
        ..Event::EMPTY
    });
}

unsafe extern "C" fn on_branch_edge(
    address: u64,
    thread_id: u32,
    target_address: u64,
    taken: u64,
    _user_data: *mut c_void,
) {
    if !ENABLE_BRANCH.load(Ordering::Relaxed) {
        return;
    }
    submit(Event {
        kind: EVENT_BRANCH_EDGE,
        thread_id,
        address,
        arg0: target_address,
        arg1: taken,
        ..Event::EMPTY
    });
}
