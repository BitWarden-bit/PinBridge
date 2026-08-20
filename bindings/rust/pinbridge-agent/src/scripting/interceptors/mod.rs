//! Synchronous Python-controlled events.
//!
//! Pin callbacks publish fixed-size requests and wait for a bounded native
//! rendezvous. This module runs the Python side on the dedicated scripting
//! thread and keeps control-flow decisions separate from telemetry routing.

mod child;
mod debugger;
mod exception;
mod hook;
mod syscall;

use super::decisions::DecisionSelector;
use super::{with_registry, STATE_RUNNING};
use pyo3::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

type HookDispatchKey = (u64, bool, Option<u32>);
type HookDispatchTarget = (String, u64);
type HookDispatchTargetSlice = Arc<[HookDispatchTarget]>;
static HOOK_DISPATCH_INDEX: OnceLock<Mutex<HashMap<HookDispatchKey, HookDispatchTargetSlice>>> =
    OnceLock::new();

fn hook_dispatch_index() -> &'static Mutex<HashMap<HookDispatchKey, HookDispatchTargetSlice>> {
    HOOK_DISPATCH_INDEX.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn publish_interests() {
    let (
        hooks,
        hook_dispatch,
        entry_all,
        entry_numbers,
        exit_all,
        exit_numbers,
        exception_all,
        exception_codes,
        debugger_mask,
    ) = with_registry(|registry| {
        let mut hooks = Vec::new();
        let mut hook_dispatch: BTreeMap<HookDispatchKey, Vec<HookDispatchTarget>> = BTreeMap::new();
        let mut entry_all = false;
        let mut entry_numbers = Vec::new();
        let mut exit_all = false;
        let mut exit_numbers = Vec::new();
        let mut exception_all = false;
        let mut exception_codes = Vec::new();
        let mut debugger_mask = 0u32;
        for (plugin_name, plugin) in registry {
            if plugin.state != STATE_RUNNING {
                continue;
            }
            for (id, subscription) in &plugin.decisions {
                match subscription.selector {
                    DecisionSelector::HookEntry => {
                        if let Some(address) = subscription.address {
                            hooks.push((address, false, subscription.thread_id));
                            hook_dispatch
                                .entry((address, false, subscription.thread_id))
                                .or_default()
                                .push((plugin_name.clone(), *id));
                        }
                    }
                    DecisionSelector::HookReturn => {
                        if let Some(address) = subscription.address {
                            hooks.push((address, true, subscription.thread_id));
                            hook_dispatch
                                .entry((address, true, subscription.thread_id))
                                .or_default()
                                .push((plugin_name.clone(), *id));
                        }
                    }
                    DecisionSelector::SyscallEntry => match &subscription.numbers {
                        Some(numbers) => entry_numbers.extend(numbers.iter().copied()),
                        None => entry_all = true,
                    },
                    DecisionSelector::SyscallExit => match &subscription.numbers {
                        Some(numbers) => exit_numbers.extend(numbers.iter().copied()),
                        None => exit_all = true,
                    },
                    DecisionSelector::ExceptionHandle => match &subscription.codes {
                        Some(codes) => exception_codes.extend(codes.iter().copied()),
                        None => exception_all = true,
                    },
                    DecisionSelector::DebuggerBreakpoint => debugger_mask |= 1 << 0,
                    DecisionSelector::DebuggerSingleStep => debugger_mask |= 1 << 1,
                    DecisionSelector::DebuggerAsyncBreak => debugger_mask |= 1 << 2,
                    DecisionSelector::ChildFollow => {}
                }
            }
        }
        (
            hooks,
            hook_dispatch,
            entry_all,
            entry_numbers,
            exit_all,
            exit_numbers,
            exception_all,
            exception_codes,
            debugger_mask,
        )
    });
    let hook_dispatch = hook_dispatch
        .into_iter()
        .map(|(key, targets)| (key, Arc::from(targets)))
        .collect();
    *hook_dispatch_index()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = hook_dispatch;
    crate::sync_intercept::publish_hook_interests(&hooks);
    crate::sync_intercept::publish_syscall_interests(
        entry_all,
        &entry_numbers,
        exit_all,
        &exit_numbers,
    );
    crate::sync_intercept::publish_exception_interests(exception_all, &exception_codes);
    crate::sync_intercept::publish_debugger_interests(debugger_mask);
}

/// Exact final-stage synchronous Hook routing. The native callback already
/// filtered by sorted (address, kind, TID); this hash index returns only the
/// all-thread and current-thread callback buckets. Arc snapshots avoid
/// cloning a potentially large target vector on every hit.
pub(super) struct HookDispatchBuckets {
    pub all_threads: Option<HookDispatchTargetSlice>,
    pub current_thread: Option<HookDispatchTargetSlice>,
}

pub(super) fn hook_dispatch_targets(
    address: u64,
    is_return: bool,
    thread_id: u32,
) -> HookDispatchBuckets {
    let index = hook_dispatch_index()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    HookDispatchBuckets {
        all_threads: index.get(&(address, is_return, None)).cloned(),
        current_thread: index.get(&(address, is_return, Some(thread_id))).cloned(),
    }
}

pub fn dispatch_pending() {
    if crate::child_process::pending() {
        child::dispatch();
    }
    for _ in 0..16 {
        if !crate::sync_intercept::pending() {
            break;
        }
        dispatch_one();
    }
}

fn dispatch_one() {
    let Some(request) = crate::sync_intercept::take_pending() else {
        return;
    };
    match request.kind {
        crate::sync_intercept::HOOK_ENTRY | crate::sync_intercept::HOOK_RETURN => {
            hook::dispatch(request)
        }
        crate::sync_intercept::SYSCALL_ENTRY | crate::sync_intercept::SYSCALL_EXIT => {
            syscall::dispatch(request)
        }
        crate::sync_intercept::EXCEPTION_HANDLE => exception::dispatch(request),
        crate::sync_intercept::DEBUGGER_BREAKPOINT
        | crate::sync_intercept::DEBUGGER_SINGLE_STEP
        | crate::sync_intercept::DEBUGGER_ASYNC_BREAK => debugger::dispatch(request),
        _ => crate::sync_intercept::complete(
            request.slot,
            request.generation,
            crate::sync_intercept::InterceptResponse::EMPTY,
        ),
    }
}

pub(super) struct Handler {
    pub plugin: String,
    pub id: u64,
    pub callback: Py<PyAny>,
    pub once: bool,
    pub order: u64,
}

pub(super) fn sort_handlers(handlers: &mut [Handler]) {
    handlers.sort_by(|left, right| {
        left.plugin
            .cmp(&right.plugin)
            .then(left.order.cmp(&right.order))
    });
}

pub(super) fn extract_word(value: &Bound<'_, PyAny>, field: &str) -> Result<u64, String> {
    value
        .extract::<u64>()
        .or_else(|_| value.extract::<i64>().map(|signed| signed as u64))
        .map_err(|_| format!("{field} must be an integer fitting 64 bits"))
}

pub(super) fn response_set(
    mask: &mut u32,
    values: &mut [u64],
    index: usize,
    value: u64,
    field: &str,
) -> Result<(), String> {
    let bit = 1u32 << index;
    if *mask & bit != 0 && values[index] != value {
        return Err(format!("conflicting values for {field}"));
    }
    *mask |= bit;
    values[index] = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_hook_dispatch_uses_exact_kind_and_thread_buckets() {
        let mut index = hook_dispatch_index()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        index.clear();
        index.insert(
            (0x1234, false, None),
            Arc::from(vec![("all.py".to_string(), 1)]),
        );
        index.insert(
            (0x1234, false, Some(7)),
            Arc::from(vec![("tid7.py".to_string(), 2)]),
        );
        index.insert(
            (0x1234, true, Some(7)),
            Arc::from(vec![("return.py".to_string(), 3)]),
        );
        drop(index);

        let selected = hook_dispatch_targets(0x1234, false, 7);
        assert_eq!(selected.all_threads.as_deref().unwrap()[0].1, 1);
        assert_eq!(selected.current_thread.as_deref().unwrap()[0].1, 2);

        let wrong_thread = hook_dispatch_targets(0x1234, false, 8);
        assert!(wrong_thread.all_threads.is_some());
        assert!(wrong_thread.current_thread.is_none());

        let wrong_kind = hook_dispatch_targets(0x1234, true, 8);
        assert!(wrong_kind.all_threads.is_none());
        assert!(wrong_kind.current_thread.is_none());

        hook_dispatch_index()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
}
