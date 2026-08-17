//! Agent access owned by Hub. Every call connects, performs one logical
//! request sequence, and drops the Client before releasing the global gate.

use pinbridge_client::{registers, Client};
use serde_json::{json, Value};
use std::io::{ErrorKind, Result};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentScript {
    pub name: String,
    pub state: u8,
    pub delivered: u64,
    pub dropped: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentOutputLine {
    pub seq: u64,
    pub plugin: String,
    pub line: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentError {
    Connection(String),
    Operation(String),
}
fn parse_numeric_register(value: &str) -> Option<u32> {
    let value = value.trim();
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map(|v| u32::from_str_radix(v, 16).ok())
        .unwrap_or_else(|| value.parse().ok())
}

pub trait AgentApi: Send + Sync + Clone + 'static {
    fn set_port(&self, port: u16);
    fn port(&self) -> u16;
    fn status(&self) -> std::result::Result<Value, AgentError>;
    fn pause(&self) -> std::result::Result<bool, AgentError>;
    fn resume(&self) -> std::result::Result<bool, AgentError>;
    fn memory_read(&self, address: u64, size: u64) -> std::result::Result<Value, AgentError>;
    fn memory_write(&self, address: u64, data: &[u8]) -> std::result::Result<Value, AgentError>;
    fn breakpoint_set(&self, address: u64) -> std::result::Result<Value, AgentError>;
    fn breakpoint_remove(&self, id: u32) -> std::result::Result<Value, AgentError>;
    fn breakpoint_list(&self) -> std::result::Result<Value, AgentError>;
    fn registers_get(&self, thread_id: u32) -> std::result::Result<Value, AgentError>;
    fn register_set(
        &self,
        thread_id: u32,
        reg: u32,
        value: u64,
    ) -> std::result::Result<Value, AgentError>;
    fn register_id(&self, value: &str) -> std::result::Result<u32, AgentError> {
        parse_numeric_register(value)
            .ok_or_else(|| AgentError::Operation(format!("invalid register: {value}")))
    }
    fn threads(&self) -> std::result::Result<Value, AgentError>;
    fn modules(&self) -> std::result::Result<Value, AgentError>;
    fn step(&self, thread_id: u32, over: bool) -> std::result::Result<Value, AgentError>;
    fn disassemble(&self, address: u64, count: u64) -> std::result::Result<Value, AgentError>;
    fn resolve(&self, addresses: &[u64]) -> std::result::Result<Value, AgentError>;
    fn resolve_name(&self, name: &str) -> std::result::Result<Value, AgentError>;
    fn script_load(&self, name: &str, source: &str) -> std::result::Result<u32, AgentError>;
    fn script_unload(&self, name: &str) -> std::result::Result<(), AgentError>;
    fn script_list(&self) -> std::result::Result<Vec<AgentScript>, AgentError>;
    fn script_output(
        &self,
        after: u64,
        limit: u32,
    ) -> std::result::Result<(u64, Vec<AgentOutputLine>), AgentError>;
    fn events_newest(&self, limit: u64) -> std::result::Result<Value, AgentError> {
        let _ = limit;
        Err(AgentError::Operation("event snapshot unsupported".into()))
    }
    fn request_count(&self) -> usize {
        0
    }
}

#[derive(Clone)]
pub struct AgentConnection {
    port: Arc<Mutex<u16>>,
    gate: Arc<Mutex<()>>,
    requests: Arc<AtomicUsize>,
}
impl AgentConnection {
    pub fn new(port: u16) -> Self {
        Self {
            port: Arc::new(Mutex::new(port)),
            gate: Arc::new(Mutex::new(())),
            requests: Arc::new(AtomicUsize::new(0)),
        }
    }
    pub fn request_count(&self) -> usize {
        self.requests.load(Ordering::Relaxed)
    }
    fn with_client<T>(
        &self,
        f: impl FnOnce(&mut Client) -> Result<T>,
    ) -> std::result::Result<T, AgentError> {
        let _gate = self
            .gate
            .lock()
            .map_err(|_| AgentError::Connection("agent transport lock poisoned".into()))?;
        let port = *self
            .port
            .lock()
            .map_err(|_| AgentError::Connection("agent port lock poisoned".into()))?;
        self.requests.fetch_add(1, Ordering::Relaxed);
        let mut c = Client::connect(port).map_err(map_error)?;
        f(&mut c).map_err(map_error)
    }
}
fn map_error(e: std::io::Error) -> AgentError {
    match e.kind() {
        ErrorKind::ConnectionRefused
        | ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::BrokenPipe
        | ErrorKind::NotConnected
        | ErrorKind::UnexpectedEof
        | ErrorKind::TimedOut => AgentError::Connection(e.to_string()),
        _ => AgentError::Operation(e.to_string()),
    }
}
fn dec(v: u64) -> String {
    v.to_string()
}
fn hex(v: u64) -> String {
    format!("0x{v:x}")
}
fn bytes(v: &[u8]) -> String {
    v.iter().map(|b| format!("{b:02x}")).collect()
}
impl AgentApi for AgentConnection {
    fn set_port(&self, p: u16) {
        let _gate = self.gate.lock().expect("agent transport lock poisoned");
        *self.port.lock().expect("agent port poisoned") = p;
    }
    fn port(&self) -> u16 {
        *self.port.lock().expect("agent port poisoned")
    }
    fn status(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c|{let p=c.ping_full()?;let (t,d,cap,k)=c.counters()?;Ok(json!({"connected":true,"agent_port":dec(self.port() as u64),"pid":dec(p.pid as u64),"abi":{"major":dec(p.abi_major as u64),"minor":dec(p.abi_minor as u64)},"total_events":dec(t),"dropped_events":dec(d),"ring_capacity":dec(cap),"kind_counts":k.into_iter().map(dec).collect::<Vec<_>>(),"arch":p.arch.map(|x|dec(x as u64)),"pointer_width":p.pointer_width.map(|x|dec(x as u64))}))})
    }
    fn pause(&self) -> std::result::Result<bool, AgentError> {
        self.with_client(Client::stop)
    }
    fn resume(&self) -> std::result::Result<bool, AgentError> {
        self.with_client(Client::resume)
    }
    fn memory_read(&self, a: u64, s: u64) -> std::result::Result<Value, AgentError> {
        if s > 1 << 20 {
            return Err(AgentError::Operation("memory read exceeds 1 MiB".into()));
        }
        self.with_client(|c| {
            let d = c.read_memory(a, s)?;
            Ok(json!({"address":hex(a),"size":dec(d.len() as u64),"data_hex":bytes(&d)}))
        })
    }
    fn memory_write(&self, a: u64, d: &[u8]) -> std::result::Result<Value, AgentError> {
        if d.len() > 1 << 20 {
            return Err(AgentError::Operation("memory write exceeds 1 MiB".into()));
        }
        self.with_client(|c|Ok(json!({"address":hex(a),"requested":dec(d.len() as u64),"written":dec(c.write_memory(a,d)?)})))
    }
    fn breakpoint_set(&self, a: u64) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| Ok(json!({"id":dec(c.bp_set(a)? as u64),"address":hex(a)})))
    }
    fn breakpoint_remove(&self, id: u32) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| Ok(json!({"id":dec(c.bp_remove(id)? as u64)})))
    }
    fn breakpoint_list(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c|{let(s,tid,a,g,e)=c.bp_list()?;Ok(json!({"stopped":s,"hit_thread_id":dec(tid as u64),"hit_address":hex(a),"stop_generation":dec(g),"breakpoints":e.into_iter().map(|(id,a,h)|json!({"id":dec(id as u64),"address":hex(a),"hits":dec(h)})).collect::<Vec<_>>() }))})
    }
    fn registers_get(&self, tid: u32) -> std::result::Result<Value, AgentError> {
        self.with_client(|c|{let arch=c.ping_full()?.arch.unwrap_or(1);let r=c.context_get(tid)?;Ok(json!({"thread_id":dec(tid as u64),"registers":r.into_iter().map(|(id,v)|json!({"id":dec(id as u64),"value":hex(v)})).collect::<Vec<_>>(),"arch":dec(arch as u64)}))})
    }
    fn register_set(&self, tid: u32, reg: u32, v: u64) -> std::result::Result<Value, AgentError> {
        self.with_client(|c|{c.context_set(tid,reg,v)?;Ok(json!({"thread_id":dec(tid as u64),"register":dec(reg as u64),"value":hex(v),"updated":true}))})
    }
    fn register_id(&self, value: &str) -> std::result::Result<u32, AgentError> {
        if let Some(id) = parse_numeric_register(value) {
            return Ok(id);
        }
        self.with_client(|c| {
            let arch = c.ping_full()?.arch.unwrap_or(1);
            registers::reg_id(arch, value)
                .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "unknown register"))
        })
    }
    fn threads(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c|Ok(json!({"threads":c.threads()?.into_iter().map(|x|dec(x as u64)).collect::<Vec<_>>() })))
    }
    fn modules(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c|Ok(json!({"modules":c.modules()?.into_iter().map(|(a,b,m,n)|json!({"base":hex(a),"end":hex(b),"is_main":m,"name":n})).collect::<Vec<_>>() })))
    }
    fn step(&self, tid: u32, over: bool) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            Ok(json!({"thread_id":dec(tid as u64),"over":over,"stopped":c.step(tid,over)?}))
        })
    }
    fn disassemble(&self, a: u64, n: u64) -> std::result::Result<Value, AgentError> {
        if n > 4096 {
            return Err(AgentError::Operation(
                "disassemble count exceeds 4096".into(),
            ));
        }
        self.with_client(|c|Ok(json!({"instructions":c.disasm(a,n)?.into_iter().map(|(a,s,k,t,b,tar)|json!({"address":hex(a),"size":dec(s as u64),"kind":dec(k as u64),"text":t,"bytes_hex":bytes(&b),"target":hex(tar)})).collect::<Vec<_>>() })))
    }
    fn resolve(&self, aa: &[u64]) -> std::result::Result<Value, AgentError> {
        if aa.len() > 1024 {
            return Err(AgentError::Operation("too many addresses".into()));
        }
        self.with_client(|c|Ok(json!({"resolutions":c.resolve(aa)?.into_iter().map(|r|json!({"kind":dec(r.kind as u64),"offset":hex(r.offset),"module":r.module,"symbol":r.symbol,"display":r.display()})).collect::<Vec<_>>() })))
    }
    fn resolve_name(&self, n: &str) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| Ok(json!({"name":n,"address":hex(c.resolve_name(n)?)})))
    }
    fn script_load(&self, n: &str, s: &str) -> std::result::Result<u32, AgentError> {
        self.with_client(|c| c.script_load(n, s))
    }
    fn script_unload(&self, n: &str) -> std::result::Result<(), AgentError> {
        self.with_client(|c| c.script_unload(n))
    }
    fn script_list(&self) -> std::result::Result<Vec<AgentScript>, AgentError> {
        self.with_client(|c| {
            Ok(c.script_list()?
                .into_iter()
                .map(|x| AgentScript {
                    name: x.name,
                    state: x.state,
                    delivered: x.delivered,
                    dropped: x.dropped,
                })
                .collect())
        })
    }
    fn script_output(
        &self,
        a: u64,
        l: u32,
    ) -> std::result::Result<(u64, Vec<AgentOutputLine>), AgentError> {
        self.with_client(|c| {
            let (next, rows) = c.script_output(a, l)?;
            Ok((
                next,
                rows.into_iter()
                    .map(|x| AgentOutputLine {
                        seq: x.seq,
                        plugin: x.plugin,
                        line: x.line,
                    })
                    .collect(),
            ))
        })
    }
    fn events_newest(&self, limit: u64) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let (next, events) = c.ring_newest(limit.min(24))?;
            let rows = events
                .into_iter()
                .map(|event| {
                    json!({
                        "sequence": dec(event.sequence),
                        "kind": dec(event.kind as u64),
                        "thread_id": dec(event.thread_id as u64),
                        "address": hex(event.address),
                        "arg0": hex(event.arg0), "arg1": hex(event.arg1),
                        "arg2": hex(event.arg2), "arg3": hex(event.arg3),
                        "arg4": hex(event.arg4), "arg5": hex(event.arg5),
                        "arg6": hex(event.arg6), "arg7": hex(event.arg7),
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({"next": dec(next), "events": rows}))
        })
    }
    fn request_count(&self) -> usize {
        AgentConnection::request_count(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starts_without_idle_stream() {
        let a = AgentConnection::new(1);
        assert_eq!(a.request_count(), 0);
        assert_eq!(<AgentConnection as AgentApi>::request_count(&a), 0);
        a.set_port(2);
        assert_eq!(a.port(), 2);
    }
}
