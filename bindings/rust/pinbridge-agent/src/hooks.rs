//! Runtime-managed hook point set (HOOK_* ops): addresses that get a
//! register-capture call inserted at instrumentation time. This powers
//! "hook all exports of a module" without spending breakpoint slots.
//! Hits surface as the existing kind-1 (hook_regs) events — this module
//! only decides *where* capture calls get inserted; the analysis callback
//! itself lives in engines.rs (on_hook_regs, unchanged arg layout
//! rcx/rdx/r8/r9).
//!
//! Two copies of the set:
//!   - master: sorted Vec behind a std mutex, touched only by the
//!     query-server/init threads;
//!   - snapshot: immutable boxed sorted Vec behind an atomic pointer, read
//!     lock-free by the instrumentation callback (hot: runs on application
//!     threads during JIT — no locks, no allocation, no I/O there).
//! Writers build a fresh snapshot and swap the pointer; retired snapshots
//! are kept forever (a preempted lock-free reader makes reclamation unsafe).
//! Set changes flush the JIT range of the affected address (same re-JIT
//! technique as bp.rs) so instrumentation re-evaluates it.

use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use pinbridge_sys::*;

pub const MAX_HOOK_POINTS: usize = 4096;

static MASTER: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
static SNAPSHOT: AtomicPtr<Vec<u64>> = AtomicPtr::new(core::ptr::null_mut());
/// Lock-free mirror of the master length for the on_ins pre-check.
static COUNT: AtomicUsize = AtomicUsize::new(0);
/// Snapshots retired by earlier swaps (as raw addresses; *mut T is not
/// Send), freed on the next update. Touched by the query-server thread only.
static RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

fn lock_master() -> std::sync::MutexGuard<'static, Vec<u64>> {
    MASTER.lock().unwrap_or_else(|e| e.into_inner())
}

/// Publishes a fresh snapshot of `master` (query-server thread only).
fn publish(master: &[u64]) {
    let snapshot = Box::new(master.to_vec());
    let old = SNAPSHOT.swap(Box::into_raw(snapshot), Ordering::AcqRel);
    COUNT.store(master.len(), Ordering::Release);
    // Retired snapshots are NEVER freed: readers are analysis/instrumentation
    // callbacks that load the pointer lock-free and can be preempted (or
    // suspended by the breaker) between the load and the read for an
    // unbounded time, so no reclamation point is provably safe without
    // hazard slots. Hook updates are rare and user-driven (each snapshot is
    // at most 4096 u64s); retiring permanently trades a bounded leak for
    // freedom from use-after-free reads. ("Freed on the next update" was a
    // real UAF window: two quick swaps freed a snapshot a preempted reader
    // could still hold.)
    if !old.is_null() {
        let mut retired = RETIRED.lock().unwrap_or_else(|e| e.into_inner());
        retired.push(old as usize);
    }
}

/// Fast pre-check for on_ins: skip the pointer chase when no hooks exist.
#[inline]
pub fn any() -> bool {
    COUNT.load(Ordering::Acquire) > 0
}

/// Lock-free membership check for the instrumentation callback.
#[inline]
pub fn contains(address: u64) -> bool {
    let snapshot = SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() {
        return false;
    }
    unsafe { &*snapshot }.binary_search(&address).is_ok()
}

/// Forces re-JIT of one address so on_ins re-evaluates it.
fn flush(address: u64) {
    unsafe {
        pb_pin_remove_instrumentation_in_range(address, address + 15);
    }
}

/// Adds a hook point. Idempotent: an already-hooked address returns true.
/// Returns false when the set is full (MAX_HOOK_POINTS).
pub fn set(address: u64) -> bool {
    let mut master = lock_master();
    if master.binary_search(&address).is_ok() {
        return true;
    }
    if master.len() >= MAX_HOOK_POINTS {
        return false;
    }
    master.push(address);
    master.sort_unstable();
    publish(&master);
    drop(master);
    flush(address);
    true
}

/// Removes a hook point (no-op when absent).
pub fn remove(address: u64) {
    let mut master = lock_master();
    if let Ok(index) = master.binary_search(&address) {
        master.remove(index);
        publish(&master);
        drop(master);
        flush(address);
    }
}

/// Removes all hook points, flushing each so stale capture calls die.
pub fn clear() {
    let mut master = lock_master();
    if master.is_empty() {
        return;
    }
    let addresses = master.clone();
    master.clear();
    publish(&master);
    drop(master);
    for address in addresses {
        flush(address);
    }
}

/// Current hook points, sorted.
pub fn list() -> Vec<u64> {
    lock_master().clone()
}
