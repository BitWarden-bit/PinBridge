//! Thread context inspection: list stopped threads, read/write GP registers.
//! All of these require the application to be stopped first.

use pinbridge_proto as proto;
use pinbridge_sys::*;

/// Canonical GP register order for CONTEXT_GET payloads, selected by the
/// running architecture (see `crate::arch::gp_registers`): ia32 reports
/// eax/ebx/.../eip/eflags, intel64 reports rax/.../r15/rip/rflags.

pub fn handle_threads() -> Vec<u8> {
    let mut count: u32 = 0;
    let mut ids = Vec::new();
    unsafe {
        if pb_pin_get_stopped_thread_count(&mut count) == PB_OK {
            for index in 0..count {
                let mut tid: PbThreadId = 0;
                if pb_pin_get_stopped_thread_id(index, &mut tid) == PB_OK {
                    ids.push(tid);
                }
            }
        }
    }
    let mut out = Vec::with_capacity(4 + ids.len() * 4);
    proto::put_u32(&mut out, ids.len() as u32);
    for tid in ids {
        proto::put_u32(&mut out, tid);
    }
    out
}

pub fn handle_context_get(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let tid = reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?;
    unsafe {
        let mut context: PbConstContextHandle = core::ptr::null();
        let status = pb_pin_get_stopped_thread_context(tid, &mut context);
        if status != PB_OK || context.is_null() {
            crate::log::line(&format!(
                "context_get tid={tid} failed: status={status} null={}",
                context.is_null()
            ));
            return Err(proto::STATUS_BAD_REQUEST);
        }
        let registers = crate::arch::gp_registers();
        let mut out = Vec::with_capacity(4 + registers.len() * 12);
        proto::put_u32(&mut out, registers.len() as u32);
        for (_name, reg) in registers {
            let mut value: u64 = 0;
            pb_pin_get_context_reg(context, *reg, &mut value);
            proto::put_u32(&mut out, *reg);
            proto::put_u64(&mut out, value);
        }
        Ok(out)
    }
}

pub fn handle_context_set(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let tid = reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?;
    let reg = reader.u32().ok_or(proto::STATUS_BAD_REQUEST)?;
    let value = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    unsafe {
        let mut context: PbContextHandle = core::ptr::null_mut();
        if pb_pin_get_stopped_thread_writeable_context(tid, &mut context) != PB_OK
            || context.is_null()
        {
            return Err(proto::STATUS_BAD_REQUEST);
        }
        if pb_pin_set_context_reg(context, reg, value) != PB_OK {
            return Err(proto::STATUS_INTERNAL);
        }
    }
    Ok(Vec::new())
}
