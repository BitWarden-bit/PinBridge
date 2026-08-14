//! Module load/unload events (kind 7/8) fed into the main ring.
//!
//! Registered unconditionally at agent init so plugins and UIs can observe
//! the image layout without an env knob. IMG callbacks hold Pin internal
//! locks, so the hot discipline applies to the ring write: no allocation,
//! no I/O — only the ring's Pin-mutex submit path (same pattern as
//! exception.rs from its context-change callback). Events carry no thread
//! id (IMG callbacks don't provide one); thread_id stays 0.
//!
//! The resolver cache invalidation is the one sanctioned exception: it
//! briefly takes the resolver's std mutex, which is otherwise only held by
//! the query-server/script threads for short cache updates.

use crate::event::{Event, EVENT_MODULE_LOAD, EVENT_MODULE_UNLOAD};
use crate::ring::submit;
use core::ffi::c_void;
use pinbridge_sys::*;

unsafe extern "C" fn on_img_load(img: PbImgHandle, _user_data: *mut c_void) {
    let mut low: u64 = 0;
    let mut high: u64 = 0;
    let mut is_main: u8 = 0;
    pb_img_low_address(img, &mut low);
    pb_img_high_address(img, &mut high);
    pb_img_is_main_executable(img, &mut is_main);
    submit(Event {
        kind: EVENT_MODULE_LOAD,
        address: low,
        arg0: low,
        arg1: high,
        arg2: is_main as u64,
        ..Event::EMPTY
    });
    // A base reused by a new image must not keep stale exports/poison.
    crate::resolve::invalidate(low);
}

unsafe extern "C" fn on_img_unload(img: PbImgHandle, _user_data: *mut c_void) {
    let mut low: u64 = 0;
    pb_img_low_address(img, &mut low);
    submit(Event {
        kind: EVENT_MODULE_UNLOAD,
        address: low,
        arg0: low,
        ..Event::EMPTY
    });
    crate::resolve::invalidate(low);
}

/// Registers the always-on image load/unload callbacks. Independent of the
/// PINBRIDGE_ENTRY_BP one-shot in lib.rs (Pin supports multiple IMG
/// instrument callbacks).
pub fn register() -> PbStatus {
    let mut load_handle = PbCallbackHandle { opaque: 0 };
    let mut unload_handle = PbCallbackHandle { opaque: 0 };
    unsafe {
        let status = pb_img_add_instrument_function(
            Some(on_img_load),
            core::ptr::null_mut(),
            &mut load_handle,
        );
        if status != PB_OK {
            return status;
        }
        pb_img_add_unload_function(Some(on_img_unload), core::ptr::null_mut(), &mut unload_handle)
    }
}
