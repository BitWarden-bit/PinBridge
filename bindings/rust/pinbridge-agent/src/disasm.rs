//! Runtime disassembly: reads target bytes (safe copy) and decodes them with
//! the ABI v1.2 XED-backed pb_disassemble.

use pinbridge_proto as proto;
use pinbridge_sys::*;

const MAX_INSNS: u64 = 128;
const MAX_BYTES: u64 = 4096;

pub fn handle_disasm(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let count = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
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

    let mut out = Vec::with_capacity(8 + decoded as usize * 96);
    proto::put_u32(&mut out, decoded as u32);
    for insn in insns.iter().take(decoded as usize) {
        proto::put_u64(&mut out, insn.address);
        proto::put_u32(&mut out, insn.size);
        proto::put_u32(&mut out, insn.kind);
        // flow target for branch/call rows (0 when unknown): direct targets
        // from XED, rip-relative indirect (IAT) by dereferencing the slot
        proto::put_u64(&mut out, flow_target(insn, &bytes, address));
        let text = unsafe { std::ffi::CStr::from_ptr(insn.text.as_ptr()) }.to_string_lossy();
        let text = text.as_bytes();
        proto::put_u32(&mut out, text.len() as u32);
        out.extend_from_slice(text);
        // raw bytes of this instruction (the caller renders the hex column)
        let offset = (insn.address - address) as usize;
        let end = offset + insn.size as usize;
        if end <= bytes.len() {
            out.extend_from_slice(&bytes[offset..end]);
        } else {
            out.extend(std::iter::repeat(0u8).take(insn.size as usize));
        }
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
            pb_pin_safe_copy(buf.as_mut_ptr() as *mut core::ffi::c_void, ea, width, &mut copied);
        }
        if copied == width {
            return u64::from_le_bytes(buf);
        }
    }
    0
}
