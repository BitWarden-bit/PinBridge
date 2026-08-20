//! Register-name tables for the query protocol's CONTEXT_GET / CONTEXT_SET.
//!
//! The agent ships raw Pin `REG` ids on the wire; this module maps those ids
//! to the canonical mnemonic for the runtime's architecture, selected by the
//! `arch` field of a PING reply (`pinbridge_proto::ARCH_X86` / `ARCH_X64`).
//! The tables duplicate the agent's `crate::arch` (which is typed against
//! `pinbridge_sys::PbRegId`) so that host-side tools stay free of the Pin SDK
//! dependency; both agree on the same numeric ids.

#[cfg(test)]
use pinbridge_proto::ARCH_X64;
use pinbridge_proto::ARCH_X86;

/// Virtual register ids reserved by the Hook action protocol for ABI-aware
/// stack arguments. They are deliberately outside Pin's PbRegId range.
pub const HOOK_STACK_ARG_BASE: u32 = 0x8000_0000;

pub fn hook_stack_arg_index(id: u32) -> Option<u32> {
    id.checked_sub(HOOK_STACK_ARG_BASE)
        .filter(|index| *index < 1024)
}

/// Canonical x86-64 general-purpose register table (name, Pin REG id).
pub const GP_X64: [(&str, u32); 18] = [
    ("rax", 10),
    ("rbx", 7),
    ("rcx", 9),
    ("rdx", 8),
    ("rsi", 4),
    ("rdi", 3),
    ("rbp", 5),
    ("rsp", 6),
    ("r8", 11),
    ("r9", 12),
    ("r10", 13),
    ("r11", 14),
    ("r12", 15),
    ("r13", 16),
    ("r14", 17),
    ("r15", 18),
    ("rip", 26),
    ("rflags", 25),
];

/// Canonical ia32 general-purpose register table (name, Pin REG id).
pub const GP_X86: [(&str, u32); 10] = [
    ("eax", 56),
    ("ebx", 53),
    ("ecx", 55),
    ("edx", 54),
    ("esi", 47),
    ("edi", 45),
    ("ebp", 49),
    ("esp", 51),
    ("eip", 58),
    ("eflags", 57),
];

/// The GP table for a PING `arch` value. Unknown values fall back to x64,
/// matching the pre-arch (x64-only) behavior.
pub fn gp_regs(arch: u32) -> &'static [(&'static str, u32)] {
    if arch == ARCH_X86 {
        &GP_X86
    } else {
        &GP_X64
    }
}

/// Canonical register name for `id`, or `reg_{id}` when unknown.
pub fn reg_name(arch: u32, id: u32) -> String {
    if let Some(index) = hook_stack_arg_index(id) {
        return format!("stack{index}");
    }
    gp_regs(arch)
        .iter()
        .find(|(_, r)| *r == id)
        .map(|(name, _)| (*name).to_string())
        .unwrap_or_else(|| format!("reg_{id}"))
}

/// Register id for `name` (case-insensitive), or `None` when unknown.
pub fn reg_id(arch: u32, name: &str) -> Option<u32> {
    let normalized = name.to_ascii_lowercase();
    for prefix in ["stack", "arg"] {
        if let Some(index) = normalized.strip_prefix(prefix) {
            if !index.is_empty() {
                if let Ok(index) = index.parse::<u32>() {
                    if index < 1024 {
                        return Some(HOOK_STACK_ARG_BASE + index);
                    }
                }
            }
        }
    }
    gp_regs(arch)
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(&normalized))
        .map(|(_, id)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x64_table_names_and_ids() {
        assert_eq!(reg_id(ARCH_X64, "rax"), Some(10));
        assert_eq!(reg_id(ARCH_X64, "R15"), Some(18));
        assert_eq!(reg_id(ARCH_X64, "rip"), Some(26));
        assert_eq!(reg_id(ARCH_X64, "rflags"), Some(25));
        // ia32 spellings are not in the x64 table.
        assert_eq!(reg_id(ARCH_X64, "eax"), None);
        assert_eq!(reg_name(ARCH_X64, 26), "rip");
        assert_eq!(reg_name(ARCH_X64, 18), "r15");
        assert_eq!(gp_regs(ARCH_X64).len(), 18);
    }

    #[test]
    fn x86_table_names_and_ids() {
        assert_eq!(reg_id(ARCH_X86, "eax"), Some(56));
        assert_eq!(reg_id(ARCH_X86, "esp"), Some(51));
        assert_eq!(reg_id(ARCH_X86, "eip"), Some(58));
        assert_eq!(reg_id(ARCH_X86, "eflags"), Some(57));
        // x64-only spellings are not in the x86 table.
        assert_eq!(reg_id(ARCH_X86, "rax"), None);
        assert_eq!(reg_name(ARCH_X86, 58), "eip");
        assert_eq!(reg_name(ARCH_X86, 51), "esp");
        assert_eq!(gp_regs(ARCH_X86).len(), 10);
    }

    #[test]
    fn unknown_arch_falls_back_to_x64() {
        assert_eq!(reg_id(999, "rax"), Some(10));
        assert_eq!(reg_id(999, "eax"), None);
        assert_eq!(reg_name(999, 26), "rip");
    }

    #[test]
    fn unknown_register_renders_as_reg_id() {
        assert_eq!(reg_name(ARCH_X64, 12345), "reg_12345");
        assert_eq!(reg_name(ARCH_X86, 12345), "reg_12345");
    }

    #[test]
    fn hook_stack_argument_names_round_trip() {
        assert_eq!(reg_id(ARCH_X86, "stack0"), Some(HOOK_STACK_ARG_BASE));
        assert_eq!(reg_id(ARCH_X64, "ARG3"), Some(HOOK_STACK_ARG_BASE + 3));
        assert_eq!(reg_name(ARCH_X86, HOOK_STACK_ARG_BASE + 7), "stack7");
        assert_eq!(reg_id(ARCH_X86, "stack1024"), None);
    }
}
