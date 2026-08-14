//! Glue shared by every Rust PinTool DLL built on `pinbridge.dll`.
//!
//! Use from the final cdylib crate:
//!
//! ```ignore
//! // build.rs
//! fn main() { pinbridge_tool::emit_link_flags(); }
//!
//! // lib.rs
//! fn my_tool_main(argc: c_int, argv: *mut *mut c_char) -> c_int { ... }
//! pinbridge_tool::tool_entry!(my_tool_main);
//! ```

/// Emits the linker flags a Rust tool DLL needs on Windows:
/// API-set pseudo DLLs and other system DLLs Pin's tool loader cannot open
/// (it only resolves preloaded modules such as kernel32/ntdll) are moved to
/// the delay-load table; the Windows delay-load helper resolves them through
/// the system loader at first call. Known offenders: api-ms-win-core-synch
/// (std SRWLock), WS2_32 (std::net), bcryptprimitives (std HashMap RandomState).
/// Must be called from the *final cdylib crate's* build.rs
/// (`cargo:rustc-link-arg` only applies to that crate's targets).
pub fn emit_link_flags() {
    println!("cargo:rustc-link-arg=/DELAYLOAD:api-ms-win-core-synch-l1-2-0.dll");
    println!("cargo:rustc-link-arg=/DELAYLOAD:WS2_32.dll");
    println!("cargo:rustc-link-arg=/DELAYLOAD:bcryptprimitives.dll");
    println!("cargo:rustc-link-arg=delayimp.lib");
}

/// Generates the three exports Pin's Windows tool loader looks up:
/// `main` (forwarding to the given tool entry function) and the
/// `ClientIntC` / `PinCommitHashC` stubs that bounce into pinbridge.dll's
/// host-glue symbols (Pin's loader does not follow PE export forwarders,
/// so real local stubs are required).
#[macro_export]
macro_rules! tool_entry {
    ($main:path) => {
        mod __pinbridge_tool_entry {
            #![allow(non_snake_case)]
            use super::*;
            use core::ffi::{c_char, c_int, c_void};

            extern "C" {
                fn pb_toolhost_client_int() -> *mut c_void;
                fn pb_toolhost_commit_hash() -> *const c_char;
            }

            #[no_mangle]
            pub extern "C" fn ClientIntC() -> *mut c_void {
                unsafe { pb_toolhost_client_int() }
            }

            #[no_mangle]
            pub extern "C" fn PinCommitHashC() -> *const c_char {
                unsafe { pb_toolhost_commit_hash() }
            }

            #[no_mangle]
            pub extern "C" fn main(argc: c_int, argv: *mut *mut c_char) -> c_int {
                let entry: fn(c_int, *mut *mut c_char) -> c_int = $main;
                entry(argc, argv)
            }
        }
    };
}
