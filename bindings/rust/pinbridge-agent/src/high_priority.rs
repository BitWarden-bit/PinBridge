//! Rare Pin notifications delivered through the dedicated priority queue.
//!
//! Out-of-memory may arrive concurrently with unknown Pin lock state, so
//! every producer is allocation-free and uses only priority::submit's
//! try-lock path. Python is never called from these callbacks.

use crate::event::{Event, EVENT_OUT_OF_MEMORY, EVENT_PIN_DETACH, EVENT_SMC};
use crate::priority::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};
use pinbridge_sys::*;

static SMC_REGISTERED: AtomicBool = AtomicBool::new(false);

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
    submit(Event {
        kind: EVENT_PIN_DETACH,
        thread_id: PB_INVALID_THREAD_ID,
        ..Event::EMPTY
    });
}

unsafe extern "C" fn on_out_of_memory(requested_size: u64, _user_data: *mut c_void) {
    submit(Event {
        kind: EVENT_OUT_OF_MEMORY,
        thread_id: PB_INVALID_THREAD_ID,
        arg0: requested_size,
        ..Event::EMPTY
    });
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
