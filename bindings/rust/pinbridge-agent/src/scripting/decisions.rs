//! Python subscriptions whose return value controls a synchronous Pin event.
//!
//! These are intentionally separate from `pb.on`: notification handlers do
//! not own control flow, while interceptors have a bounded native waiter and
//! an explicit conservative fallback.

use crate::{new_map, TlsFreeMap, TlsFreeSet};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use pyo3::prelude::*;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static PYTHON_DECISION_ACTIVE: AtomicBool = AtomicBool::new(false);

pub const PUBLIC_DECISION_NAMES: [&str; 9] = [
    "child.follow",
    "hook.entry",
    "hook.return",
    "syscall.entry",
    "syscall.exit",
    "exception.handle",
    "debugger.breakpoint",
    "debugger.single_step",
    "debugger.async_break",
];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DecisionSelector {
    ChildFollow,
    HookEntry,
    HookReturn,
    SyscallEntry,
    SyscallExit,
    ExceptionHandle,
    DebuggerBreakpoint,
    DebuggerSingleStep,
    DebuggerAsyncBreak,
}

impl DecisionSelector {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "child.follow" | "child_follow" | "follow_child" => Some(Self::ChildFollow),
            "hook.entry" | "hook_entry" => Some(Self::HookEntry),
            "hook.return" | "hook_return" => Some(Self::HookReturn),
            "syscall.entry" | "syscall_entry" => Some(Self::SyscallEntry),
            "syscall.exit" | "syscall_exit" => Some(Self::SyscallExit),
            "exception.handle" | "exception_handle" | "exception.intercept" => {
                Some(Self::ExceptionHandle)
            }
            "debugger.breakpoint" | "debugger_breakpoint" => Some(Self::DebuggerBreakpoint),
            "debugger.single_step" | "debugger_single_step" => Some(Self::DebuggerSingleStep),
            "debugger.async_break" | "debugger_async_break" => Some(Self::DebuggerAsyncBreak),
            _ => None,
        }
    }

    pub fn is_hook(self) -> bool {
        matches!(self, Self::HookEntry | Self::HookReturn)
    }

    pub fn is_debugger(self) -> bool {
        matches!(
            self,
            Self::DebuggerBreakpoint | Self::DebuggerSingleStep | Self::DebuggerAsyncBreak
        )
    }
}

pub struct DecisionSubscription {
    pub selector: DecisionSelector,
    pub callback: Py<PyAny>,
    pub once: bool,
    pub order: u64,
    pub address: Option<u64>,
    pub thread_id: Option<u32>,
    pub numbers: Option<TlsFreeSet<u32>>,
    pub codes: Option<TlsFreeSet<u32>>,
}

impl DecisionSubscription {
    pub fn new(
        selector: DecisionSelector,
        callback: Py<PyAny>,
        once: bool,
        address: Option<u64>,
        thread_id: Option<u32>,
        numbers: Option<TlsFreeSet<u32>>,
        codes: Option<TlsFreeSet<u32>>,
    ) -> (u64, Self) {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        (
            id,
            Self {
                selector,
                callback,
                once,
                order: id,
                address,
                thread_id,
                numbers,
                codes,
            },
        )
    }
}

#[derive(Clone, Copy)]
struct HookLease {
    subscribers: u32,
    remove_at_zero: bool,
}

impl HookLease {
    fn acquire(&mut self, created_by_scripts: bool) {
        self.subscribers = self.subscribers.saturating_add(1);
        self.remove_at_zero |= created_by_scripts;
    }

    fn release(&mut self) -> bool {
        self.subscribers = self.subscribers.saturating_sub(1);
        self.subscribers == 0 && self.remove_at_zero
    }
}

static mut HOOK_LEASES: Option<TlsFreeMap<u64, HookLease>> = None;
static mut PENDING_HOOK_REMOVALS: Option<Vec<u64>> = None;

fn with_hook_leases<R>(f: impl FnOnce(&mut TlsFreeMap<u64, HookLease>) -> R) -> R {
    unsafe {
        let leases = &mut *core::ptr::addr_of_mut!(HOOK_LEASES);
        f(leases.get_or_insert_with(new_map))
    }
}

pub fn acquire_hook(address: u64, created_by_scripts: bool) {
    let revived = cancel_pending_hook_removal(address);
    with_hook_leases(|leases| {
        leases
            .entry(address)
            .and_modify(|lease| lease.acquire(created_by_scripts || revived))
            .or_insert(HookLease {
                subscribers: 1,
                remove_at_zero: created_by_scripts || revived,
            });
    });
}

fn cancel_pending_hook_removal(address: u64) -> bool {
    unsafe {
        let pending = &mut *core::ptr::addr_of_mut!(PENDING_HOOK_REMOVALS);
        let Some(pending) = pending.as_mut() else {
            return false;
        };
        let before = pending.len();
        pending.retain(|pending_address| *pending_address != address);
        before != pending.len()
    }
}

pub fn release_hook(address: u64) -> bool {
    with_hook_leases(|leases| {
        let remove = leases
            .get_mut(&address)
            .map(HookLease::release)
            .unwrap_or(false);
        if leases
            .get(&address)
            .map(|lease| lease.subscribers == 0)
            .unwrap_or(false)
        {
            leases.remove(&address);
        }
        remove
    })
}

pub fn queue_hook_removal(address: u64) {
    unsafe {
        let pending = &mut *core::ptr::addr_of_mut!(PENDING_HOOK_REMOVALS);
        let pending = pending.get_or_insert_with(Vec::new);
        if !pending.contains(&address) {
            pending.push(address);
        }
    }
}

pub fn take_hook_removals() -> Vec<u64> {
    unsafe {
        (*core::ptr::addr_of_mut!(PENDING_HOOK_REMOVALS))
            .take()
            .unwrap_or_default()
    }
}

pub fn has_hook_removals() -> bool {
    unsafe {
        (*core::ptr::addr_of!(PENDING_HOOK_REMOVALS))
            .as_ref()
            .map(|pending| !pending.is_empty())
            .unwrap_or(false)
    }
}

pub struct PythonDecisionGuard;

impl PythonDecisionGuard {
    pub fn enter() -> Self {
        PYTHON_DECISION_ACTIVE.store(true, Ordering::Release);
        Self
    }
}

impl Drop for PythonDecisionGuard {
    fn drop(&mut self) {
        PYTHON_DECISION_ACTIVE.store(false, Ordering::Release);
    }
}

pub fn python_decision_active() -> bool {
    PYTHON_DECISION_ACTIVE.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_decision_names_parse() {
        assert_eq!(
            DecisionSelector::parse("child.follow"),
            Some(DecisionSelector::ChildFollow)
        );
        assert_eq!(
            DecisionSelector::parse("follow_child"),
            Some(DecisionSelector::ChildFollow)
        );
        assert_eq!(
            DecisionSelector::parse("hook.entry"),
            Some(DecisionSelector::HookEntry)
        );
        assert_eq!(
            DecisionSelector::parse("hook_return"),
            Some(DecisionSelector::HookReturn)
        );
        assert_eq!(
            DecisionSelector::parse("syscall.entry"),
            Some(DecisionSelector::SyscallEntry)
        );
        assert_eq!(
            DecisionSelector::parse("exception.handle"),
            Some(DecisionSelector::ExceptionHandle)
        );
        assert_eq!(
            DecisionSelector::parse("debugger.single_step"),
            Some(DecisionSelector::DebuggerSingleStep)
        );
        assert_eq!(DecisionSelector::parse("unknown"), None);
    }

    #[test]
    fn hook_lease_keeps_a_script_owned_point_until_the_last_consumer() {
        let mut lease = HookLease {
            subscribers: 1,
            remove_at_zero: true,
        };
        lease.acquire(false);
        assert!(!lease.release());
        assert!(lease.release());

        let mut external = HookLease {
            subscribers: 1,
            remove_at_zero: false,
        };
        external.acquire(false);
        assert!(!external.release());
        assert!(!external.release());
    }
}
