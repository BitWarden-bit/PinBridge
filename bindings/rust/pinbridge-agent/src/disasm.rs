//! Runtime disassembly: reads target bytes (safe copy) and decodes them with
//! the ABI v1.2 XED-backed pb_disassemble.

use pinbridge_proto as proto;
use pinbridge_sys::*;

const MAX_INSNS: u64 = 128;
const MAX_BYTES: u64 = 4096;
pub const RANGE_KIND_ALL: u32 = 1 << 0;
pub const RANGE_KIND_CALL: u32 = 1 << 1;
pub const RANGE_KIND_SYSCALL: u32 = 1 << 2;
pub const RANGE_KIND_BRANCH: u32 = 1 << 3;
pub const RANGE_KIND_RETURN: u32 = 1 << 4;
pub const RANGE_KIND_MASK: u32 =
    RANGE_KIND_ALL | RANGE_KIND_CALL | RANGE_KIND_SYSCALL | RANGE_KIND_BRANCH | RANGE_KIND_RETURN;
const MAX_RANGE_BYTES: u64 = 4 * 1024 * 1024;

pub struct RangeScan {
    pub decoded: u64,
    pub matched: u64,
    pub addresses: Vec<u64>,
    pub truncated: bool,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisasmRow {
    pub address: u64,
    pub size: u32,
    pub kind: u32,
    pub target: u64,
    pub text: String,
    pub bytes: Vec<u8>,
}

/// Decode target instructions entirely inside the Agent. This path is safe
/// for synchronous Python interceptors: it uses PIN_SafeCopy plus the local
/// XED backend and never opens a loopback query-server connection.
pub fn disassemble_local(address: u64, count: u64) -> Result<Vec<DisasmRow>, u8> {
    if count == 0 || count > MAX_INSNS {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let wanted = (count * 15).min(MAX_BYTES);
    let mut bytes = vec![0u8; wanted as usize];
    let mut copied: u64 = 0;
    unsafe {
        pb_pin_safe_copy(
            bytes.as_mut_ptr() as *mut core::ffi::c_void,
            address,
            wanted,
            &mut copied,
        );
    }
    if copied == 0 {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    bytes.truncate(copied as usize);

    let mut insns: Vec<PbDisasmInsn> = vec![unsafe { core::mem::zeroed() }; count as usize];
    let mut decoded: u64 = 0;
    let status = unsafe {
        pb_disassemble(
            bytes.as_ptr(),
            copied,
            address,
            insns.as_mut_ptr(),
            count,
            &mut decoded,
        )
    };
    if status != PB_OK {
        return Err(proto::STATUS_INTERNAL);
    }

    Ok(insns
        .iter()
        .take(decoded as usize)
        .map(|insn| {
            let text = unsafe { std::ffi::CStr::from_ptr(insn.text.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            let offset = insn.address.saturating_sub(address) as usize;
            let end = offset.saturating_add(insn.size as usize);
            let instruction_bytes = bytes
                .get(offset..end)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| vec![0; insn.size as usize]);
            DisasmRow {
                address: insn.address,
                size: insn.size,
                kind: insn.kind,
                target: flow_target(insn, &bytes, address),
                text,
                bytes: instruction_bytes,
            }
        })
        .collect())
}

/// Sequentially decode one bounded code range and retain addresses matching
/// the selected instruction classes. The caller must provide an instruction
/// boundary as `start`; an undecodable gap makes the scan incomplete.
pub fn scan_range(start: u64, end: u64, kind_mask: u32) -> Result<RangeScan, u8> {
    if start == 0
        || end <= start
        || end - start > MAX_RANGE_BYTES
        || kind_mask == 0
        || kind_mask & !RANGE_KIND_MASK != 0
    {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let mut cursor = start;
    let mut decoded = 0u64;
    let mut matched = 0u64;
    let mut addresses = Vec::new();
    let mut complete = true;
    while cursor < end {
        let rows = match disassemble_local(cursor, MAX_INSNS) {
            Ok(rows) if !rows.is_empty() => rows,
            _ => {
                complete = false;
                break;
            }
        };
        let mut next = cursor;
        for row in rows {
            if row.address >= end {
                break;
            }
            let row_end = row.address.saturating_add(row.size as u64);
            if row_end <= row.address {
                complete = false;
                break;
            }
            next = next.max(row_end);
            decoded += 1;
            if row_end <= end && range_kind_matches(&row, kind_mask) {
                matched += 1;
                if addresses.len() < crate::hooks::MAX_HOOK_POINTS {
                    addresses.push(row.address);
                }
            }
        }
        if next <= cursor {
            complete = false;
            break;
        }
        cursor = next.min(end);
    }
    addresses.sort_unstable();
    addresses.dedup();
    Ok(RangeScan {
        decoded,
        matched,
        truncated: matched as usize > addresses.len(),
        addresses,
        complete: complete && cursor >= end,
    })
}

fn range_kind_matches(row: &DisasmRow, kind_mask: u32) -> bool {
    if kind_mask & RANGE_KIND_ALL != 0 {
        return true;
    }
    if row.kind == 2 && kind_mask & RANGE_KIND_CALL != 0 {
        return true;
    }
    if row.kind == 1 && kind_mask & RANGE_KIND_BRANCH != 0 {
        return true;
    }
    if row.kind == 3 && kind_mask & RANGE_KIND_RETURN != 0 {
        return true;
    }
    if kind_mask & RANGE_KIND_SYSCALL != 0 {
        let text = row.text.trim().to_ascii_lowercase();
        return text.starts_with("syscall")
            || text.starts_with("sysenter")
            || text.starts_with("int 0x2e")
            || text.starts_with("int 2eh");
    }
    false
}

pub fn handle_disasm(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let count = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let rows = disassemble_local(address, count)?;

    let mut out = Vec::with_capacity(8 + rows.len() * 96);
    proto::put_u32(&mut out, rows.len() as u32);
    for insn in rows {
        proto::put_u64(&mut out, insn.address);
        proto::put_u32(&mut out, insn.size);
        proto::put_u32(&mut out, insn.kind);
        proto::put_u64(&mut out, insn.target);
        let text = insn.text.as_bytes();
        proto::put_u32(&mut out, text.len() as u32);
        out.extend_from_slice(text);
        out.extend_from_slice(&insn.bytes);
    }
    Ok(out)
}

/// Best-effort successor address for a branch/call row without a thread
/// context: direct targets come from XED; rip-relative memory-indirect
/// (import thunks, IAT calls) by reading the slot. Register-based indirect
/// targets need a live context and are left 0 here.
fn flow_target(insn: &PbDisasmInsn, bytes: &[u8], base: u64) -> u64 {
    if insn.kind != 1 && insn.kind != 2 {
        return 0;
    }
    let offset = (insn.address - base) as usize;
    let end = offset + insn.size as usize;
    if end > bytes.len() {
        return 0;
    }
    let mut flow: PbFlowInsn = unsafe { core::mem::zeroed() };
    let status = unsafe {
        pb_disassemble_flow(
            bytes[offset..end].as_ptr(),
            insn.size as u64,
            insn.address,
            &mut flow,
        )
    };
    if status != PB_OK {
        return 0;
    }
    if flow.has_target != 0 {
        return flow.target;
    }
    if flow.ind_mem != 0 && flow.base_reg == crate::arch::instr_ptr_reg() as i32 {
        let ea = (insn.address + insn.size as u64).wrapping_add(flow.disp as u64);
        let width = crate::arch::pointer_width() as u64;
        let mut buf = [0u8; 8];
        let mut copied: u64 = 0;
        unsafe {
            pb_pin_safe_copy(
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                ea,
                width,
                &mut copied,
            );
        }
        if copied == width {
            return u64::from_le_bytes(buf);
        }
    }
    0
}
