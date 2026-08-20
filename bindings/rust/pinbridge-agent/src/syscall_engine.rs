//! Syscall entry/exit capture engine. Registered once at startup; runtime
//! gating is a single atomic check inside the callback (no re-JIT needed).
//!
//! Optional number/address filter (SYSCALL_FILTER op): an immutable snapshot behind
//! an atomic pointer. The analysis callbacks only ever *read* the pointer
//! (lock-free, no allocation, no std locks); the query-server thread builds
//! a new boxed snapshot and swaps it in. Retired snapshots are never freed:
//! a callback preempted between the lock-free load and the bitmap read would
//! otherwise chase freed memory (no safe reclamation point exists without
//! hazard slots; snapshots are small and updates are rare). A null
//! pointer means "mode all, all addresses" (record everything).

use crate::event::{Event, EVENT_SYSCALL};
use crate::ring::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU64, Ordering};
use pinbridge_sys::*;

static ENABLED: AtomicBool = AtomicBool::new(true);
static OBSERVATION_ENABLED: AtomicBool = AtomicBool::new(false);
static SYSCALL_TLS_KEY: AtomicI32 = AtomicI32::new(PB_INVALID_TLS_KEY);
static SYSCALL_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    SYSCALL_GENERATION.load(Ordering::Acquire)
}

#[inline]
fn next_generation() -> u64 {
    SYSCALL_GENERATION.fetch_add(1, Ordering::AcqRel) + 1
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_observation_enabled(on: bool) {
    OBSERVATION_ENABLED.store(on, Ordering::Release);
}

#[inline]
fn observation_enabled() -> bool {
    OBSERVATION_ENABLED.load(Ordering::Acquire)
}

/// Windows syscall numbers are < 0x1000: one bit each, 512 bytes total.
pub const SYSCALL_NUMBER_LIMIT: usize = 4096;

/// Pin's IA-32 Windows syscall value includes the service-class bits (for
/// example `0x3000f`); the low 12 bits are the native syscall ordinal. Keep
/// the public event/filter number in the same 0..0xfff space as x64.
#[inline]
fn canonical_number(number: u64) -> u64 {
    if crate::arch::is_32() {
        number & (SYSCALL_NUMBER_LIMIT as u64 - 1)
    } else {
        number
    }
}

struct FilterSnapshot {
    mode: u8, // 0 = all, 1 = only listed numbers
    bitmap: [u64; SYSCALL_NUMBER_LIMIT / 64],
    // [scope_start, scope_end), or 0/0 for every caller address.
    scope_start: u64,
    scope_end: u64,
}

static FILTER: AtomicPtr<FilterSnapshot> = AtomicPtr::new(core::ptr::null_mut());
/// Snapshots retired by earlier swaps (as raw addresses; *mut T is not
/// Send). NEVER freed — see set_filter for why reclamation is unsafe.
static RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

/// Lock-free hot-path check: may this syscall number at this caller address be
/// recorded? The syscall-entry context supplies the user-mode syscall site;
/// exit callbacks deliberately reuse the entry decision stored in Pin TLS.
#[inline]
fn capture_allowed(number: u64, instruction_pointer: u64) -> bool {
    let snapshot = FILTER.load(Ordering::Acquire);
    if snapshot.is_null() {
        return true;
    }
    let snapshot = unsafe { &*snapshot };
    if snapshot.scope_end != 0
        && (instruction_pointer < snapshot.scope_start || instruction_pointer >= snapshot.scope_end)
    {
        return false;
    }
    if snapshot.mode != 0 {
        if number as usize >= SYSCALL_NUMBER_LIMIT {
            return false; // not representable in the bitmap: treat as unlisted
        }
        return snapshot.bitmap[number as usize / 64] & (1u64 << (number % 64)) != 0;
    }
    true
}

/// Pin does not guarantee that the syscall-number register still contains the
/// number in the exit context (on Windows it commonly contains the return
/// value). Store `number + 1` as an opaque, non-null Pin TLS value at entry;
/// syscall numbers are small integers, so this needs no allocation.
#[inline]
fn encode_syscall_state(number: u64, capture: bool) -> Option<*const c_void> {
    let encoded = number
        .checked_mul(2)?
        .checked_add(capture as u64)?
        .checked_add(1)?;
    let encoded = usize::try_from(encoded).ok()?;
    Some(encoded as *const c_void)
}

#[inline]
fn decode_syscall_state(data: *mut c_void) -> Option<(u64, bool)> {
    let encoded = data as usize;
    if encoded == 0 {
        None
    } else {
        let value = (encoded - 1) as u64;
        Some((value >> 1, value & 1 != 0))
    }
}

#[inline]
unsafe fn remember_syscall_state(thread_id: PbThreadId, number: u64, capture: bool) -> bool {
    let key = SYSCALL_TLS_KEY.load(Ordering::Acquire);
    let Some(data) = encode_syscall_state(number, capture) else {
        return false;
    };
    if key == PB_INVALID_TLS_KEY {
        return false;
    }
    let mut set = 0u8;
    pb_pin_set_thread_data(key, data, thread_id, &mut set) == PB_OK && set != 0
}

#[inline]
unsafe fn take_syscall_state(thread_id: PbThreadId) -> Option<(u64, bool)> {
    let key = SYSCALL_TLS_KEY.load(Ordering::Acquire);
    if key == PB_INVALID_TLS_KEY {
        return None;
    }
    let mut data = core::ptr::null_mut();
    if pb_pin_get_thread_data(key, thread_id, &mut data) != PB_OK || data.is_null() {
        return None;
    }

    // Clear the slot before emitting the exit event so a malformed callback
    // sequence cannot reuse a stale syscall number.
    let mut cleared = 0u8;
    let _ = pb_pin_set_thread_data(key, core::ptr::null(), thread_id, &mut cleared);
    decode_syscall_state(data)
}

/// Installs a new filter (query-server thread). mode 0 = record all,
/// mode 1 = record only listed numbers. Numbers >= 0x1000 are ignored
/// (unrepresentable in the bitmap).
pub fn set_filter(mode: u8, numbers: &[u32], scope_start: u64, scope_end: u64) {
    let mut snapshot = Box::new(FilterSnapshot {
        mode,
        bitmap: [0; SYSCALL_NUMBER_LIMIT / 64],
        scope_start,
        scope_end,
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
    // are rare (plugin load/unload, UI command) and each snapshot is small;
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
    // Clear any unpaired value before beginning a new entry/exit pair.
    let _ = take_syscall_state(thread_id);
    let mut number: u64 = 0;
    let mut instruction_pointer = 0u64;
    let mut args = [0u64; 6];
    pb_pin_get_syscall_number(context, standard, &mut number);
    let _ = pb_pin_get_context_reg(
        context as PbConstContextHandle,
        crate::arch::instr_ptr_reg(),
        &mut instruction_pointer,
    );
    number = canonical_number(number);
    for (index, slot) in args.iter_mut().enumerate() {
        pb_pin_get_syscall_argument(context, standard, index as u32, slot);
    }
    if let Some(response) = crate::sync_intercept::decide_syscall(
        crate::sync_intercept::SYSCALL_ENTRY,
        thread_id,
        context,
        standard,
        number,
        args,
        0,
        0,
    ) {
        crate::sync_intercept::apply_syscall_response(
            context,
            standard,
            crate::sync_intercept::SYSCALL_ENTRY,
            &response,
        );
    }
    let capture = enabled() && capture_allowed(number, instruction_pointer);
    let intercept_exit =
        crate::sync_intercept::syscall_interested(crate::sync_intercept::SYSCALL_EXIT, number);
    if (capture || intercept_exit) && !remember_syscall_state(thread_id, number, capture) {
        return;
    }
    if !capture {
        return;
    }
    let event = Event {
        kind: EVENT_SYSCALL,
        thread_id,
        // Syscall entry needs all eight argument slots. The otherwise-unused
        // generic address field carries the shared dual-lane generation.
        address: next_generation(),
        arg0: number,
        arg1: 0, // entry
        arg2: args[0],
        arg3: args[1],
        arg4: args[2],
        arg5: args[3],
        arg6: args[4],
        arg7: args[5],
        ..Event::EMPTY
    };
    if observation_enabled() {
        crate::observation::submit(event);
    }
    crate::hook_events::submit_syscall(event);
    submit(event);
    crate::record::submit_global(context as PbConstContextHandle, event);
}

unsafe extern "C" fn on_exit(
    thread_id: PbThreadId,
    context: PbContextHandle,
    standard: PbSyscallStandard,
    _user_data: *mut c_void,
) {
    // Always consume the entry decision, even if capture was disabled between
    // entry and exit, so a later syscall can never reuse stale TLS state.
    let state = take_syscall_state(thread_id);
    let mut return_value: u64 = 0;
    let mut errno: u64 = 0;
    let Some((number, capture)) = state else {
        return;
    };
    pb_pin_get_syscall_return(context, standard, &mut return_value);
    pb_pin_get_syscall_errno(context, standard, &mut errno);
    if let Some(response) = crate::sync_intercept::decide_syscall(
        crate::sync_intercept::SYSCALL_EXIT,
        thread_id,
        context,
        standard,
        number,
        [0; 6],
        return_value,
        errno,
    ) {
        crate::sync_intercept::apply_syscall_response(
            context,
            standard,
            crate::sync_intercept::SYSCALL_EXIT,
            &response,
        );
    }
    if !capture {
        return;
    }
    let event = Event {
        kind: EVENT_SYSCALL,
        thread_id,
        address: next_generation(),
        arg0: number,
        arg1: 1, // exit
        arg3: return_value,
        arg4: errno,
        ..Event::EMPTY
    };
    if observation_enabled() {
        crate::observation::submit(event);
    }
    crate::hook_events::submit_syscall(event);
    submit(event);
    crate::record::submit_global(context as PbConstContextHandle, event);
}

pub fn register() -> PbStatus {
    let mut tls_key = PB_INVALID_TLS_KEY;
    let tls_status = unsafe { pb_pin_create_thread_data_key(None, &mut tls_key) };
    if tls_status != PB_OK {
        return tls_status;
    }
    if tls_key == PB_INVALID_TLS_KEY {
        return PB_ERR_INTERNAL;
    }
    SYSCALL_TLS_KEY.store(tls_key, Ordering::Release);

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
        pb_pin_add_syscall_exit_function(Some(on_exit), core::ptr::null_mut(), &mut handle_exit)
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_syscall_state, encode_syscall_state};

    #[test]
    fn syscall_number_tls_encoding_round_trips() {
        for number in [0, 0x46, 0xfff] {
            for capture in [false, true] {
                let encoded = encode_syscall_state(number, capture).unwrap();
                assert!(!encoded.is_null());
                assert_eq!(
                    decode_syscall_state(encoded.cast_mut()),
                    Some((number, capture))
                );
            }
        }
    }

    #[test]
    fn syscall_number_tls_encoding_rejects_invalid_values() {
        assert_eq!(decode_syscall_state(core::ptr::null_mut()), None);
        assert!(encode_syscall_state(u64::MAX, true).is_none());
    }
}
