//! Runtime-architecture facts for the agent, derived once from the running
//! image's pointer width (an ia32 Pin runtime only ever loads an ia32 agent,
//! and vice versa). Every x64-only assumption — the x86-64 park stub, the
//! RAX..R15/RIP register table, 8-byte pointer reads — is gated on `is_64()`,
//! so an ia32 build reports `eax/eip/esp` and never reaches for a 64-bit
//! register.

use pinbridge_proto::arch_from_pointer_width;
use pinbridge_sys::PbRegId;

/// Pointer width of the running image: 4 (ia32) or 8 (intel64).
#[inline]
pub fn pointer_width() -> u32 {
    core::mem::size_of::<usize>() as u32
}

#[inline]
pub fn is_64() -> bool {
    pointer_width() == 8
}

#[inline]
pub fn is_32() -> bool {
    pointer_width() == 4
}

/// Wire arch id for the PING reply (`pinbridge_proto::ARCH_*`).
pub fn wire_id() -> u32 {
    arch_from_pointer_width(pointer_width())
}

pub fn name() -> &'static str {
    if is_64() {
        "x64"
    } else {
        "x86"
    }
}

/// Instruction-pointer register for this architecture (EIP on ia32, RIP on
/// intel64). `PB_REG_INST_PTR` is the same value as `PB_REG_RIP`, but naming
/// it per-arch makes the intent explicit and keeps x86 from "blindly using
/// RIP".
#[inline]
pub fn instr_ptr_reg() -> PbRegId {
    if is_64() {
        pinbridge_sys::PB_REG_RIP
    } else {
        pinbridge_sys::PB_REG_EIP
    }
}

/// Stack-pointer register for this architecture (ESP on ia32, RSP on intel64).
#[inline]
pub fn stack_ptr_reg() -> PbRegId {
    if is_64() {
        pinbridge_sys::PB_REG_RSP
    } else {
        pinbridge_sys::PB_REG_ESP
    }
}

/// Return-value register for the native integer ABI.
#[inline]
pub fn return_reg() -> PbRegId {
    if is_64() {
        pinbridge_sys::PB_REG_RAX
    } else {
        pinbridge_sys::PB_REG_EAX
    }
}

/// The four register slots exposed by the hook event. On ia32 these are the
/// native ECX/EDX/EAX/EBX values; stdcall stack arguments are exposed in the
/// additional a4..a7 slots and can be addressed by `stackN` rules.
pub fn hook_arg_regs() -> [PbRegId; 4] {
    if is_64() {
        [
            pinbridge_sys::PB_REG_RCX,
            pinbridge_sys::PB_REG_RDX,
            pinbridge_sys::PB_REG_R8,
            pinbridge_sys::PB_REG_R9,
        ]
    } else {
        [
            pinbridge_sys::PB_REG_ECX,
            pinbridge_sys::PB_REG_EDX,
            pinbridge_sys::PB_REG_EAX,
            pinbridge_sys::PB_REG_EBX,
        ]
    }
}

/// Canonical general-purpose register table for CONTEXT_GET: the 8 GP
/// registers + instruction pointer + flags. ia32 reports its 32-bit names
/// (eax/ebx/.../eip/eflags); intel64 reports the full 64-bit set including
/// r8-r15.
pub fn gp_registers() -> &'static [(&'static str, PbRegId)] {
    if is_64() {
        &GP_X64
    } else {
        &GP_X86
    }
}

/// Canonical mnemonic for a GP register id in this architecture (e.g. EIP on
/// ia32, RIP on intel64). `None` for non-GP ids.
pub fn gp_name(id: PbRegId) -> Option<&'static str> {
    gp_registers()
        .iter()
        .find(|(_, r)| *r == id)
        .map(|(name, _)| *name)
}

static GP_X64: [(&str, PbRegId); 18] = [
    ("rax", pinbridge_sys::PB_REG_RAX),
    ("rbx", pinbridge_sys::PB_REG_RBX),
    ("rcx", pinbridge_sys::PB_REG_RCX),
    ("rdx", pinbridge_sys::PB_REG_RDX),
    ("rsi", pinbridge_sys::PB_REG_RSI),
    ("rdi", pinbridge_sys::PB_REG_RDI),
    ("rbp", pinbridge_sys::PB_REG_RBP),
    ("rsp", pinbridge_sys::PB_REG_RSP),
    ("r8", pinbridge_sys::PB_REG_R8),
    ("r9", pinbridge_sys::PB_REG_R9),
    ("r10", pinbridge_sys::PB_REG_R10),
    ("r11", pinbridge_sys::PB_REG_R11),
    ("r12", pinbridge_sys::PB_REG_R12),
    ("r13", pinbridge_sys::PB_REG_R13),
    ("r14", pinbridge_sys::PB_REG_R14),
    ("r15", pinbridge_sys::PB_REG_R15),
    ("rip", pinbridge_sys::PB_REG_RIP),
    ("rflags", pinbridge_sys::PB_REG_RFLAGS),
];

static GP_X86: [(&str, PbRegId); 10] = [
    ("eax", pinbridge_sys::PB_REG_EAX),
    ("ebx", pinbridge_sys::PB_REG_EBX),
    ("ecx", pinbridge_sys::PB_REG_ECX),
    ("edx", pinbridge_sys::PB_REG_EDX),
    ("esi", pinbridge_sys::PB_REG_ESI),
    ("edi", pinbridge_sys::PB_REG_EDI),
    ("ebp", pinbridge_sys::PB_REG_EBP),
    ("esp", pinbridge_sys::PB_REG_ESP),
    ("eip", pinbridge_sys::PB_REG_EIP),
    ("eflags", pinbridge_sys::PB_REG_EFLAGS),
];
