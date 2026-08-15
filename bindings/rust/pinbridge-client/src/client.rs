//! Blocking client for the pinbridge-agent query protocol.

use pinbridge_proto as proto;
use std::io::{Error, ErrorKind, Result};
use std::net::TcpStream;
use std::time::Duration;

pub const KIND_NAMES: [&str; 8] = [
    "hook_regs",
    "memory",
    "exec",
    "branch_edge",
    "syscall",
    "context_change",
    "module_load",
    "module_unload",
];

#[derive(Default, Debug)]
pub struct Snapshot {
    pub connected: bool,
    pub abi: (u32, u32),
    pub pid: u32,
    pub total: u64,
    pub dropped: u64,
    pub capacity: u64,
    pub kind_counts: [u64; 8],
    pub newest: Vec<proto::EventRecord>,
}

/// Full PING snapshot. `arch` and `pointer_width` are the additive tail
/// fields (`pinbridge_proto::ARCH_*`); they are `None` against an agent that
/// predates the extension.
#[derive(Debug, Clone, Copy)]
pub struct PingInfo {
    pub abi_major: u32,
    pub abi_minor: u32,
    pub pid: u32,
    pub total: u64,
    pub arch: Option<u32>,
    pub pointer_width: Option<u32>,
}

pub struct Client {
    stream: TcpStream,
}

impl Client {
    pub fn connect(port: u16) -> Result<Client> {
        let stream = TcpStream::connect(("127.0.0.1", port))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        Ok(Client { stream })
    }

    fn request(&mut self, op_code: u8, payload: &[u8]) -> Result<Vec<u8>> {
        proto::write_frame(&mut self.stream, op_code, proto::STATUS_OK, payload)?;
        let (resp_op, status, body) = proto::read_frame(&mut self.stream)?;
        if resp_op != op_code {
            return Err(Error::new(ErrorKind::InvalidData, "op mismatch"));
        }
        if status != proto::STATUS_OK {
            // failures may carry a human-readable reason in the payload
            // (e.g. SCRIPT_LOAD returns the compile error text)
            let detail = String::from_utf8_lossy(&body).into_owned();
            let message = if detail.is_empty() {
                format!("server status {status}")
            } else {
                format!("server status {status}: {detail}")
            };
            return Err(Error::new(ErrorKind::Other, message));
        }
        Ok(body)
    }

    pub fn ping(&mut self) -> Result<(u32, u32, u32, u64)> {
        let info = self.ping_full()?;
        Ok((info.abi_major, info.abi_minor, info.pid, info.total))
    }

    /// Full PING snapshot. `arch` and `pointer_width` are the additive tail
    /// fields (pinbridge_proto::ARCH_*); they are `None` against an agent that
    /// predates the extension.
    pub fn ping_full(&mut self) -> Result<PingInfo> {
        let body = self.request(proto::op::PING, &[])?;
        let mut r = proto::Reader::new(&body);
        let abi_major = r.u32_or_err()?;
        let abi_minor = r.u32_or_err()?;
        let pid = r.u32_or_err()?;
        let total = r.u64_or_err()?;
        let arch = r.u32();
        let pointer_width = r.u32();
        Ok(PingInfo {
            abi_major,
            abi_minor,
            pid,
            total,
            arch,
            pointer_width,
        })
    }

    pub fn counters(&mut self) -> Result<(u64, u64, u64, [u64; 8])> {
        let body = self.request(proto::op::COUNTERS, &[])?;
        let mut r = proto::Reader::new(&body);
        let total = r.u64_or_err()?;
        let dropped = r.u64_or_err()?;
        let capacity = r.u64_or_err()?;
        let mut kinds = [0u64; 8];
        for slot in &mut kinds {
            *slot = r.u64_or_err()?;
        }
        Ok((total, dropped, capacity, kinds))
    }

    /// Newest `limit` retained events (oldest first), plus the ring total.
    pub fn ring_newest(&mut self, limit: u64) -> Result<(u64, Vec<proto::EventRecord>)> {
        let (total, ..) = self.counters()?;
        let after = total.saturating_sub(limit);
        self.ring_page(after, limit).map(|(_, _, events)| (total, events))
    }

    /// Cursor-paged ring read: events with sequence > `after`.
    pub fn ring_page(&mut self, after: u64, limit: u64) -> Result<(u64, u64, Vec<proto::EventRecord>)> {
        let mut request = Vec::with_capacity(16);
        proto::put_u64(&mut request, after);
        proto::put_u64(&mut request, limit);
        let body = self.request(proto::op::RING_PAGE, &request)?;
        let mut r = proto::Reader::new(&body);
        let _total = r.u64_or_err()?;
        let missed = r.u64_or_err()?;
        let next = r.u64_or_err()?;
        let count = r.u64_or_err()?;
        let mut events = Vec::with_capacity(count as usize);
        let mut rest = r.remaining();
        for _ in 0..count {
            let record = proto::EventRecord::decode(rest)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short event"))?;
            events.push(record);
            rest = &rest[proto::EVENT_WIRE_LEN..];
        }
        Ok((missed, next, events))
    }

    pub fn stop(&mut self) -> Result<bool> {
        let body = self.request(proto::op::STOP, &[])?;
        Ok(body.first().copied().unwrap_or(0) != 0)
    }

    pub fn resume(&mut self) -> Result<bool> {
        let body = self.request(proto::op::RESUME, &[])?;
        Ok(body.first().copied().unwrap_or(0) != 0)
    }

    pub fn read_memory(&mut self, address: u64, size: u64) -> Result<Vec<u8>> {
        let mut request = Vec::with_capacity(16);
        proto::put_u64(&mut request, address);
        proto::put_u64(&mut request, size);
        let body = self.request(proto::op::READ_MEM, &request)?;
        let mut r = proto::Reader::new(&body);
        let _address = r.u64_or_err()?;
        let copied = r.u64_or_err()?;
        let data = r.remaining();
        if data.len() < copied as usize {
            return Err(Error::new(ErrorKind::InvalidData, "short read payload"));
        }
        Ok(data[..copied as usize].to_vec())
    }

    pub fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<u64> {
        let mut request = Vec::with_capacity(16 + data.len());
        proto::put_u64(&mut request, address);
        proto::put_u64(&mut request, data.len() as u64);
        request.extend_from_slice(data);
        let body = self.request(proto::op::WRITE_MEM, &request)?;
        let mut r = proto::Reader::new(&body);
        r.u64_or_err()
    }

    pub fn bp_set(&mut self, address: u64) -> Result<u32> {
        let mut request = Vec::with_capacity(8);
        proto::put_u64(&mut request, address);
        let body = self.request(proto::op::BP_SET, &request)?;
        proto::Reader::new(&body).u32_or_err()
    }

    /// Lists breakpoints. Returns (stopped, hit_tid, hit_addr, stop_gen,
    /// entries); hit_tid == u32::MAX after a manual pause or resume, and
    /// stop_gen bumps on every completed stop.
    pub fn bp_list(&mut self) -> Result<(bool, u32, u64, u64, Vec<(u32, u64, u64)>)> {
        let body = self.request(proto::op::BP_LIST, &[])?;
        if body.len() < 21 {
            return Err(Error::new(ErrorKind::InvalidData, "short bp list"));
        }
        let stopped = body[0] != 0;
        let mut r = proto::Reader::new(&body[1..]);
        let hit_tid = r.u32_or_err()?;
        let hit_addr = r.u64_or_err()?;
        let stop_gen = r.u64_or_err()?;
        let count = r.u32_or_err()?;
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let id = r.u32_or_err()?;
            let address = r.u64_or_err()?;
            let hits = r.u64_or_err()?;
            entries.push((id, address, hits));
        }
        Ok((stopped, hit_tid, hit_addr, stop_gen, entries))
    }

    /// Minimal entry-breakpoint status. New agents append the planted entry
    /// address to BP_LIST; `None` preserves compatibility with older agents.
    pub fn entry_stop_status(&mut self) -> Result<(bool, u32, u64, Option<u64>)> {
        let body = self.request(proto::op::BP_LIST, &[])?;
        if body.len() < 21 {
            return Err(Error::new(ErrorKind::InvalidData, "short bp list"));
        }
        let stopped = body[0] != 0;
        let mut r = proto::Reader::new(&body[1..]);
        let hit_tid = r.u32_or_err()?;
        let hit_addr = r.u64_or_err()?;
        let _stop_gen = r.u64_or_err()?;
        let count = r.u32_or_err()? as usize;
        let rows_len = count
            .checked_mul(20)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "bp list count overflow"))?;
        r.skip(rows_len)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short bp list entries"))?;
        Ok((stopped, hit_tid, hit_addr, r.u64()))
    }

    pub fn bp_remove(&mut self, id: u32) -> Result<u32> {
        let mut request = Vec::with_capacity(4);
        proto::put_u32(&mut request, id);
        let body = self.request(proto::op::BP_REMOVE, &request)?;
        proto::Reader::new(&body).u32_or_err()
    }

    pub fn modules(&mut self) -> Result<Vec<(u64, u64, bool, String)>> {
        let body = self.request(proto::op::MODULES, &[])?;
        let mut r = proto::Reader::new(&body);
        let count = r.u32_or_err()?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let low = r.u64_or_err()?;
            let high = r.u64_or_err()?;
            let is_main = r.u8_or_err()? != 0;
            let name_len = r.u32_or_err()? as usize;
            let rest = r.remaining();
            if rest.len() < name_len {
                return Err(Error::new(ErrorKind::InvalidData, "short module name"));
            }
            let name = String::from_utf8_lossy(&rest[..name_len]).into_owned();
            r.skip(name_len)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short module entry"))?;
            out.push((low, high, is_main, name));
        }
        Ok(out)
    }

    pub fn threads(&mut self) -> Result<Vec<u32>> {
        let body = self.request(proto::op::THREADS, &[])?;
        let mut r = proto::Reader::new(&body);
        let count = r.u32_or_err()?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            out.push(r.u32_or_err()?);
        }
        Ok(out)
    }

    /// (reg_id, value) pairs in the server's canonical GP register order.
    pub fn context_get(&mut self, thread_id: u32) -> Result<Vec<(u32, u64)>> {
        let mut request = Vec::with_capacity(4);
        proto::put_u32(&mut request, thread_id);
        let body = self.request(proto::op::CONTEXT_GET, &request)?;
        let mut r = proto::Reader::new(&body);
        let count = r.u32_or_err()?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let reg = r.u32_or_err()?;
            let value = r.u64_or_err()?;
            out.push((reg, value));
        }
        Ok(out)
    }

    pub fn context_set(&mut self, thread_id: u32, reg: u32, value: u64) -> Result<()> {
        let mut request = Vec::with_capacity(16);
        proto::put_u32(&mut request, thread_id);
        proto::put_u32(&mut request, reg);
        proto::put_u64(&mut request, value);
        self.request(proto::op::CONTEXT_SET, &request)?;
        Ok(())
    }

    pub fn engine_set(&mut self, kind: u32, on: bool) -> Result<()> {
        let mut request = Vec::with_capacity(5);
        proto::put_u32(&mut request, kind);
        request.push(on as u8);
        self.request(proto::op::ENGINE_SET, &request)?;
        Ok(())
    }

    pub fn exc_policy_set(&mut self, enabled: bool, code: u32) -> Result<()> {
        let mut request = Vec::with_capacity(5);
        request.push(enabled as u8);
        proto::put_u32(&mut request, code);
        self.request(proto::op::EXC_POLICY_SET, &request)?;
        Ok(())
    }

    pub fn exc_policy_get(&mut self) -> Result<(bool, u32, bool)> {
        let body = self.request(proto::op::EXC_POLICY_GET, &[])?;
        if body.len() < 6 {
            return Err(Error::new(ErrorKind::InvalidData, "short policy payload"));
        }
        let enabled = body[0] != 0;
        let code = u32::from_le_bytes(body[1..5].try_into().unwrap());
        let pending = body[5] != 0;
        Ok((enabled, code, pending))
    }

    pub fn step(&mut self, thread_id: u32, over: bool) -> Result<bool> {
        let mut request = Vec::with_capacity(5);
        proto::put_u32(&mut request, thread_id);
        request.push(over as u8);
        let body = self.request(proto::op::STEP, &request)?;
        Ok(body.first().copied().unwrap_or(0) != 0)
    }

    /// (address, size, kind, text, bytes, target) rows decoded at `address`.
    /// target is the flow successor for branch/call rows when computable
    /// without a thread context (direct targets, rip-relative IAT slots), 0
    /// otherwise.
    pub fn disasm(
        &mut self,
        address: u64,
        count: u64,
    ) -> Result<Vec<(u64, u32, u32, String, Vec<u8>, u64)>> {
        let mut request = Vec::with_capacity(12);
        proto::put_u64(&mut request, address);
        proto::put_u64(&mut request, count);
        let body = self.request(proto::op::DISASM, &request)?;
        let mut r = proto::Reader::new(&body);
        let total = r.u32_or_err()?;
        let mut out = Vec::with_capacity(total as usize);
        for _ in 0..total {
            let address = r.u64_or_err()?;
            let size = r.u32_or_err()?;
            let kind = r.u32_or_err()?;
            let target = r.u64_or_err()?;
            let text_len = r.u32_or_err()? as usize;
            let rest = r.remaining();
            if rest.len() < text_len + size as usize {
                return Err(Error::new(ErrorKind::InvalidData, "short disasm row"));
            }
            let text = String::from_utf8_lossy(&rest[..text_len]).into_owned();
            let raw = &rest[text_len..text_len + size as usize];
            let bytes = raw.to_vec();
            r.skip(text_len + size as usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short disasm row"))?;
            out.push((address, size, kind, text, bytes, target));
        }
        Ok(out)
    }

    /// One resolved address: kind 0 = unknown, 1 = module+offset,
    /// 2 = export (symbol, offset past it). Import thunks resolve through
    /// one chase level to the real API name.
    pub fn resolve(&mut self, addresses: &[u64]) -> Result<Vec<Resolution>> {
        let mut request = Vec::with_capacity(4 + addresses.len() * 8);
        proto::put_u32(&mut request, addresses.len() as u32);
        for address in addresses {
            proto::put_u64(&mut request, *address);
        }
        let body = self.request(proto::op::RESOLVE, &request)?;
        let mut r = proto::Reader::new(&body);
        let count = r.u32_or_err()? as usize;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = r.u8_or_err()?;
            let _base = r.u64_or_err()?;
            let offset = r.u64_or_err()?;
            let module = read_short_str(&mut r)?;
            let symbol = read_short_str(&mut r)?;
            out.push(Resolution { kind, offset, module, symbol });
        }
        Ok(out)
    }

    /// "module!Export" -> absolute address (0 when unknown).
    pub fn resolve_name(&mut self, spec: &str) -> Result<u64> {
        let mut request = Vec::with_capacity(2 + spec.len());
        request.extend_from_slice(&(spec.len() as u16).to_le_bytes());
        request.extend_from_slice(spec.as_bytes());
        let body = self.request(proto::op::RESOLVE_NAME, &request)?;
        proto::Reader::new(&body).u64_or_err()
    }
    /// Loads (or replaces) the in-process Python script. The server's error
    /// text (compile errors etc.) surfaces through the returned Err.
    pub fn script_load(&mut self, name: &str, source: &str) -> Result<u32> {
        let mut request = Vec::with_capacity(6 + name.len() + source.len());
        request.extend_from_slice(&(name.len() as u16).to_le_bytes());
        request.extend_from_slice(name.as_bytes());
        proto::put_u32(&mut request, source.len() as u32);
        request.extend_from_slice(source.as_bytes());
        let body = self.request(proto::op::SCRIPT_LOAD, &request)?;
        proto::Reader::new(&body).u32_or_err()
    }

    /// Unloads script(s) by name; an empty name unloads all.
    pub fn script_unload(&mut self, name: &str) -> Result<()> {
        let mut request = Vec::with_capacity(2 + name.len());
        request.extend_from_slice(&(name.len() as u16).to_le_bytes());
        request.extend_from_slice(name.as_bytes());
        self.request(proto::op::SCRIPT_UNLOAD, &request)?;
        Ok(())
    }

    /// Old single-script SCRIPT_STATUS snapshot.
    ///
    /// Deprecated: the server is replacing op 0x42 with the multi-script
    /// SCRIPT_LIST; use [`Client::script_list`] once the new server lands.
    /// Kept so current consumers (script_e2e, CLI `script status`) keep
    /// working against the old server.
    pub fn script_status(&mut self) -> Result<ScriptStatus> {
        let body = self.request(proto::op::SCRIPT_STATUS, &[])?;
        if body.len() < 20 {
            return Err(Error::new(ErrorKind::InvalidData, "short script status"));
        }
        let loaded = body[0] != 0;
        let state = body[1];
        let mut r = proto::Reader::new(&body[2..]);
        let delivered = r.u64_or_err()?;
        let dropped = r.u64_or_err()?;
        let name_len = r.u16_or_err()? as usize;
        let rest = r.remaining();
        if rest.len() < name_len {
            return Err(Error::new(ErrorKind::InvalidData, "short script name"));
        }
        let name = String::from_utf8_lossy(&rest[..name_len]).into_owned();
        Ok(ScriptStatus {
            loaded,
            state,
            delivered,
            dropped,
            name,
        })
    }

    /// Multi-script listing (new server on op 0x42). Wire-level only until
    /// the new server lands; against the old single-script server this
    /// mis-parses by design.
    pub fn script_list(&mut self) -> Result<Vec<ScriptListEntry>> {
        // op 0x42: SCRIPT_STATUS today, SCRIPT_LIST on the new server.
        let body = self.request(proto::op::SCRIPT_STATUS, &[])?;
        let mut r = proto::Reader::new(&body);
        let count = r.u32_or_err()?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let name = read_short_str(&mut r)?;
            let state = r.u8_or_err()?;
            let delivered = r.u64_or_err()?;
            let dropped = r.u64_or_err()?;
            out.push(ScriptListEntry {
                name,
                state,
                delivered,
                dropped,
            });
        }
        Ok(out)
    }

    /// Paged plugin output lines (new server, op 0x43). Wire-level only
    /// until the server handler lands.
    pub fn script_output(&mut self, after: u64, limit: u32) -> Result<(u64, Vec<OutputLine>)> {
        let mut request = Vec::with_capacity(12);
        proto::put_u64(&mut request, after);
        proto::put_u32(&mut request, limit);
        let body = self.request(proto::op::SCRIPT_OUTPUT, &request)?;
        let mut r = proto::Reader::new(&body);
        let next = r.u64_or_err()?;
        let count = r.u32_or_err()?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let seq = r.u64_or_err()?;
            let plugin = read_short_str(&mut r)?;
            let line = read_short_str(&mut r)?;
            out.push(OutputLine { seq, plugin, line });
        }
        Ok((next, out))
    }

    /// Named exports of a loaded module: (address, name) pairs.
    pub fn exports(&mut self, module: &str) -> Result<Vec<(u64, String)>> {
        let mut request = Vec::with_capacity(2 + module.len());
        request.extend_from_slice(&(module.len() as u16).to_le_bytes());
        request.extend_from_slice(module.as_bytes());
        let body = self.request(proto::op::EXPORTS, &request)?;
        let mut r = proto::Reader::new(&body);
        let count = r.u32_or_err()?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let address = r.u64_or_err()?;
            let name = read_short_str(&mut r)?;
            out.push((address, name));
        }
        Ok(out)
    }

    /// Syscall number filter; mode 0 = all syscalls, 1 = only the listed
    /// numbers.
    pub fn syscall_filter(&mut self, mode: u8, numbers: &[u32]) -> Result<()> {
        let mut request = Vec::with_capacity(3 + numbers.len() * 4);
        request.push(mode);
        request.extend_from_slice(&(numbers.len() as u16).to_le_bytes());
        for number in numbers {
            proto::put_u32(&mut request, *number);
        }
        self.request(proto::op::SYSCALL_FILTER, &request)?;
        Ok(())
    }

    /// Adds `address` to the runtime hook point set. Returns false when the
    /// 4096-entry cap is hit.
    pub fn hook_set(&mut self, address: u64) -> Result<bool> {
        let mut request = Vec::with_capacity(8);
        proto::put_u64(&mut request, address);
        let body = self.request(proto::op::HOOK_SET, &request)?;
        Ok(proto::Reader::new(&body).u32_or_err()? != 0)
    }

    pub fn hook_remove(&mut self, address: u64) -> Result<()> {
        let mut request = Vec::with_capacity(8);
        proto::put_u64(&mut request, address);
        self.request(proto::op::HOOK_REMOVE, &request)?;
        Ok(())
    }

    pub fn hook_clear(&mut self) -> Result<()> {
        self.request(proto::op::HOOK_CLEAR, &[])?;
        Ok(())
    }

    pub fn hook_list(&mut self) -> Result<Vec<u64>> {
        let body = self.request(proto::op::HOOK_LIST, &[])?;
        let mut r = proto::Reader::new(&body);
        let count = r.u32_or_err()?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            out.push(r.u64_or_err()?);
        }
        Ok(out)
    }

    /// Adds or replaces a synchronous native Hook action rule. Register ids
    /// are Pin `PbRegId` values for the target architecture; `match_reg=0`
    /// means unconditional and `thread_id=u32::MAX` means all threads.
    pub fn hook_rule_set(
        &mut self,
        address: u64,
        thread_id: u32,
        match_reg: u32,
        match_mask: u64,
        match_value: u64,
        set_reg: u32,
        set_value: u64,
    ) -> Result<bool> {
        let mut request = Vec::with_capacity(44);
        proto::put_u64(&mut request, address);
        proto::put_u32(&mut request, thread_id);
        proto::put_u32(&mut request, match_reg);
        proto::put_u64(&mut request, match_mask);
        proto::put_u64(&mut request, match_value);
        proto::put_u32(&mut request, set_reg);
        proto::put_u64(&mut request, set_value);
        let body = self.request(proto::op::HOOK_RULE_SET, &request)?;
        Ok(proto::Reader::new(&body).u32_or_err()? != 0)
    }

    pub fn hook_rule_clear(&mut self) -> Result<()> {
        self.request(proto::op::HOOK_RULE_CLEAR, &[])?;
        Ok(())
    }

    /// Arms the trace recording channel: records the given event kinds for
    /// instructions in [lo, hi) into a .pbtr file at `path`. kinds are event
    /// kind numbers (2 memory, 3 exec, 4 branch, 9 exec_bytes, 10
    /// mem_value). The server's reason text surfaces through Err.
    pub fn trace_start(&mut self, kinds: &[u32], lo: u64, hi: u64, path: &str) -> Result<()> {
        let mut kinds_mask: u32 = 0;
        for kind in kinds {
            kinds_mask |= 1 << kind;
        }
        let mut request = Vec::with_capacity(22 + path.len());
        proto::put_u32(&mut request, kinds_mask);
        proto::put_u64(&mut request, lo);
        proto::put_u64(&mut request, hi);
        request.extend_from_slice(&(path.len() as u16).to_le_bytes());
        request.extend_from_slice(path.as_bytes());
        let body = self.request(proto::op::TRACE_START, &request)?;
        let ok = proto::Reader::new(&body).u32_or_err()?;
        if ok == 0 {
            return Err(Error::new(ErrorKind::Other, "trace start refused"));
        }
        Ok(())
    }

    /// Arms recording for multiple address ranges and an optional thread
    /// allowlist. Empty `threads` means all threads.
    pub fn trace_start_spec(
        &mut self,
        kinds: &[u32],
        ranges: &[(u64, u64)],
        threads: &[u32],
        path: &str,
    ) -> Result<()> {
        if ranges.is_empty() || ranges.len() > 16 || threads.len() > 64 {
            return Err(Error::new(ErrorKind::InvalidInput, "invalid trace spec size"));
        }
        let mut kinds_mask: u32 = 0;
        for kind in kinds {
            if *kind >= 32 {
                return Err(Error::new(ErrorKind::InvalidInput, "invalid trace kind"));
            }
            kinds_mask |= 1 << kind;
        }
        if path.len() > u16::MAX as usize {
            return Err(Error::new(ErrorKind::InvalidInput, "trace path too long"));
        }
        let mut request = Vec::with_capacity(8 + ranges.len() * 16 + threads.len() * 4 + path.len());
        proto::put_u32(&mut request, kinds_mask);
        request.extend_from_slice(&(ranges.len() as u16).to_le_bytes());
        for (lo, hi) in ranges {
            proto::put_u64(&mut request, *lo);
            proto::put_u64(&mut request, *hi);
        }
        request.extend_from_slice(&(threads.len() as u16).to_le_bytes());
        for tid in threads {
            proto::put_u32(&mut request, *tid);
        }
        request.extend_from_slice(&(path.len() as u16).to_le_bytes());
        request.extend_from_slice(path.as_bytes());
        let body = self.request(proto::op::TRACE_START_SPEC, &request)?;
        if proto::Reader::new(&body).u32_or_err()? == 0 {
            return Err(Error::new(ErrorKind::Other, "trace start refused"));
        }
        Ok(())
    }

    /// Adds executable/data ranges to an armed trace. Existing thread and
    /// kind filters remain unchanged; the native recorder updates its gate
    /// before subsequent ring claims.
    pub fn trace_extend(&mut self, ranges: &[(u64, u64)]) -> Result<()> {
        if ranges.is_empty() || ranges.len() > 16 {
            return Err(Error::new(ErrorKind::InvalidInput, "invalid trace extension size"));
        }
        let mut request = Vec::with_capacity(2 + ranges.len() * 16);
        request.extend_from_slice(&(ranges.len() as u16).to_le_bytes());
        for (lo, hi) in ranges {
            proto::put_u64(&mut request, *lo);
            proto::put_u64(&mut request, *hi);
        }
        self.request(proto::op::TRACE_EXTEND, &request)?;
        Ok(())
    }

    /// Queries the virtual memory region containing `address`.
    pub fn memory_region(&mut self, address: u64) -> Result<Option<MemoryRegion>> {
        let mut request = Vec::with_capacity(8);
        proto::put_u64(&mut request, address);
        let body = self.request(proto::op::MEMORY_REGION, &request)?;
        if body.is_empty() || body[0] == 0 {
            return Ok(None);
        }
        let mut r = proto::Reader::new(&body[1..]);
        Ok(Some(MemoryRegion {
            base: r.u64_or_err()?,
            size: r.u64_or_err()?,
            allocation_base: r.u64_or_err()?,
            protect: r.u32_or_err()?,
            state: r.u32_or_err()?,
            kind: r.u32_or_err()?,
        }))
    }

    /// Stops the recording session (waits for the file drain, ~5s bound).
    /// Returns (recorded, dropped).
    pub fn trace_stop(&mut self) -> Result<(u64, u64)> {
        let body = self.request(proto::op::TRACE_STOP, &[])?;
        let mut r = proto::Reader::new(&body);
        Ok((r.u64_or_err()?, r.u64_or_err()?))
    }

    /// (active, recorded, dropped) snapshot of the recording channel.
    pub fn trace_status(&mut self) -> Result<(bool, u64, u64)> {
        let body = self.request(proto::op::TRACE_STATUS, &[])?;
        if body.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "short trace status"));
        }
        let active = body[0] != 0;
        let mut r = proto::Reader::new(&body[1..]);
        Ok((active, r.u64_or_err()?, r.u64_or_err()?))
    }
}

/// Windows VirtualQuery result for a target address.
pub struct MemoryRegion {
    pub base: u64,
    pub size: u64,
    pub allocation_base: u64,
    pub protect: u32,
    pub state: u32,
    pub kind: u32,
}

/// SCRIPT_STATUS snapshot; state: 0 none, 1 running, 2 error.
pub struct ScriptStatus {
    pub loaded: bool,
    pub state: u8,
    pub delivered: u64,
    pub dropped: u64,
    pub name: String,
}

/// One row of the multi-script SCRIPT_LIST reply.
pub struct ScriptListEntry {
    pub name: String,
    pub state: u8,
    pub delivered: u64,
    pub dropped: u64,
}

/// One plugin output line from SCRIPT_OUTPUT.
pub struct OutputLine {
    pub seq: u64,
    pub plugin: String,
    pub line: String,
}

/// Resolution result; `display()` renders x64dbg-style text.
pub struct Resolution {
    pub kind: u8,
    pub offset: u64,
    pub module: String,
    pub symbol: String,
}

impl Resolution {
    pub fn display(&self) -> Option<String> {
        match self.kind {
            2 if self.offset > 0 => Some(format!("{}!{}+0x{:x}", self.module, self.symbol, self.offset)),
            2 => Some(format!("{}!{}", self.module, self.symbol)),
            1 => Some(format!("{}+0x{:x}", self.module, self.offset)),
            _ => None,
        }
    }
}

fn read_short_str(r: &mut proto::Reader) -> Result<String> {
    let head = r.remaining();
    if head.len() < 2 {
        return Err(Error::new(ErrorKind::InvalidData, "short string"));
    }
    let len = u16::from_le_bytes(head[..2].try_into().unwrap()) as usize;
    let rest = &head[2..];
    if rest.len() < len {
        return Err(Error::new(ErrorKind::InvalidData, "short string"));
    }
    let text = String::from_utf8_lossy(&rest[..len]).into_owned();
    r.skip(2 + len)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short string"))?;
    Ok(text)
}

trait ReaderExt {
    fn u32_or_err(&mut self) -> Result<u32>;
    fn u64_or_err(&mut self) -> Result<u64>;
    fn u8_or_err(&mut self) -> Result<u8>;
    fn u16_or_err(&mut self) -> Result<u16>;
}

impl<'a> ReaderExt for proto::Reader<'a> {
    fn u32_or_err(&mut self) -> Result<u32> {
        self.u32()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short payload"))
    }
    fn u64_or_err(&mut self) -> Result<u64> {
        self.u64()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short payload"))
    }
    fn u8_or_err(&mut self) -> Result<u8> {
        self.u8()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short payload"))
    }
    fn u16_or_err(&mut self) -> Result<u16> {
        self.u16()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "short payload"))
    }
}
