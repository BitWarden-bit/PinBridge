//! Python subscriptions whose return value controls a synchronous Pin event.
//!
//! These are intentionally separate from `pb.on`: notification handlers do
//! not own control flow, while interceptors have a bounded native waiter and
//! an explicit conservative fallback.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use pyo3::prelude::*;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static PYTHON_DECISION_ACTIVE: AtomicBool = AtomicBool::new(false);

pub const PUBLIC_DECISION_NAMES: [&str; 1] = ["child.follow"];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DecisionSelector {
    ChildFollow,
}

impl DecisionSelector {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "child.follow" | "child_follow" | "follow_child" => Some(Self::ChildFollow),
            _ => None,
        }
    }
}

pub struct DecisionSubscription {
    pub selector: DecisionSelector,
    pub callback: Py<PyAny>,
    pub once: bool,
    pub order: u64,
}

impl DecisionSubscription {
    pub fn new(selector: DecisionSelector, callback: Py<PyAny>, once: bool) -> (u64, Self) {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        (
            id,
            Self {
                selector,
                callback,
                once,
                order: id,
            },
        )
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
        assert_eq!(DecisionSelector::parse("unknown"), None);
    }
}
