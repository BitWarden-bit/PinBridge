//! Single-step engine (step into / step over) on instrumentation primitives.
//!
//! Precision comes from *parking*: the analysis callback for the landing
//! instruction blocks the application thread on a Pin semaphore instead of
//! returning, so the observed state is exactly at the landing instruction.
//! The breaker then suspends the remaining threads as usual.
//!
//! - step into: park on the first executed instruction whose address differs
//!   from the resume point (replays of the resume point are suppressed).
//! - decoded step: publish a private set of successor addresses. These are
//!   instrumented independently from user/Python breakpoints and only the
//!   requested thread may claim a landing.
//! - fallback step: when decoding fails, use exec capture and park on the
//!   first instruction after the one replayed from the stop.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

const MODE_INTO: u8 = 0;
const MODE_OVER: u8 = 1;
const MODE_CANDIDATES: u8 = 2;
const MAX_CANDIDATES: usize = 3;

static STEP_TID: AtomicU32 = AtomicU32::new(u32::MAX);
static STEP_START_IP: AtomicU64 = AtomicU64::new(0);
static STEP_MODE: AtomicU32 = AtomicU32::new(MODE_INTO as u32);
static STEPPING: AtomicBool = AtomicBool::new(false);
static REPLAY_PENDING: AtomicBool = AtomicBool::new(false);
static CANDIDATE_COUNT: AtomicUsize = AtomicUsize::new(0);
static CANDIDATES: [AtomicU64; MAX_CANDIDATES] =
    [const { AtomicU64::new(0) }; MAX_CANDIDATES];

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
    REPLAY_PENDING.store(true, Ordering::Release);
    CANDIDATE_COUNT.store(0, Ordering::Release);
    STEPPING.store(true, Ordering::Release);
}

fn publish_candidates(thread_id: u32, start_ip: u64, candidates: &[u64]) -> usize {
    STEPPING.store(false, Ordering::Release);
    let count = candidates.len().min(MAX_CANDIDATES);
    for (index, slot) in CANDIDATES.iter().enumerate() {
        slot.store(candidates.get(index).copied().unwrap_or(0), Ordering::Relaxed);
    }
    STEP_TID.store(thread_id, Ordering::Relaxed);
    STEP_START_IP.store(start_ip, Ordering::Relaxed);
    STEP_MODE.store(MODE_CANDIDATES as u32, Ordering::Relaxed);
    REPLAY_PENDING.store(true, Ordering::Relaxed);
    CANDIDATE_COUNT.store(count, Ordering::Release);
    STEPPING.store(count != 0, Ordering::Release);
    count
}

/// Arms decoded successor instrumentation while the application is stopped.
/// Candidate callbacks are separate from the ordinary breakpoint table, so
/// clearing a step can never remove a user/CLI/Python breakpoint.
pub fn arm_candidates(
    thread_id: u32,
    start_ip: u64,
    candidates: &[u64],
) -> Result<usize, PbStatus> {
    let count = publish_candidates(thread_id, start_ip, candidates);
    if count == 0 {
        return Err(PB_ERR_INVALID_ARGUMENT);
    }
    for address in candidates.iter().take(count) {
        let status = unsafe {
            pb_pin_remove_instrumentation_in_range(*address, address.saturating_add(15))
        };
        if status != PB_OK {
            cancel();
            return Err(status);
        }
    }
    Ok(count)
}

/// Instrumentation-time lookup for the private successor slots.
pub fn candidate_index(address: u64) -> Option<usize> {
    if !STEPPING.load(Ordering::Acquire)
        || STEP_MODE.load(Ordering::Acquire) != MODE_CANDIDATES as u32
    {
        return None;
    }
    let count = CANDIDATE_COUNT.load(Ordering::Acquire).min(MAX_CANDIDATES);
    (0..count).find(|&index| CANDIDATES[index].load(Ordering::Acquire) == address)
}

/// Address stored in a private successor slot. Analysis callbacks use this
/// before claiming so the resumed start instruction can be suppressed once.
pub fn candidate_address(index: usize) -> u64 {
    if index >= CANDIDATE_COUNT.load(Ordering::Acquire).min(MAX_CANDIDATES) {
        return 0;
    }
    CANDIDATES[index].load(Ordering::Acquire)
}

/// Suppresses exactly one replay of the stopped instruction, on the requested
/// thread only. A self-loop can then claim the same address as a real landing.
pub fn consume_start_replay(thread_id: u32, address: u64) -> bool {
    if !STEPPING.load(Ordering::Acquire)
        || STEP_TID.load(Ordering::Acquire) != thread_id
        || STEP_START_IP.load(Ordering::Acquire) != address
    {
        return false;
    }
    REPLAY_PENDING.swap(false, Ordering::AcqRel)
}

/// Claims one decoded successor. Wrong threads and unrelated breakpoint
/// addresses are ignored; compare_exchange guarantees a single winner.
pub fn claim_candidate(index: usize, thread_id: u32) -> Option<u64> {
    if index >= CANDIDATE_COUNT.load(Ordering::Acquire).min(MAX_CANDIDATES)
        || STEP_MODE.load(Ordering::Acquire) != MODE_CANDIDATES as u32
        || STEP_TID.load(Ordering::Acquire) != thread_id
    {
        return None;
    }
    let address = CANDIDATES[index].load(Ordering::Acquire);
    if address == 0
        || STEPPING
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return None;
    }
    Some(address)
}

/// Same claim operation for a normal user breakpoint that happens to share a
/// successor address. The user breakpoint remains live and reports its hit.
pub fn claim_breakpoint(thread_id: u32, address: u64) -> bool {
    let Some(index) = candidate_index(address) else {
        return false;
    };
    claim_candidate(index, thread_id).is_some()
}

/// Disarms every form of step. Already-instrumented private callbacks remain
/// harmless because they re-check this state before doing anything.
pub fn cancel() {
    STEPPING.store(false, Ordering::Release);
    REPLAY_PENDING.store(false, Ordering::Release);
    CANDIDATE_COUNT.store(0, Ordering::Release);
    for slot in &CANDIDATES {
        slot.store(0, Ordering::Relaxed);
    }
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
    if consume_start_replay(thread_id, address) {
        return true;
    }
    if STEP_MODE.load(Ordering::Acquire) != MODE_INTO as u32 {
        return false;
    }
    if !park_current(thread_id, address) {
        crate::bp::request_stop();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn candidate_step_matches_thread_address_and_consumes_replay_once() {
        let _guard = TEST_LOCK.lock().unwrap();
        cancel();
        assert_eq!(publish_candidates(7, 0x1000, &[0x1000, 0x2000]), 2);

        assert!(!consume_start_replay(8, 0x1000));
        assert!(consume_start_replay(7, 0x1000));
        assert!(!consume_start_replay(7, 0x1000));

        let index = candidate_index(0x2000).unwrap();
        assert_eq!(claim_candidate(index, 8), None);
        assert_eq!(claim_candidate(index, 7), Some(0x2000));
        assert_eq!(claim_candidate(index, 7), None);
        cancel();
    }

    #[test]
    fn unrelated_breakpoint_is_not_a_step_landing() {
        let _guard = TEST_LOCK.lock().unwrap();
        cancel();
        publish_candidates(3, 0x3000, &[0x3001, 0x3010]);
        assert!(!claim_breakpoint(3, 0x4000));
        assert!(!claim_breakpoint(4, 0x3001));
        assert!(claim_breakpoint(3, 0x3001));
        cancel();
    }
}
