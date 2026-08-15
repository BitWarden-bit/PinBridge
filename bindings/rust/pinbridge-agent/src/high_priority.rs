//! Rare Pin notifications delivered through the dedicated priority queue.
//!
//! Out-of-memory may arrive concurrently with unknown Pin lock state, so
//! every producer is allocation-free and uses only priority::submit's
//! try-lock path. Python is never called from these callbacks.

use crate::event::{Event, EVENT_OUT_OF_MEMORY, EVENT_PIN_DETACH, EVENT_SMC};
use crate::priority::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use pinbridge_sys::*;

static SMC_REGISTERED: AtomicBool = AtomicBool::new(false);
static OOM_WRITER: AtomicBool = AtomicBool::new(false);
static OOM_TOTAL: AtomicU64 = AtomicU64::new(0);
static OOM_PUBLISHED: AtomicU64 = AtomicU64::new(0);
static OOM_REQUESTED_SIZE: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" fn on_smc(trace_start: u64, trace_end: u64, _user_data: *mut c_void) {
    submit(Event {
        kind: EVENT_SMC,
        thread_id: PB_INVALID_THREAD_ID,
        address: trace_start,
        arg0: trace_start,
        arg1: trace_end,
        ..Event::EMPTY
    });
}

unsafe extern "C" fn on_detach(_user_data: *mut c_void) {
    crate::pin_session::note_detached();
    submit(Event {
        kind: EVENT_PIN_DETACH,
        thread_id: PB_INVALID_THREAD_ID,
        ..Event::EMPTY
    });
}

unsafe extern "C" fn on_out_of_memory(requested_size: u64, _user_data: *mut c_void) {
    let occurrence = OOM_TOTAL.fetch_add(1, Ordering::AcqRel) + 1;
    crate::emergency::record_out_of_memory(requested_size, occurrence);

    // Publish a coherent latest-value slot without waiting for another OOM
    // callback. The priority ring remains the full event stream; this slot is
    // the allocation-free fallback when its try-lock loses a race. An
    // overlapping writer never blocks here: its durable log and ring attempt
    // remain valid even if this fallback slot is busy.
    if OOM_WRITER
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        OOM_REQUESTED_SIZE.store(requested_size, Ordering::Relaxed);
        OOM_PUBLISHED.store(occurrence, Ordering::Release);
        OOM_WRITER.store(false, Ordering::Release);
    }
    submit(Event {
        kind: EVENT_OUT_OF_MEMORY,
        thread_id: PB_INVALID_THREAD_ID,
        arg0: requested_size,
        arg1: occurrence,
        ..Event::EMPTY
    });
}

/// Coherent, allocation-free latest OOM snapshot. A concurrent writer makes
/// this tick skip the slot; the next scripting tick retries it.
pub fn oom_snapshot() -> Option<(u64, u64)> {
    for _ in 0..3 {
        if OOM_WRITER.load(Ordering::Acquire) {
            continue;
        }
        let first = OOM_PUBLISHED.load(Ordering::Acquire);
        if first == 0 {
            return None;
        }
        let requested_size = OOM_REQUESTED_SIZE.load(Ordering::Relaxed);
        let second = OOM_PUBLISHED.load(Ordering::Acquire);
        if first == second && !OOM_WRITER.load(Ordering::Acquire) {
            return Some((first, requested_size));
        }
    }
    None
}

pub fn oom_total() -> u64 {
    OOM_TOTAL.load(Ordering::Acquire)
}

/// SMC tracking is registered lazily on the first Python subscription. Pin
/// documents potentially unbounded tracking memory once this callback is in
/// use, so applications that never request SMC events pay no such cost.
pub fn enable_smc() -> PbStatus {
    if SMC_REGISTERED.load(Ordering::Acquire) {
        return PB_OK;
    }
    // pb.on() runs on a Pin internal thread after the program has started.
    // Pin requires callback registration from that context to hold the
    // client lock; registrations performed during tool initialization do not.
    let lock_status = unsafe { pb_pin_lock_client() };
    if lock_status != PB_OK {
        return lock_status;
    }
    let status = unsafe { pb_trace_add_smc_detected_function(Some(on_smc), core::ptr::null_mut()) };
    let unlock_status = unsafe { pb_pin_unlock_client() };
    if status == PB_OK {
        SMC_REGISTERED.store(true, Ordering::Release);
    }
    if status == PB_OK && unlock_status != PB_OK {
        return unlock_status;
    }
    status
}

/// Detach removes callback registrations while the subscription interest
/// flag remains valid in Rust memory. Attach callbacks already hold Pin's
/// client/vm locks, so re-register directly without recursive locking.
pub fn reregister_smc_after_attach() -> PbStatus {
    if !SMC_REGISTERED.load(Ordering::Acquire) {
        return PB_OK;
    }
    unsafe { pb_trace_add_smc_detected_function(Some(on_smc), core::ptr::null_mut()) }
}

/// Registers always-on emergency/detach sources. JIT and Probe use distinct
/// Pin registration APIs; calling the probed variant from a JIT tool can wedge
/// PIN_StartProgram, so mode selection is explicit here and in the C bridge.
pub fn register() -> (PbStatus, PbStatus) {
    unsafe {
        let oom = pb_pin_add_out_of_memory_function(Some(on_out_of_memory), core::ptr::null_mut());
        let mut probe_mode = 0u8;
        let mode_status = pb_pin_is_probe_mode(&mut probe_mode);
        if mode_status != PB_OK {
            return (oom, mode_status);
        }
        let mut detach_handle = PbCallbackHandle { opaque: 0 };
        let detach = if probe_mode != 0 {
            pb_pin_add_detach_function_probed(
                Some(on_detach),
                core::ptr::null_mut(),
                &mut detach_handle,
            )
        } else {
            pb_pin_add_detach_function(Some(on_detach), core::ptr::null_mut(), &mut detach_handle)
        };
        (oom, detach)
    }
}
