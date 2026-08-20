//! Dedicated bounded lane for one-click function-call logs.
//!
//! Function entry/return records must remain visible even while instruction,
//! memory, or branch tracing floods the compatibility ring. Producers only
//! perform a try-lock and fixed POD write. The wider record carries sixteen
//! signature-resolved arguments without changing the generic event ABI.

use crate::event::{Event, EVENT_HOOK_REGS, EVENT_HOOK_RETURN, EVENT_SYSCALL};
use crate::ring::lock_spin;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use pinbridge_proto::{
    HookLogRecord, HOOK_LOG_FLAG_FUNCTION, HOOK_LOG_FLAG_SIGNATURE, HOOK_LOG_FLAG_SYSCALL,
};
use pinbridge_sys::*;

pub const HOOK_EVENT_CAPACITY: usize = 32_768;

#[repr(C)]
struct FileTime {
    low: u32,
    high: u32,
}

extern "system" {
    fn GetSystemTimePreciseAsFileTime(time: *mut FileTime);
}

#[inline]
fn unix_time_ns() -> u64 {
    const WINDOWS_TO_UNIX_100NS: u64 = 116_444_736_000_000_000;
    let mut time = FileTime { low: 0, high: 0 };
    unsafe { GetSystemTimePreciseAsFileTime(&mut time) };
    let ticks = ((time.high as u64) << 32) | time.low as u64;
    ticks
        .saturating_sub(WINDOWS_TO_UNIX_100NS)
        .saturating_mul(100)
}

struct HookRing {
    events: [HookLogRecord; HOOK_EVENT_CAPACITY],
    total: u64,
}

impl HookRing {
    const fn new() -> Self {
        Self {
            events: [HookLogRecord::EMPTY; HOOK_EVENT_CAPACITY],
            total: 0,
        }
    }

    fn push(&mut self, mut event: HookLogRecord) -> u64 {
        self.total += 1;
        event.sequence = self.total;
        self.events[((self.total - 1) % HOOK_EVENT_CAPACITY as u64) as usize] = event;
        self.total
    }

    fn page_into(&self, after: u64, limit: usize, out: &mut Vec<HookLogRecord>) -> u64 {
        let retained = self.total.min(HOOK_EVENT_CAPACITY as u64);
        let oldest = self.total - retained + 1;
        let first = (after + 1).max(oldest);
        let missed = first.saturating_sub(after + 1);
        if first <= self.total {
            let end = (first + limit as u64 - 1).min(self.total);
            for sequence in first..=end {
                out.push(self.events[((sequence - 1) % HOOK_EVENT_CAPACITY as u64) as usize]);
            }
        }
        missed
    }
}

struct HookChannel {
    ring: UnsafeCell<HookRing>,
    mutex: AtomicUsize,
    submitted: AtomicU64,
    retained_total: AtomicU64,
}

unsafe impl Sync for HookChannel {}

impl HookChannel {
    const fn new() -> Self {
        Self {
            ring: UnsafeCell::new(HookRing::new()),
            mutex: AtomicUsize::new(0),
            submitted: AtomicU64::new(0),
            retained_total: AtomicU64::new(0),
        }
    }

    fn init(&self) -> PbStatus {
        let mut handle: PbMutexHandle = core::ptr::null_mut();
        let status = unsafe { pb_pin_mutex_init(&mut handle) };
        if status == PB_OK {
            self.mutex.store(handle as usize, Ordering::Release);
        }
        status
    }

    #[inline]
    fn submit(&self, mut event: HookLogRecord) {
        self.submitted.fetch_add(1, Ordering::Relaxed);
        let mutex = self.mutex.load(Ordering::Acquire) as PbMutexHandle;
        if mutex.is_null() {
            return;
        }
        unsafe {
            let mut acquired = 0u8;
            if pb_pin_mutex_try_lock(mutex, &mut acquired) != PB_OK || acquired == 0 {
                return;
            }
            // Read the clock only for a record that will actually land. Under
            // a Hook flood, contended producers now shed the event before the
            // relatively expensive precise system-time call.
            event.timestamp_unix_ns = unix_time_ns();
            let total = (&mut *self.ring.get()).push(event);
            self.retained_total.store(total, Ordering::Release);
            let _ = pb_pin_mutex_unlock(mutex);
        }
    }

    fn total(&self) -> u64 {
        self.retained_total.load(Ordering::Acquire)
    }

    fn dropped(&self) -> u64 {
        self.submitted
            .load(Ordering::Relaxed)
            .saturating_sub(self.retained_total.load(Ordering::Acquire))
    }

    fn try_page(
        &self,
        after: u64,
        limit: usize,
        out: &mut Vec<HookLogRecord>,
    ) -> Option<(u64, u64)> {
        let mutex = self.mutex.load(Ordering::Acquire) as PbMutexHandle;
        if mutex.is_null() {
            return None;
        }
        unsafe {
            if !lock_spin(mutex) {
                return None;
            }
            let ring = &*self.ring.get();
            let missed = ring.page_into(after, limit, out);
            let total = ring.total;
            let _ = pb_pin_mutex_unlock(mutex);
            Some((missed, total))
        }
    }
}

static CHANNEL: HookChannel = HookChannel::new();
static SYSCALL_CHANNEL: HookChannel = HookChannel::new();

pub fn init() -> PbStatus {
    let status = CHANNEL.init();
    if status != PB_OK {
        return status;
    }
    SYSCALL_CHANNEL.init()
}

/// Compatibility/raw ABI event: preserve its eight generic slots.
#[inline]
pub fn submit(event: Event) {
    submit_with_flags(event, 0);
}

/// Raw ABI function record. This remains distinguishable from an instruction
/// Hook even when no signature is registered yet.
#[inline]
pub fn submit_function(event: Event) {
    submit_with_flags(event, HOOK_LOG_FLAG_FUNCTION);
}

/// Timestamped Syscall record retained independently from both the generic
/// telemetry ring and the Hook/API lane. Arguments 0..7 preserve the generic
/// Syscall payload; argument 8 carries the Syscall generation formerly stored
/// in Event::address.
#[inline]
pub fn submit_syscall(event: Event) {
    if event.kind != EVENT_SYSCALL {
        return;
    }
    let mut arguments = [0u64; 16];
    arguments[..8].copy_from_slice(&[
        event.arg0, event.arg1, event.arg2, event.arg3, event.arg4, event.arg5, event.arg6,
        event.arg7,
    ]);
    arguments[8] = event.address;
    SYSCALL_CHANNEL.submit(HookLogRecord {
        kind: EVENT_SYSCALL,
        thread_id: event.thread_id,
        address: 0,
        argument_count: 9,
        flags: HOOK_LOG_FLAG_SYSCALL,
        arguments,
        ..HookLogRecord::EMPTY
    });
}

#[inline]
fn submit_with_flags(event: Event, flags: u32) {
    let mut arguments = [0u64; 16];
    arguments[..8].copy_from_slice(&[
        event.arg0, event.arg1, event.arg2, event.arg3, event.arg4, event.arg5, event.arg6,
        event.arg7,
    ]);
    CHANNEL.submit(HookLogRecord {
        kind: event.kind,
        thread_id: event.thread_id,
        address: event.address,
        argument_count: if event.kind == EVENT_HOOK_RETURN {
            0
        } else {
            8
        },
        flags,
        arguments,
        ..HookLogRecord::EMPTY
    });
}

#[inline]
pub fn submit_signature_entry(
    address: u64,
    thread_id: u32,
    argument_count: u32,
    arguments: [u64; 16],
) {
    CHANNEL.submit(HookLogRecord {
        kind: EVENT_HOOK_REGS,
        thread_id,
        address,
        argument_count: argument_count.min(16),
        flags: HOOK_LOG_FLAG_SIGNATURE | HOOK_LOG_FLAG_FUNCTION,
        arguments,
        ..HookLogRecord::EMPTY
    });
}

#[inline]
pub fn submit_signature_return(address: u64, thread_id: u32, return_value: u64) {
    let mut arguments = [0u64; 16];
    arguments[0] = return_value;
    CHANNEL.submit(HookLogRecord {
        kind: EVENT_HOOK_RETURN,
        thread_id,
        address,
        flags: HOOK_LOG_FLAG_SIGNATURE | HOOK_LOG_FLAG_FUNCTION,
        arguments,
        ..HookLogRecord::EMPTY
    });
}

pub fn total() -> u64 {
    CHANNEL.total()
}

pub fn dropped() -> u64 {
    CHANNEL.dropped()
}

pub fn try_page(after: u64, limit: usize, out: &mut Vec<HookLogRecord>) -> Option<(u64, u64)> {
    CHANNEL.try_page(after, limit, out)
}

pub fn syscall_total() -> u64 {
    SYSCALL_CHANNEL.total()
}

pub fn syscall_dropped() -> u64 {
    SYSCALL_CHANNEL.dropped()
}

pub fn try_syscall_page(
    after: u64,
    limit: usize,
    out: &mut Vec<HookLogRecord>,
) -> Option<(u64, u64)> {
    SYSCALL_CHANNEL.try_page(after, limit, out)
}
