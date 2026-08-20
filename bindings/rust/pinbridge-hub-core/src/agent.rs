//! Agent access owned by Hub. Every call connects, performs one logical
//! request sequence, and drops the Client before releasing the global gate.

use pinbridge_client::{registers, Client};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
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
    fn breakpoint_inventory(&self) -> std::result::Result<Value, AgentError> {
        self.breakpoint_list()
    }
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
    fn module_exports(&self, module: &str) -> std::result::Result<Value, AgentError> {
        let _ = module;
        Err(AgentError::Operation("module exports unsupported".into()))
    }
    fn hook_set(&self, address: u64) -> std::result::Result<Value, AgentError> {
        let _ = address;
        Err(AgentError::Operation("instruction Hook unsupported".into()))
    }
    fn hook_function_set(&self, address: u64) -> std::result::Result<Value, AgentError> {
        let _ = address;
        Err(AgentError::Operation(
            "function call Hook unsupported".into(),
        ))
    }
    fn hook_signature_set(
        &self,
        address: u64,
        calling_convention: u32,
        return_kind: u32,
        parameter_count: u32,
        float_parameter_mask: u32,
    ) -> std::result::Result<(), AgentError> {
        let _ = (
            address,
            calling_convention,
            return_kind,
            parameter_count,
            float_parameter_mask,
        );
        Err(AgentError::Operation(
            "typed Hook signature capture unsupported".into(),
        ))
    }
    fn hook_signature_remove(&self, address: u64) -> std::result::Result<(), AgentError> {
        let _ = address;
        Err(AgentError::Operation(
            "typed Hook signature capture unsupported".into(),
        ))
    }
    fn hook_remove(&self, address: u64) -> std::result::Result<Value, AgentError> {
        let _ = address;
        Err(AgentError::Operation("instruction Hook unsupported".into()))
    }
    fn hook_clear(&self) -> std::result::Result<Value, AgentError> {
        Err(AgentError::Operation("instruction Hook unsupported".into()))
    }
    fn hook_list(&self) -> std::result::Result<Value, AgentError> {
        Err(AgentError::Operation("instruction Hook unsupported".into()))
    }
    fn hook_inventory(&self) -> std::result::Result<Value, AgentError> {
        self.hook_list()
    }
    fn hook_inventory_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> std::result::Result<Value, AgentError> {
        self.hook_inventory_page_filtered(offset, limit, None)
    }
    fn hook_inventory_page_filtered(
        &self,
        offset: usize,
        limit: usize,
        function_log: Option<bool>,
    ) -> std::result::Result<Value, AgentError> {
        let mut value = self.hook_inventory()?;
        let (total, start, returned) = {
            let Some(hooks) = value.get_mut("hooks").and_then(Value::as_array_mut) else {
                return Ok(value);
            };
            if let Some(expected) = function_log {
                hooks.retain(|hook| {
                    hook.get("function_log")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                        == expected
                });
            }
            let total = hooks.len();
            let start = offset.min(total);
            let end = start.saturating_add(limit).min(total);
            let page = hooks.drain(start..end).collect::<Vec<_>>();
            *hooks = page;
            (total, start, hooks.len())
        };
        if let Some(object) = value.as_object_mut() {
            object.insert("count".into(), Value::String(total.to_string()));
            object.insert("offset".into(), Value::String(start.to_string()));
            object.insert("returned".into(), Value::String(returned.to_string()));
        }
        Ok(value)
    }
    fn hook_callback_count(&self, address: Option<u64>) -> std::result::Result<usize, AgentError> {
        let inventory = self.hook_inventory()?;
        Ok(inventory["hooks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|hook| {
                address.is_none()
                    || hook["address"].as_str().and_then(|value| {
                        value
                            .strip_prefix("0x")
                            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
                            .or_else(|| value.parse().ok())
                    }) == address
            })
            .filter_map(|hook| hook["callbacks"].as_array())
            .map(Vec::len)
            .sum())
    }
    fn hook_monitor(&self, limit: u64, before: u64) -> std::result::Result<Value, AgentError> {
        let _ = (limit, before);
        Err(AgentError::Operation("Hook monitor unsupported".into()))
    }
    fn hook_module(&self, module: &str) -> std::result::Result<Value, AgentError> {
        let _ = module;
        Err(AgentError::Operation("DLL Hook unsupported".into()))
    }
    fn hook_targets_apply(
        &self,
        module: &str,
        targets: &[(u64, String)],
    ) -> std::result::Result<Value, AgentError> {
        let _ = (module, targets);
        Err(AgentError::Operation(
            "explicit Hook target application unsupported".into(),
        ))
    }
    fn hook_range(
        &self,
        start: u64,
        end: u64,
        kind_mask: u32,
        apply: bool,
    ) -> std::result::Result<Value, AgentError> {
        let _ = (start, end, kind_mask, apply);
        Err(AgentError::Operation("range Hook unsupported".into()))
    }
    fn syscall_config_set(
        &self,
        enabled: bool,
        numbers: &[u32],
        scope_start: u64,
        scope_end: u64,
    ) -> std::result::Result<Value, AgentError> {
        let _ = (enabled, numbers, scope_start, scope_end);
        Err(AgentError::Operation("syscall capture unsupported".into()))
    }
    fn syscall_monitor(&self, limit: u64) -> std::result::Result<Value, AgentError> {
        let _ = limit;
        Err(AgentError::Operation("syscall monitor unsupported".into()))
    }
    fn syscall_monitor_window(
        &self,
        limit: u64,
        before: u64,
    ) -> std::result::Result<Value, AgentError> {
        if before == 0 {
            self.syscall_monitor(limit)
        } else {
            Err(AgentError::Operation(
                "paged syscall monitor unsupported".into(),
            ))
        }
    }
    fn memory_map(&self) -> std::result::Result<Value, AgentError> {
        Err(AgentError::Operation("memory map unsupported".into()))
    }
    fn exception_monitor(&self, limit: u64) -> std::result::Result<Value, AgentError> {
        let _ = limit;
        Err(AgentError::Operation(
            "exception monitor unsupported".into(),
        ))
    }
    fn exception_policy_get(&self) -> std::result::Result<Value, AgentError> {
        Err(AgentError::Operation("exception policy unsupported".into()))
    }
    fn exception_policy_set(
        &self,
        enabled: bool,
        code: u32,
    ) -> std::result::Result<Value, AgentError> {
        let _ = (enabled, code);
        Err(AgentError::Operation("exception policy unsupported".into()))
    }
    fn exception_inventory(&self) -> std::result::Result<Value, AgentError> {
        Err(AgentError::Operation(
            "exception inventory unsupported".into(),
        ))
    }
    fn trace_start_spec(
        &self,
        kinds: &[u32],
        ranges: &[(u64, u64)],
        threads: &[u32],
        path: &str,
    ) -> std::result::Result<Value, AgentError> {
        let _ = (kinds, ranges, threads, path);
        Err(AgentError::Operation("Trace recording unsupported".into()))
    }
    fn trace_status(&self) -> std::result::Result<Value, AgentError> {
        Err(AgentError::Operation("Trace recording unsupported".into()))
    }
    fn trace_stop(&self) -> std::result::Result<Value, AgentError> {
        Err(AgentError::Operation("Trace recording unsupported".into()))
    }
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
    pointer_width: Arc<AtomicUsize>,
    hook_symbols: Arc<Mutex<HashMap<u64, (String, String)>>>,
}
impl AgentConnection {
    pub fn new(port: u16) -> Self {
        Self {
            port: Arc::new(Mutex::new(port)),
            gate: Arc::new(Mutex::new(())),
            requests: Arc::new(AtomicUsize::new(0)),
            pointer_width: Arc::new(AtomicUsize::new(0)),
            hook_symbols: Arc::new(Mutex::new(HashMap::new())),
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
        self.pointer_width.store(0, Ordering::Release);
    }
    fn port(&self) -> u16 {
        *self.port.lock().expect("agent port poisoned")
    }
    fn status(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let p = c.ping_full()?;
            if let Some(width) = p.pointer_width {
                self.pointer_width.store(width as usize, Ordering::Release);
            }
            let (t, d, cap, k) = c.counters()?;
            Ok(json!({"connected":true,"agent_port":dec(self.port() as u64),"pid":dec(p.pid as u64),"abi":{"major":dec(p.abi_major as u64),"minor":dec(p.abi_minor as u64)},"total_events":dec(t),"dropped_events":dec(d),"ring_capacity":dec(cap),"kind_counts":k.into_iter().map(dec).collect::<Vec<_>>(),"arch":p.arch.map(|x|dec(x as u64)),"pointer_width":p.pointer_width.map(|x|dec(x as u64))}))
        })
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
    fn breakpoint_inventory(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let (stopped, thread_id, hit_address, stop_generation, entries) = c.bp_list()?;
            let inventory = c.script_inventory()?;
            let breakpoints = entries
                .into_iter()
                .map(|(id, address, hits)| {
                    let callbacks = inventory
                        .breakpoints
                        .iter()
                        .filter(|binding| binding.id == id)
                        .map(|binding| {
                            json!({
                                "plugin": binding.plugin,
                                "callback": binding.callback,
                                "description": binding.description,
                                "once": binding.once,
                                "thread_id": binding.thread_id.map(|value| dec(value as u64)),
                                "last_stop_generation": dec(binding.last_stop_generation),
                                "last_action": binding.last_action,
                                "last_return": binding.last_return,
                                "last_error": binding.last_error,
                            })
                        })
                        .collect::<Vec<_>>();
                    json!({
                        "id": dec(id as u64),
                        "address": hex(address),
                        "hits": dec(hits),
                        "callback_count": dec(callbacks.len() as u64),
                        "callbacks": callbacks,
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "stopped": stopped,
                "hit_thread_id": dec(thread_id as u64),
                "hit_address": hex(hit_address),
                "stop_generation": dec(stop_generation),
                "breakpoints": breakpoints,
            }))
        })
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
    fn module_exports(&self, module: &str) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let exports = c.exports(module)?;
            Ok(json!({
                "module": module,
                "count": dec(exports.len() as u64),
                "exports": exports.into_iter().map(|(address, name)| json!({
                    "address": hex(address),
                    "name": name,
                })).collect::<Vec<_>>(),
            }))
        })
    }
    fn hook_set(&self, address: u64) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let hooked = c.hook_set(address)?;
            Ok(json!({"address":hex(address),"hooked":hooked}))
        })
    }
    fn hook_function_set(&self, address: u64) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let (armed, total, capacity_full) = c.hook_set_batch(&[address])?;
            let function_log = c.hook_function_list()?.binary_search(&address).is_ok();
            Ok(json!({
                "address": hex(address),
                "hooked": function_log,
                "function_log": function_log,
                "armed": dec(armed as u64),
                "total_hooks": dec(total as u64),
                "capacity_full": capacity_full,
            }))
        })
    }
    fn hook_signature_set(
        &self,
        address: u64,
        calling_convention: u32,
        return_kind: u32,
        parameter_count: u32,
        float_parameter_mask: u32,
    ) -> std::result::Result<(), AgentError> {
        self.with_client(|client| {
            let accepted = client.hook_signature_set(
                address,
                calling_convention,
                return_kind,
                parameter_count,
                float_parameter_mask,
            )?;
            if !accepted {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidInput,
                    "Agent rejected Hook signature layout",
                ));
            }
            Ok(())
        })
    }
    fn hook_signature_remove(&self, address: u64) -> std::result::Result<(), AgentError> {
        self.with_client(|client| client.hook_signature_remove(address))
    }
    fn hook_remove(&self, address: u64) -> std::result::Result<Value, AgentError> {
        let value = self.with_client(|c| {
            c.hook_remove(address)?;
            Ok(json!({"address":hex(address),"removed":true}))
        })?;
        self.hook_symbols
            .lock()
            .map_err(|_| AgentError::Operation("Hook symbol cache poisoned".into()))?
            .remove(&address);
        Ok(value)
    }
    fn hook_clear(&self) -> std::result::Result<Value, AgentError> {
        let value = self.with_client(|c| {
            let removed = c.hook_list()?.len();
            c.hook_clear()?;
            Ok(json!({"removed":dec(removed as u64)}))
        })?;
        self.hook_symbols
            .lock()
            .map_err(|_| AgentError::Operation("Hook symbol cache poisoned".into()))?
            .clear();
        Ok(value)
    }
    fn hook_list(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let hooks = c.hook_list()?;
            let functions = c.hook_function_list()?;
            Ok(json!({
                "count": dec(hooks.len() as u64),
                "capacity": "32768",
                "hooks": hooks.into_iter().map(|address| json!({
                    "address":hex(address),
                    "function_log": functions.binary_search(&address).is_ok(),
                })).collect::<Vec<_>>(),
            }))
        })
    }
    fn hook_inventory(&self) -> std::result::Result<Value, AgentError> {
        self.hook_inventory_page(0, 32768)
    }
    fn hook_inventory_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> std::result::Result<Value, AgentError> {
        self.hook_inventory_page_filtered(offset, limit, None)
    }
    fn hook_inventory_page_filtered(
        &self,
        offset: usize,
        limit: usize,
        function_log: Option<bool>,
    ) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let mut addresses = c.hook_list()?;
            let functions = c.hook_function_list()?;
            if let Some(expected) = function_log {
                addresses.retain(|address| (functions.binary_search(address).is_ok()) == expected);
            }
            let total = addresses.len();
            let start = offset.min(total);
            let end = start.saturating_add(limit.clamp(1, 32768)).min(total);
            let page_addresses = &addresses[start..end];
            let script_inventory = c.script_inventory()?;
            // Build callback ownership once. Scanning every decision for
            // every Hook turns a 20k inventory into O(Hooks * callbacks).
            let mut callbacks_by_address: HashMap<u64, Vec<Value>> = HashMap::new();
            for binding in &script_inventory.decisions {
                let Some(address) = binding.address else {
                    continue;
                };
                if !matches!(binding.selector.as_str(), "hook.entry" | "hook.return") {
                    continue;
                }
                if page_addresses.binary_search(&address).is_err() {
                    continue;
                }
                callbacks_by_address
                    .entry(address)
                    .or_default()
                    .push(json!({
                        "id": dec(binding.id),
                        "plugin": binding.plugin,
                        "selector": binding.selector,
                        "callback": binding.callback,
                        "description": binding.description,
                        "once": binding.once,
                        "thread_id": binding.thread_id.map(|value| dec(value as u64)),
                        "last_generation": dec(binding.last_generation),
                        "last_return": binding.last_return,
                        "last_error": binding.last_error,
                    }));
            }
            let mut resolutions = Vec::with_capacity(page_addresses.len());
            // 1024 keeps the worst-case resolved symbol response below the
            // Agent protocol's 1 MiB frame cap.
            for chunk in page_addresses.chunks(1024) {
                resolutions.extend(c.resolve(chunk)?);
            }
            let hooks = page_addresses
                .iter()
                .zip(resolutions)
                .map(|(address, resolution)| {
                    let callbacks = callbacks_by_address.remove(address).unwrap_or_default();
                    json!({
                        "address": hex(*address),
                        "module": resolution.module,
                        "symbol": resolution.symbol,
                        "offset": hex(resolution.offset),
                        "display": resolution.display(),
                        "function_log": functions.binary_search(address).is_ok(),
                        "callbacks": callbacks,
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "count": dec(total as u64),
                "offset": dec(start as u64),
                "returned": dec(hooks.len() as u64),
                "capacity":"32768",
                "hooks":hooks,
            }))
        })
    }
    fn hook_callback_count(&self, address: Option<u64>) -> std::result::Result<usize, AgentError> {
        self.with_client(|c| {
            let inventory = c.script_inventory()?;
            Ok(inventory
                .decisions
                .iter()
                .filter(|binding| {
                    matches!(binding.selector.as_str(), "hook.entry" | "hook.return")
                        && address
                            .map(|value| binding.address == Some(value))
                            .unwrap_or(true)
                })
                .count())
        })
    }
    fn hook_module(&self, module: &str) -> std::result::Result<Value, AgentError> {
        let (value, symbols) = self.with_client(|c| {
            let exports = c.exports(module)?;
            let export_count = exports.len();
            let mut seen = HashSet::with_capacity(export_count);
            let mut addresses = Vec::with_capacity(export_count);
            let mut symbols = Vec::with_capacity(export_count);
            for (address, name) in exports {
                if seen.insert(address) {
                    addresses.push(address);
                    symbols.push((address, (module.to_string(), name)));
                }
            }
            let unique_count = addresses.len();
            let (armed, total, capacity_full) = c.hook_set_batch(&addresses)?;
            Ok((
                json!({
                    "module": module,
                    "exports": dec(export_count as u64),
                    "unique_addresses": dec(unique_count as u64),
                    "armed": dec(armed as u64),
                    "total_hooks": dec(total as u64),
                    "skipped_aliases": dec(export_count.saturating_sub(unique_count) as u64),
                    "capacity_full": capacity_full,
                }),
                symbols,
            ))
        })?;
        self.hook_symbols
            .lock()
            .map_err(|_| AgentError::Operation("Hook symbol cache poisoned".into()))?
            .extend(symbols);
        Ok(value)
    }
    fn hook_targets_apply(
        &self,
        module: &str,
        targets: &[(u64, String)],
    ) -> std::result::Result<Value, AgentError> {
        let (value, symbols) = self.with_client(|client| {
            let addresses = targets
                .iter()
                .map(|(address, _)| *address)
                .collect::<Vec<_>>();
            let (armed, total, capacity_full) = client.hook_set_batch(&addresses)?;
            let symbols = targets
                .iter()
                .map(|(address, name)| (*address, (module.to_string(), name.clone())))
                .collect::<Vec<_>>();
            Ok((
                json!({
                    "module": module,
                    "requested": dec(addresses.len() as u64),
                    "armed": dec(armed as u64),
                    "total_hooks": dec(total as u64),
                    "capacity_full": capacity_full,
                }),
                symbols,
            ))
        })?;
        self.hook_symbols
            .lock()
            .map_err(|_| AgentError::Operation("Hook symbol cache poisoned".into()))?
            .extend(symbols);
        Ok(value)
    }
    fn hook_monitor(&self, limit: u64, before: u64) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let cached_width = self.pointer_width.load(Ordering::Acquire);
            let pointer_width = if cached_width == 0 {
                let width = c.ping_full()?.pointer_width.unwrap_or(8) as usize;
                self.pointer_width.store(width, Ordering::Release);
                width
            } else {
                cached_width
            };
            let (total, dropped, next, events) =
                c.hook_events_window(limit.clamp(1, 4096), before)?;
            let mut symbols = HashMap::with_capacity(events.len());
            {
                let cache = self
                    .hook_symbols
                    .lock()
                    .map_err(|_| std::io::Error::other("Hook symbol cache poisoned"))?;
                for event in &events {
                    if let Some(symbol) = cache.get(&event.address) {
                        symbols.insert(event.address, symbol.clone());
                    }
                }
            }
            let mut missing = events
                .iter()
                .map(|event| event.address)
                .filter(|address| !symbols.contains_key(address))
                .collect::<Vec<_>>();
            missing.sort_unstable();
            missing.dedup();
            let mut discovered = Vec::new();
            for chunk in missing.chunks(1024) {
                for (address, resolution) in chunk.iter().copied().zip(c.resolve(chunk)?) {
                    let value = (resolution.module, resolution.symbol);
                    symbols.insert(address, value.clone());
                    discovered.push((address, value));
                }
            }
            if !discovered.is_empty() {
                self.hook_symbols
                    .lock()
                    .map_err(|_| std::io::Error::other("Hook symbol cache poisoned"))?
                    .extend(discovered);
            }
            let hooks = events.into_iter()
                .map(|event| {
                    let symbol = symbols.get(&event.address);
                    let function_hook = event.flags & 2 != 0;
                    json!({
                    "sequence": dec(event.sequence),
                    "timestamp_unix_ns": dec(event.timestamp_unix_ns),
                    "kind": if !function_hook { "hit" } else if event.kind == 14 { "return" } else { "entry" },
                    "hook_type": if function_hook { "api" } else { "instruction" },
                    "thread_id": dec(event.thread_id as u64),
                    "address": hex(event.address),
                    "module": symbol.and_then(|(module, _)| (!module.is_empty()).then(|| module.clone())),
                    "symbol": symbol.and_then(|(_, name)| (!name.is_empty()).then(|| name.clone())),
                    "display": symbol.and_then(|(module, name)| (!name.is_empty()).then(|| format!("{module}!{name}"))),
                    "signature_capture": event.flags & 1 != 0,
                    "argument_count": dec(event.argument_count as u64),
                    "arguments": event.arguments[..event.argument_count.min(16) as usize].iter().copied().map(hex).collect::<Vec<_>>(),
                    "return_value": (function_hook && event.kind == 14).then(|| hex(event.arguments[0])),
                })})
                .collect::<Vec<_>>();
            Ok(json!({
                "lane_total": dec(total),
                "lane_dropped": dec(dropped),
                "history_overwritten": dec(total.saturating_sub(32768)),
                "next_cursor": dec(next),
                "capacity": "32768",
                "pointer_width": dec(pointer_width as u64),
                "window_before": dec(before),
                "events": hooks,
            }))
        })
    }
    fn hook_range(
        &self,
        start: u64,
        end: u64,
        kind_mask: u32,
        apply: bool,
    ) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let result = c.hook_range(start, end, kind_mask, apply)?;
            Ok(json!({
                "start": hex(start),
                "end": hex(end),
                "decoded": dec(result.decoded),
                "matched": dec(result.matched),
                "added": dec(result.added as u64),
                "total_hooks": dec(result.total as u64),
                "capacity_full": result.capacity_full,
                "truncated": result.truncated,
                "complete": result.complete,
                "applied": result.applied,
                "addresses": result.addresses.into_iter().map(hex).collect::<Vec<_>>(),
            }))
        })
    }
    fn syscall_config_set(
        &self,
        enabled: bool,
        numbers: &[u32],
        scope_start: u64,
        scope_end: u64,
    ) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            c.engine_set(5, enabled)?;
            if numbers.is_empty() {
                c.syscall_filter_scoped(0, &[], scope_start, scope_end)?;
            } else {
                c.syscall_filter_scoped(1, numbers, scope_start, scope_end)?;
            }
            Ok(json!({
                "enabled": enabled,
                "mode": if numbers.is_empty() { "all" } else { "selected" },
                "numbers": numbers.iter().map(|value| format!("0x{value:x}")).collect::<Vec<_>>(),
                "scope_start": hex(scope_start),
                "scope_end": hex(scope_end),
            }))
        })
    }
    fn syscall_monitor(&self, limit: u64) -> std::result::Result<Value, AgentError> {
        self.syscall_monitor_window(limit, 0)
    }
    fn syscall_monitor_window(
        &self,
        limit: u64,
        before: u64,
    ) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let keep = limit.clamp(1, 4096);
            let (total, dropped, next, events) = c.syscall_events_window(keep, before)?;
            let rows = events
                .into_iter()
                .map(|event| {
                    let entry = event.arguments[1] == 0;
                    json!({
                        "sequence": dec(event.sequence),
                        "timestamp_unix_ns": dec(event.timestamp_unix_ns),
                        "generation": dec(event.arguments[8]),
                        "thread_id": dec(event.thread_id as u64),
                        "number": format!("0x{:x}", event.arguments[0]),
                        "number_decimal": dec(event.arguments[0]),
                        "phase": if entry { "entry" } else { "exit" },
                        "kind": if entry { "entry" } else { "return" },
                        "hook_type": "syscall",
                        "arguments": if entry {
                            event.arguments[2..8].iter().copied().map(hex).collect::<Vec<_>>()
                        } else { Vec::<String>::new() },
                        "return_value": (!entry).then(|| hex(event.arguments[3])),
                        "errno": (!entry).then(|| hex(event.arguments[4])),
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "ring_total": dec(total),
                "ring_dropped": dec(dropped),
                "ring_capacity": "32768",
                "history_overwritten": dec(total.saturating_sub(32768)),
                "next_cursor": dec(next),
                "scan_limit": dec(keep),
                "window_before": dec(before),
                "events": rows,
            }))
        })
    }
    fn memory_map(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let layout = c.memory_map()?;
            Ok(json!({
                "regions": layout.regions.into_iter().map(|region| json!({
                    "base": hex(region.base),
                    "size": hex(region.size),
                    "allocation_base": hex(region.allocation_base),
                    "allocation_protect": format!("0x{:x}", region.allocation_protect),
                    "protect": format!("0x{:x}", region.protect),
                    "state": format!("0x{:x}", region.state),
                    "type": format!("0x{:x}", region.kind),
                })).collect::<Vec<_>>(),
                "heaps": layout.heaps.into_iter().map(hex).collect::<Vec<_>>(),
                "modules": layout.modules.into_iter().map(|module| json!({
                    "base": hex(module.low),
                    "end": hex(module.high),
                    "entry": hex(module.entry),
                    "mapped_size": hex(module.mapped_size),
                    "image_type": dec(module.image_type as u64),
                    "is_main": module.is_main,
                    "name": module.name,
                    "sections": module.sections.into_iter().map(|section| json!({
                        "name": section.name,
                        "address": hex(section.address),
                        "size": hex(section.size),
                        "kind": dec(section.kind as u64),
                        "readable": section.readable,
                        "writable": section.writable,
                        "executable": section.executable,
                        "mapped": section.mapped,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            }))
        })
    }
    fn exception_monitor(&self, limit: u64) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let arch = c.ping_full()?.arch.unwrap_or(1) as u32;
            let (total, dropped, next, events) = c.priority_newest(limit.clamp(1, 1024))?;
            // A kind-33 row is emitted after exception.handle has finished and
            // carries the final context. Join it to the original kind-6 edge
            // by context generation so consumers see one complete route.
            let dispositions = events
                .iter()
                .filter(|event| event.kind == 33)
                .map(|event| {
                    (
                        event.arg1,
                        (
                            event.sequence,
                            event.arg2,
                            event.arg3,
                            event.arg4 as u32,
                            event.arg5 != 0,
                            event.arg6 != 0,
                            event.arg7 != 0,
                        ),
                    )
                })
                .collect::<HashMap<_, _>>();
            let rows = events
                .into_iter()
                .filter_map(|event| match (event.kind, event.arg0) {
                    // Context-change reason 4 is a target exception. Other
                    // context edges remain available to scripts but do not
                    // belong in the exception monitor.
                    (6, 4) => {
                        let disposition = dispositions.get(&event.arg3).copied();
                        let (
                            disposition_sequence,
                            system_ip,
                            final_ip,
                            register_mask,
                            interceptor_ran,
                            final_ip_known,
                            system_ip_known,
                        ) = disposition.unwrap_or((
                            0,
                            event.arg4,
                            event.arg4,
                            0,
                            false,
                            event.arg5 != 0,
                            event.arg5 != 0,
                        ));
                        let modified_registers = registers::gp_regs(arch)
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| register_mask & (1u32 << index) != 0)
                            .map(|(_, (name, _))| *name)
                            .collect::<Vec<_>>();
                        Some(json!({
                            "sequence": dec(event.sequence),
                            "source": "target",
                            "thread_id": dec(event.thread_id as u64),
                            "code": format!("0x{:08x}", event.arg1 as u32),
                            "address": hex(event.address),
                            "from_ip": hex(event.arg2),
                            "from_ip_known": event.arg6 != 0,
                            "to_ip": hex(event.arg4),
                            "to_ip_known": event.arg5 != 0,
                            "system_to_ip": hex(system_ip),
                            "system_to_ip_known": system_ip_known,
                            "final_to_ip": hex(final_ip),
                            "final_to_ip_known": final_ip_known,
                            "disposition_available": disposition.is_some(),
                            "disposition_sequence": dec(disposition_sequence),
                            "interceptor_ran": interceptor_ran,
                            "takeover_applied": register_mask != 0,
                            "modified_register_mask": format!("0x{register_mask:08x}"),
                            "modified_registers": modified_registers,
                            "generation": dec(event.arg3),
                            "reason": dec(event.arg0),
                        }))
                    }
                    (24, _) => Some(json!({
                        "sequence": dec(event.sequence),
                        "source": "pin_internal",
                        "thread_id": dec(event.thread_id as u64),
                        "code": dec(event.arg0),
                        "address": hex(event.address),
                        "exception_address": hex(event.arg1),
                        "fault_address": hex(event.arg2),
                        "fault_address_known": event.arg5 != 0,
                        "access_type": dec(event.arg3),
                        "exception_class": dec(event.arg4),
                    })),
                    _ => None,
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "lane_total": dec(total),
                "lane_dropped": dec(dropped),
                "next": dec(next),
                "events": rows,
            }))
        })
    }
    fn exception_policy_get(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let (enabled, code, pending) = c.exc_policy_get()?;
            Ok(json!({
                "enabled": enabled,
                "code": format!("0x{code:08x}"),
                "pending": pending,
            }))
        })
    }
    fn exception_policy_set(
        &self,
        enabled: bool,
        code: u32,
    ) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            c.exc_policy_set(enabled, code)?;
            let (enabled, code, pending) = c.exc_policy_get()?;
            Ok(json!({
                "enabled": enabled,
                "code": format!("0x{code:08x}"),
                "pending": pending,
            }))
        })
    }
    fn exception_inventory(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|c| {
            let inventory = c.script_inventory()?;
            let interceptors = inventory
                .decisions
                .into_iter()
                .filter(|binding| binding.selector == "exception.handle")
                .map(|binding| json!({
                    "id": dec(binding.id),
                    "plugin": binding.plugin,
                    "callback": binding.callback,
                    "description": binding.description,
                    "once": binding.once,
                    "thread_id": binding.thread_id.map(|value| dec(value as u64)),
                    "codes": binding.codes.map(|codes| codes.into_iter().map(|code| format!("0x{code:08x}")).collect::<Vec<_>>()),
                    "last_generation": dec(binding.last_generation),
                    "last_return": binding.last_return,
                    "last_error": binding.last_error,
                }))
                .collect::<Vec<_>>();
            Ok(json!({"interceptors": interceptors}))
        })
    }
    fn trace_start_spec(
        &self,
        kinds: &[u32],
        ranges: &[(u64, u64)],
        threads: &[u32],
        path: &str,
    ) -> std::result::Result<Value, AgentError> {
        self.with_client(|client| {
            client.trace_start_spec(kinds, ranges, threads, path)?;
            let status = client.trace_status_detail()?;
            Ok(json!({
                "state": status.state_name(),
                "active": status.active,
                "recorded": dec(status.recorded),
                "dropped": dec(status.dropped),
            }))
        })
    }
    fn trace_status(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|client| {
            let status = client.trace_status_detail()?;
            Ok(json!({
                "state": status.state_name(),
                "active": status.active,
                "recorded": dec(status.recorded),
                "dropped": dec(status.dropped),
            }))
        })
    }
    fn trace_stop(&self) -> std::result::Result<Value, AgentError> {
        self.with_client(|client| {
            let (recorded, dropped) = client.trace_stop()?;
            let status = client.trace_status_detail()?;
            Ok(json!({
                "state": status.state_name(),
                "active": status.active,
                "recorded": dec(recorded),
                "dropped": dec(dropped),
            }))
        })
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
