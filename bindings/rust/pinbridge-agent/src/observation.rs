//! Dedicated lane for native-filtered, medium-rate Python observations.
//!
//! These records stay compatible with the main telemetry ring, but unrelated
//! instruction/memory floods cannot evict this copy. Producers must apply a
//! native selector before submitting so this lane does not become an
//! unfiltered replacement for the telemetry ring.

use crate::event::Event;
use crate::event_channel::EventChannel;
use pinbridge_sys::PbStatus;

pub const OBSERVATION_CAPACITY: usize = 16_384;

static CHANNEL: EventChannel<OBSERVATION_CAPACITY> = EventChannel::new();

pub fn init() -> PbStatus {
    CHANNEL.init()
}

#[inline]
pub fn submit(event: Event) {
    CHANNEL.submit(event);
}

pub fn total() -> u64 {
    CHANNEL.total()
}

pub fn dropped() -> u64 {
    CHANNEL.dropped()
}

pub fn try_page(after: u64, limit: usize, out: &mut Vec<Event>) -> Option<(u64, u64)> {
    CHANNEL.try_page(after, limit, out)
}
