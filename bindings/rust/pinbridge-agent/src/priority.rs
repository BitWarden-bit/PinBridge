//! Dedicated bounded queue for rare events that must not be displaced by
//! instruction/memory telemetry.
//!
//! Producers obey the same hot-path rules as the main ring: fixed POD writes,
//! no allocation, and a try-only Pin mutex.  The scripting host drains this
//! queue before the normal telemetry page on every tick.

use crate::event::Event;
use crate::event_channel::EventChannel;
use pinbridge_sys::*;

pub const PRIORITY_CAPACITY: usize = 4096;

static CHANNEL: EventChannel<PRIORITY_CAPACITY> = EventChannel::new();

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
