//! Coordination for Python-configured policies that execute in Pin callbacks.

use pinbridge_sys::{PbStatus, PB_OK};

pub fn initialize() -> PbStatus {
    let status = super::memory_translation::initialize();
    if status != PB_OK {
        return status;
    }
    super::xed_decode::initialize()
}

pub fn reregister_after_attach() -> PbStatus {
    let status = super::memory_translation::reregister_after_attach();
    if status != PB_OK {
        return status;
    }
    super::xed_decode::reregister_after_attach()
}

pub fn refresh_best_effort(reason: &str) {
    if let Err(status) = super::instrumentation::publish() {
        if status != PB_OK {
            crate::log::line(&format!(
                "instrumentation policy refresh failed ({reason}) -> {status}"
            ));
        }
    }
    if let Err(status) = super::memory_translation::publish() {
        if status != PB_OK {
            crate::log::line(&format!(
                "memory translation policy refresh failed ({reason}) -> {status}"
            ));
        }
    }
    if let Err(status) = super::code_fetch::publish() {
        if status != PB_OK {
            crate::log::line(&format!(
                "code fetch policy refresh failed ({reason}) -> {status}"
            ));
        }
    }
    if let Err(status) = super::xed_decode::publish() {
        if status != PB_OK {
            crate::log::line(&format!(
                "XED decode policy refresh failed ({reason}) -> {status}"
            ));
        }
    }
}
