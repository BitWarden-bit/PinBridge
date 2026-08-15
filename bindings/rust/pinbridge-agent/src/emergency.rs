//! Allocation-free emergency diagnostics for callbacks that may run after
//! the process heap is exhausted.

use core::ffi::c_void;

extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn WriteFile(
        file: *mut c_void,
        buffer: *const u8,
        size: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

const FILE_APPEND_DATA: u32 = 0x4;
const FILE_SHARE_READ: u32 = 0x1;
const FILE_SHARE_WRITE: u32 = 0x2;
const OPEN_ALWAYS: u32 = 4;
const INVALID_HANDLE_VALUE: *mut c_void = usize::MAX as *mut c_void;

// CWD-relative on purpose: resolving a configured or machine-specific path in
// the OOM callback would allocate. The launcher controls where the record lands
// through the target process working directory.
const OOM_PATH: &[u8] = b"pinbridge_oom.log\0";
static mut OOM_PATH_W: [u16; 260] = [0; 260];

/// Precompute the wide path while the process heap is healthy. This must run
/// before the native out-of-memory callback is registered.
pub fn initialize() {
    unsafe {
        for (index, byte) in OOM_PATH.iter().take(259).enumerate() {
            OOM_PATH_W[index] = *byte as u16;
        }
    }
}

fn append_bytes(out: &mut [u8], len: &mut usize, bytes: &[u8]) {
    let take = bytes.len().min(out.len().saturating_sub(*len));
    out[*len..*len + take].copy_from_slice(&bytes[..take]);
    *len += take;
}

fn append_hex(out: &mut [u8], len: &mut usize, value: u64) {
    let mut digits = [0u8; 16];
    let mut index = digits.len();
    let mut remaining = value;
    loop {
        index -= 1;
        digits[index] = b"0123456789abcdef"[(remaining & 0xf) as usize];
        remaining >>= 4;
        if remaining == 0 {
            break;
        }
    }
    append_bytes(out, len, &digits[index..]);
}

fn format_oom_line(requested_size: u64, occurrence: u64, out: &mut [u8]) -> usize {
    let mut len = 0usize;
    append_bytes(out, &mut len, b"OOM requested=0x");
    append_hex(out, &mut len, requested_size);
    append_bytes(out, &mut len, b" occurrence=0x");
    append_hex(out, &mut len, occurrence);
    append_bytes(out, &mut len, b"\n");
    len
}

/// Best-effort emergency record for Pin's allocation-failure callback. This
/// path performs no Rust allocation, takes no lock, and uses a pre-widened
/// filename so it remains usable when the process heap is exhausted.
pub fn record_out_of_memory(requested_size: u64, occurrence: u64) {
    unsafe {
        let file = CreateFileW(
            core::ptr::addr_of!(OOM_PATH_W).cast::<u16>(),
            FILE_APPEND_DATA,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            core::ptr::null_mut(),
            OPEN_ALWAYS,
            0,
            core::ptr::null_mut(),
        );
        if file.is_null() || file == INVALID_HANDLE_VALUE {
            return;
        }

        let mut line = [0u8; 96];
        let len = format_oom_line(requested_size, occurrence, &mut line);
        let mut written = 0u32;
        let _ = WriteFile(
            file,
            line.as_ptr(),
            len as u32,
            &mut written,
            core::ptr::null_mut(),
        );
        let _ = CloseHandle(file);
    }
}

#[cfg(test)]
mod tests {
    use super::format_oom_line;

    #[test]
    fn oom_line_formatting_is_fixed_and_allocation_free() {
        let mut line = [0u8; 96];
        let len = format_oom_line(0x1234, 0x2a, &mut line);
        assert_eq!(&line[..len], b"OOM requested=0x1234 occurrence=0x2a\n");
    }
}
