//! Bounded rendezvous for Pin's synchronous child-follow callback.
//!
//! The Pin callback cannot call Python and the CHILD_PROCESS handle is valid
//! only for that callback. It therefore copies pid/argv into one fixed slot,
//! waits on a Pin semaphore for a bounded time, and returns a conservative
//! "do not follow" decision on every failure path.

use core::ffi::{c_char, c_void};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

const MAX_ARGUMENTS: usize = 64;
const ARGUMENT_BYTES: usize = 8192;
const DEFAULT_TIMEOUT_MS: u32 = 2000;
const MAX_TIMEOUT_MS: u32 = 10_000;

const IDLE: u32 = 0;
const WRITING: u32 = 1;
const PENDING: u32 = 2;
const HANDLING: u32 = 3;
const DECIDED: u32 = 4;
const CANCELLED: u32 = 5;

struct RequestSlot {
    process_id: u32,
    argc: u32,
    offsets: [u16; MAX_ARGUMENTS],
    lengths: [u16; MAX_ARGUMENTS],
    bytes: [u8; ARGUMENT_BYTES],
}

impl RequestSlot {
    const fn new() -> Self {
        Self {
            process_id: 0,
            argc: 0,
            offsets: [0; MAX_ARGUMENTS],
            lengths: [0; MAX_ARGUMENTS],
            bytes: [0; ARGUMENT_BYTES],
        }
    }
}

pub struct ChildRequest {
    pub generation: u64,
    pub process_id: u32,
    pub arguments: Vec<Vec<u8>>,
}

static STATE: AtomicU32 = AtomicU32::new(IDLE);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACTIVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static RESULT_FOLLOW: AtomicU32 = AtomicU32::new(0);
static RESPONSE_SEMAPHORE: AtomicUsize = AtomicUsize::new(0);
static TIMEOUT_MS: AtomicU32 = AtomicU32::new(DEFAULT_TIMEOUT_MS);
static TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static DECISIONS: AtomicU64 = AtomicU64::new(0);
static FOLLOWED: AtomicU64 = AtomicU64::new(0);
static REJECTED: AtomicU64 = AtomicU64::new(0);
static mut REQUEST: RequestSlot = RequestSlot::new();

unsafe fn capture_request(child: PbChildProcessHandle) -> bool {
    let slot = &mut *core::ptr::addr_of_mut!(REQUEST);
    let mut process_id = 0u32;
    if pb_child_process_get_id(child, &mut process_id) != PB_OK {
        return false;
    }
    let mut argc = 0i32;
    if pb_child_process_get_command_line_count(child, &mut argc) != PB_OK
        || argc < 0
        || argc as usize > MAX_ARGUMENTS
    {
        return false;
    }

    slot.process_id = process_id;
    slot.argc = argc as u32;
    let mut used = 0usize;
    for index in 0..argc as usize {
        let mut required = 0u64;
        let status = pb_child_process_get_command_line_argument(
            child,
            index as i32,
            core::ptr::null_mut(),
            0,
            &mut required,
        );
        if status != PB_ERR_BUFFER_TOO_SMALL
            || required == 0
            || required > u16::MAX as u64
            || used.saturating_add(required as usize) > ARGUMENT_BYTES
        {
            return false;
        }
        let destination = slot.bytes.as_mut_ptr().add(used) as *mut c_char;
        if pb_child_process_get_command_line_argument(
            child,
            index as i32,
            destination,
            required,
            &mut required,
        ) != PB_OK
        {
            return false;
        }
        slot.offsets[index] = used as u16;
        slot.lengths[index] = (required - 1) as u16;
        used += required as usize;
    }
    true
}

unsafe extern "C" fn on_follow_child(child: PbChildProcessHandle, _user_data: *mut c_void) -> u8 {
    if !crate::scripting::python_ready()
        || STATE
            .compare_exchange(IDLE, WRITING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return 0;
    }
    if !capture_request(child) {
        STATE.store(IDLE, Ordering::Release);
        return 0;
    }

    let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    ACTIVE_GENERATION.store(generation, Ordering::Release);
    RESULT_FOLLOW.store(0, Ordering::Release);
    let semaphore = RESPONSE_SEMAPHORE.load(Ordering::Acquire) as PbSemaphoreHandle;
    if semaphore.is_null() {
        STATE.store(IDLE, Ordering::Release);
        return 0;
    }
    let _ = pb_pin_semaphore_clear(semaphore);
    STATE.store(PENDING, Ordering::Release);

    let mut woke = 0u8;
    let _wait_status =
        pb_pin_semaphore_timed_wait(semaphore, TIMEOUT_MS.load(Ordering::Relaxed), &mut woke);
    loop {
        match STATE.load(Ordering::Acquire) {
            DECIDED => {
                let follow = RESULT_FOLLOW.load(Ordering::Acquire) != 0;
                DECISIONS.fetch_add(1, Ordering::Relaxed);
                if follow {
                    FOLLOWED.fetch_add(1, Ordering::Relaxed);
                } else {
                    REJECTED.fetch_add(1, Ordering::Relaxed);
                }
                STATE.store(IDLE, Ordering::Release);
                return follow as u8;
            }
            PENDING => {
                if STATE
                    .compare_exchange(PENDING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                    return 0;
                }
            }
            HANDLING => {
                if STATE
                    .compare_exchange(HANDLING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                    return 0;
                }
            }
            CANCELLED => return 0,
            _ => return 0,
        }
    }
}

pub fn init_and_register() -> PbStatus {
    let configured = std::env::var("PINBRIDGE_SCRIPT_DECISION_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(1, MAX_TIMEOUT_MS);
    TIMEOUT_MS.store(configured, Ordering::Release);

    unsafe {
        let mut semaphore: PbSemaphoreHandle = core::ptr::null_mut();
        let status = pb_pin_semaphore_init(&mut semaphore);
        if status != PB_OK {
            return status;
        }
        RESPONSE_SEMAPHORE.store(semaphore as usize, Ordering::Release);
        let mut callback = PbCallbackHandle { opaque: 0 };
        pb_pin_add_follow_child_process_function(
            Some(on_follow_child),
            core::ptr::null_mut(),
            &mut callback,
        )
    }
}

/// Claims one request for the scripting thread and copies the fixed slot into
/// ordinary owned values. No Pin handle crosses the callback boundary.
pub fn take_pending() -> Option<ChildRequest> {
    if STATE
        .compare_exchange(PENDING, HANDLING, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        if STATE
            .compare_exchange(CANCELLED, IDLE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            crate::log::line("child.follow cancelled request reaped");
        }
        return None;
    }
    unsafe {
        let slot = &*core::ptr::addr_of!(REQUEST);
        let mut arguments = Vec::with_capacity(slot.argc as usize);
        for index in 0..slot.argc as usize {
            let start = slot.offsets[index] as usize;
            let end = start + slot.lengths[index] as usize;
            arguments.push(slot.bytes[start..end].to_vec());
        }
        Some(ChildRequest {
            generation: ACTIVE_GENERATION.load(Ordering::Acquire),
            process_id: slot.process_id,
            arguments,
        })
    }
}

/// Publishes the Python decision unless the native callback already timed
/// out. A cancelled request owns the slot until this function retires it.
pub fn complete(generation: u64, follow: bool) {
    if ACTIVE_GENERATION.load(Ordering::Acquire) != generation {
        return;
    }
    RESULT_FOLLOW.store(follow as u32, Ordering::Release);
    if STATE
        .compare_exchange(HANDLING, DECIDED, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let semaphore = RESPONSE_SEMAPHORE.load(Ordering::Acquire) as PbSemaphoreHandle;
        if !semaphore.is_null() {
            unsafe {
                let _ = pb_pin_semaphore_set(semaphore);
            }
        }
    } else if STATE.load(Ordering::Acquire) == CANCELLED {
        STATE.store(IDLE, Ordering::Release);
    }
}

pub fn pending() -> bool {
    matches!(STATE.load(Ordering::Acquire), PENDING | CANCELLED)
}

pub fn timeout_count() -> u64 {
    TIMEOUTS.load(Ordering::Relaxed)
}

pub fn decision_counts() -> (u64, u64, u64) {
    (
        DECISIONS.load(Ordering::Relaxed),
        FOLLOWED.load(Ordering::Relaxed),
        REJECTED.load(Ordering::Relaxed),
    )
}
