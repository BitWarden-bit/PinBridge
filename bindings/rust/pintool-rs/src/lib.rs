//! Minimal PinTool in Rust: Pin loads this DLL with `-t` and calls `main`.
//!
//! The DLL links `pinbridge.dll` (the PinBridge C ABI) and drives Pin through
//! `pb_*` calls. `pinbridge.dll` runs on PinCRT while this DLL keeps its own
//! (statically linked) CRT; the ABI never moves ownership of memory across
//! the boundary, so the two CRTs never interact.
//!
//! Run:
//!   pin.exe -t pintool_rs.dll -- <application>

use core::ffi::{c_char, c_int};
use pinbridge_sys::*;

fn tool_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    unsafe {
        if pb_pin_init(argc, argv) != PB_OK {
            eprintln!("[pintool-rs] pb_pin_init failed");
            return 1;
        }

        let mut buffer = [0 as c_char; 128];
        let mut required: u64 = 0;
        if pb_pin_version(buffer.as_mut_ptr(), buffer.len() as u64, &mut required) == PB_OK {
            let version = std::ffi::CStr::from_ptr(buffer.as_ptr()).to_string_lossy();
            let line = format!(
                "[pintool-rs] {version} via PinBridge ABI {}.{}",
                PB_ABI_VERSION_MAJOR, PB_ABI_VERSION_MINOR
            );
            // Console handles of the host process are unreliable from a tool
            // DLL, so record the banner to a log file as verifiable proof.
            let _ = std::fs::write("pintool-rs.log", format!("{line}\n"));
            eprintln!("{line}");
        }

        pb_pin_start_program_default()
    }
}

pinbridge_tool::tool_entry!(tool_main);
