//! Fixed-size event records written from Pin analysis callbacks.
//!
//! Rules of the hot path: no allocation, no panics, no I/O. A callback only
//! fills a POD record and pushes it into the bounded ring.

/// Architecture-specific integer argument registers plus ABI stack snapshots
/// captured at a runtime instruction hook.
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
/// Lossless run-length marker. The previous logical record is repeated
/// `arg0` additional times; `sequence` is the final logical sequence number.
pub const EVENT_REPEAT: u32 = 12;
/// Per-instruction register snapshot component. arg0=register id, arg1/arg2
/// contain the value (low/high), arg3=value width, arg7=frame id.
pub const EVENT_REG_SNAPSHOT: u32 = 13;
/// Runtime hook placed on a function's `ret` instruction. `arg0` is the
/// pre-action return register (RAX/EAX), `arg1..arg4` are the captured integer
/// register slots, and `arg5..arg7` are the first three ABI stack arguments.
pub const EVENT_HOOK_RETURN: u32 = 14;

// Low-frequency process lifecycle kinds.  They use the same wire record as
// telemetry, but are consumed by the scripting host's named subscriptions
// (`pb.on("thread.start", ...)`, etc.).  Keep these ids below 32 so old
// u32 watch masks can still represent them when a diagnostic consumer asks
// for the raw records.
/// Pin application thread created. arg0=flags, address=context IP.
pub const EVENT_THREAD_START: u32 = 15;
/// Pin application thread exited. arg0=exit code, address=context IP.
pub const EVENT_THREAD_EXIT: u32 = 16;
/// The application is about to begin executing.
pub const EVENT_PROCESS_START: u32 = 17;
/// Python-deliverable exit lifecycle edge. arg1=1 is an early user-mode exit
/// request; arg1=2 is the pre-Fini Python cleanup phase. Cleanup arg2 says
/// whether the early edge was observed and arg3 says whether Pin's native
/// PrepareForFini was already reached (normally false for the usable window).
pub const EVENT_PROCESS_EXIT: u32 = 18;
pub const PROCESS_EXIT_SOURCE_API: u64 = 1;
pub const PROCESS_EXIT_SOURCE_PREPARE_FINI: u64 = 2;
/// Native-only final fini edge. Python delivery is not promised at this point.
pub const EVENT_PROCESS_FINI: u32 = 19;
/// Pin detected self-modifying code. address/arg0=start, arg1=end.
pub const EVENT_SMC: u32 = 20;
/// Pin completed a detach operation.
pub const EVENT_PIN_DETACH: u32 = 21;
/// Pin completed a reattach sequence.
pub const EVENT_PIN_ATTACH: u32 = 22;
/// Pin reported an allocation failure. arg0=requested size.
pub const EVENT_OUT_OF_MEMORY: u32 = 23;
/// Pin's own internal exception handler ran. address=physical IP, arg0=code,
/// arg1=exception address, arg2=fault address, arg3=access type,
/// arg4=exception class, arg5=fault address known.
pub const EVENT_PIN_INTERNAL_EXCEPTION: u32 = 24;
/// A statically decoded instruction observed during Pin instrumentation.
/// address=instruction address, arg0=size, arg1=XED category,
/// arg2=XED extension, arg3=opcode/iclass, arg4=memory operand count,
/// arg5=control-flow flags (fall-through/branch/call/return/syscall).
pub const EVENT_INSTRUCTION_DECODE: u32 = 25;
/// Pin is about to report an application breakpoint to the debugger.
/// address=IP, arg0=Pin debugging-event id, arg1=SP, arg2=flags,
/// arg3=integer return register.
pub const EVENT_DEBUGGER_BREAKPOINT: u32 = 26;
/// Pin is about to report a single-step stop to the debugger.
pub const EVENT_DEBUGGER_SINGLE_STEP: u32 = 27;
/// Pin is about to report an asynchronous debugger interruption.
pub const EVENT_DEBUGGER_ASYNC_BREAK: u32 = 28;
/// Pin created a dynamic TRACE for instrumentation. address=start,
/// arg0=size, arg1=BBL count, arg2=instruction count, arg3=fall-through,
/// arg4=containing routine address, arg7=policy generation.
pub const EVENT_TRACE_INSTRUMENT: u32 = 29;
/// Pin discovered or snapshotted a routine. address=start, arg0=size,
/// arg1=instruction count, arg2=routine id, arg3=dynamic, arg4=artificial,
/// arg7=policy generation.
pub const EVENT_ROUTINE_INSTRUMENT: u32 = 30;
/// One BBL inside a newly instrumented TRACE. address=start, arg0=size,
/// arg1=instruction count, arg2=fall-through, arg3=original,
/// arg7=policy generation.
pub const EVENT_BBL_INSTRUMENT: u32 = 31;

pub const EVENT_KIND_COUNT: usize = 9;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Event {
    pub sequence: u64,
    pub kind: u32,
    pub thread_id: u32,
    /// Instruction address of the capture point (exception IP for
    /// `context_change`).
    pub address: u64,
    /// hook_regs: rcx/ecx | memory: EA | branch_edge: target | syscall: number
    /// | context_change: reason
    pub arg0: u64,
    /// hook_regs: rdx/edx | memory: size | branch_edge: taken | syscall: phase
    /// (0=entry, 1=exit) | context_change: info (exception code)
    pub arg1: u64,
    /// hook_regs: r8/eax | memory: access | syscall: arg0 | context_change: ip
    pub arg2: u64,
    /// hook_regs: r9/ebx | syscall: arg1 (entry) / return value (exit)
    pub arg3: u64,
    /// hook_regs: ABI stack arg0 | syscall: arg2 (entry) / errno (exit)
    pub arg4: u64,
    /// hook_regs: ABI stack arg1 | syscall: arg3
    pub arg5: u64,
    /// hook_regs: ABI stack arg2 | syscall: arg4
    pub arg6: u64,
    /// hook_regs: ABI stack arg3 | syscall: arg5
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
        kind_name(self.kind)
    }
}

pub fn kind_name(kind: u32) -> &'static str {
    match kind {
        EVENT_HOOK_REGS => "hook_regs",
        EVENT_MEMORY => "memory",
        EVENT_EXEC => "exec",
        EVENT_BRANCH_EDGE => "branch_edge",
        EVENT_SYSCALL => "syscall",
        EVENT_CONTEXT_CHANGE => "context_change",
        EVENT_MODULE_LOAD => "module_load",
        EVENT_MODULE_UNLOAD => "module_unload",
        EVENT_REPEAT => "repeat",
        EVENT_REG_SNAPSHOT => "reg_snapshot",
        EVENT_HOOK_RETURN => "hook_return",
        EVENT_THREAD_START => "thread_start",
        EVENT_THREAD_EXIT => "thread_exit",
        EVENT_PROCESS_START => "process_start",
        EVENT_PROCESS_EXIT => "process_exit",
        EVENT_PROCESS_FINI => "process_fini",
        EVENT_SMC => "smc",
        EVENT_PIN_DETACH => "pin_detach",
        EVENT_PIN_ATTACH => "pin_attach",
        EVENT_OUT_OF_MEMORY => "out_of_memory",
        EVENT_PIN_INTERNAL_EXCEPTION => "pin_internal_exception",
        EVENT_INSTRUCTION_DECODE => "instruction_decode",
        EVENT_DEBUGGER_BREAKPOINT => "debugger_breakpoint",
        EVENT_DEBUGGER_SINGLE_STEP => "debugger_single_step",
        EVENT_DEBUGGER_ASYNC_BREAK => "debugger_async_break",
        EVENT_TRACE_INSTRUMENT => "trace_instrument",
        EVENT_ROUTINE_INSTRUMENT => "routine_instrument",
        EVENT_BBL_INSTRUMENT => "basic_block_instrument",
        _ => "unknown",
    }
}
