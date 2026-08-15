//! Instrumentation-time engine dispatch and the analysis capture callbacks.
//!
//! Two-level filtering keeps the hot path bounded:
//!   1. instrumentation time — immutable kind/range rules decide which capture calls are inserted;
//!   2. analysis time — the same snapshot applies exact kind/range/thread rules before submission.

use crate::event::{
    Event, EVENT_BRANCH_EDGE, EVENT_EXEC, EVENT_HOOK_REGS, EVENT_HOOK_RETURN, EVENT_MEMORY,
    EVENT_BBL_INSTRUMENT, EVENT_INSTRUCTION_DECODE, EVENT_ROUTINE_INSTRUMENT, EVENT_SYSCALL,
    EVENT_TRACE_INSTRUMENT,
};
use crate::ring::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

static ENGINE_ACTIVE: AtomicBool = AtomicBool::new(true);
static TRACE_START: AtomicU64 = AtomicU64::new(0x1000);
static TRACE_END: AtomicU64 = AtomicU64::new(u64::MAX);
static HOOK_START: AtomicU64 = AtomicU64::new(0);
static HOOK_END: AtomicU64 = AtomicU64::new(0); // empty range: hook engine off
static ENABLE_EXEC: AtomicBool = AtomicBool::new(true);
static ENABLE_MEMORY: AtomicBool = AtomicBool::new(true);
static ENABLE_BRANCH: AtomicBool = AtomicBool::new(true);
static DEFAULT_INSTRUMENTATION_KINDS: AtomicU64 = AtomicU64::new(0);

pub const INSTRUMENT_EXEC: u32 = 1 << EVENT_EXEC;
pub const INSTRUMENT_MEMORY: u32 = 1 << EVENT_MEMORY;
pub const INSTRUMENT_BRANCH: u32 = 1 << EVENT_BRANCH_EDGE;
pub const INSTRUMENT_DECODE: u32 = 1 << EVENT_INSTRUCTION_DECODE;
pub const INSTRUMENT_TRACE: u32 = 1 << EVENT_TRACE_INSTRUMENT;
pub const INSTRUMENT_ROUTINE: u32 = 1 << EVENT_ROUTINE_INSTRUMENT;
pub const INSTRUMENT_BBL: u32 = 1 << EVENT_BBL_INSTRUMENT;
pub const INSTRUMENT_ALL: u32 =
    INSTRUMENT_EXEC | INSTRUMENT_MEMORY | INSTRUMENT_BRANCH | INSTRUMENT_DECODE
        | INSTRUMENT_TRACE | INSTRUMENT_ROUTINE | INSTRUMENT_BBL;
pub const MAX_INSTRUMENTATION_RANGES: usize = 64;
pub const MAX_INSTRUMENTATION_THREADS: usize = 64;

pub struct InstrumentationPolicyConfig {
    pub kinds: u32,
    pub ranges: Vec<(u64, u64)>,
    pub threads: Vec<u32>,
}

struct InstrumentationRule {
    kinds: u32,
    start: u64,
    end: u64,
    threads: Vec<u32>,
}

impl InstrumentationRule {
    #[inline]
    fn matches_instrumentation(&self, address: u64, kind: u32) -> bool {
        self.kinds & (1 << kind) != 0 && address >= self.start && address < self.end
    }

    #[inline]
    fn matches_analysis(&self, address: u64, thread_id: u32, kind: u32) -> bool {
        self.matches_instrumentation(address, kind)
            && (self.threads.is_empty() || self.threads.binary_search(&thread_id).is_ok())
    }
}

struct InstrumentationPolicy {
    rules: Vec<InstrumentationRule>,
}

static INSTRUMENTATION_POLICY: AtomicPtr<InstrumentationPolicy> =
    AtomicPtr::new(core::ptr::null_mut());
static RETIRED_INSTRUMENTATION_POLICIES: std::sync::Mutex<Vec<usize>> =
    std::sync::Mutex::new(Vec::new());
static POLICY_GENERATION: AtomicU64 = AtomicU64::new(0);

fn default_kinds() -> u32 {
    let mut kinds = 0;
    if ENABLE_EXEC.load(Ordering::Relaxed) {
        kinds |= INSTRUMENT_EXEC;
    }
    if ENABLE_MEMORY.load(Ordering::Relaxed) {
        kinds |= INSTRUMENT_MEMORY;
    }
    if ENABLE_BRANCH.load(Ordering::Relaxed) {
        kinds |= INSTRUMENT_BRANCH;
    }
    kinds
}

fn default_policy() -> InstrumentationPolicy {
    let (start, end) = trace_range();
    InstrumentationPolicy {
        rules: vec![InstrumentationRule {
            kinds: DEFAULT_INSTRUMENTATION_KINDS.load(Ordering::Relaxed) as u32,
            start,
            end,
            threads: Vec::new(),
        }],
    }
}

fn publish_policy(policy: InstrumentationPolicy) -> u64 {
    let replacement = Box::into_raw(Box::new(policy));
    let old = INSTRUMENTATION_POLICY.swap(replacement, Ordering::AcqRel);
    if !old.is_null() {
        RETIRED_INSTRUMENTATION_POLICIES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(old as usize);
    }
    POLICY_GENERATION.fetch_add(1, Ordering::AcqRel) + 1
}

fn policy() -> &'static InstrumentationPolicy {
    let snapshot = INSTRUMENTATION_POLICY.load(Ordering::Acquire);
    if snapshot.is_null() {
        // configure_from_env publishes before Pin starts the application.
        // This branch is only a defensive identity used by unit tests.
        static EMPTY: InstrumentationPolicy = InstrumentationPolicy { rules: Vec::new() };
        &EMPTY
    } else {
        unsafe { &*snapshot }
    }
}

#[inline]
pub(crate) fn wants_at_instrumentation(address: u64, kind: u32) -> bool {
    policy()
        .rules
        .iter()
        .any(|rule| rule.matches_instrumentation(address, kind))
}

pub(crate) fn wants_instrumentation_kind(kind: u32) -> bool {
    policy()
        .rules
        .iter()
        .any(|rule| rule.kinds & (1 << kind) != 0)
}

pub(crate) fn policy_generation() -> u64 {
    POLICY_GENERATION.load(Ordering::Acquire)
}

#[inline]
fn wants_at_analysis(address: u64, thread_id: u32, kind: u32) -> bool {
    policy()
        .rules
        .iter()
        .any(|rule| rule.matches_analysis(address, thread_id, kind))
}

#[cfg(test)]
mod instrumentation_tests {
    use super::*;

    #[test]
    fn native_rule_keeps_kind_range_and_thread_as_one_conjunction() {
        let rule = InstrumentationRule {
            kinds: INSTRUMENT_EXEC,
            start: 0x1000,
            end: 0x1100,
            threads: vec![3, 7],
        };
        assert!(rule.matches_instrumentation(0x1000, EVENT_EXEC));
        assert!(!rule.matches_instrumentation(0x1100, EVENT_EXEC));
        assert!(!rule.matches_instrumentation(0x1000, EVENT_MEMORY));
        assert!(rule.matches_analysis(0x1080, 7, EVENT_EXEC));
        assert!(!rule.matches_analysis(0x1080, 6, EVENT_EXEC));
    }
}

/// Replaces all Python-owned instrumentation policies with one immutable
/// native snapshot. Each policy keeps its own kind/range/thread conjunction;
/// multiple plugins are combined as a logical OR without widening filters
/// into an accidental cross product.
pub fn set_instrumentation_policies(
    configs: &[InstrumentationPolicyConfig],
) -> Result<u64, PbStatus> {
    let mut rules = Vec::new();
    let mut flush_ranges = Vec::new();
    if configs.is_empty() {
        let default = default_policy();
        for rule in &default.rules {
            flush_ranges.push((rule.start, rule.end));
        }
        let kinds = default.rules.iter().fold(0, |mask, rule| mask | rule.kinds);
        ENABLE_EXEC.store(kinds & INSTRUMENT_EXEC != 0, Ordering::Release);
        ENABLE_MEMORY.store(kinds & INSTRUMENT_MEMORY != 0, Ordering::Release);
        ENABLE_BRANCH.store(kinds & INSTRUMENT_BRANCH != 0, Ordering::Release);
        let generation = publish_policy(default);
        for (start, end) in flush_ranges {
            let status = unsafe { pb_pin_remove_instrumentation_in_range(start, end) };
            if status != PB_OK {
                return Err(status);
            }
        }
        crate::instrumentation_lifecycle::request_routine_snapshot(generation);
        return Ok(generation);
    }

    for config in configs {
        if config.kinds == 0 || config.kinds & !INSTRUMENT_ALL != 0 {
            return Err(PB_ERR_INVALID_ARGUMENT);
        }
        if config.ranges.is_empty()
            || config.ranges.len() > MAX_INSTRUMENTATION_RANGES
            || config.threads.len() > MAX_INSTRUMENTATION_THREADS
        {
            return Err(PB_ERR_INVALID_ARGUMENT);
        }
        let mut threads = config.threads.clone();
        threads.sort_unstable();
        threads.dedup();
        for &(start, end) in &config.ranges {
            if start >= end || rules.len() >= MAX_INSTRUMENTATION_RANGES {
                return Err(PB_ERR_INVALID_ARGUMENT);
            }
            rules.push(InstrumentationRule {
                kinds: config.kinds,
                start,
                end,
                threads: threads.clone(),
            });
            flush_ranges.push((start, end));
        }
    }
    flush_ranges.sort_unstable();
    flush_ranges.dedup();
    let kinds = rules.iter().fold(0, |mask, rule| mask | rule.kinds);
    ENABLE_EXEC.store(kinds & INSTRUMENT_EXEC != 0, Ordering::Release);
    ENABLE_MEMORY.store(kinds & INSTRUMENT_MEMORY != 0, Ordering::Release);
    ENABLE_BRANCH.store(kinds & INSTRUMENT_BRANCH != 0, Ordering::Release);
    let generation = publish_policy(InstrumentationPolicy { rules });
    for (start, end) in flush_ranges {
        let status = unsafe { pb_pin_remove_instrumentation_in_range(start, end) };
        if status != PB_OK {
            return Err(status);
        }
    }
    crate::instrumentation_lifecycle::request_routine_snapshot(generation);
    Ok(generation)
}

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
    DEFAULT_INSTRUMENTATION_KINDS.store(default_kinds() as u64, Ordering::Relaxed);
    publish_policy(default_policy());
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

    let runtime_hook = crate::hooks::any() && crate::hooks::contains(address);
    let (hook_start, hook_end) = hook_range();
    if !runtime_hook && in_range(address, hook_start, hook_end) {
        pb_ins_insert_capture_regs(ins, Some(on_hook_regs), core::ptr::null_mut());
    }

    // Runtime hook points receive a borrowed Pin context. The callback still
    // emits the normal kind-1 event, then applies any precompiled action rules
    // directly to that context before the application instruction continues.
    if runtime_hook {
        crate::hooks::mark_return(address, query_bool(ins, pb_ins_is_ret));
        pb_ins_insert_capture_regs_ctx(ins, Some(on_hook_context), core::ptr::null_mut());
    }

    // Python only publishes immutable mapping rules. If this instruction is
    // in a configured selector range, the fixed ABI primitive rewrites its
    // application memory operands through native tool-register callbacks.
    crate::scripting::instrument_memory_translation(ins, address);

    // record channel: independent of the main trace range and engine
    // enables; insertions are inert until a session arms (analysis-time
    // re-check inside the record callbacks)
    let (rec_lo, rec_hi) = crate::record::instrumentation_range();
    let recording = crate::record::instrumentation_enabled()
        && in_range(address, rec_lo, rec_hi);

    if !policy()
        .rules
        .iter()
        .any(|rule| address >= rule.start && address < rule.end)
    {
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

    // Unlike the runtime `instruction` event, this is emitted once when Pin
    // has fully decoded the instruction for instrumentation. All values are
    // copied now; no borrowed INS/XED object crosses into the Python thread.
    if wants_at_instrumentation(address, EVENT_INSTRUCTION_DECODE) {
        let mut category: i32 = 0;
        let mut extension: i32 = 0;
        let mut opcode: u32 = 0;
        let mut memory_operands: u32 = 0;
        let _ = pb_ins_category(ins, &mut category);
        let _ = pb_ins_extension(ins, &mut extension);
        let _ = pb_ins_opcode(ins, &mut opcode);
        let _ = pb_ins_memory_operand_count(ins, &mut memory_operands);
        let mut flags = 0u64;
        if query_bool(ins, pb_ins_has_fall_through) {
            flags |= 1;
        }
        if is_branch {
            flags |= 1 << 1;
        }
        if is_call {
            flags |= 1 << 2;
        }
        if is_ret {
            flags |= 1 << 3;
        }
        if query_bool(ins, pb_ins_is_syscall) {
            flags |= 1 << 4;
        }
        submit(Event {
            kind: EVENT_INSTRUCTION_DECODE,
            thread_id: PB_INVALID_THREAD_ID,
            address,
            arg0: size,
            arg1: category as u32 as u64,
            arg2: extension as u32 as u64,
            arg3: opcode as u64,
            arg4: memory_operands as u64,
            arg5: flags,
            ..Event::EMPTY
        });
    }

    if ENABLE_EXEC.load(Ordering::Relaxed) && wants_at_instrumentation(address, EVENT_EXEC) {
        pb_ins_insert_exec(ins, Some(on_exec), core::ptr::null_mut());
    }
    if ENABLE_MEMORY.load(Ordering::Relaxed) && wants_at_instrumentation(address, EVENT_MEMORY) {
        pb_ins_insert_memory_operands(ins, Some(on_memory), core::ptr::null_mut());
    }
    if ENABLE_BRANCH.load(Ordering::Relaxed)
        && wants_at_instrumentation(address, EVENT_BRANCH_EDGE)
        && (is_branch || is_call || is_ret)
    {
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
    let event = Event {
        kind: EVENT_HOOK_REGS,
        thread_id,
        address,
        arg0: rcx,
        arg1: rdx,
        arg2: r8,
        arg3: r9,
        ..Event::EMPTY
    };
    if crate::hooks::observation_enabled() {
        crate::observation::submit(event);
    }
    submit(event);
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
    let event = if is_return {
        let mut return_value = 0;
        let _ = pb_pin_get_context_reg(
            context as PbConstContextHandle,
            crate::arch::return_reg(),
            &mut return_value,
        );
        Event {
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
        }
    } else {
        Event {
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
        }
    };
    if crate::hooks::observation_enabled() {
        crate::observation::submit(event);
    }
    submit(event);
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
    if !ENABLE_MEMORY.load(Ordering::Relaxed)
        || !wants_at_analysis(instruction_address, thread_id, EVENT_MEMORY)
    {
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
    if !ENABLE_EXEC.load(Ordering::Relaxed) || !wants_at_analysis(address, thread_id, EVENT_EXEC) {
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
    if !ENABLE_BRANCH.load(Ordering::Relaxed)
        || !wants_at_analysis(address, thread_id, EVENT_BRANCH_EDGE)
    {
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
