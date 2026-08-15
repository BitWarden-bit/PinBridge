//! Low-frequency Pin lifecycle producers.
//!
//! These callbacks execute on Pin/application threads.  They never acquire
//! the GIL and never allocate: each callback captures a fixed POD event and
//! submits it to the native ring.  The scripting internal thread is the only
//! place where a Python handler is invoked.

use crate::event::{
    Event, EVENT_PROCESS_EXIT, EVENT_PROCESS_FINI, EVENT_PROCESS_START, EVENT_THREAD_EXIT,
    EVENT_THREAD_START,
};
use crate::ring::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use pinbridge_sys::*;

static PROCESS_STARTED: AtomicBool = AtomicBool::new(false);
static PROCESS_EXITING: AtomicBool = AtomicBool::new(false);
static PROCESS_FINISHED: AtomicBool = AtomicBool::new(false);
static PROCESS_EXIT_CODE: AtomicI32 = AtomicI32::new(0);
static EXIT_GENERATION: AtomicU64 = AtomicU64::new(0);
static EXIT_ACKNOWLEDGED: AtomicU64 = AtomicU64::new(0);
static EXIT_GRACE_MS: AtomicU32 = AtomicU32::new(1000);
static EXIT_ROUTINES_ARMED: AtomicU32 = AtomicU32::new(0);
static EXIT_ROUTINE_HITS: AtomicU32 = AtomicU32::new(0);
static EXIT_SOURCE: AtomicU32 = AtomicU32::new(0);

const EXIT_SOURCE_API: u32 = 1;
const EXIT_SOURCE_PREPARE_FINI: u32 = 2;

#[inline]
unsafe fn context_ip(context: PbConstContextHandle) -> u64 {
    if context.is_null() {
        return 0;
    }
    let mut ip = 0;
    let _ = pb_pin_get_context_reg(context, crate::arch::instr_ptr_reg(), &mut ip);
    ip
}

unsafe extern "C" fn on_application_start(_user_data: *mut c_void) {
    PROCESS_STARTED.store(true, Ordering::Release);
    submit(Event {
        kind: EVENT_PROCESS_START,
        thread_id: PB_INVALID_THREAD_ID,
        ..Event::EMPTY
    });
}

unsafe fn notify_process_exit(code: i32, source: u32) {
    if PROCESS_EXITING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    PROCESS_EXIT_CODE.store(code, Ordering::Release);
    EXIT_SOURCE.store(source, Ordering::Release);
    let generation = EXIT_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    submit(Event {
        kind: EVENT_PROCESS_EXIT,
        thread_id: PB_INVALID_THREAD_ID,
        arg0: code as i64 as u64,
        arg1: source as u64,
        ..Event::EMPTY
    });

    // PrepareForFini is the last point where Pin's internal scripting thread
    // can still run. Give it a bounded handoff window for process.exit. This
    // is never an unbounded dependency on Python: a slow or wedged handler
    // cannot prevent the target from terminating after the configured grace.
    if crate::scripting::python_ready() {
        let grace_ms = EXIT_GRACE_MS.load(Ordering::Relaxed);
        for _ in 0..grace_ms {
            if EXIT_ACKNOWLEDGED.load(Ordering::Acquire) >= generation {
                break;
            }
            let _ = pb_pin_sleep(1);
        }
    }
}

unsafe extern "C" fn on_process_exit_routine(code: u64) {
    EXIT_ROUTINE_HITS.fetch_add(1, Ordering::Relaxed);
    notify_process_exit(code as i32, EXIT_SOURCE_API);
}

unsafe fn instrument_exit_routine(img: PbImgHandle, name: &'static [u8]) {
    let mut routine = PbRtnHandle { opaque: 0 };
    if pb_rtn_find_by_name(img, name.as_ptr() as *const i8, &mut routine) != PB_OK
        || routine.opaque == 0
    {
        return;
    }

    let mut arguments: PbIargListHandle = core::ptr::null_mut();
    if pb_iarg_list_alloc(&mut arguments) != PB_OK {
        return;
    }
    let descriptor = PbIargDescriptor {
        r#type: PB_IARG_FUNCARG_ENTRYPOINT_VALUE,
        reserved: 0,
        value: 0,
        value2: 0,
    };
    if pb_iarg_list_add(arguments, &descriptor, 1) == PB_OK
        && pb_rtn_open(routine) == PB_OK
    {
        if pb_rtn_insert_call(
            routine,
            PB_IPOINT_BEFORE,
            on_process_exit_routine as *const () as usize as u64,
            arguments,
        ) == PB_OK
        {
            EXIT_ROUTINES_ARMED.fetch_add(1, Ordering::Relaxed);
        }
        let _ = pb_rtn_close(routine);
    }
    let _ = pb_iarg_list_free(arguments);
}

unsafe extern "C" fn on_image_load(img: PbImgHandle, _user_data: *mut c_void) {
    // RtlExitUserProcess is the stable native path used by the CRT after
    // main returns. ExitProcess is included for applications that call it
    // directly and resolve to a real routine rather than a forwarder.
    instrument_exit_routine(img, b"RtlExitUserProcess\0");
    instrument_exit_routine(img, b"ExitProcess\0");
}

unsafe extern "C" fn on_prepare_for_fini(_user_data: *mut c_void) {
    // Fallback for termination paths that bypass the instrumented user-mode
    // exit routines. Pin may no longer schedule internal threads here, so
    // the early routine edge above is the reliable Python notification.
    notify_process_exit(
        PROCESS_EXIT_CODE.load(Ordering::Acquire),
        EXIT_SOURCE_PREPARE_FINI,
    );
}

unsafe extern "C" fn on_thread_start(
    thread_id: PbThreadId,
    context: PbContextHandle,
    flags: i32,
    _user_data: *mut c_void,
) {
    submit(Event {
        kind: EVENT_THREAD_START,
        thread_id,
        address: context_ip(context as PbConstContextHandle),
        arg0: flags as i64 as u64,
        ..Event::EMPTY
    });
}

unsafe extern "C" fn on_thread_fini(
    thread_id: PbThreadId,
    context: PbConstContextHandle,
    code: i32,
    _user_data: *mut c_void,
) {
    submit(Event {
        kind: EVENT_THREAD_EXIT,
        thread_id,
        address: context_ip(context),
        arg0: code as i64 as u64,
        ..Event::EMPTY
    });
}

/// Called by the agent's existing fini callback.  This native edge is useful
/// to trace/log consumers, but the Python thread may already be stopped, so
/// the supported Python `process.exit` notification is emitted at
/// PrepareForFini instead.
pub unsafe fn record_fini(code: i32) {
    PROCESS_EXIT_CODE.store(code, Ordering::Release);
    PROCESS_FINISHED.store(true, Ordering::Release);
    submit(Event {
        kind: EVENT_PROCESS_FINI,
        thread_id: PB_INVALID_THREAD_ID,
        arg0: code as i64 as u64,
        ..Event::EMPTY
    });
}

pub fn process_started() -> bool {
    PROCESS_STARTED.load(Ordering::Acquire)
}

pub fn process_exiting() -> bool {
    PROCESS_EXITING.load(Ordering::Acquire)
}

pub fn exit_delivery_pending() -> bool {
    EXIT_ACKNOWLEDGED.load(Ordering::Acquire) < EXIT_GENERATION.load(Ordering::Acquire)
}

/// Called by the single scripting host after it has attempted delivery to
/// every running plugin for the current tick.
pub fn acknowledge_exit_delivery() {
    let generation = EXIT_GENERATION.load(Ordering::Acquire);
    EXIT_ACKNOWLEDGED.store(generation, Ordering::Release);
}

#[allow(dead_code)]
pub fn process_finished() -> bool {
    PROCESS_FINISHED.load(Ordering::Acquire)
}

pub fn process_exit_code() -> i32 {
    PROCESS_EXIT_CODE.load(Ordering::Acquire)
}

pub fn process_exit_source() -> u32 {
    EXIT_SOURCE.load(Ordering::Acquire)
}

pub fn exit_probe_counts() -> (u32, u32) {
    (
        EXIT_ROUTINES_ARMED.load(Ordering::Relaxed),
        EXIT_ROUTINE_HITS.load(Ordering::Relaxed),
    )
}

/// Registers all Python-deliverable lifecycle sources.  Fini itself is
/// recorded from lib.rs so the existing summary callback remains the single
/// finalization owner.
pub fn register() -> PbStatus {
    let grace_ms = std::env::var("PINBRIDGE_SCRIPT_EXIT_GRACE_MS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1000)
        .min(5000);
    EXIT_GRACE_MS.store(grace_ms, Ordering::Release);
    unsafe {
        let mut application_start = PbCallbackHandle { opaque: 0 };
        let status = pb_pin_add_application_start_function(
            Some(on_application_start),
            core::ptr::null_mut(),
            &mut application_start,
        );
        if status != PB_OK {
            return status;
        }

        let mut prepare_fini = PbCallbackHandle { opaque: 0 };
        let status = pb_pin_add_prepare_for_fini_function(
            Some(on_prepare_for_fini),
            core::ptr::null_mut(),
            &mut prepare_fini,
        );
        if status != PB_OK {
            return status;
        }

        let mut thread_start = PbCallbackHandle { opaque: 0 };
        let status = pb_pin_add_thread_start_function(
            Some(on_thread_start),
            core::ptr::null_mut(),
            &mut thread_start,
        );
        if status != PB_OK {
            return status;
        }

        let mut thread_fini = PbCallbackHandle { opaque: 0 };
        let status = pb_pin_add_thread_fini_function(
            Some(on_thread_fini),
            core::ptr::null_mut(),
            &mut thread_fini,
        );
        if status != PB_OK {
            return status;
        }

        let mut image_load = PbCallbackHandle { opaque: 0 };
        pb_img_add_instrument_function(
            Some(on_image_load),
            core::ptr::null_mut(),
            &mut image_load,
        )
    }
}
