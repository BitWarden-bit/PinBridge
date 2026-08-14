//! Syscall entry/exit capture engine. Registered once at startup; runtime
//! gating is a single atomic check inside the callback (no re-JIT needed).
//!
//! Optional number filter (SYSCALL_FILTER op): an immutable snapshot behind
//! an atomic pointer. The analysis callbacks only ever *read* the pointer
//! (lock-free, no allocation, no std locks); the query-server thread builds
//! a new boxed snapshot and swaps it in. Retired snapshots are never freed:
//! a callback preempted between the lock-free load and the bitmap read would
//! otherwise chase freed memory (no safe reclamation point exists without
//! hazard slots; snapshots are 516 bytes and updates are rare). A null
//! pointer means "mode all" (record everything).

use crate::event::{Event, EVENT_SYSCALL};
use crate::ring::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use pinbridge_sys::*;

static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Windows syscall numbers are < 0x1000: one bit each, 512 bytes total.
const SYSCALL_NUMBER_LIMIT: usize = 4096;

struct FilterSnapshot {
    mode: u8, // 0 = all, 1 = only listed numbers
    bitmap: [u64; SYSCALL_NUMBER_LIMIT / 64],
}

static FILTER: AtomicPtr<FilterSnapshot> = AtomicPtr::new(core::ptr::null_mut());
/// Snapshots retired by earlier swaps (as raw addresses; *mut T is not
/// Send). NEVER freed — see set_filter for why reclamation is unsafe.
static RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

/// Lock-free hot-path check: may this syscall number be recorded?
#[inline]
fn number_allowed(number: u64) -> bool {
    let snapshot = FILTER.load(Ordering::Acquire);
    if snapshot.is_null() {
        return true;
    }
    let snapshot = unsafe { &*snapshot };
    if snapshot.mode == 0 {
        return true;
    }
    if number as usize >= SYSCALL_NUMBER_LIMIT {
        return false; // not representable in the bitmap: treat as unlisted
    }
    snapshot.bitmap[number as usize / 64] & (1u64 << (number % 64)) != 0
}

/// Installs a new filter (query-server thread). mode 0 = record all,
/// mode 1 = record only listed numbers. Numbers >= 0x1000 are ignored
/// (unrepresentable in the bitmap).
pub fn set_filter(mode: u8, numbers: &[u32]) {
    let mut snapshot = Box::new(FilterSnapshot {
        mode,
        bitmap: [0; SYSCALL_NUMBER_LIMIT / 64],
    });
    if mode != 0 {
        for &number in numbers {
            if (number as usize) < SYSCALL_NUMBER_LIMIT {
                snapshot.bitmap[number as usize / 64] |= 1u64 << (number % 64);
            }
        }
    }
    let old = FILTER.swap(Box::into_raw(snapshot), Ordering::AcqRel);
    // Retired snapshots are NEVER freed: the analysis callbacks read the
    // pointer lock-free and can be preempted (or suspended by the breaker)
    // between the load and the bitmap read for an unbounded time, so no
    // reclamation point is provably safe without hazard slots. Filter updates
    // are rare (plugin load/unload, UI command) and each snapshot is 516
    // bytes; retiring permanently trades a bounded leak for freedom from
    // use-after-free reads. ("Freed on the next update" was a real UAF
    // window: two quick swaps freed a snapshot a preempted reader could
    // still hold.)
    if !old.is_null() {
        let mut retired = RETIRED.lock().unwrap_or_else(|e| e.into_inner());
        retired.push(old as usize);
    }
}

unsafe extern "C" fn on_entry(
    thread_id: PbThreadId,
    context: PbContextHandle,
    standard: PbSyscallStandard,
    _user_data: *mut c_void,
) {
    if !enabled() {
        return;
    }
    let mut number: u64 = 0;
    let mut args = [0u64; 6];
    pb_pin_get_syscall_number(context, standard, &mut number);
    if !number_allowed(number) {
        return;
    }
    for (index, slot) in args.iter_mut().enumerate() {
        pb_pin_get_syscall_argument(context, standard, index as u32, slot);
    }
    submit(Event {
        kind: EVENT_SYSCALL,
        thread_id,
        arg0: number,
        arg1: 0, // entry
        arg2: args[0],
        arg3: args[1],
        arg4: args[2],
        arg5: args[3],
        arg6: args[4],
        arg7: args[5],
        ..Event::EMPTY
    });
}

unsafe extern "C" fn on_exit(
    thread_id: PbThreadId,
    context: PbContextHandle,
    standard: PbSyscallStandard,
    _user_data: *mut c_void,
) {
    if !enabled() {
        return;
    }
    let mut number: u64 = 0;
    let mut return_value: u64 = 0;
    let mut errno: u64 = 0;
    pb_pin_get_syscall_number(context, standard, &mut number);
    if !number_allowed(number) {
        return;
    }
    pb_pin_get_syscall_return(context, standard, &mut return_value);
    pb_pin_get_syscall_errno(context, standard, &mut errno);
    submit(Event {
        kind: EVENT_SYSCALL,
        thread_id,
        arg0: number,
        arg1: 1, // exit
        arg3: return_value,
        arg4: errno,
        ..Event::EMPTY
    });
}

pub fn register() -> PbStatus {
    let mut handle_entry = PbCallbackHandle { opaque: 0 };
    let mut handle_exit = PbCallbackHandle { opaque: 0 };
    unsafe {
        let status = pb_pin_add_syscall_entry_function(
            Some(on_entry),
            core::ptr::null_mut(),
            &mut handle_entry,
        );
        if status != PB_OK {
            return status;
        }
        pb_pin_add_syscall_exit_function(
            Some(on_exit),
            core::ptr::null_mut(),
            &mut handle_exit,
        )
    }
}
