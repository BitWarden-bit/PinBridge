//! Application-debugger event bridge.
//!
//! Pin invokes these callbacks before it reports a breakpoint, single-step,
//! or asynchronous interruption to the attached application debugger. The
//! callback always emits an allocation-free observation to the priority
//! queue. It waits for Python only when a synchronous interceptor exists.

use crate::event::{
    Event, EVENT_DEBUGGER_ASYNC_BREAK, EVENT_DEBUGGER_BREAKPOINT, EVENT_DEBUGGER_SINGLE_STEP,
};
use core::ffi::c_void;
use pinbridge_sys::*;

fn event_kind(event: PbDebuggingEvent) -> Option<u32> {
    match event {
        PB_DEBUGGING_EVENT_BREAKPOINT => Some(EVENT_DEBUGGER_BREAKPOINT),
        PB_DEBUGGING_EVENT_SINGLE_STEP => Some(EVENT_DEBUGGER_SINGLE_STEP),
        PB_DEBUGGING_EVENT_ASYNC_BREAK => Some(EVENT_DEBUGGER_ASYNC_BREAK),
        _ => None,
    }
}

unsafe fn context_word(context: PbContextHandle, register: PbRegId) -> u64 {
    let mut value = 0;
    if !context.is_null() {
        let _ = pb_pin_get_context_reg(context as PbConstContextHandle, register, &mut value);
    }
    value
}

unsafe extern "C" fn on_debugging_event(
    thread_id: PbThreadId,
    event: PbDebuggingEvent,
    context: PbContextHandle,
    _user_data: *mut c_void,
) -> u8 {
    let Some(kind) = event_kind(event) else {
        return 1;
    };
    let ip = context_word(context, crate::arch::instr_ptr_reg());
    crate::priority::submit(Event {
        kind,
        thread_id,
        address: ip,
        arg0: event as u64,
        arg1: context_word(context, crate::arch::stack_ptr_reg()),
        arg2: context_word(context, crate::arch::flags_reg()),
        arg3: context_word(context, crate::arch::return_reg()),
        ..Event::EMPTY
    });

    match crate::sync_intercept::decide_debugger(thread_id, event, context) {
        Some(response) => {
            crate::sync_intercept::apply_debugger_response(context, event, &response) as u8
        }
        None => 1,
    }
}

pub fn register() -> PbStatus {
    for event in [
        PB_DEBUGGING_EVENT_BREAKPOINT,
        PB_DEBUGGING_EVENT_SINGLE_STEP,
        PB_DEBUGGING_EVENT_ASYNC_BREAK,
    ] {
        let status = unsafe {
            pb_pin_intercept_debugging_event(event, Some(on_debugging_event), core::ptr::null_mut())
        };
        if status != PB_OK {
            // Restore the default Pin handlers for any event registered before
            // this failure. Tool initialization is single-threaded here.
            for rollback in [
                PB_DEBUGGING_EVENT_BREAKPOINT,
                PB_DEBUGGING_EVENT_SINGLE_STEP,
                PB_DEBUGGING_EVENT_ASYNC_BREAK,
            ] {
                if rollback == event {
                    break;
                }
                unsafe {
                    let _ = pb_pin_intercept_debugging_event(rollback, None, core::ptr::null_mut());
                }
            }
            return status;
        }
    }
    PB_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_public_pin_debugging_events_have_wire_kinds() {
        assert_eq!(event_kind(PB_DEBUGGING_EVENT_BREAKPOINT), Some(26));
        assert_eq!(event_kind(PB_DEBUGGING_EVENT_SINGLE_STEP), Some(27));
        assert_eq!(event_kind(PB_DEBUGGING_EVENT_ASYNC_BREAK), Some(28));
        assert_eq!(event_kind(99), None);
    }
}
