//! Fixed-size event records written from Pin analysis callbacks.
//!
//! Rules of the hot path: no allocation, no panics, no I/O. A callback only
//! fills a POD record and pushes it into the bounded ring.

/// Win64 integer argument registers captured at an instruction hook.
pub const EVENT_HOOK_REGS: u32 = 1;
/// One memory operand access (EA + size + access tag).
pub const EVENT_MEMORY: u32 = 2;
/// One executed instruction (address + static size).
pub const EVENT_EXEC: u32 = 3;
/// One executed control-flow edge (branch/call/return target + taken).
pub const EVENT_BRANCH_EDGE: u32 = 4;
/// Syscall entry (number + 6 args) / exit (number + return + errno).
pub const EVENT_SYSCALL: u32 = 5;
/// Pin context-change notification (reason + info + ip).
pub const EVENT_CONTEXT_CHANGE: u32 = 6;
/// Image loaded (arg0=low, arg1=high, arg2=is_main; address=low).
pub const EVENT_MODULE_LOAD: u32 = 7;
/// Image unloaded (arg0=low; address=low).
pub const EVENT_MODULE_UNLOAD: u32 = 8;

// Record-channel-only kinds (never submitted to the main ring; they live in
// the record slab and the .pbtr file — see record.rs).
/// exec with instruction bytes: arg0=static_len, arg1=bytes[0..8),
/// arg2=bytes[8..15) zero-padded.
pub const EVENT_EXEC_BYTES: u32 = 9;
/// memory operand with value: arg0=EA, arg1=size, arg2=access tag, arg3=value.
pub const EVENT_MEM_VALUE: u32 = 10;
/// recorder annotation marker: address=0, arg0=tag, arg1=value.
pub const EVENT_MARKER: u32 = 11;

pub const EVENT_KIND_COUNT: usize = 9;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Event {
    pub sequence: u64,
    pub kind: u32,
    pub thread_id: u32,
    /// Instruction address of the capture point.
    pub address: u64,
    /// hook_regs: rcx | memory: EA | branch_edge: target | syscall: number
    /// | context_change: reason
    pub arg0: u64,
    /// hook_regs: rdx | memory: size | branch_edge: taken | syscall: phase
    /// (0=entry, 1=exit) | context_change: info (exception code)
    pub arg1: u64,
    /// hook_regs: r8 | memory: access | syscall: arg0 | context_change: ip
    pub arg2: u64,
    /// hook_regs: r9 | syscall: arg1 (entry) / return value (exit)
    pub arg3: u64,
    /// syscall: arg2 (entry) / errno (exit)
    pub arg4: u64,
    /// syscall: arg3
    pub arg5: u64,
    /// syscall: arg4
    pub arg6: u64,
    /// syscall: arg5
    pub arg7: u64,
}

impl Event {
    pub const EMPTY: Event = Event {
        sequence: 0,
        kind: 0,
        thread_id: 0,
        address: 0,
        arg0: 0,
        arg1: 0,
        arg2: 0,
        arg3: 0,
        arg4: 0,
        arg5: 0,
        arg6: 0,
        arg7: 0,
    };

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            EVENT_HOOK_REGS => "hook_regs",
            EVENT_MEMORY => "memory",
            EVENT_EXEC => "exec",
            EVENT_BRANCH_EDGE => "branch_edge",
            EVENT_SYSCALL => "syscall",
            EVENT_CONTEXT_CHANGE => "context_change",
            EVENT_MODULE_LOAD => "module_load",
            EVENT_MODULE_UNLOAD => "module_unload",
            _ => "unknown",
        }
    }
}
