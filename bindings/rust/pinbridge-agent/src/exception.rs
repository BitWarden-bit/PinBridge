//! Context-change events and the exception pause policy.
//! Every context change lands in the compatibility ring. True exception
//! edges are also mirrored into the high-priority ring with one generation
//! so Python observation survives telemetry floods without duplicate calls.

use crate::event::{Event, EVENT_CONTEXT_CHANGE};
use crate::ring::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use pinbridge_sys::*;

static POLICY_ENABLED: AtomicBool = AtomicBool::new(false);
static POLICY_CODE: AtomicU32 = AtomicU32::new(0); // 0 = any exception code
static PENDING: AtomicBool = AtomicBool::new(false);
static EXCEPTION_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    EXCEPTION_GENERATION.load(Ordering::Acquire)
}

pub fn set_policy(enabled: bool, code: u32) {
    POLICY_CODE.store(code, Ordering::Relaxed);
    POLICY_ENABLED.store(enabled, Ordering::Relaxed);
    PENDING.store(false, Ordering::Relaxed);
}

pub fn policy() -> (bool, u32, bool) {
    (
        POLICY_ENABLED.load(Ordering::Relaxed),
        POLICY_CODE.load(Ordering::Relaxed),
        PENDING.load(Ordering::Relaxed),
    )
}

unsafe extern "C" fn on_context_change(
    thread_id: PbThreadId,
    reason: PbContextChangeReason,
    from: PbConstContextHandle,
    to: PbContextHandle,
    info: i32,
    _user_data: *mut c_void,
) {
    let mut rip: u64 = 0;
    pb_pin_get_context_reg(from, crate::arch::instr_ptr_reg(), &mut rip);
    let exception_generation = if reason == PB_CONTEXT_CHANGE_REASON_EXCEPTION {
        EXCEPTION_GENERATION.fetch_add(1, Ordering::AcqRel) + 1
    } else {
        0
    };
    let event = Event {
        kind: EVENT_CONTEXT_CHANGE,
        thread_id,
        address: rip,
        arg0: reason as u64,
        arg1: info as i64 as u64,
        arg2: rip,
        arg3: exception_generation,
        ..Event::EMPTY
    };
    if exception_generation != 0 {
        crate::priority::submit(event);
    }
    submit(event);
    crate::record::submit_global(from, event);

    if reason == PB_CONTEXT_CHANGE_REASON_EXCEPTION {
        if let Some(response) = crate::sync_intercept::decide_exception(
            thread_id,
            reason,
            from,
            to,
            info as u32,
        ) {
            crate::sync_intercept::apply_exception_response(to, &response);
        }
    }

    if reason == PB_CONTEXT_CHANGE_REASON_EXCEPTION
        && POLICY_ENABLED.load(Ordering::Relaxed)
        && (POLICY_CODE.load(Ordering::Relaxed) == 0
            || POLICY_CODE.load(Ordering::Relaxed) == info as u32)
    {
        PENDING.store(true, Ordering::Relaxed);
        crate::bp::request_stop();
    }
}

pub fn register() -> PbStatus {
    let mut handle = PbCallbackHandle { opaque: 0 };
    unsafe {
        pb_pin_add_context_change_function(
            Some(on_context_change),
            core::ptr::null_mut(),
            &mut handle,
        )
    }
}
