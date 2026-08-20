//! Low-frequency Pin lifecycle producers.
//!
//! These callbacks execute on Pin/application threads. Runtime event
//! callbacks never acquire the GIL or allocate: each captures a fixed POD
//! event and submits it to the native ring. The one-time application-start
//! callback may initialize the embedded interpreter before target code runs;
//! Python plugin handlers still execute only on the scripting thread.

use crate::event::{
    Event, EVENT_PIN_ATTACH, EVENT_PROCESS_EXIT, EVENT_PROCESS_FINI, EVENT_PROCESS_START,
    EVENT_THREAD_EXIT, EVENT_THREAD_START, PROCESS_EXIT_SOURCE_API,
    PROCESS_EXIT_SOURCE_PREPARE_FINI,
};
use crate::priority::submit;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use pinbridge_sys::*;

static PROCESS_STARTED: AtomicBool = AtomicBool::new(false);
static PROCESS_EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static PROCESS_PREPARING: AtomicBool = AtomicBool::new(false);
static NATIVE_PREPARE_REACHED: AtomicBool = AtomicBool::new(false);
static PROCESS_FINISHED: AtomicBool = AtomicBool::new(false);
static PROCESS_EXIT_CODE: AtomicI32 = AtomicI32::new(0);
static EXIT_GENERATION: AtomicU64 = AtomicU64::new(0);
static EXIT_ACKNOWLEDGED: AtomicU64 = AtomicU64::new(0);
static EXIT_GRACE_MS: AtomicU32 = AtomicU32::new(1000);
static EXIT_ROUTINES_ARMED: AtomicU32 = AtomicU32::new(0);
static EXIT_ROUTINE_HITS: AtomicU32 = AtomicU32::new(0);

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
    // Pin starts internal threads and the application together. Initialize
    // embedded Python here, after tool loading but before the target can enter
    // a blocking console read and contend for the process standard streams.
    if !PROCESS_STARTED.load(Ordering::Acquire) {
        crate::scripting::initialize_before_application();
    }
    crate::pin_session::note_application_started();
    if PROCESS_STARTED.swap(true, Ordering::AcqRel) {
        // Pin documents another application-start notification after a
        // successful reattach, following image initialization.
        submit(Event {
            kind: EVENT_PIN_ATTACH,
            thread_id: PB_INVALID_THREAD_ID,
            ..Event::EMPTY
        });
        return;
    }
    submit(Event {
        kind: EVENT_PROCESS_START,
        thread_id: PB_INVALID_THREAD_ID,
        ..Event::EMPTY
    });
}

unsafe fn submit_python_exit_edge(
    code: i32,
    source: u64,
    had_exit_request: bool,
    native_prepare_reached: bool,
) {
    PROCESS_EXIT_CODE.store(code, Ordering::Release);
    let generation = EXIT_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    submit(Event {
        kind: EVENT_PROCESS_EXIT,
        thread_id: PB_INVALID_THREAD_ID,
        arg0: code as i64 as u64,
        arg1: source,
        arg2: had_exit_request as u64,
        arg3: native_prepare_reached as u64,
        ..Event::EMPTY
    });

    // Both Python-deliverable exit phases get a bounded handoff window. A
    // slow or wedged handler can never prevent termination after the grace.
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
    if PROCESS_EXIT_REQUESTED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let code = code as i32;
        submit_python_exit_edge(code, PROCESS_EXIT_SOURCE_API, false, false);
        // Pin stops scheduling its internal scripting thread before the real
        // PrepareForFini callback on Windows. Open a usable Python cleanup
        // window here, after process.exit handlers and before returning to
        // the application's exit routine.
        if !PROCESS_PREPARING.swap(true, Ordering::AcqRel) {
            submit_python_exit_edge(code, PROCESS_EXIT_SOURCE_PREPARE_FINI, true, false);
        }
    }
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
    if pb_iarg_list_add(arguments, &descriptor, 1) == PB_OK && pb_rtn_open(routine) == PB_OK {
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
    NATIVE_PREPARE_REACHED.store(true, Ordering::Release);
    if PROCESS_PREPARING.swap(true, Ordering::AcqRel) {
        return;
    }
    // This is a distinct high-priority phase, not an alias for the earlier
    // ExitProcess/RtlExitUserProcess edge. If no user-mode exit routine was
    // observed, the same record also acts as process.exit's fallback.
    let had_exit_request = PROCESS_EXIT_REQUESTED.load(Ordering::Acquire);
    submit_python_exit_edge(
        PROCESS_EXIT_CODE.load(Ordering::Acquire),
        PROCESS_EXIT_SOURCE_PREPARE_FINI,
        had_exit_request,
        true,
    );
}

unsafe extern "C" fn on_thread_start(
    thread_id: PbThreadId,
    context: PbContextHandle,
    flags: i32,
    _user_data: *mut c_void,
) {
    crate::hooks::thread_start(thread_id);
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
    crate::hooks::thread_fini(thread_id);
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
/// supported Python exit/cleanup notifications run in the earlier exit-API
/// window instead.
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

pub fn process_exit_requested() -> bool {
    PROCESS_EXIT_REQUESTED.load(Ordering::Acquire)
}

pub fn process_preparing() -> bool {
    PROCESS_PREPARING.load(Ordering::Acquire)
}

pub fn native_prepare_reached() -> bool {
    NATIVE_PREPARE_REACHED.load(Ordering::Acquire)
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
        pb_img_add_instrument_function(Some(on_image_load), core::ptr::null_mut(), &mut image_load)
    }
}

/// Probe-mode lifecycle registration. Routine call insertion, context-bearing
/// thread callbacks and prepare-for-fini are intentionally omitted: probe
/// mode must leave protected application code native and only uses Pin's
/// documented application-start notification here.
pub fn register_probe() -> PbStatus {
    unsafe {
        let mut application_start = PbCallbackHandle { opaque: 0 };
        pb_pin_add_application_start_function(
            Some(on_application_start),
            core::ptr::null_mut(),
            &mut application_start,
        )
    }
}
