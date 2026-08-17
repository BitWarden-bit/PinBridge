//! Generic execution-range traps for script-directed debugger strategies.
//!
//! A trap is a half-open application address range. Pin inserts one native
//! analysis callback before every instruction discovered in a live range.
//! The first matching application thread is redirected through the normal
//! exact-stop park path before that instruction executes. Python never runs
//! on the instrumentation or analysis path; it receives `execution.trap`
//! only after the breaker has stopped all application threads.

use crate::event::{Event, EVENT_EXECUTION_TRAP};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

pub const MAX_EXECUTION_TRAPS: usize = 64;
const ANY_THREAD: u32 = PB_INVALID_THREAD_ID;

struct Slot {
    used: bool,
    id: u32,
    // The script/control thread mutates slot metadata while application
    // analysis callbacks read it. Keep hot-path fields atomic: the Pin mutex
    // protects table ownership, but must never be taken for every executed
    // instruction.
    start: AtomicU64,
    end: AtomicU64,
    thread_id: AtomicU32,
    active: AtomicBool,
    one_shot: AtomicBool,
    hits: AtomicU64,
}

impl Slot {
    const fn empty() -> Self {
        Self {
            used: false,
            id: 0,
            start: AtomicU64::new(0),
            end: AtomicU64::new(0),
            thread_id: AtomicU32::new(ANY_THREAD),
            active: AtomicBool::new(false),
            one_shot: AtomicBool::new(false),
            hits: AtomicU64::new(0),
        }
    }
}

static mut SLOTS: [Slot; MAX_EXECUTION_TRAPS] = [const { Slot::empty() }; MAX_EXECUTION_TRAPS];
static TABLE_MUTEX: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_COUNT: AtomicUsize = AtomicUsize::new(0);
static NEXT_ID: AtomicU32 = AtomicU32::new(1);
static BOUND_START: AtomicU64 = AtomicU64::new(u64::MAX);
static BOUND_END: AtomicU64 = AtomicU64::new(0);

// Only one exact stop may be in flight. The winning analysis callback records
// every slot matching that same (thread,address), then asks bp's breaker to
// stop the world. The breaker publishes those records after STOPPED=true.
static STOP_CLAIMED: AtomicBool = AtomicBool::new(false);
static PENDING_MASK: AtomicU64 = AtomicU64::new(0);
static PENDING_TID: AtomicU32 = AtomicU32::new(ANY_THREAD);
static PENDING_ADDRESS: AtomicU64 = AtomicU64::new(0);

fn lock_table() -> bool {
    let mutex = TABLE_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    !mutex.is_null() && unsafe { pb_pin_mutex_lock(mutex) == PB_OK }
}

fn unlock_table() {
    let mutex = TABLE_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if !mutex.is_null() {
        unsafe {
            pb_pin_mutex_unlock(mutex);
        }
    }
}

#[inline]
fn matches(slot: &Slot, address: u64, thread_id: u32) -> bool {
    if !slot.active.load(Ordering::Acquire) {
        return false;
    }
    let start = slot.start.load(Ordering::Relaxed);
    let end = slot.end.load(Ordering::Relaxed);
    let filter = slot.thread_id.load(Ordering::Relaxed);
    address >= start && address < end && (filter == ANY_THREAD || filter == thread_id)
}

pub fn init() -> PbStatus {
    unsafe {
        let mut mutex: PbMutexHandle = core::ptr::null_mut();
        let status = pb_pin_mutex_init(&mut mutex);
        if status == PB_OK {
            TABLE_MUTEX.store(mutex as usize, Ordering::Release);
        }
        status
    }
}

pub fn any_active() -> bool {
    ACTIVE_COUNT.load(Ordering::Relaxed) != 0
}

unsafe fn recompute_bounds_locked() {
    let mut start = u64::MAX;
    let mut end = 0u64;
    for slot in (*core::ptr::addr_of!(SLOTS)).iter() {
        if slot.used && slot.active.load(Ordering::Relaxed) {
            start = start.min(slot.start.load(Ordering::Relaxed));
            end = end.max(slot.end.load(Ordering::Relaxed));
        }
    }
    BOUND_START.store(start, Ordering::Release);
    BOUND_END.store(end, Ordering::Release);
}

/// Instrumentation-time range check. The Pin mutex protects non-atomic slot
/// metadata while a control thread creates or retires slots.
pub fn wants(address: u64) -> bool {
    let bound_start = BOUND_START.load(Ordering::Acquire);
    let bound_end = BOUND_END.load(Ordering::Acquire);
    if !any_active() || address < bound_start || address >= bound_end || !lock_table() {
        return false;
    }
    let wanted = unsafe {
        (*core::ptr::addr_of!(SLOTS)).iter().any(|slot| {
            slot.used
                && slot.active.load(Ordering::Relaxed)
                && address >= slot.start.load(Ordering::Relaxed)
                && address < slot.end.load(Ordering::Relaxed)
        })
    };
    unlock_table();
    wanted
}

/// Adds the fixed native execution callback to one decoded application
/// instruction. The callback receives Pin's writable application context, so
/// the existing exact-stop engine can preserve the pre-instruction RIP.
pub unsafe fn instrument(ins: PbInsHandle) {
    pb_ins_insert_capture_regs_ctx(ins, Some(on_execute), core::ptr::null_mut());
}

unsafe extern "C" fn on_execute(
    address: u64,
    thread_id: u32,
    context: PbContextHandle,
    _arg0: u64,
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _user_data: *mut c_void,
) {
    if !any_active()
        || STOP_CLAIMED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return;
    }

    let mut mask = 0u64;
    let slots = &*core::ptr::addr_of!(SLOTS);
    for (index, slot) in slots.iter().enumerate() {
        if !matches(slot, address, thread_id) {
            continue;
        }
        if slot.one_shot.load(Ordering::Relaxed)
            && slot
                .active
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            continue;
        }
        if slot.one_shot.load(Ordering::Relaxed) {
            ACTIVE_COUNT.fetch_sub(1, Ordering::Relaxed);
        }
        slot.hits.fetch_add(1, Ordering::Relaxed);
        mask |= 1u64 << index;
    }

    if mask == 0 {
        STOP_CLAIMED.store(false, Ordering::Release);
        return;
    }
    PENDING_TID.store(thread_id, Ordering::Release);
    PENDING_ADDRESS.store(address, Ordering::Release);
    PENDING_MASK.store(mask, Ordering::Release);
    crate::bp::exact_stop(context, thread_id, address);
}

/// Breaker-thread hook. Called only after all application threads are stopped
/// and their saved contexts are stable, so Python can safely read registers
/// as soon as it receives the priority event.
pub fn publish_stopped(stop_generation: u64) {
    if PENDING_MASK.load(Ordering::Acquire) == 0 || !lock_table() {
        return;
    }
    let mask = PENDING_MASK.swap(0, Ordering::AcqRel);
    let thread_id = PENDING_TID.load(Ordering::Acquire);
    let address = PENDING_ADDRESS.load(Ordering::Acquire);
    let mut events = [Event::EMPTY; MAX_EXECUTION_TRAPS];
    let mut count = 0usize;
    unsafe {
        let slots = &*core::ptr::addr_of!(SLOTS);
        for (index, slot) in slots.iter().enumerate() {
            if mask & (1u64 << index) == 0 {
                continue;
            }
            events[count] = Event {
                kind: EVENT_EXECUTION_TRAP,
                thread_id,
                address,
                arg0: slot.id as u64,
                arg1: slot.start.load(Ordering::Acquire),
                arg2: slot.end.load(Ordering::Acquire),
                arg3: slot.hits.load(Ordering::Acquire),
                arg4: stop_generation,
                arg5: (slot.one_shot.load(Ordering::Acquire) as u64)
                    | ((slot.thread_id.load(Ordering::Acquire) != ANY_THREAD) as u64) << 1,
                arg6: slot.thread_id.load(Ordering::Acquire) as u64,
                ..Event::EMPTY
            };
            count += 1;
        }
    }
    unlock_table();
    for event in events.into_iter().take(count) {
        crate::priority::submit(event);
    }
}

/// Resume completes the stop transaction. Persistent traps may claim their
/// next execution only after the previous stopped world has been released.
pub fn on_resume() {
    PENDING_MASK.store(0, Ordering::Release);
    PENDING_TID.store(ANY_THREAD, Ordering::Release);
    PENDING_ADDRESS.store(0, Ordering::Release);
    STOP_CLAIMED.store(false, Ordering::Release);
}

pub fn set(start: u64, end: u64, one_shot: bool, thread_id: Option<u32>) -> Result<u32, PbStatus> {
    if start == 0 || start >= end || thread_id == Some(ANY_THREAD) {
        return Err(PB_ERR_INVALID_ARGUMENT);
    }
    if crate::pin_session::is_probe_mode() {
        return Err(PB_ERR_UNSUPPORTED);
    }
    if !lock_table() {
        return Err(PB_ERR_INVALID_STATE);
    }
    let pending = PENDING_MASK.load(Ordering::Acquire);
    let mut result = Err(PB_ERR_OUT_OF_MEMORY);
    unsafe {
        let slots = &mut *core::ptr::addr_of_mut!(SLOTS);
        if let Some((_index, slot)) = slots.iter_mut().enumerate().find(|(index, slot)| {
            (!slot.used || !slot.active.load(Ordering::Relaxed)) && pending & (1u64 << index) == 0
        }) {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            slot.used = true;
            slot.id = id;
            slot.start.store(start, Ordering::Relaxed);
            slot.end.store(end, Ordering::Relaxed);
            slot.thread_id
                .store(thread_id.unwrap_or(ANY_THREAD), Ordering::Relaxed);
            slot.hits.store(0, Ordering::Relaxed);
            slot.one_shot.store(one_shot, Ordering::Relaxed);
            slot.active.store(true, Ordering::Release);
            ACTIVE_COUNT.fetch_add(1, Ordering::Relaxed);
            recompute_bounds_locked();
            result = Ok(id);
        }
    }
    unlock_table();

    if result.is_ok() {
        unsafe {
            let status = pb_pin_remove_instrumentation_in_range(start, end);
            if status != PB_OK {
                if let Ok(id) = result {
                    let _ = remove(id);
                }
                return Err(status);
            }
        }
    }
    result
}

pub fn remove(id: u32) -> bool {
    if !lock_table() {
        return false;
    }
    let mut removed = None;
    unsafe {
        let slots = &mut *core::ptr::addr_of_mut!(SLOTS);
        if let Some((index, slot)) = slots
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.used && slot.id == id)
        {
            if slot.active.swap(false, Ordering::AcqRel) {
                ACTIVE_COUNT.fetch_sub(1, Ordering::Relaxed);
            }
            removed = Some((
                index,
                slot.start.load(Ordering::Relaxed),
                slot.end.load(Ordering::Relaxed),
            ));
            if PENDING_MASK.load(Ordering::Acquire) & (1u64 << index) == 0 {
                slot.used = false;
            }
            recompute_bounds_locked();
        }
    }
    unlock_table();
    if let Some((_index, start, end)) = removed {
        unsafe {
            let _ = pb_pin_remove_instrumentation_in_range(start, end);
        }
        true
    } else {
        false
    }
}

/// Active trap table for diagnostics and tests.
pub fn list() -> Vec<(u32, u64, u64, Option<u32>, bool, u64)> {
    let mut out = Vec::with_capacity(MAX_EXECUTION_TRAPS);
    if !lock_table() {
        return out;
    }
    unsafe {
        for slot in (*core::ptr::addr_of!(SLOTS)).iter() {
            if slot.used && slot.active.load(Ordering::Relaxed) {
                out.push((
                    slot.id,
                    slot.start.load(Ordering::Relaxed),
                    slot.end.load(Ordering::Relaxed),
                    (slot.thread_id.load(Ordering::Relaxed) != ANY_THREAD)
                        .then_some(slot.thread_id.load(Ordering::Relaxed)),
                    slot.one_shot.load(Ordering::Relaxed),
                    slot.hits.load(Ordering::Relaxed),
                ));
            }
        }
    }
    unlock_table();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_open_ranges_and_thread_filters_are_exact() {
        let slot = Slot {
            used: true,
            id: 1,
            start: AtomicU64::new(0x1000),
            end: AtomicU64::new(0x2000),
            thread_id: AtomicU32::new(7),
            active: AtomicBool::new(true),
            one_shot: AtomicBool::new(true),
            hits: AtomicU64::new(0),
        };
        assert!(matches(&slot, 0x1000, 7));
        assert!(matches(&slot, 0x1fff, 7));
        assert!(!matches(&slot, 0x2000, 7));
        assert!(!matches(&slot, 0x1000, 8));
    }
}
