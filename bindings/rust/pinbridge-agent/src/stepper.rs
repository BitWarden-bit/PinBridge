//! Single-step engine (step into / step over) on instrumentation primitives.
//!
//! Precision comes from *parking*: the analysis callback for the landing
//! instruction blocks the application thread on a Pin semaphore instead of
//! returning, so the observed state is exactly at the landing instruction.
//! The breaker then suspends the remaining threads as usual.
//!
//! - step into: park on the first executed instruction whose address differs
//!   from the resume point (replays of the resume point are suppressed).
//! - step over: if the current instruction is a call (metadata table), plant
//!   a one-shot breakpoint on the fallthrough and park on its hit; otherwise
//!   degrade to step into.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

const MODE_INTO: u8 = 0;
const MODE_OVER: u8 = 1;

static STEP_TID: AtomicU32 = AtomicU32::new(u32::MAX);
static STEP_START_IP: AtomicU64 = AtomicU64::new(0);
static STEP_MODE: AtomicU32 = AtomicU32::new(MODE_INTO as u32);
static STEPPING: AtomicBool = AtomicBool::new(false);

static PARK_SEM: AtomicUsize = AtomicUsize::new(0);
static PARKED: AtomicBool = AtomicBool::new(false);
static PARKED_TID: AtomicU32 = AtomicU32::new(0);
static PARKED_IP: AtomicU64 = AtomicU64::new(0);

/// Park coordination. A parked thread is NOT a safe point for
/// stop_application_threads, so the breaker must release it before
/// suspending — and a thread must never start parking while the breaker is
/// already suspending (it would block forever and deadlock the stop). This
/// Pin mutex + SUSPENDING flag close that race (std locks in analysis
/// callbacks kill the process; only Pin locks are safe here).
static PARK_LOCK: AtomicUsize = AtomicUsize::new(0);
static SUSPENDING: AtomicBool = AtomicBool::new(false);

pub fn init() -> PbStatus {
    unsafe {
        let mut semaphore: PbSemaphoreHandle = core::ptr::null_mut();
        let status = pb_pin_semaphore_init(&mut semaphore);
        if status != PB_OK {
            return status;
        }
        PARK_SEM.store(semaphore as usize, Ordering::Release);
        let mut mutex: PbMutexHandle = core::ptr::null_mut();
        let status = pb_pin_mutex_init(&mut mutex);
        if status == PB_OK {
            PARK_LOCK.store(mutex as usize, Ordering::Release);
        }
        status
    }
}

pub fn is_parked() -> bool {
    PARKED.load(Ordering::Acquire)
}

/// Releases the parked application thread (idempotent).
pub fn release() {
    if !PARKED.swap(false, Ordering::AcqRel) {
        return;
    }
    let semaphore = PARK_SEM.load(Ordering::Acquire) as PbSemaphoreHandle;
    if !semaphore.is_null() {
        unsafe {
            pb_pin_semaphore_set(semaphore);
        }
    }
}

/// Breaker side, called right before stop_application_threads: no new parks
/// from now on, and the currently parked thread (if any) is freed so it can
/// reach a safe point.
pub fn begin_suspend() {
    let mutex = PARK_LOCK.load(Ordering::Acquire) as PbMutexHandle;
    if mutex.is_null() {
        SUSPENDING.store(true, Ordering::Release);
        release();
        return;
    }
    unsafe {
        pb_pin_mutex_lock(mutex);
        SUSPENDING.store(true, Ordering::Release);
        release();
        pb_pin_mutex_unlock(mutex);
    }
}

/// Breaker side, after the application is running again.
pub fn end_suspend() {
    SUSPENDING.store(false, Ordering::Release);
}

/// Freezes the calling application thread exactly at `address` (pre-execution,
/// inside an analysis callback) until the breaker releases it. At most one
/// thread parks at a time; returns false when a suspension is already in
/// flight or another thread holds the park slot — the caller must then fall
/// back to a plain request_stop() (the in-flight stop will catch it).
pub fn park_current(thread_id: u32, address: u64) -> bool {
    let mutex = PARK_LOCK.load(Ordering::Acquire) as PbMutexHandle;
    if !mutex.is_null() {
        let mut granted = false;
        unsafe {
            if pb_pin_mutex_lock(mutex) == PB_OK {
                if !SUSPENDING.load(Ordering::Acquire) && !PARKED.load(Ordering::Acquire) {
                    PARKED_TID.store(thread_id, Ordering::Release);
                    PARKED_IP.store(address, Ordering::Release);
                    STEPPING.store(false, Ordering::Release);
                    PARKED.store(true, Ordering::Release);
                    granted = true;
                }
                pb_pin_mutex_unlock(mutex);
            }
        }
        if !granted {
            return false;
        }
    } else {
        if !PARKED.swap(true, Ordering::AcqRel) {
            PARKED_TID.store(thread_id, Ordering::Release);
            PARKED_IP.store(address, Ordering::Release);
            STEPPING.store(false, Ordering::Release);
        }
    }
    // (no logging here: analysis callback on an application thread)
    // the landing thread must freeze; the breaker suspends the rest
    crate::bp::request_stop();
    let semaphore = PARK_SEM.load(Ordering::Acquire) as PbSemaphoreHandle;
    if !semaphore.is_null() {
        unsafe {
            pb_pin_semaphore_wait(semaphore);
            pb_pin_semaphore_clear(semaphore);
        }
    }
    true
}

/// Arms the step. `start_ip` is the rip of the stopped thread.
pub fn arm(thread_id: u32, start_ip: u64, over: bool) {
    STEP_TID.store(thread_id, Ordering::Release);
    STEP_START_IP.store(start_ip, Ordering::Release);
    STEP_MODE.store(if over { MODE_OVER as u32 } else { MODE_INTO as u32 }, Ordering::Release);
    STEPPING.store(true, Ordering::Release);
}

/// Exec-capture side of the step machinery. Returns true when the callback
/// was consumed by the stepper (suppress the normal event handling).
pub fn on_step_event(thread_id: u32, address: u64) -> bool {
    if !STEPPING.load(Ordering::Acquire) {
        return false;
    }
    if STEP_TID.load(Ordering::Acquire) != thread_id {
        return false;
    }
    if address == STEP_START_IP.load(Ordering::Acquire) {
        return true; // replay of the instruction we stepped from
    }
    if STEP_MODE.load(Ordering::Acquire) == MODE_INTO as u32 {
        if !park_current(thread_id, address) {
            crate::bp::request_stop();
        }
    }
    true
}

/// Breakpoint-hit side. Returns true when the hit was consumed by the
/// stepper (replay suppression, or the one-shot landing after parking).
pub fn on_bp_event(address: u64) -> bool {
    if !STEPPING.load(Ordering::Acquire) {
        return false;
    }
    if address == STEP_START_IP.load(Ordering::Acquire) {
        return true;
    }
    // landing on the one-shot breakpoint (step over) or any other breakpoint:
    // park on whichever thread hit it
    let tid = STEP_TID.load(Ordering::Acquire);
    if !park_current(tid, address) {
        crate::bp::request_stop();
    }
    true
}
