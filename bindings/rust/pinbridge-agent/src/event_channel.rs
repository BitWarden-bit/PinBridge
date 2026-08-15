//! Reusable bounded event lane backed by a Pin mutex.
//!
//! Analysis callbacks only try the mutex and never allocate or wait. Readers
//! reserve their output buffer before entering the bounded spin section. The
//! same primitive backs rare priority notifications and native-filtered
//! observation streams without letting either class evict the other.

use crate::event::Event;
use crate::ring::{lock_spin, Ring};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

pub struct EventChannel<const CAPACITY: usize> {
    ring: UnsafeCell<Ring<CAPACITY>>,
    mutex: AtomicUsize,
    submitted: AtomicU64,
    retained_total: AtomicU64,
}

// The ring is accessed only while its Pin mutex is held. The atomics are the
// lock-free publication surface used outside that critical section.
unsafe impl<const CAPACITY: usize> Sync for EventChannel<CAPACITY> {}

impl<const CAPACITY: usize> EventChannel<CAPACITY> {
    pub const fn new() -> Self {
        Self {
            ring: UnsafeCell::new(Ring::new()),
            mutex: AtomicUsize::new(0),
            submitted: AtomicU64::new(0),
            retained_total: AtomicU64::new(0),
        }
    }

    pub fn init(&self) -> PbStatus {
        let mut handle: PbMutexHandle = core::ptr::null_mut();
        let status = unsafe { pb_pin_mutex_init(&mut handle) };
        if status == PB_OK {
            self.mutex.store(handle as usize, Ordering::Release);
        }
        status
    }

    #[inline]
    pub fn submit(&self, event: Event) {
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
            let total = (&mut *self.ring.get()).push(event);
            self.retained_total.store(total, Ordering::Release);
            let _ = pb_pin_mutex_unlock(mutex);
        }
    }

    pub fn total(&self) -> u64 {
        self.retained_total.load(Ordering::Acquire)
    }

    pub fn dropped(&self) -> u64 {
        self.submitted
            .load(Ordering::Relaxed)
            .saturating_sub(self.retained_total.load(Ordering::Acquire))
    }

    pub fn try_page(
        &self,
        after: u64,
        limit: usize,
        out: &mut Vec<Event>,
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
            let total = ring.total();
            let _ = pb_pin_mutex_unlock(mutex);
            Some((missed, total))
        }
    }
}
