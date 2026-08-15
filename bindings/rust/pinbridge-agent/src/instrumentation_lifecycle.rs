//! Static Pin instrumentation lifecycle observations.
//!
//! Python publishes kind/range rules through the normal instrumentation
//! policy. Pin callbacks copy metadata into the ordinary event ring; they do
//! not call Python and never expose borrowed TRACE/BBL/RTN handles.

use crate::event::{Event, EVENT_BBL_INSTRUMENT, EVENT_ROUTINE_INSTRUMENT, EVENT_TRACE_INSTRUMENT};
use core::ffi::c_void;
use core::sync::atomic::{AtomicU64, Ordering};
use pinbridge_sys::*;

const MAX_SNAPSHOT_IMAGES: usize = 4096;
const MAX_SNAPSHOT_SECTIONS: usize = 65_536;
const MAX_SNAPSHOT_ROUTINES: usize = 262_144;
static PENDING_ROUTINE_SNAPSHOT: AtomicU64 = AtomicU64::new(0);

#[inline]
unsafe fn query_bool<T: Copy>(
    handle: T,
    query: unsafe extern "C" fn(T, *mut u8) -> PbStatus,
) -> u64 {
    let mut value = 0u8;
    (query(handle, &mut value) == PB_OK && value != 0) as u64
}

unsafe fn routine_event(routine: PbRtnHandle, generation: u64) -> bool {
    let mut valid = 0u8;
    if pb_rtn_valid(routine, &mut valid) != PB_OK || valid == 0 {
        return false;
    }
    let mut address = 0u64;
    if pb_rtn_address(routine, &mut address) != PB_OK
        || !crate::engines::wants_at_instrumentation(address, EVENT_ROUTINE_INSTRUMENT)
    {
        return false;
    }
    let mut size = 0u64;
    let mut id = 0u32;
    let mut instruction_count = 0u32;
    let _ = pb_rtn_size(routine, &mut size);
    let _ = pb_rtn_id(routine, &mut id);
    if pb_rtn_open(routine) == PB_OK {
        let _ = pb_rtn_num_ins(routine, &mut instruction_count);
        let _ = pb_rtn_close(routine);
    }
    crate::ring::submit(Event {
        kind: EVENT_ROUTINE_INSTRUMENT,
        thread_id: PB_INVALID_THREAD_ID,
        address,
        arg0: size,
        arg1: instruction_count as u64,
        arg2: id as u64,
        arg3: query_bool(routine, pb_rtn_is_dynamic),
        arg4: query_bool(routine, pb_rtn_is_artificial),
        arg7: generation,
        ..Event::EMPTY
    });
    true
}

unsafe extern "C" fn on_routine(routine: PbRtnHandle, _user_data: *mut c_void) {
    if crate::engines::wants_instrumentation_kind(EVENT_ROUTINE_INSTRUMENT) {
        let _ = routine_event(routine, crate::engines::policy_generation());
    }
}

unsafe fn bbl_events(trace: PbTraceHandle, generation: u64) {
    if !crate::engines::wants_instrumentation_kind(EVENT_BBL_INSTRUMENT) {
        return;
    }
    let mut count = 0u32;
    if pb_trace_num_bbl(trace, &mut count) != PB_OK {
        return;
    }
    let mut bbl = PbBblHandle { opaque: 0 };
    if pb_trace_bbl_head(trace, &mut bbl) != PB_OK {
        return;
    }
    for _ in 0..count {
        let mut valid = 0u8;
        if pb_bbl_valid(bbl, &mut valid) != PB_OK || valid == 0 {
            break;
        }
        let mut address = 0u64;
        if pb_bbl_address(bbl, &mut address) == PB_OK
            && crate::engines::wants_at_instrumentation(address, EVENT_BBL_INSTRUMENT)
        {
            let mut size = 0u64;
            let mut instructions = 0u32;
            let _ = pb_bbl_size(bbl, &mut size);
            let _ = pb_bbl_num_ins(bbl, &mut instructions);
            crate::ring::submit(Event {
                kind: EVENT_BBL_INSTRUMENT,
                thread_id: PB_INVALID_THREAD_ID,
                address,
                arg0: size,
                arg1: instructions as u64,
                arg2: query_bool(bbl, pb_bbl_has_fall_through),
                arg3: query_bool(bbl, pb_bbl_original),
                arg7: generation,
                ..Event::EMPTY
            });
        }
        let mut next = PbBblHandle { opaque: 0 };
        if pb_bbl_next(bbl, &mut next) != PB_OK {
            break;
        }
        bbl = next;
    }
}

unsafe extern "C" fn on_trace(trace: PbTraceHandle, _user_data: *mut c_void) {
    let generation = crate::engines::policy_generation();
    let mut address = 0u64;
    if pb_trace_address(trace, &mut address) != PB_OK {
        return;
    }
    if crate::engines::wants_at_instrumentation(address, EVENT_TRACE_INSTRUMENT) {
        let mut size = 0u64;
        let mut bbl_count = 0u32;
        let mut instruction_count = 0u32;
        let mut routine_address = 0u64;
        let _ = pb_trace_size(trace, &mut size);
        let _ = pb_trace_num_bbl(trace, &mut bbl_count);
        let _ = pb_trace_num_ins(trace, &mut instruction_count);
        let mut routine = PbRtnHandle { opaque: 0 };
        if pb_trace_rtn(trace, &mut routine) == PB_OK {
            let _ = pb_rtn_address(routine, &mut routine_address);
        }
        crate::ring::submit(Event {
            kind: EVENT_TRACE_INSTRUMENT,
            thread_id: PB_INVALID_THREAD_ID,
            address,
            arg0: size,
            arg1: bbl_count as u64,
            arg2: instruction_count as u64,
            arg3: query_bool(trace, pb_trace_has_fall_through),
            arg4: routine_address,
            arg7: generation,
            ..Event::EMPTY
        });
    }
    bbl_events(trace, generation);
}

unsafe fn emit_routine_snapshot_locked(generation: u64) -> (usize, usize, usize, usize) {
    let mut image_total = 0usize;
    let mut section_total = 0usize;
    let mut routine_total = 0usize;
    let mut matched_total = 0usize;
    let mut image = PbImgHandle { opaque: 0 };
    if pb_app_img_head(&mut image) != PB_OK {
        return (image_total, section_total, routine_total, matched_total);
    }
    for _ in 0..MAX_SNAPSHOT_IMAGES {
        let mut valid = 0u8;
        if pb_img_valid(image, &mut valid) != PB_OK || valid == 0 {
            break;
        }
        image_total += 1;
        let mut section = PbSecHandle { opaque: 0 };
        if pb_img_sec_head(image, &mut section) == PB_OK {
            loop {
                if section_total >= MAX_SNAPSHOT_SECTIONS || routine_total >= MAX_SNAPSHOT_ROUTINES
                {
                    return (image_total, section_total, routine_total, matched_total);
                }
                let mut section_valid = 0u8;
                if pb_sec_valid(section, &mut section_valid) != PB_OK || section_valid == 0 {
                    break;
                }
                section_total += 1;
                let mut routine = PbRtnHandle { opaque: 0 };
                if pb_sec_rtn_head(section, &mut routine) == PB_OK {
                    loop {
                        if routine_total >= MAX_SNAPSHOT_ROUTINES {
                            return (image_total, section_total, routine_total, matched_total);
                        }
                        let mut routine_valid = 0u8;
                        if pb_rtn_valid(routine, &mut routine_valid) != PB_OK || routine_valid == 0
                        {
                            break;
                        }
                        routine_total += 1;
                        if routine_event(routine, generation) {
                            matched_total += 1;
                        }
                        let mut next = PbRtnHandle { opaque: 0 };
                        if pb_rtn_next(routine, &mut next) != PB_OK {
                            break;
                        }
                        routine = next;
                    }
                }
                let mut next = PbSecHandle { opaque: 0 };
                if pb_sec_next(section, &mut next) != PB_OK {
                    break;
                }
                section = next;
            }
        }
        let mut next = PbImgHandle { opaque: 0 };
        if pb_img_next(image, &mut next) != PB_OK {
            break;
        }
        image = next;
    }
    (image_total, section_total, routine_total, matched_total)
}

/// Replays loaded routines after a hot-loaded Python policy is published.
/// Every loop is bounded so corrupt SDK traversal cannot hang the script host.
pub fn emit_routine_snapshot(generation: u64) {
    if !crate::engines::wants_instrumentation_kind(EVENT_ROUTINE_INSTRUMENT) {
        return;
    }
    unsafe {
        // The policy is published by a Pin internal scripting thread, not an
        // application callback. Pin requires the client lock while such a
        // thread traverses IMG/SEC/RTN objects.
        if pb_pin_lock_client() != PB_OK {
            return;
        }
        let stats = emit_routine_snapshot_locked(generation);
        let _ = pb_pin_unlock_client();
        crate::log::line(&format!(
            "routine instrumentation snapshot generation={generation} images={} sections={} routines={} matched={}",
            stats.0, stats.1, stats.2, stats.3
        ));
    }
}

/// Defers replay until the script host has installed the plugin's event
/// cursor. Policies are commonly published from `pb_init`; events submitted
/// there are intentionally skipped when that cursor is initialized.
pub fn request_routine_snapshot(generation: u64) {
    if crate::engines::wants_instrumentation_kind(EVENT_ROUTINE_INSTRUMENT) {
        PENDING_ROUTINE_SNAPSHOT.store(generation, Ordering::Release);
    } else {
        PENDING_ROUTINE_SNAPSHOT.store(0, Ordering::Release);
    }
}

pub fn emit_pending_routine_snapshot() {
    let generation = PENDING_ROUTINE_SNAPSHOT.swap(0, Ordering::AcqRel);
    if generation != 0 {
        emit_routine_snapshot(generation);
    }
}

pub fn register() -> PbStatus {
    unsafe {
        let mut routine_handle = PbCallbackHandle { opaque: 0 };
        let status = pb_rtn_add_instrument_function(
            Some(on_routine),
            core::ptr::null_mut(),
            &mut routine_handle,
        );
        if status != PB_OK {
            return status;
        }
        let mut trace_handle = PbCallbackHandle { opaque: 0 };
        pb_trace_add_instrument_function(Some(on_trace), core::ptr::null_mut(), &mut trace_handle)
    }
}
