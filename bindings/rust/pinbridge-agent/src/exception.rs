//! Context-change events and the exception pause policy.
//! Every context change is mirrored into both the high-priority and
//! compatibility rings with one generation. This preserves signal/APC/
//! callback transitions as well as exceptions during telemetry floods.

use crate::event::{Event, EVENT_CONTEXT_CHANGE};
use crate::ring::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use pinbridge_sys::*;

static POLICY_ENABLED: AtomicBool = AtomicBool::new(false);
static POLICY_CODE: AtomicU32 = AtomicU32::new(0); // 0 = any exception code
static PENDING: AtomicBool = AtomicBool::new(false);
static CONTEXT_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    CONTEXT_GENERATION.load(Ordering::Acquire)
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
    let mut from_ip = 0u64;
    let from_ip_known = !from.is_null()
        && pb_pin_get_context_reg(from, crate::arch::instr_ptr_reg(), &mut from_ip) == PB_OK;
    let mut to_ip = 0u64;
    let to_ip_known = !to.is_null()
        && pb_pin_get_context_reg(
            to as PbConstContextHandle,
            crate::arch::instr_ptr_reg(),
            &mut to_ip,
        ) == PB_OK;
    let context_generation = CONTEXT_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    let event = Event {
        kind: EVENT_CONTEXT_CHANGE,
        thread_id,
        address: from_ip,
        arg0: reason as u64,
        arg1: info as i64 as u64,
        arg2: from_ip,
        arg3: context_generation,
        arg4: to_ip,
        arg5: to_ip_known as u64,
        arg6: from_ip_known as u64,
        ..Event::EMPTY
    };
    crate::priority::submit(event);
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
