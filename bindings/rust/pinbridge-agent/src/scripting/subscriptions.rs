//! Typed script subscriptions and stop-decision aggregation.
//!
//! This module owns no Pin handles and performs no target operation.  It is
//! deliberately limited to script-thread data: Python callbacks, per-plugin
//! breakpoint options, and the lease count used to share one native
//! breakpoint between plugins.

use crate::{new_map, TlsFreeMap};
use core::sync::atomic::{AtomicU64, Ordering};
use pyo3::prelude::*;

static NEXT_ORDER: AtomicU64 = AtomicU64::new(1);

pub struct BreakpointSubscription {
    pub callback: Py<PyAny>,
    pub once: bool,
    pub thread_id: Option<u32>,
    pub order: u64,
}

impl BreakpointSubscription {
    pub fn new(callback: Py<PyAny>, once: bool, thread_id: Option<u32>) -> Self {
        Self {
            callback,
            once,
            thread_id,
            order: NEXT_ORDER.fetch_add(1, Ordering::Relaxed),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StopAction {
    Stay,
    Resume,
    StepInto,
    StepOver,
}

impl StopAction {
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "stay" | "stop" => Some(Self::Stay),
            "resume" | "continue" => Some(Self::Resume),
            "step_into" | "step" | "into" => Some(Self::StepInto),
            "step_over" | "over" => Some(Self::StepOver),
            _ => None,
        }
    }
}

/// Combines decisions from every handler attached to one stop.  A missing
/// return is represented as Stay by the caller.  Conflicting active actions
/// stay stopped; silently choosing one plugin over another is unsafe.
pub fn merge_action(current: Option<StopAction>, next: StopAction) -> StopAction {
    let Some(current) = current else {
        return next;
    };
    if current == next {
        return current;
    }
    if current == StopAction::Stay || next == StopAction::Stay {
        return StopAction::Stay;
    }
    // resume versus a single step can safely choose the more restrictive
    // step.  Two different step requests are ambiguous and remain stopped.
    if current == StopAction::Resume {
        return next;
    }
    if next == StopAction::Resume {
        return current;
    }
    StopAction::Stay
}

#[derive(Copy, Clone)]
struct NativeLease {
    subscribers: u32,
    /// The breakpoint did not exist before the first script subscription,
    /// so the script host is responsible for removing it at zero users.
    remove_at_zero: bool,
}

impl NativeLease {
    fn acquire(&mut self, created_by_scripts: bool) {
        self.subscribers = self.subscribers.saturating_add(1);
        self.remove_at_zero |= created_by_scripts;
    }

    fn release(&mut self) -> bool {
        self.subscribers = self.subscribers.saturating_sub(1);
        self.subscribers == 0 && self.remove_at_zero
    }
}

// Script-thread only.  This cannot be thread_local because the privately
// mapped agent has no usable Rust TLS slot on Pin internal threads.
static mut NATIVE_LEASES: Option<TlsFreeMap<u32, NativeLease>> = None;
static mut PENDING_NATIVE_REMOVALS: Option<Vec<u32>> = None;

fn with_leases<R>(f: impl FnOnce(&mut TlsFreeMap<u32, NativeLease>) -> R) -> R {
    unsafe {
        let leases = &mut *core::ptr::addr_of_mut!(NATIVE_LEASES);
        f(leases.get_or_insert_with(new_map))
    }
}

/// Adds one Python subscription to a native breakpoint id.  Call only when
/// the plugin did not already have a handler for this id.
pub fn acquire_native(id: u32, created_by_scripts: bool) {
    // A replacement plugin may bind the same address before the next host
    // tick processes the old plugin's deferred removal.  Revive that lease
    // instead of removing a breakpoint underneath the new handler.
    let revived = cancel_pending_removal(id);
    let created_by_scripts = created_by_scripts || revived;
    with_leases(|leases| {
        leases
            .entry(id)
            .and_modify(|lease| lease.acquire(created_by_scripts))
            .or_insert(NativeLease {
                subscribers: 1,
                remove_at_zero: created_by_scripts,
            });
    });
}

fn cancel_pending_removal(id: u32) -> bool {
    unsafe {
        let pending = &mut *core::ptr::addr_of_mut!(PENDING_NATIVE_REMOVALS);
        let Some(pending) = pending.as_mut() else {
            return false;
        };
        let before = pending.len();
        pending.retain(|pending_id| *pending_id != id);
        before != pending.len()
    }
}

/// Releases one subscription.  True means the native breakpoint should be
/// removed now; false means another plugin or a legacy owner still needs it.
pub fn release_native(id: u32) -> bool {
    with_leases(|leases| {
        let remove_native = leases.get_mut(&id).map(NativeLease::release).unwrap_or(false);
        if leases.get(&id).map(|lease| lease.subscribers == 0).unwrap_or(false) {
            leases.remove(&id);
        }
        remove_native
    })
}

/// Plugin unload can occur while the query server is waiting for the script
/// mailbox, so it cannot issue a loopback BP_REMOVE immediately.  Queue the
/// id for the next normal host tick instead.
pub fn queue_native_removal(id: u32) {
    unsafe {
        let pending = &mut *core::ptr::addr_of_mut!(PENDING_NATIVE_REMOVALS);
        let pending = pending.get_or_insert_with(Vec::new);
        if !pending.contains(&id) {
            pending.push(id);
        }
    }
}

pub fn take_native_removals() -> Vec<u32> {
    unsafe {
        (*core::ptr::addr_of_mut!(PENDING_NATIVE_REMOVALS))
            .take()
            .unwrap_or_default()
    }
}

pub fn has_native_removals() -> bool {
    unsafe {
        (*core::ptr::addr_of!(PENDING_NATIVE_REMOVALS))
            .as_ref()
            .map(|pending| !pending.is_empty())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_action_names_are_explicit() {
        assert_eq!(StopAction::from_name("stay"), Some(StopAction::Stay));
        assert_eq!(StopAction::from_name("continue"), Some(StopAction::Resume));
        assert_eq!(StopAction::from_name("step_into"), Some(StopAction::StepInto));
        assert_eq!(StopAction::from_name("over"), Some(StopAction::StepOver));
        assert_eq!(StopAction::from_name("skip"), None);
    }

    #[test]
    fn conservative_action_aggregation() {
        assert_eq!(merge_action(None, StopAction::Resume), StopAction::Resume);
        assert_eq!(
            merge_action(Some(StopAction::Resume), StopAction::StepInto),
            StopAction::StepInto
        );
        assert_eq!(
            merge_action(Some(StopAction::StepInto), StopAction::StepOver),
            StopAction::Stay
        );
        assert_eq!(
            merge_action(Some(StopAction::Resume), StopAction::Stay),
            StopAction::Stay
        );
    }

    #[test]
    fn native_lease_removes_only_script_owned_last_user() {
        let mut owned = NativeLease {
            subscribers: 1,
            remove_at_zero: true,
        };
        owned.acquire(false);
        assert!(!owned.release());
        assert!(owned.release());

        let mut legacy = NativeLease {
            subscribers: 1,
            remove_at_zero: false,
        };
        assert!(!legacy.release());
    }
}
