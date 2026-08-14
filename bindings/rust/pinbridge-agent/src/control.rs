//! Control plane handlers: stop/resume/read/write/modules.
//! Runs on the query-server internal thread (never an application thread).

use core::ffi::c_void;
use pinbridge_proto as proto;
use pinbridge_sys::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub static STOPPED: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn GetCurrentProcess() -> *mut c_void;
    fn WriteProcessMemory(
        process: *mut c_void,
        base_address: *mut c_void,
        buffer: *const c_void,
        size: usize,
        written: *mut usize,
    ) -> i32;
    fn FlushInstructionCache(process: *mut c_void, base: *const c_void, size: usize) -> i32;
}

pub fn is_stopped() -> bool {
    STOPPED.load(Ordering::Acquire)
}

pub fn handle_stop() -> Vec<u8> {
    if is_stopped() {
        return vec![1]; // already stopped: idempotent
    }
    let stopped = crate::bp::control_command(crate::bp::CMD_STOP);
    vec![stopped as u8]
}

pub fn handle_resume() -> Vec<u8> {
    if !is_stopped() {
        // Resuming a running application is a Pin contract violation
        // (pinvm assert kills the whole process). Rapid clients can land
        // here right after a step's internal resume — refuse instead.
        return vec![0];
    }
    // Swallow the one replayed breakpoint execution on resume (steps arm the
    // stepper instead and must not touch this).
    crate::bp::arm_resume_skip();
    let resumed = crate::bp::control_command(crate::bp::CMD_RESUME);
    vec![resumed as u8]
}

pub const READ_MEM_MAX: u64 = 65536;

pub fn handle_read_mem(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let size = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    if size == 0 || size > READ_MEM_MAX {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let mut buffer = vec![0u8; size as usize];
    let mut copied: u64 = 0;
    unsafe {
        pb_pin_safe_copy(
            buffer.as_mut_ptr() as *mut c_void,
            address,
            size,
            &mut copied,
        );
    }
    buffer.truncate(copied as usize);
    let mut out = Vec::with_capacity(16 + buffer.len());
    proto::put_u64(&mut out, address);
    proto::put_u64(&mut out, copied);
    out.extend_from_slice(&buffer);
    Ok(out)
}

pub const WRITE_MEM_MAX: u64 = 65536;

pub fn handle_write_mem(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let len = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)? as usize;
    let data = reader.remaining();
    if len == 0 || len as u64 > WRITE_MEM_MAX || data.len() < len {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    if !is_stopped() {
        // writing a running target is a contract violation (matches the old
        // debugger discipline: stop -> mutate -> resume)
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let mut written: usize = 0;
    let ok = unsafe {
        WriteProcessMemory(
            GetCurrentProcess(),
            address as *mut c_void,
            data.as_ptr() as *const c_void,
            len,
            &mut written,
        )
    };
    if ok != 0 && written > 0 {
        unsafe {
            FlushInstructionCache(GetCurrentProcess(), address as *const c_void, written);
        }
    }
    let mut out = Vec::with_capacity(8);
    proto::put_u64(&mut out, written as u64);
    Ok(out)
}

const MAX_MODULES: usize = 512;

pub fn handle_modules() -> Vec<u8> {
    let mut out = Vec::with_capacity(4096);
    let mut count: u32 = 0;
    let mut entries = Vec::new();
    unsafe {
        let mut img = PbImgHandle { opaque: 0 };
        if pb_app_img_head(&mut img) != PB_OK {
            proto::put_u32(&mut out, 0);
            return out;
        }
        let mut valid: u8 = 0;
        pb_img_valid(img, &mut valid);
        while valid != 0 && count as usize <= MAX_MODULES {
            let mut low: u64 = 0;
            let mut high: u64 = 0;
            let mut is_main: u8 = 0;
            pb_img_low_address(img, &mut low);
            pb_img_high_address(img, &mut high);
            pb_img_is_main_executable(img, &mut is_main);
            let mut name_buf = [0 as std::os::raw::c_char; 512];
            let mut needed: u64 = 0;
            let name = if pb_img_name(img, name_buf.as_mut_ptr(), 512, &mut needed) == PB_OK {
                std::ffi::CStr::from_ptr(name_buf.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            };
            entries.push((low, high, is_main, name));
            count += 1;
            let mut next = PbImgHandle { opaque: 0 };
            if pb_img_next(img, &mut next) != PB_OK {
                break;
            }
            img = next;
            valid = 0;
            pb_img_valid(img, &mut valid);
        }
    }
    proto::put_u32(&mut out, count);
    for (low, high, is_main, name) in entries {
        proto::put_u64(&mut out, low);
        proto::put_u64(&mut out, high);
        out.push(is_main);
        let bytes = name.as_bytes();
        proto::put_u32(&mut out, bytes.len() as u32);
        out.extend_from_slice(bytes);
    }
    out
}
