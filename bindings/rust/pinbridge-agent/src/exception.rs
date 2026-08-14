//! Context-change events and the exception pause policy.
//! Every context change lands in the main ring; when the policy matches an
//! exception, the breaker is asked to stop the application.

use crate::event::{Event, EVENT_CONTEXT_CHANGE};
use crate::ring::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use pinbridge_sys::*;

static POLICY_ENABLED: AtomicBool = AtomicBool::new(false);
static POLICY_CODE: AtomicU32 = AtomicU32::new(0); // 0 = any exception code
static PENDING: AtomicBool = AtomicBool::new(false);

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
    pb_pin_get_context_reg(from, PB_REG_RIP, &mut rip);
    submit(Event {
        kind: EVENT_CONTEXT_CHANGE,
        thread_id,
        arg0: reason as u64,
        arg1: info as i64 as u64,
        arg2: rip,
        ..Event::EMPTY
    });

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
