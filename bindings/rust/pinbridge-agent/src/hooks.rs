//! Runtime-managed hook point set (HOOK_* ops): addresses that get a
//! register-capture call inserted at instrumentation time. This powers
//! "hook all exports of a module" without spending breakpoint slots.
//! Hits surface as the existing kind-1 (hook_regs) events — this module
//! only decides *where* capture calls get inserted; the analysis callback
//! itself lives in engines.rs (on_hook_regs, fixed four-slot register layout
//! selected by the target architecture).
//!
//! Two copies of the set:
//!   - master: sorted Vec behind a std mutex, touched only by the
//!     query-server/init threads;
//!   - snapshot: immutable boxed sorted Vec behind an atomic pointer, read
//!     lock-free by the instrumentation callback (hot: runs on application
//!     threads during JIT — no locks, no allocation, no I/O there).
//! Writers build a fresh snapshot and swap the pointer; retired snapshots
//! are kept forever (a preempted lock-free reader makes reclamation unsafe).
//! Set changes flush the JIT range of the affected address (same re-JIT
//! technique as bp.rs) so instrumentation re-evaluates it.

use core::ffi::c_void;
use core::sync::atomic::{AtomicI32, AtomicPtr, AtomicUsize, Ordering};
use pinbridge_sys::*;

pub const MAX_HOOK_POINTS: usize = 4096;
pub const MAX_HOOK_RULES: usize = 16384;
/// Virtual PbRegId values used only by HookRule for ABI-aware stack args.
pub const HOOK_STACK_ARG_BASE: PbRegId = 0x8000_0000;

#[inline]
fn stack_arg_index(reg: PbRegId) -> Option<u32> {
    reg.checked_sub(HOOK_STACK_ARG_BASE).filter(|index| *index < 1024)
}

/// A synchronous action evaluated on the application thread at a runtime
/// hook.  `match_reg == PB_REG_INVALID_` means unconditional; otherwise the
/// rule fires when `(register & match_mask) == (match_value & match_mask)`.
/// Virtual `stackN` registers address ABI-aware integer stack arguments.
/// `thread_id == PB_INVALID_THREAD_ID` means all threads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HookRule {
    pub address: u64,
    pub thread_id: PbThreadId,
    pub match_reg: PbRegId,
    pub match_mask: u64,
    pub match_value: u64,
    pub set_reg: PbRegId,
    pub set_value: u64,
}

static MASTER: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
static SNAPSHOT: AtomicPtr<Vec<u64>> = AtomicPtr::new(core::ptr::null_mut());
/// Lock-free mirror of the master length for the on_ins pre-check.
static COUNT: AtomicUsize = AtomicUsize::new(0);
/// Snapshots retired by earlier swaps (as raw addresses; *mut T is not
/// Send), freed on the next update. Touched by the query-server thread only.
static RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

static RULES_MASTER: std::sync::Mutex<Vec<HookRule>> = std::sync::Mutex::new(Vec::new());
static RULES_SNAPSHOT: AtomicPtr<Vec<HookRule>> = AtomicPtr::new(core::ptr::null_mut());
static RULES_RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
static RULES_READERS: AtomicUsize = AtomicUsize::new(0);
static RETURNS_MASTER: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
static RETURNS_SNAPSHOT: AtomicPtr<Vec<u64>> = AtomicPtr::new(core::ptr::null_mut());
static RETURNS_RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
static ACTION_TLS_KEY: AtomicI32 = AtomicI32::new(PB_INVALID_TLS_KEY);

fn lock_master() -> std::sync::MutexGuard<'static, Vec<u64>> {
    MASTER.lock().unwrap_or_else(|e| e.into_inner())
}

/// Publishes a fresh snapshot of `master` (query-server thread only).
fn publish(master: &[u64]) {
    let snapshot = Box::new(master.to_vec());
    let old = SNAPSHOT.swap(Box::into_raw(snapshot), Ordering::AcqRel);
    COUNT.store(master.len(), Ordering::Release);
    // Retired snapshots are NEVER freed: readers are analysis/instrumentation
    // callbacks that load the pointer lock-free and can be preempted (or
    // suspended by the breaker) between the load and the read for an
    // unbounded time, so no reclamation point is provably safe without
    // hazard slots. Hook updates are rare and user-driven (each snapshot is
    // at most 4096 u64s); retiring permanently trades a bounded leak for
    // freedom from use-after-free reads. ("Freed on the next update" was a
    // real UAF window: two quick swaps freed a snapshot a preempted reader
    // could still hold.)
    if !old.is_null() {
        let mut retired = RETIRED.lock().unwrap_or_else(|e| e.into_inner());
        retired.push(old as usize);
    }
}

/// Fast pre-check for on_ins: skip the pointer chase when no hooks exist.
#[inline]
pub fn any() -> bool {
    COUNT.load(Ordering::Acquire) > 0
}

/// Lock-free membership check for the instrumentation callback.
#[inline]
pub fn contains(address: u64) -> bool {
    let snapshot = SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() {
        return false;
    }
    unsafe { &*snapshot }.binary_search(&address).is_ok()
}

/// Forces re-JIT of one address so on_ins re-evaluates it.
fn flush(address: u64) {
    unsafe {
        pb_pin_remove_instrumentation_in_range(address, address + 15);
    }
}

/// Adds a hook point. Idempotent: an already-hooked address returns true.
/// Returns false when the set is full (MAX_HOOK_POINTS).
pub fn set(address: u64) -> bool {
    let mut master = lock_master();
    if master.binary_search(&address).is_ok() {
        return true;
    }
    if master.len() >= MAX_HOOK_POINTS {
        return false;
    }
    master.push(address);
    master.sort_unstable();
    publish(&master);
    drop(master);
    flush(address);
    true
}

/// Removes a hook point (no-op when absent).
pub fn remove(address: u64) {
    let mut master = lock_master();
    if let Ok(index) = master.binary_search(&address) {
        master.remove(index);
        publish(&master);
        drop(master);
        flush(address);
    }
}

/// Removes all hook points, flushing each so stale capture calls die.
pub fn clear() {
    let mut master = lock_master();
    if master.is_empty() {
        return;
    }
    let addresses = master.clone();
    master.clear();
    publish(&master);
    drop(master);
    for address in addresses {
        flush(address);
    }
}

/// Current hook points, sorted.
pub fn list() -> Vec<u64> {
    lock_master().clone()
}

fn publish_rules(rules: &[HookRule]) {
    let snapshot = Box::new(rules.to_vec());
    let old = RULES_SNAPSHOT.swap(Box::into_raw(snapshot), Ordering::AcqRel);
    // Analysis callbacks announce a read-side critical section. Reclaim old
    // copies only at a quiescent publication; otherwise retain them until a
    // later update observes no readers.
    if !old.is_null() {
        let mut retired = RULES_RETIRED.lock().unwrap_or_else(|e| e.into_inner());
        retired.push(old as usize);
        if RULES_READERS.load(Ordering::SeqCst) == 0 {
            for address in retired.drain(..) {
                unsafe { drop(Box::from_raw(address as *mut Vec<HookRule>)) };
            }
        }
    }
}

struct RulesReadGuard;

impl RulesReadGuard {
    fn new() -> Self {
        RULES_READERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for RulesReadGuard {
    fn drop(&mut self) {
        RULES_READERS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Records whether a discovered hook point is a return instruction. This is
/// populated during Pin instrumentation, including points outside the trace
/// range, so return events remain reliable when tracing is narrowed.
pub fn mark_return(address: u64, is_return: bool) {
    let mut addresses = RETURNS_MASTER.lock().unwrap_or_else(|e| e.into_inner());
    let present = addresses.binary_search(&address).is_ok();
    if is_return && !present {
        addresses.push(address);
        addresses.sort_unstable();
    } else if !is_return && present {
        if let Ok(index) = addresses.binary_search(&address) {
            addresses.remove(index);
        }
    } else {
        return;
    }
    let snapshot = Box::new(addresses.clone());
    let old = RETURNS_SNAPSHOT.swap(Box::into_raw(snapshot), Ordering::AcqRel);
    if !old.is_null() {
        let mut retired = RETURNS_RETIRED.lock().unwrap_or_else(|e| e.into_inner());
        retired.push(old as usize);
    }
}

#[inline]
pub fn is_return(address: u64) -> bool {
    let snapshot = RETURNS_SNAPSHOT.load(Ordering::Acquire);
    !snapshot.is_null() && unsafe { (&*snapshot).binary_search(&address).is_ok() }
}

/// Adds or replaces one synchronous action rule. The hook address itself must
/// already be armed with `hook_set`/`hookall`; changing rules needs no re-JIT.
pub fn set_rule(rule: HookRule) -> bool {
    if rule.address == 0
        || rule.set_reg == PB_REG_INVALID_
        || (rule.match_reg != PB_REG_INVALID_ && rule.match_mask == 0)
    {
        return false;
    }
    let mut rules = RULES_MASTER.lock().unwrap_or_else(|e| e.into_inner());
    let same = |item: &HookRule| {
        item.address == rule.address
            && item.thread_id == rule.thread_id
            && item.match_reg == rule.match_reg
            && item.match_mask == rule.match_mask
            && item.match_value == rule.match_value
            && item.set_reg == rule.set_reg
    };
    if let Some(existing) = rules.iter_mut().find(|item| same(item)) {
        existing.set_value = rule.set_value;
    } else {
        if rules.len() >= MAX_HOOK_RULES {
            return false;
        }
        rules.push(rule);
    }
    rules.sort_unstable_by_key(|item| item.address);
    publish_rules(&rules);
    true
}

/// Removes all synchronous action rules. Hook points remain armed.
pub fn clear_rules() {
    let mut rules = RULES_MASTER.lock().unwrap_or_else(|e| e.into_inner());
    if rules.is_empty() {
        return;
    }
    rules.clear();
    publish_rules(&rules);
}

/// Allocates the Pin TLS slot used to suppress the one callback replay caused
/// by `pb_pin_execute_at` after a context write.
pub fn init() -> PbStatus {
    let mut key = PB_INVALID_TLS_KEY;
    let status = unsafe { pb_pin_create_thread_data_key(None, &mut key) };
    if status == PB_OK {
        ACTION_TLS_KEY.store(key, Ordering::Release);
    }
    status
}

#[inline]
unsafe fn set_replay_guard(thread_id: PbThreadId, address: u64) -> bool {
    let key = ACTION_TLS_KEY.load(Ordering::Acquire);
    if key == PB_INVALID_TLS_KEY {
        return false;
    }
    let encoded = address.checked_add(1).unwrap_or(address) as usize as *const c_void;
    let mut set = 0;
    pb_pin_set_thread_data(key, encoded, thread_id, &mut set) == PB_OK && set != 0
}

/// Consumes the guard left before `pb_pin_execute_at`; true means this is the
/// replayed instruction and its action must not be applied a second time.
pub unsafe fn take_replay_guard(thread_id: PbThreadId, address: u64) -> bool {
    let key = ACTION_TLS_KEY.load(Ordering::Acquire);
    if key == PB_INVALID_TLS_KEY {
        return false;
    }
    let mut data = core::ptr::null_mut();
    if pb_pin_get_thread_data(key, thread_id, &mut data) != PB_OK || data.is_null() {
        return false;
    }
    let expected = address.checked_add(1).unwrap_or(address) as usize;
    if data as usize != expected {
        return false;
    }
    let mut cleared = 0;
    let _ = pb_pin_set_thread_data(key, core::ptr::null(), thread_id, &mut cleared);
    true
}

unsafe fn clear_replay_guard(thread_id: PbThreadId) {
    let key = ACTION_TLS_KEY.load(Ordering::Acquire);
    if key != PB_INVALID_TLS_KEY {
        let mut cleared = 0;
        let _ = pb_pin_set_thread_data(key, core::ptr::null(), thread_id, &mut cleared);
    }
}

/// Applies all matching rules in the current Pin analysis callback. Returns
/// how many context writes succeeded. No locks, allocation, I/O, or Python
/// calls occur on this hot path.
pub unsafe fn apply_rules(
    address: u64,
    thread_id: PbThreadId,
    context: PbContextHandle,
    captured_args: [u64; 4],
) -> u32 {
    let _read_guard = RulesReadGuard::new();
    let snapshot = RULES_SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() || context.is_null() {
        return 0;
    }
    let rules = &*snapshot;
    let mut index = rules.partition_point(|item| item.address < address);
    let mut writes = 0;
    while index < rules.len() && rules[index].address == address {
        let rule = rules[index];
        index += 1;
        if rule.thread_id != PB_INVALID_THREAD_ID && rule.thread_id != thread_id {
            continue;
        }
        // ABI stack offsets describe function-entry layout. A return hook is
        // never at function entry, so applying a stack rule there could read
        // or overwrite an unrelated caller slot.
        if is_return(address)
            && (stack_arg_index(rule.match_reg).is_some()
                || stack_arg_index(rule.set_reg).is_some())
        {
            continue;
        }
        if rule.match_reg != PB_REG_INVALID_ {
            let current = crate::arch::hook_arg_regs()
                .iter()
                .position(|reg| *reg == rule.match_reg)
                .map(|index| captured_args[index])
                .or_else(|| {
                    if let Some(index) = stack_arg_index(rule.match_reg) {
                        let mut value = 0;
                        return (pb_pin_get_context_stack_arg(
                            context as PbConstContextHandle,
                            index,
                            &mut value,
                        ) == PB_OK)
                            .then_some(value);
                    }
                    let mut value = 0;
                    (pb_pin_get_context_reg(
                        context as PbConstContextHandle,
                        rule.match_reg,
                        &mut value,
                    ) == PB_OK)
                        .then_some(value)
                });
            if current
                .map(|value| {
                    (value & rule.match_mask) == (rule.match_value & rule.match_mask)
                })
                != Some(true)
            {
                continue;
            }
        }
        let applied = if let Some(index) = stack_arg_index(rule.set_reg) {
            pb_pin_set_context_stack_arg(context, index, rule.set_value) == PB_OK
        } else {
            pb_pin_set_context_reg(context, rule.set_reg, rule.set_value) == PB_OK
        };
        if applied {
            writes += 1;
        }
    }
    writes
}

/// Marks the current thread for one callback replay and transfers control to
/// the modified context. Returns false if the TLS guard could not be armed.
pub unsafe fn execute_modified_context(
    thread_id: PbThreadId,
    address: u64,
    context: PbContextHandle,
) -> bool {
    if !set_replay_guard(thread_id, address) {
        return false;
    }
    let status = pb_pin_execute_at(context as PbConstContextHandle);
    if status != PB_OK {
        clear_replay_guard(thread_id);
        false
    } else {
        true
    }
}
