//! Dedicated bounded queue for rare events that must not be displaced by
//! instruction/memory telemetry.
//!
//! Producers obey the same hot-path rules as the main ring: fixed POD writes,
//! no allocation, and a try-only Pin mutex.  The scripting host drains this
//! queue before the normal telemetry page on every tick.

use crate::event::Event;
use crate::ring::{lock_spin, Ring};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

pub const PRIORITY_CAPACITY: usize = 4096;

static mut PRIORITY_RING: Ring<PRIORITY_CAPACITY> = Ring::new();
static PRIORITY_MUTEX: AtomicUsize = AtomicUsize::new(0);
static SUBMITTED: AtomicU64 = AtomicU64::new(0);
static RETAINED_TOTAL: AtomicU64 = AtomicU64::new(0);

pub fn init() -> PbStatus {
    let mut handle: PbMutexHandle = core::ptr::null_mut();
    let status = unsafe { pb_pin_mutex_init(&mut handle) };
    if status == PB_OK {
        PRIORITY_MUTEX.store(handle as usize, Ordering::Release);
    }
    status
}

#[inline]
pub fn submit(event: Event) {
    SUBMITTED.fetch_add(1, Ordering::Relaxed);
    let mutex = PRIORITY_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if mutex.is_null() {
        return;
    }
    unsafe {
        let mut acquired = 0u8;
        if pb_pin_mutex_try_lock(mutex, &mut acquired) != PB_OK || acquired == 0 {
            return;
        }
        #[allow(static_mut_refs)]
        let ring = &mut *core::ptr::addr_of_mut!(PRIORITY_RING);
        let total = ring.push(event);
        RETAINED_TOTAL.store(total, Ordering::Release);
        let _ = pb_pin_mutex_unlock(mutex);
    }
}

pub fn total() -> u64 {
    RETAINED_TOTAL.load(Ordering::Acquire)
}

pub fn dropped() -> u64 {
    SUBMITTED
        .load(Ordering::Relaxed)
        .saturating_sub(RETAINED_TOTAL.load(Ordering::Acquire))
}

pub fn try_page(after: u64, limit: usize, out: &mut Vec<Event>) -> Option<(u64, u64)> {
    let mutex = PRIORITY_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if mutex.is_null() {
        return None;
    }
    unsafe {
        if !lock_spin(mutex) {
            return None;
        }
        #[allow(static_mut_refs)]
        let ring = &*core::ptr::addr_of!(PRIORITY_RING);
        let missed = ring.page_into(after, limit, out);
        let total = ring.total();
        let _ = pb_pin_mutex_unlock(mutex);
        Some((missed, total))
    }
}
