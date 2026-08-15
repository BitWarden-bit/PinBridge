//! Per-plugin high-frequency instrumentation configuration.
//!
//! Python owns declarative kind/range/thread filters. This module unions the
//! live plugin specs without losing each spec's conjunction and publishes one
//! immutable snapshot for instrumentation/analysis callbacks. No Python runs
//! on either Pin hot path.

use super::{with_registry, STATE_RUNNING};
use pinbridge_sys::{PbStatus, PB_OK};

#[derive(Clone)]
pub struct Spec {
    pub kinds: u32,
    pub ranges: Vec<(u64, u64)>,
    pub threads: Vec<u32>,
}

pub fn publish() -> Result<u64, PbStatus> {
    let configs = with_registry(|registry| {
        registry
            .values()
            .filter(|plugin| plugin.state == STATE_RUNNING)
            .filter_map(|plugin| plugin.instrumentation.as_ref())
            .map(|spec| crate::engines::InstrumentationPolicyConfig {
                kinds: spec.kinds,
                ranges: spec.ranges.clone(),
                threads: spec.threads.clone(),
            })
            .collect::<Vec<_>>()
    });
    crate::engines::set_instrumentation_policies(&configs)
}

pub fn publish_best_effort(reason: &str) {
    if let Err(status) = publish() {
        if status != PB_OK {
            crate::log::line(&format!(
                "instrumentation policy refresh failed ({reason}) -> {status}"
            ));
        }
    }
}
