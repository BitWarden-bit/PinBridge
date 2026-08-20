//! Binary query protocol between `pinbridge-agent` (server, inside the Pin
//! process) and out-of-process UIs (clients). Loopback TCP only.
//!
//! Frame: `[u32 payload_len LE][u8 op][u8 status][payload]`.
//! Requests carry `status = 0`; responses set `status != 0` on failure.
//! `payload_len` counts op + status + payload. All integers little-endian.
//!
//! Discipline: when no client is connected the agent side costs nothing
//! (a blocked accept only). Reads take the Pin mutex for a short copy and
//! serialize after unlocking, so the hot path never waits on a UI.

use std::io::{Read, Write};

pub const DEFAULT_PORT: u16 = 9001;
pub const MAX_FRAME: u32 = 1 << 20;
pub const HEADER_LEN: usize = 6;

pub mod op {
    pub const PING: u8 = 0x01;
    pub const COUNTERS: u8 = 0x02;
    pub const RING_PAGE: u8 = 0x03;
    /// Newest retained events from the rare/high-priority lane. This lane is
    /// where target context changes and Pin-internal exceptions are mirrored,
    /// so debugger UIs do not lose exceptions behind execution telemetry.
    pub const PRIORITY_NEWEST: u8 = 0x04;

    // control plane
    pub const STOP: u8 = 0x10;
    pub const RESUME: u8 = 0x11;
    pub const READ_MEM: u8 = 0x12;
    pub const WRITE_MEM: u8 = 0x13;
    pub const BP_SET: u8 = 0x14;
    pub const BP_LIST: u8 = 0x15;
    pub const BP_REMOVE: u8 = 0x16;
    pub const MODULES: u8 = 0x17;

    // state inspection & engine policy
    pub const THREADS: u8 = 0x20;
    pub const CONTEXT_GET: u8 = 0x21;
    pub const CONTEXT_SET: u8 = 0x22;
    pub const ENGINE_SET: u8 = 0x23;
    pub const EXC_POLICY_SET: u8 = 0x24;
    pub const EXC_POLICY_GET: u8 = 0x25;
    pub const STEP: u8 = 0x26;
    pub const DISASM: u8 = 0x30;
    pub const RESOLVE: u8 = 0x31;
    pub const RESOLVE_NAME: u8 = 0x32;
    pub const EXPORTS: u8 = 0x33;
    pub const SYSCALL_FILTER: u8 = 0x34;
    // trace recording channel (record.rs / .pbtr file)
    pub const TRACE_START: u8 = 0x35;
    pub const TRACE_STOP: u8 = 0x36;
    pub const TRACE_STATUS: u8 = 0x37;
    /// Structured trace specification: multiple ranges and thread allowlist.
    pub const TRACE_START_SPEC: u8 = 0x38;
    /// Add address ranges to an armed structured trace.
    pub const TRACE_EXTEND: u8 = 0x39;

    // Target memory layout inspection.
    pub const MEMORY_REGION: u8 = 0x28;
    pub const MEMORY_MAP: u8 = 0x29;

    // script host
    pub const SCRIPT_LOAD: u8 = 0x40;
    pub const SCRIPT_UNLOAD: u8 = 0x41;
    /// Multi-plugin listing (replaces the single-script SCRIPT_STATUS).
    pub const SCRIPT_LIST: u8 = 0x42;
    /// Old single-script status op id, kept for the deprecated client path.
    pub const SCRIPT_STATUS: u8 = 0x42;
    pub const SCRIPT_OUTPUT: u8 = 0x43;

    // runtime hook point set
    pub const HOOK_SET: u8 = 0x50;
    pub const HOOK_REMOVE: u8 = 0x51;
    pub const HOOK_CLEAR: u8 = 0x52;
    pub const HOOK_LIST: u8 = 0x53;
    pub const HOOK_RULE_SET: u8 = 0x54;
    pub const HOOK_RULE_CLEAR: u8 = 0x55;
    /// Batched address insertion: one immutable snapshot publication and
    /// coalesced Pin JIT invalidation for large DLL export sets. Addresses
    /// accepted through this path also emit routine return values.
    pub const HOOK_SET_BATCH: u8 = 0x56;
    /// Function-call logging subset: [u32 count][count x u64 entry address].
    pub const HOOK_FUNCTION_LIST: u8 = 0x57;
    /// Newest records from the Hook-only bounded lane.
    pub const HOOK_EVENTS_NEWEST: u8 = 0x58;
    /// Install the compact ABI capture layout for one function entry.
    pub const HOOK_SIGNATURE_SET: u8 = 0x59;
    /// Remove the compact ABI capture layout for one function entry.
    pub const HOOK_SIGNATURE_REMOVE: u8 = 0x5a;
    /// Scan one bounded code range by instruction class and optionally add
    /// every match as a plain instruction Hook in one snapshot publication.
    pub const HOOK_RANGE: u8 = 0x5b;
    /// Newest records from the Syscall-only timestamped lane.
    pub const SYSCALL_EVENTS_NEWEST: u8 = 0x5c;
}

pub const STATUS_OK: u8 = 0;
pub const STATUS_BAD_REQUEST: u8 = 1;
pub const STATUS_INTERNAL: u8 = 2;

/// Target-architecture ids, appended to the PING reply so clients can tell
/// which Pin runtime (ia32/intel64) the agent was loaded into. Readers that
/// predate these fields simply ignore the trailing bytes, so the extension is
/// wire-backward-compatible.
pub const ARCH_X86: u32 = 0;
pub const ARCH_X64: u32 = 1;

/// Maps a pointer width (4 or 8 bytes) to the wire arch id above. Anything
/// else returns 0xFFFF_FFFF so a malformed agent can never be mistaken for a
/// valid architecture.
pub fn arch_from_pointer_width(width: u32) -> u32 {
    match width {
        4 => ARCH_X86,
        8 => ARCH_X64,
        _ => u32::MAX,
    }
}

/// Wire image of one agent event (88 bytes payload).
#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub struct EventRecord {
    pub sequence: u64,
    pub kind: u32,
    pub thread_id: u32,
    pub address: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
    pub arg4: u64,
    pub arg5: u64,
    pub arg6: u64,
    pub arg7: u64,
}

pub const EVENT_WIRE_LEN: usize = 88;

impl EventRecord {
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.kind.to_le_bytes());
        out.extend_from_slice(&self.thread_id.to_le_bytes());
        out.extend_from_slice(&self.address.to_le_bytes());
        out.extend_from_slice(&self.arg0.to_le_bytes());
        out.extend_from_slice(&self.arg1.to_le_bytes());
        out.extend_from_slice(&self.arg2.to_le_bytes());
        out.extend_from_slice(&self.arg3.to_le_bytes());
        out.extend_from_slice(&self.arg4.to_le_bytes());
        out.extend_from_slice(&self.arg5.to_le_bytes());
        out.extend_from_slice(&self.arg6.to_le_bytes());
        out.extend_from_slice(&self.arg7.to_le_bytes());
    }

    pub fn decode(bytes: &[u8]) -> Option<EventRecord> {
        if bytes.len() < EVENT_WIRE_LEN {
            return None;
        }
        // length checked above: the fixed-size slices cannot fail
        let u64_at = |i: usize| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        Some(EventRecord {
            sequence: u64_at(0),
            kind: u32_at(8),
            thread_id: u32_at(12),
            address: u64_at(16),
            arg0: u64_at(24),
            arg1: u64_at(32),
            arg2: u64_at(40),
            arg3: u64_at(48),
            arg4: u64_at(56),
            arg5: u64_at(64),
            arg6: u64_at(72),
            arg7: u64_at(80),
        })
    }
}

/// Dedicated one-click function-call record. Unlike the compatibility event
/// record this carries up to sixteen signature-resolved argument slots.
#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub struct HookLogRecord {
    pub sequence: u64,
    /// UTC Unix time captured in Agent at the actual Hook hit.
    pub timestamp_unix_ns: u64,
    pub kind: u32,
    pub thread_id: u32,
    pub address: u64,
    pub argument_count: u32,
    /// Bit 0: values were captured using an installed function signature.
    pub flags: u32,
    pub arguments: [u64; 16],
}

pub const HOOK_LOG_WIRE_LEN: usize = 168;
pub const HOOK_LOG_FLAG_SIGNATURE: u32 = 1;
/// The record belongs to an API/function-call Hook, not a plain instruction Hook.
pub const HOOK_LOG_FLAG_FUNCTION: u32 = 1 << 1;
/// The record belongs to the dedicated Syscall event lane.
pub const HOOK_LOG_FLAG_SYSCALL: u32 = 1 << 2;

impl HookLogRecord {
    pub const EMPTY: HookLogRecord = HookLogRecord {
        sequence: 0,
        timestamp_unix_ns: 0,
        kind: 0,
        thread_id: 0,
        address: 0,
        argument_count: 0,
        flags: 0,
        arguments: [0; 16],
    };

    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.timestamp_unix_ns.to_le_bytes());
        out.extend_from_slice(&self.kind.to_le_bytes());
        out.extend_from_slice(&self.thread_id.to_le_bytes());
        out.extend_from_slice(&self.address.to_le_bytes());
        out.extend_from_slice(&self.argument_count.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        for argument in self.arguments {
            out.extend_from_slice(&argument.to_le_bytes());
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<HookLogRecord> {
        if bytes.len() < HOOK_LOG_WIRE_LEN {
            return None;
        }
        let u64_at = |index: usize| u64::from_le_bytes(bytes[index..index + 8].try_into().unwrap());
        let u32_at = |index: usize| u32::from_le_bytes(bytes[index..index + 4].try_into().unwrap());
        let mut arguments = [0u64; 16];
        for (index, argument) in arguments.iter_mut().enumerate() {
            *argument = u64_at(40 + index * 8);
        }
        Some(HookLogRecord {
            sequence: u64_at(0),
            timestamp_unix_ns: u64_at(8),
            kind: u32_at(16),
            thread_id: u32_at(20),
            address: u64_at(24),
            argument_count: u32_at(32),
            flags: u32_at(36),
            arguments,
        })
    }
}

pub fn write_frame(
    stream: &mut impl Write,
    op_code: u8,
    status: u8,
    payload: &[u8],
) -> std::io::Result<()> {
    let len = (payload.len() + 2) as u32;
    if len > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(&[op_code, status])?;
    stream.write_all(payload)?;
    stream.flush()
}

pub fn read_frame(stream: &mut impl Read) -> std::io::Result<(u8, u8, Vec<u8>)> {
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header)?;
    let len = u32::from_le_bytes(header[0..4].try_into().unwrap());
    if len < 2 || len > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "bad frame length",
        ));
    }
    let op_code = header[4];
    let status = header[5];
    let mut payload = vec![0u8; (len - 2) as usize];
    stream.read_exact(&mut payload)?;
    Ok((op_code, status, payload))
}

// ---- payload helpers (shared by server emit and client parse) ----

pub fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_log_record_round_trips_all_signature_slots() {
        let mut arguments = [0u64; 16];
        for (index, argument) in arguments.iter_mut().enumerate() {
            *argument = 0x1000 + index as u64;
        }
        let record = HookLogRecord {
            sequence: 42,
            timestamp_unix_ns: 1_777_777_777_123_456_700,
            kind: 14,
            thread_id: 7,
            address: 0x7ff6_1234_5678,
            argument_count: 16,
            flags: HOOK_LOG_FLAG_SIGNATURE,
            arguments,
        };
        let mut encoded = Vec::new();
        record.encode(&mut encoded);
        assert_eq!(encoded.len(), HOOK_LOG_WIRE_LEN);
        let decoded = HookLogRecord::decode(&encoded).unwrap();
        assert_eq!(decoded.sequence, record.sequence);
        assert_eq!(decoded.timestamp_unix_ns, record.timestamp_unix_ns);
        assert_eq!(decoded.kind, record.kind);
        assert_eq!(decoded.thread_id, record.thread_id);
        assert_eq!(decoded.address, record.address);
        assert_eq!(decoded.argument_count, record.argument_count);
        assert_eq!(decoded.flags, record.flags);
        assert_eq!(decoded.arguments, record.arguments);
        assert!(HookLogRecord::decode(&encoded[..HOOK_LOG_WIRE_LEN - 1]).is_none());
    }
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0 }
    }

    pub fn u32(&mut self) -> Option<u32> {
        let v = u32::from_le_bytes(self.bytes.get(self.pos..self.pos + 4)?.try_into().ok()?);
        self.pos += 4;
        Some(v)
    }

    pub fn u8(&mut self) -> Option<u8> {
        let v = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }

    pub fn u16(&mut self) -> Option<u16> {
        let v = u16::from_le_bytes(self.bytes.get(self.pos..self.pos + 2)?.try_into().ok()?);
        self.pos += 2;
        Some(v)
    }

    pub fn skip(&mut self, count: usize) -> Option<()> {
        self.bytes.get(self.pos..self.pos + count)?;
        self.pos += count;
        Some(())
    }

    pub fn u64(&mut self) -> Option<u64> {
        let v = u64::from_le_bytes(self.bytes.get(self.pos..self.pos + 8)?.try_into().ok()?);
        self.pos += 8;
        Some(v)
    }

    pub fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.pos..]
    }
}
