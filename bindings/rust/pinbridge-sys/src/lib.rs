//! Raw FFI bindings for the frozen PinBridge C ABI v1.0 (`pinbridge.dll`).
//!
//! Everything here is `unsafe` to call and follows the rules of
//! `include/pinbridge/pinbridge.h`: opaque handles are borrowed unless the
//! header says otherwise, caller-provided buffers use the
//! `(buffer, capacity, required_size)` triple, and no memory ownership crosses
//! the DLL boundary. Regenerate with `tools/generate_rust_bindings.py`.

#![allow(
    non_snake_case,
    non_camel_case_types,
    non_upper_case_globals,
    dead_code
)]

include!("bindings.rs");
