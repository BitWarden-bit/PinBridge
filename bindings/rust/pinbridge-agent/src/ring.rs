//! Bounded global event ring.
//!
//! Analysis callbacks on every application thread push into this ring.
//! The lock is a **Pin** mutex (pb_pin_mutex_*), not a Rust/Win32 lock:
//! analysis callbacks run on application threads under Pin's VM, and only
//! Pin's own primitives are proven safe there (a std::sync::Mutex in the
//! hot path intermittently killed the process during verification).
//! Per-thread lock-free rings remain the planned optimization.
//!
//! Two hard rules keep the ring from deadlocking the control plane (both
//! learned from field wedges, see query_server.rs handle_ring_page):
//!   1. submit() only TRIES the lock — a blocked submitter drops the event.
//!      A blocking wait lets one preempted holder freeze every application
//!      thread (and anything else that wants the ring) behind it.
//!   2. Readers copy into CALLER-RESERVED buffers — never allocate while
//!      holding a Pin mutex. malloc takes the process-heap lock, and there
//!      are threads in this process that hold the heap lock while blocking
//!      on Pin locks: allocating under a Pin mutex is a classic AB-BA
//!      deadlock (ring mutex -> heap vs heap -> ring mutex).

use crate::event::{Event, EVENT_KIND_COUNT};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

pub const RING_CAPACITY: usize = 65536;

pub struct Ring {
    events: [Event; RING_CAPACITY],
    total: u64,
}

impl Ring {
    pub const fn new() -> Self {
        Ring {
            events: [Event::EMPTY; RING_CAPACITY],
            total: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, mut event: Event) {
        self.total += 1;
        event.sequence = self.total;
        self.events[((self.total - 1) % RING_CAPACITY as u64) as usize] = event;
        // Lock-free mirror of the CONTENT edge (unlike TOTAL_SEQ, counts only
        // events that actually landed). Paging cursors must be based on this:
        // a TOTAL_SEQ-based cursor sails past the content when submits drop.
        RING_TOTAL.store(self.total, Ordering::Release);
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    /// Allocation-free newest-N copy: appends the newest `max` retained
    /// events (oldest first) to `out`.
    pub fn drain_newest_into(&self, max: usize, out: &mut Vec<Event>) {
        let retained = self.total.min(RING_CAPACITY as u64);
        let take = retained.min(max as u64);
        let first = self.total - take; // sequence space: (first, total]
        for sequence in first + 1..=self.total {
            out.push(self.events[((sequence - 1) % RING_CAPACITY as u64) as usize]);
        }
    }

    /// Allocation-free cursor paging: appends up to `limit` retained events
    /// with sequence > `after` (oldest first) to `out`, returning how many
    /// events were skipped because the cursor fell behind the oldest
    /// retained slot.
    pub fn page_into(&self, after: u64, limit: usize, out: &mut Vec<Event>) -> u64 {
        let retained = self.total.min(RING_CAPACITY as u64);
        let oldest = self.total - retained + 1; // first retained sequence
        let first = (after + 1).max(oldest);
        let missed = first.saturating_sub(after + 1);
        if first <= self.total {
            let end = (first + limit as u64 - 1).min(self.total);
            for sequence in first..=end {
                out.push(self.events[((sequence - 1) % RING_CAPACITY as u64) as usize]);
            }
        }
        missed
    }
}

static mut RING: Ring = Ring::new();
static RING_MUTEX: AtomicUsize = AtomicUsize::new(0);
static KIND_COUNTERS: [AtomicU64; EVENT_KIND_COUNT] =
    [const { AtomicU64::new(0) }; EVENT_KIND_COUNT];
/// Lock-free mirror of Ring::total: bumped in submit() without the mutex, so
/// threads that must never block on a Pin lock (script host) can still track
/// the live edge. Counts SUBMIT ATTEMPTS — including events later dropped by
/// the try-lock — so it runs ahead of the actual content.
static TOTAL_SEQ: AtomicU64 = AtomicU64::new(0);
/// Lock-free mirror of the content edge (bumped in push() under the mutex):
/// the sequence of the newest event actually retained. All paging cursors
/// (ring_newest, script host tick, COUNTERS clients) key off THIS, because a
/// TOTAL_SEQ-based cursor lands past the content once try-lock drops begin.
static RING_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Creates the Pin mutex. Must run before instrumentation starts.
pub fn init() -> PbStatus {
    let mut handle: PbMutexHandle = core::ptr::null_mut();
    let status = unsafe { pb_pin_mutex_init(&mut handle) };
    if status == PB_OK {
        RING_MUTEX.store(handle as usize, Ordering::Release);
    }
    status
}

/// Hot-path entry point: record one event. No allocation, no I/O; if the
/// lock is unavailable the event is dropped (counted via `total` ahead of
/// the slot write, so readers can see the gap).
#[inline]
pub fn submit(event: Event) {
    let kind = event.kind as usize;
    if kind < EVENT_KIND_COUNT {
        KIND_COUNTERS[kind].fetch_add(1, Ordering::Relaxed);
    }
    TOTAL_SEQ.fetch_add(1, Ordering::Relaxed);
    let mutex = RING_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if mutex.is_null() {
        return;
    }
    unsafe {
        // try-lock only: never park an application thread on this mutex (see
        // rule 1 above). Drops on contention are the documented design.
        let mut acquired: u8 = 0;
        if pb_pin_mutex_try_lock(mutex, &mut acquired) != PB_OK || acquired == 0 {
            return;
        }
        #[allow(static_mut_refs)]
        let ring = &mut *core::ptr::addr_of_mut!(RING);
        ring.push(event);
        pb_pin_mutex_unlock(mutex);
    }
}

/// Lock-free total events ever submitted (attempts; runs ahead of content
/// when the try-lock drops). Readers must tolerate that.
pub fn total_seq() -> u64 {
    TOTAL_SEQ.load(Ordering::Relaxed)
}

/// Lock-free content edge: sequence of the newest event actually retained.
/// The cursor base for all paging (see RING_TOTAL above).
pub fn ring_total() -> u64 {
    RING_TOTAL.load(Ordering::Acquire)
}

/// Bounded mutex acquisition for ring readers: submitters release the lock
/// between events, so a short spin rides through momentary flood contention
/// instead of answering "busy" almost every time. Never parks — after
/// SPIN_LIMIT misses the caller gets the busy signal.
const SPIN_LIMIT: u32 = 1024;

unsafe fn lock_spin(mutex: PbMutexHandle) -> bool {
    let mut acquired: u8 = 0;
    for _ in 0..SPIN_LIMIT {
        if pb_pin_mutex_try_lock(mutex, &mut acquired) != PB_OK {
            return false;
        }
        if acquired != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// Non-blocking cursor page for readers that must never park on the ring
/// mutex (query server, script host, fini). The caller MUST reserve `limit`
/// capacity in `out` BEFORE calling, so the critical section is a plain
/// memcpy (see rule 2 above). Returns None when the mutex stays busy past
/// the spin limit (caller retries later); on success Some((missed, total)).
pub fn try_page(after: u64, limit: usize, out: &mut Vec<Event>) -> Option<(u64, u64)> {
    let mutex = RING_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if mutex.is_null() {
        return None;
    }
    unsafe {
        if !lock_spin(mutex) {
            return None;
        }
        #[allow(static_mut_refs)]
        let ring = &*core::ptr::addr_of!(RING);
        let missed = ring.page_into(after, limit, out);
        let total = ring.total();
        pb_pin_mutex_unlock(mutex);
        Some((missed, total))
    }
}

/// Non-blocking newest-N copy (same locking/allocation rules as try_page).
pub fn try_drain_newest(max: usize, out: &mut Vec<Event>) -> Option<()> {
    let mutex = RING_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if mutex.is_null() {
        return None;
    }
    unsafe {
        if !lock_spin(mutex) {
            return None;
        }
        #[allow(static_mut_refs)]
        let ring = &*core::ptr::addr_of!(RING);
        ring.drain_newest_into(max, out);
        pb_pin_mutex_unlock(mutex);
        Some(())
    }
}

pub fn kind_count(kind: usize) -> u64 {
    KIND_COUNTERS[kind].load(Ordering::Relaxed)
}
