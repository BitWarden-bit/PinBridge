//! TEMPORARY diagnostics: first-chance vectored handler that dumps the
//! faulting thread's stack (module + offset per frame) to a crash file with
//! RAW Win32 writes — no log.rs mutex (it may be held by the dying thread),
//! no Rust TLS. Pin's UPC dispatcher still runs afterwards and kills the
//! process; we only observe. Remove when the script-host crash is fixed.

use core::ffi::c_void;

#[repr(C)]
struct ExceptionRecord {
    code: u32,
    flags: u32,
    record: *mut c_void,
    address: *mut c_void,
    num_params: u32,
    _pad: u32,
    info: [*mut c_void; 15],
}

#[repr(C)]
struct ExceptionPointers {
    record: *mut ExceptionRecord,
    context: *mut c_void,
}

extern "system" {
    fn AddVectoredExceptionHandler(first: u32, handler: *mut c_void) -> *mut c_void;
    fn RtlCaptureStackBackTrace(
        frames_to_skip: u32,
        frames_to_capture: u32,
        backtrace: *mut *mut c_void,
        hash: *mut u32,
    ) -> u16;
    fn GetCurrentThreadId() -> u32;
    fn GetModuleHandleExA(flags: u32, name: *const c_void, out: *mut *mut c_void) -> i32;
    fn GetModuleFileNameA(module: *mut c_void, buffer: *mut u8, size: u32) -> u32;
    fn CreateFileA(
        name: *const u8,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn WriteFile(file: *mut c_void, buffer: *const u8, size: u32, written: *mut u32, overlapped: *mut c_void) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

const FROM_ADDRESS_UNCHANGED: u32 = 0x2 | 0x4;
const GENERIC_WRITE: u32 = 0x4000_0000;
const CREATE_ALWAYS: u32 = 2;
const FILE_APPEND_DATA: u32 = 0x4;
const OPEN_ALWAYS: u32 = 4;
// CWD-relative on purpose: no machine-specific paths baked into the binary.
// The tests launch pin with cwd = the agent's directory, so the files land
// next to pinbridge_agent.dll.
const CRASH_PATH: &[u8] = b"crash_dump.txt\0";
const HEAPLOG_PATH: &[u8] = b"heap_log.txt\0";

/// CRASH_PATH pre-widened at install() (the VEH handler must not run the
/// A->W conversion — it allocates from the process heap).
static mut CRASH_PATH_W: [u16; 260] = [0; 260];

/// TEMP hunt aid gate: PINBRIDGE_HEAP_CHECK_FAST=1 raises the validation
/// cadence (per tick / per query-server op). Same tri-state reasoning as
/// heap_check's gate (no OnceLock: contended init parks via std TLS).
pub fn heap_check_fast_enabled() -> bool {
    static ON: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
    match ON.load(core::sync::atomic::Ordering::Relaxed) {
        2 => true,
        1 => false,
        _ => {
            let on = std::env::var("PINBRIDGE_HEAP_CHECK_FAST").ok().as_deref() == Some("1");
            ON.store(if on { 2 } else { 1 }, core::sync::atomic::Ordering::Relaxed);
            on
        }
    }
}

/// Current thread's OS id (diagnostics).
pub fn os_tid() -> u32 {
    unsafe { GetCurrentThreadId() }
}

extern "system" {
    fn GetProcessHeap() -> *mut c_void;
    fn HeapValidate(heap: *mut c_void, flags: u32, entry: *const c_void) -> i32;
    fn GetProcessHeaps(count: u32, out: *mut *mut c_void) -> u32;
}

/// DIAGNOSTIC: validate every process heap. Returns the count of BAD heaps.
/// HeapValidate takes the heap lock, so it serializes with concurrent
/// allocs — safe to call from the script thread's tick.
/// Temporary heap-corruption hunt aid: active only with
/// PINBRIDGE_HEAP_CHECK=1 (full scans are too heavy to run always).
/// On failure the detecting thread's stack lands in heap_log.txt — the
/// detector is not the corruptor, but the call-site/timing bracket is.
pub fn heap_check(where_from: &str) {
    // Tri-state lazy gate (0 = unread, 1 = off, 2 = on). NOT a OnceLock:
    // contended get_or_init parks via std's TLS-keyed parker, and this
    // module's TLS index is never assigned (Pin maps it privately) — a
    // first-call race between the scripting/query/breaker threads would
    // touch a foreign TLS slot. An idempotent racy env read is fine.
    static ON: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
    match ON.load(core::sync::atomic::Ordering::Relaxed) {
        2 => {}
        1 => return,
        _ => {
            let on = std::env::var("PINBRIDGE_HEAP_CHECK").ok().as_deref() == Some("1");
            ON.store(if on { 2 } else { 1 }, core::sync::atomic::Ordering::Relaxed);
            if !on {
                return;
            }
        }
    }
    let mut heaps: [*mut c_void; 16] = [core::ptr::null_mut(); 16];
    let count = unsafe { GetProcessHeaps(16, heaps.as_mut_ptr()) };
    let mut bad = 0u32;
    for heap in heaps.iter().take(count.min(16) as usize) {
        if !heap.is_null() && unsafe { HeapValidate(*heap, 0, core::ptr::null()) } == 0 {
            bad += 1;
        }
    }
    if bad > 0 {
        let file = unsafe {
            CreateFileA(
                HEAPLOG_PATH.as_ptr(),
                FILE_APPEND_DATA,
                0,
                core::ptr::null_mut(),
                OPEN_ALWAYS,
                0,
                core::ptr::null_mut(),
            )
        };
        if !file.is_null() && file != usize::MAX as *mut c_void {
            let os_tid = unsafe { GetCurrentThreadId() };
            let mut pin_tid: pinbridge_sys::PbThreadId = 0;
            unsafe {
                pinbridge_sys::pb_pin_thread_id(&mut pin_tid);
            }
            write_all(
                file,
                &format!(
                    "HEAP_CORRUPT from={where_from} bad={bad}/{count} pin_tid={pin_tid} os_tid={os_tid}\n"
                ),
            );
            let mut frames: [*mut c_void; 32] = [core::ptr::null_mut(); 32];
            let captured = unsafe {
                RtlCaptureStackBackTrace(1, 32, frames.as_mut_ptr(), core::ptr::null_mut())
            };
            for frame in frames.iter().take(captured as usize) {
                let (base, name) = module_of(*frame);
                write_all(
                    file,
                    &format!(
                        "  {:?} {}+0x{:x}\n",
                        *frame,
                        name,
                        (*frame as usize).wrapping_sub(base)
                    ),
                );
            }
            unsafe {
                CloseHandle(file);
            }
        }
    }
}


fn write_all(file: *mut c_void, text: &str) {
    let mut written: u32 = 0;
    unsafe {
        WriteFile(file, text.as_ptr(), text.len() as u32, &mut written, core::ptr::null_mut());
    }
}

fn module_of(address: *mut c_void) -> (usize, String) {
    let mut module: *mut c_void = core::ptr::null_mut();
    let ok = unsafe { GetModuleHandleExA(FROM_ADDRESS_UNCHANGED, address, &mut module) };
    if ok == 0 || module.is_null() {
        return (0, String::new());
    }
    let mut buffer = [0u8; 260];
    let len = unsafe { GetModuleFileNameA(module, buffer.as_mut_ptr(), buffer.len() as u32) };
    let name = std::str::from_utf8(&buffer[..len as usize])
        .unwrap_or("")
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_string();
    (module as usize, name)
}

unsafe extern "system" fn on_fault(pointers: *mut ExceptionPointers) -> i32 {
    if pointers.is_null() || (*pointers).record.is_null() {
        return 0; // EXCEPTION_CONTINUE_SEARCH
    }
    let record = &*(*pointers).record;
    if record.code != 0xC0000005 && record.code != 0xC00000FD {
        return 0;
    }
    // This handler fires on APPLICATION threads for every first-chance AV
    // (the fixtures raise handled AVs in loops). NO heap allocation, no Pin
    // calls, no loader lock here: the breaker can suspend the thread at any
    // point, and a thread suspended inside this handler while holding the
    // process-heap or loader lock wedges the whole control plane (every
    // query-server reply allocates). Observed: stop during the AV window
    // leaving a 0-byte crash dump + a dead control plane. CreateFileW (no
    // A->W conversion alloc), hand-rolled formatting, WriteFile — nothing
    // else.
    let file = CreateFileW(
        CRASH_PATH_W.as_ptr(),
        GENERIC_WRITE,
        0,
        core::ptr::null_mut(),
        CREATE_ALWAYS,
        0,
        core::ptr::null_mut(),
    );
    if file.is_null() || file == usize::MAX as *mut c_void {
        return 0;
    }
    let os_tid = GetCurrentThreadId();
    let access = if record.num_params >= 2 {
        record.info[1] as u64
    } else {
        0
    };
    let mut line = [0u8; 160];
    let mut len = 0usize;
    fn push(out: &mut [u8], len: &mut usize, bytes: &[u8]) {
        let take = bytes.len().min(out.len() - *len);
        out[*len..*len + take].copy_from_slice(&bytes[..take]);
        *len += take;
    }
    fn hex(value: u64, out: &mut [u8; 16]) -> &[u8] {
        let mut index = 16;
        let mut value = value;
        loop {
            index -= 1;
            out[index] = b"0123456789abcdef"[(value & 0xf) as usize];
            value >>= 4;
            if value == 0 {
                break;
            }
        }
        &out[index..]
    }
    push(&mut line, &mut len, b"CRASH code=0x");
    let mut buf = [0u8; 16];
    push(&mut line, &mut len, hex(record.code as u64, &mut buf));
    push(&mut line, &mut len, b" ip=0x");
    push(&mut line, &mut len, hex(record.address as u64, &mut buf));
    push(&mut line, &mut len, b" access=0x");
    push(&mut line, &mut len, hex(access, &mut buf));
    push(&mut line, &mut len, b" os_tid=");
    // os_tid as decimal (hand-rolled; u32 max 10 digits)
    let mut dec = [0u8; 10];
    let mut index = 10;
    let mut tid = os_tid;
    loop {
        index -= 1;
        dec[index] = b'0' + (tid % 10) as u8;
        tid /= 10;
        if tid == 0 {
            break;
        }
    }
    push(&mut line, &mut len, &dec[index..]);
    push(&mut line, &mut len, b"\n");
    let mut written: u32 = 0;
    WriteFile(file, line.as_ptr(), len as u32, &mut written, core::ptr::null_mut());
    // raw frame addresses only — module resolution takes the loader lock
    let mut frames: [*mut c_void; 48] = [core::ptr::null_mut(); 48];
    let captured = RtlCaptureStackBackTrace(0, 48, frames.as_mut_ptr(), core::ptr::null_mut());
    for frame in frames.iter().take(captured as usize) {
        let mut line = [0u8; 24];
        let mut len = 0usize;
        push(&mut line, &mut len, b"  0x");
        push(&mut line, &mut len, hex(*frame as u64, &mut buf));
        push(&mut line, &mut len, b"\n");
        WriteFile(file, line.as_ptr(), len as u32, &mut written, core::ptr::null_mut());
    }
    CloseHandle(file);
    0 // keep searching: Pin's own dispatcher still reports and kills
}

pub fn install() {
    // Pre-widen the crash path: the vectored handler must stay allocation-
    // free, and CreateFileA's A->W conversion allocates from the process heap.
    unsafe {
        for (index, byte) in CRASH_PATH.iter().take(259).enumerate() {
            CRASH_PATH_W[index] = *byte as u16;
        }
    }
    // NOTE: no OS-loader work here — agent_main runs inside Pin's tool-load
    // sequence; LoadLibrary here killed the process (ntdll TPP fault).
    let handler = unsafe { AddVectoredExceptionHandler(1, on_fault as *mut c_void) };
    crate::log::line(&format!("crash dump handler -> {handler:p}"));
    // Pin's own chain: the UPC dispatcher swallows tool-internal exceptions
    // before the Windows vectored chain, so ALSO hook the Pin-level handler.
    let mut pin_handle = pinbridge_sys::PbCallbackHandle { opaque: 0 };
    let status = unsafe {
        pinbridge_sys::pb_pin_add_internal_exception_handler(
            Some(on_pin_fault),
            core::ptr::null_mut(),
            &mut pin_handle,
        )
    };
    crate::log::line(&format!("pin crash handler -> {status}"));
}

unsafe extern "C" fn on_pin_fault(
    thread_id: pinbridge_sys::PbThreadId,
    exception_info: pinbridge_sys::PbExceptionInfoHandle,
    physical_context: pinbridge_sys::PbPhysicalContextHandle,
    _user_data: *mut c_void,
) -> pinbridge_sys::PbExceptHandlingResult {
    let mut code: pinbridge_sys::PbExceptionCode = 0;
    let mut address: u64 = 0;
    pinbridge_sys::pb_pin_get_exception_code(exception_info, &mut code);
    pinbridge_sys::pb_pin_get_exception_address(exception_info, &mut address);
    let mut exception_class: pinbridge_sys::PbExceptionClass =
        pinbridge_sys::PB_EXCEPTCLASS_NONE;
    let _ = pinbridge_sys::pb_pin_get_exception_class(code, &mut exception_class);
    let mut fault_address = 0u64;
    let mut fault_address_known = 0u8;
    let _ = pinbridge_sys::pb_pin_get_faulty_access_address(
        exception_info,
        &mut fault_address,
        &mut fault_address_known,
    );
    let mut access_type: pinbridge_sys::PbFaultyAccessType =
        pinbridge_sys::PB_FAULTY_ACCESS_TYPE_UNKNOWN;
    let _ = pinbridge_sys::pb_pin_get_faulty_access_type(exception_info, &mut access_type);
    // Snapshot the exact physical IP/SP before doing any file I/O. The
    // borrowed physical context is valid only for this Pin callback.
    let instr_ptr = crate::arch::instr_ptr_reg();
    let stack_ptr = crate::arch::stack_ptr_reg();
    let mut ip: u64 = 0;
    let mut sp: u64 = 0;
    pinbridge_sys::pb_pin_get_physical_context_reg(physical_context, instr_ptr, &mut ip);
    pinbridge_sys::pb_pin_get_physical_context_reg(physical_context, stack_ptr, &mut sp);
    let event = crate::event::Event {
        kind: crate::event::EVENT_PIN_INTERNAL_EXCEPTION,
        thread_id,
        address: ip,
        arg0: code as u64,
        arg1: address,
        arg2: fault_address,
        arg3: access_type as u64,
        arg4: exception_class as u64,
        arg5: fault_address_known as u64,
        ..crate::event::Event::EMPTY
    };
    let file = CreateFileA(
        CRASH_PATH.as_ptr(),
        FILE_APPEND_DATA,
        0,
        core::ptr::null_mut(),
        OPEN_ALWAYS,
        0,
        core::ptr::null_mut(),
    );
    if file.is_null() || file == usize::MAX as *mut c_void {
        crate::priority::submit(event);
        return pinbridge_sys::PB_EHR_UNHANDLED;
    }
    let os_tid = GetCurrentThreadId();
    write_all(
        file,
        &format!(
            "PIN_CRASH code={code} fault_addr=0x{address:x} pin_tid={thread_id} os_tid={os_tid}\n"
        ),
    );
    // exact register state at the fault, from the physical context. Use the
    // per-arch instruction/stack pointers (eip/esp on ia32) rather than the
    // x64 rip/rsp names.
    let ip_name = crate::arch::gp_name(instr_ptr).unwrap_or("ip");
    let sp_name = crate::arch::gp_name(stack_ptr).unwrap_or("sp");
    let (ip_base, ip_mod) = module_of(ip as *mut c_void);
    write_all(
        file,
        &format!(
            "  {ip_name}=0x{ip:x} ({}+0x{:x}) {sp_name}=0x{sp:x}\n",
            ip_mod,
            ip.wrapping_sub(ip_base as u64)
        ),
    );
    // Full GP regs with arch-correct names (eax/eip/esp/ebp on ia32).
    for (name, reg) in crate::arch::gp_registers() {
        let mut value: u64 = 0;
        pinbridge_sys::pb_pin_get_physical_context_reg(physical_context, *reg, &mut value);
        write_all(file, &format!("  {}=0x{value:x}\n", *name));
    }
    // TEMP: real backtrace of the faulting thread (arch-independent).
    let mut frames: [*mut c_void; 48] = [core::ptr::null_mut(); 48];
    let captured = RtlCaptureStackBackTrace(0, 48, frames.as_mut_ptr(), core::ptr::null_mut());
    write_all(file, "  backtrace:\n");
    for frame in frames.iter().take(captured as usize) {
        let (base, name) = module_of(*frame);
        write_all(
            file,
            &format!(
                "    {:?} {}+0x{:x}\n",
                *frame,
                name,
                (*frame as usize).wrapping_sub(base)
            ),
        );
    }
    // TEMP heap-corruption hunt: 64-bit pointer walks (register->memory dumps
    // and an rbp frame chain) are meaningless on ia32, where the 32-bit
    // register halves would fabricate truncated pointers — skip rather than
    // lie.
    if crate::arch::is_64() {
        let read_reg = |id: pinbridge_sys::PbRegId| -> u64 {
            let mut value: u64 = 0;
            pinbridge_sys::pb_pin_get_physical_context_reg(physical_context, id, &mut value);
            value
        };
        let rax = read_reg(pinbridge_sys::PB_REG_RAX);
        let r8 = read_reg(pinbridge_sys::PB_REG_R8);
        let rbp = read_reg(pinbridge_sys::PB_REG_RBP);
        let dump_mem = |label: &str, base: u64| {
            let mut buffer = [0u8; 0x60];
            let mut copied: u64 = 0;
            pinbridge_sys::pb_pin_safe_copy(
                buffer.as_mut_ptr() as *mut c_void,
                base,
                0x60,
                &mut copied,
            );
            write_all(file, &format!("  mem {label} @0x{base:x} ({copied}B):"));
            for chunk in buffer[..copied as usize].chunks(8) {
                write_all(file, " ");
                for byte in chunk {
                    write_all(file, &format!("{byte:02x}"));
                }
            }
            write_all(file, "\n");
        };
        dump_mem("r8-0x10", r8.wrapping_sub(0x10));
        dump_mem("rax", rax);
        dump_mem("rsp", sp);
        // rbp frame-chain walk (process memory, direct reads, sanity-bounded)
        let mut frame = rbp;
        for _ in 0..32 {
            if frame == 0 || frame % 8 != 0 {
                break;
            }
            let prev = unsafe { *(frame as *const u64) };
            let ret = unsafe { *((frame + 8) as *const u64) };
            if ret == 0 {
                break;
            }
            let (base, name) = module_of(ret as *mut c_void);
            write_all(
                file,
                &format!("  ret 0x{ret:x} {}+0x{:x}\n", name, ret.wrapping_sub(base as u64)),
            );
            if prev <= frame {
                break;
            }
            frame = prev;
        }
    }
    CloseHandle(file);
    // The native crash record is complete. If Pin keeps the process alive,
    // the scripting thread will later deliver this POD snapshot to Python.
    crate::priority::submit(event);
    pinbridge_sys::PB_EHR_UNHANDLED // let Pin's default reporter kill us
}
