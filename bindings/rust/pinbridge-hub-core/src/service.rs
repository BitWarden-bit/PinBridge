use crate::{
    activities::Journal,
    agent::{AgentApi, AgentError},
    control::{Caller, ChannelActor, ControlMode, ControlState},
    hook_signature::{self, HookSignature},
    script_service::{OutputRequest, ScriptRequest, ScriptService},
    session::Session,
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

pub const TOOL_NAMES: &[&str] = &[
    "control_status",
    "control_handoff_to_ai",
    "control_takeover_manual",
    "control_pause_automation",
    "session_status",
    "session_set_agent_port",
    "target_pause",
    "target_resume",
    "target_step_into",
    "target_step_over",
    "breakpoint_set",
    "breakpoint_remove",
    "breakpoint_list",
    "breakpoint_inventory",
    "registers_get",
    "register_set",
    "memory_read",
    "memory_write",
    "disassemble",
    "modules_list",
    "module_exports",
    "hook_targets_query",
    "hook_monitor_apply",
    "hook_set",
    "hook_function_set",
    "hook_signature_set",
    "hook_signature_remove",
    "hook_remove",
    "hook_clear",
    "hook_list",
    "hook_inventory",
    "hook_monitor",
    "hook_events_query",
    "hook_events_export",
    "event_index_query",
    "event_index_export",
    "trace_scope_query",
    "trace_record_start",
    "trace_record_status",
    "trace_record_stop",
    "trace_index_query",
    "trace_index_export",
    "hook_module",
    "hook_range_preview",
    "hook_range_set",
    "syscall_config_get",
    "syscall_config_set",
    "syscall_monitor",
    "memory_map",
    "exception_monitor",
    "exception_policy_get",
    "exception_policy_set",
    "exception_inventory",
    "threads_list",
    "address_resolve",
    "activity_list",
    "activity_get",
    "script_inject",
    "script_replace",
    "script_start",
    "script_stop",
    "script_remove",
    "script_list",
    "script_get",
    "script_status",
    "script_output",
];
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubError {
    Validation(String),
    Permission(String),
    Agent(String),
    Connection(String),
    Internal(String),
    Operation {
        operation_id: String,
        source: Box<HubError>,
    },
}
impl std::fmt::Display for HubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Validation(x) => format!("validation_failed: {x}"),
                Self::Permission(x) => format!("permission_denied: {x}"),
                Self::Agent(x) => format!("agent_failed: {x}"),
                Self::Connection(x) => format!("connection_failed: {x}"),
                Self::Internal(x) => format!("internal_error: {x}"),
                Self::Operation { source, .. } => source.to_string(),
            }
        )
    }
}
impl HubError {
    pub fn operation_id(&self) -> Option<&str> {
        match self {
            Self::Operation { operation_id, .. } => Some(operation_id),
            _ => None,
        }
    }
}
impl From<AgentError> for HubError {
    fn from(e: AgentError) -> Self {
        match e {
            AgentError::Connection(x) => Self::Connection(x),
            AgentError::Operation(x) => Self::Agent(x),
        }
    }
}
pub struct HubService<A: AgentApi> {
    pub agent: A,
    pub control: ControlState,
    pub journal: Journal,
    pub scripts: ScriptService<A>,
    pub session: Session,
    breakpoint_owners: Mutex<BTreeMap<u32, BTreeSet<String>>>,
    hook_owners: Mutex<BTreeMap<u64, BTreeSet<String>>>,
    hook_module_owners: Mutex<BTreeMap<String, BTreeSet<String>>>,
    hook_signatures: Mutex<BTreeMap<u64, HookSignature>>,
    hook_target_selections: Mutex<BTreeMap<String, HookTargetSelection>>,
    next_hook_target_selection: AtomicU64,
    trace_scope_selections: Mutex<BTreeMap<String, TraceScopeSelection>>,
    next_trace_scope_selection: AtomicU64,
    trace_session: Mutex<Option<TraceSession>>,
    syscall_config: Mutex<SyscallConfig>,
}

#[derive(Clone)]
struct HookTargetSelection {
    module: String,
    digest: String,
    targets: Vec<(u64, String)>,
    export_count: usize,
    matched_export_count: usize,
}

#[derive(Clone)]
struct TraceScopeSelection {
    module: String,
    module_base: u64,
    module_end: u64,
    ranges: Vec<(u64, u64)>,
    rva_ranges: Vec<(u64, u64)>,
    kinds: Vec<u32>,
    kind_names: Vec<String>,
    threads: Vec<u32>,
    digest: String,
}

#[derive(Clone)]
struct TraceSession {
    selection_id: String,
    selection_digest: String,
    module: String,
    module_base: u64,
    module_end: u64,
    ranges: Vec<(u64, u64)>,
    rva_ranges: Vec<(u64, u64)>,
    kind_names: Vec<String>,
    threads: Vec<u32>,
    path: String,
    active: bool,
    recorded: u64,
    dropped: u64,
    local_index: Option<Value>,
}

#[derive(Clone)]
struct SyscallConfig {
    enabled: bool,
    numbers: Vec<u32>,
    scope: String,
    module: Option<String>,
    module_base: u64,
    module_end: u64,
    rva_begin: u64,
    rva_end: u64,
    scope_start: u64,
    scope_end: u64,
}

impl Default for SyscallConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            numbers: Vec::new(),
            scope: "all".into(),
            module: None,
            module_base: 0,
            module_end: 0,
            rva_begin: 0,
            rva_end: 0,
            scope_start: 0,
            scope_end: 0,
        }
    }
}

impl SyscallConfig {
    fn to_json(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "mode": if self.numbers.is_empty() { "all" } else { "selected" },
            "numbers": self.numbers.iter().map(|value| format!("0x{value:x}")).collect::<Vec<_>>(),
            "scope": self.scope,
            "module": self.module,
            "module_base": format!("0x{:x}", self.module_base),
            "module_end": format!("0x{:x}", self.module_end),
            "rva_begin": format!("0x{:x}", self.rva_begin),
            "rva_end": format!("0x{:x}", self.rva_end),
            "scope_start": format!("0x{:x}", self.scope_start),
            "scope_end": format!("0x{:x}", self.scope_end),
        })
    }
}
impl<A: AgentApi> HubService<A> {
    pub fn new(agent: A) -> Self {
        let port = agent.port();
        Self {
            scripts: ScriptService::new(agent.clone()),
            agent,
            control: ControlState::default(),
            journal: Journal::default(),
            session: Session::new(port),
            hook_owners: Mutex::new(BTreeMap::new()),
            hook_module_owners: Mutex::new(BTreeMap::new()),
            hook_signatures: Mutex::new(BTreeMap::new()),
            hook_target_selections: Mutex::new(BTreeMap::new()),
            next_hook_target_selection: AtomicU64::new(1),
            trace_scope_selections: Mutex::new(BTreeMap::new()),
            next_trace_scope_selection: AtomicU64::new(1),
            trace_session: Mutex::new(None),
            syscall_config: Mutex::new(SyscallConfig::default()),
            breakpoint_owners: Mutex::new(BTreeMap::new()),
        }
    }
    pub fn set_target(&self, target: Option<String>) {
        self.session.set_target(target);
        if let Ok(mut owners) = self.breakpoint_owners.lock() {
            owners.clear();
        }
        if let Ok(mut owners) = self.hook_owners.lock() {
            owners.clear();
        }
        if let Ok(mut owners) = self.hook_module_owners.lock() {
            owners.clear();
        }
        if let Ok(mut signatures) = self.hook_signatures.lock() {
            signatures.clear();
        }
        if let Ok(mut selections) = self.hook_target_selections.lock() {
            selections.clear();
        }
        if let Ok(mut selections) = self.trace_scope_selections.lock() {
            selections.clear();
        }
        if let Ok(mut session) = self.trace_session.lock() {
            *session = None;
        }
        if let Ok(mut config) = self.syscall_config.lock() {
            *config = SyscallConfig::default();
        }
    }

    fn resolve_syscall_scope(
        &self,
        args: &Map<String, Value>,
    ) -> Result<(String, Option<String>, u64, u64, u64, u64, u64, u64), HubError> {
        let scope = args
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("all")
            .trim()
            .to_ascii_lowercase();
        if scope == "all" {
            return Ok((scope, None, 0, 0, 0, 0, 0, 0));
        }
        if !matches!(scope.as_str(), "module" | "rva") {
            return Err(HubError::Validation(
                "syscall scope must be all, module, or rva".into(),
            ));
        }
        let requested = bounded_module_name(req(args, "module")?)?.to_string();
        let modules = self.agent.modules().map_err(HubError::from)?;
        let rows = modules
            .get("modules")
            .and_then(Value::as_array)
            .ok_or_else(|| HubError::Agent("Agent returned an invalid module list".into()))?;
        let module = rows
            .iter()
            .find(|row| {
                row.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| module_key(name) == module_key(&requested))
            })
            .ok_or_else(|| HubError::Validation(format!("module is not loaded: {requested}")))?;
        let module_name = module
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&requested)
            .to_string();
        let module_base = module
            .get("base")
            .map(parse_value_u64)
            .transpose()?
            .ok_or_else(|| HubError::Agent("module base is missing".into()))?;
        let module_high = module
            .get("end")
            .map(parse_value_u64)
            .transpose()?
            .ok_or_else(|| HubError::Agent("module end is missing".into()))?;
        let module_end = module_high
            .checked_add(1)
            .ok_or_else(|| HubError::Validation("module end overflows the address space".into()))?;
        if module_end <= module_base {
            return Err(HubError::Agent(
                "Agent returned an invalid module range".into(),
            ));
        }
        if scope == "module" {
            return Ok((
                scope,
                Some(module_name),
                module_base,
                module_end,
                0,
                module_end - module_base,
                module_base,
                module_end,
            ));
        }

        let rva_begin = parse_u64(req(args, "rva_begin")?)?;
        let rva_end = parse_u64(req(args, "rva_end")?)?;
        if rva_end <= rva_begin {
            return Err(HubError::Validation(
                "syscall RVA end must be greater than RVA begin".into(),
            ));
        }
        let module_size = module_end - module_base;
        if rva_end > module_size {
            return Err(HubError::Validation(format!(
                "syscall RVA range exceeds module size 0x{module_size:x}"
            )));
        }
        let scope_start = module_base
            .checked_add(rva_begin)
            .ok_or_else(|| HubError::Validation("syscall RVA begin overflows".into()))?;
        let scope_end = module_base
            .checked_add(rva_end)
            .ok_or_else(|| HubError::Validation("syscall RVA end overflows".into()))?;
        Ok((
            scope,
            Some(module_name),
            module_base,
            module_end,
            rva_begin,
            rva_end,
            scope_start,
            scope_end,
        ))
    }

    pub fn call(
        &self,
        caller: Caller,
        name: &str,
        args: &Map<String, Value>,
    ) -> Result<Value, HubError> {
        if !TOOL_NAMES.contains(&name) && name != "events_newest" {
            return Err(HubError::Validation(format!("unknown tool: {name}")));
        }
        if name == "events_newest" {
            if caller.actor == ChannelActor::Ai {
                return Err(HubError::Permission(
                    "event snapshots are adapter-internal".into(),
                ));
            }
            let limit = args
                .get("limit")
                .and_then(Value::as_str)
                .unwrap_or("24")
                .parse::<u64>()
                .map_err(|_| HubError::Validation("event limit must be decimal".into()))?;
            if limit == 0 || limit > 24 {
                return Err(HubError::Validation("event limit must be 1..24".into()));
            }
            return self.agent.events_newest(limit).map_err(HubError::from);
        }
        if name == "activity_list" {
            return self.activity_list(args);
        }
        if name == "activity_get" {
            return self.activity_get(args);
        }
        // The Hub's internal/UI poller is a system reader. It must not create
        // 250ms/100Hz activity noise, while AI and explicit human reads remain
        // auditable. System writes still flow into the normal denial path.
        if caller.actor == ChannelActor::System && !is_write(name) {
            return self.dispatch_guarded(caller, name, args);
        }
        let (purpose, parent) = self.audit_args(args)?;
        if name == "control_handoff_to_ai" {
            let activity = self
                .journal
                .begin(actor(caller), name, purpose.clone(), parent.clone());
            let operation_id = activity.id().to_string();
            let mode = match parse_mode(args.get("mode")) {
                Ok(mode) => mode,
                Err(error) => {
                    activity.finish("error", json!({"kind":"validation"}));
                    return Err(HubError::Operation {
                        operation_id,
                        source: Box::new(error),
                    });
                }
            };
            return match self.control.handoff(caller, mode) {
                Ok(status) => {
                    let value = serde_json::to_value(status)
                        .map_err(|e| HubError::Internal(e.to_string()))?;
                    activity.finish("ok", json!({"kind":"control"}));
                    let mut value = value;
                    if let Value::Object(ref mut map) = value {
                        map.insert("operation_id".into(), Value::String(operation_id));
                    }
                    Ok(value)
                }
                Err(error) => {
                    activity.finish("denied", json!({"kind":"permission"}));
                    Err(HubError::Operation {
                        operation_id,
                        source: Box::new(HubError::Permission(error)),
                    })
                }
            };
        }
        if name == "control_takeover_manual" {
            let activity = self
                .journal
                .begin(actor(caller), name, purpose.clone(), parent.clone());
            let operation_id = activity.id().to_string();
            let guard = match self.control.begin_takeover(caller) {
                Ok(guard) => guard,
                Err(error) => {
                    activity.finish("denied", json!({"kind":"permission"}));
                    return Err(HubError::Operation {
                        operation_id,
                        source: Box::new(HubError::Permission(error)),
                    });
                }
            };
            let paused = self.agent.pause().map_err(HubError::from);
            drop(guard);
            let (outcome, target_pause) = match paused {
                Ok(value) => ("ok", json!({"attempted":true,"paused":value})),
                Err(error) => (
                    "partial",
                    json!({"attempted":true,"paused":false,"error":error.to_string()}),
                ),
            };
            let value = json!({"status":self.control.status(),"target_pause":target_pause});
            activity.finish(
                outcome,
                json!({"kind":"control","mode":"manual","target_pause":value["target_pause"]}),
            );
            let mut value = value;
            if let Value::Object(ref mut map) = value {
                map.insert("operation_id".into(), Value::String(operation_id));
            }
            return Ok(value);
        }
        if name == "control_pause_automation" {
            let activity = self
                .journal
                .begin(actor(caller), name, purpose.clone(), parent.clone());
            let operation_id = activity.id().to_string();
            return match self.control.pause_automation(caller) {
                Ok(status) => {
                    let value = serde_json::to_value(status)
                        .map_err(|e| HubError::Internal(e.to_string()))?;
                    activity.finish("ok", json!({"kind":"control"}));
                    let mut value = value;
                    if let Value::Object(ref mut map) = value {
                        map.insert("operation_id".into(), Value::String(operation_id));
                    }
                    Ok(value)
                }
                Err(error) => {
                    activity.finish("denied", json!({"kind":"permission"}));
                    Err(HubError::Operation {
                        operation_id,
                        source: Box::new(HubError::Permission(error)),
                    })
                }
            };
        }
        let activity = self.journal.begin(actor(caller), name, purpose, parent);
        let operation_id = activity.id().to_string();
        let result = self.dispatch_guarded(caller, name, args);
        match result {
            Ok(mut v) => {
                if let Value::Object(ref mut map) = v {
                    map.insert("operation_id".into(), Value::String(operation_id.clone()));
                }
                activity.finish("ok", resource_refs(name, &v));
                Ok(v)
            }
            Err(e) => {
                activity.finish("error", json!({"kind":error_kind(&e)}));
                Err(HubError::Operation {
                    operation_id,
                    source: Box::new(e),
                })
            }
        }
    }
    fn dispatch_guarded(
        &self,
        caller: Caller,
        name: &str,
        args: &Map<String, Value>,
    ) -> Result<Value, HubError> {
        let write = is_write(name);
        let _g = if write {
            let guard = self.control.write_guard().map_err(HubError::Permission)?;
            if caller.actor == ChannelActor::Ai {
                self.control
                    .ensure_ai_write()
                    .map_err(HubError::Permission)?
            } else if caller.actor == ChannelActor::Human {
                self.control
                    .ensure_human_write()
                    .map_err(HubError::Permission)?
            } else {
                return Err(HubError::Permission(
                    "system cannot perform target writes".into(),
                ));
            }
            Some(guard)
        } else {
            None
        };
        match name {
            "control_status" => serde_json::to_value(self.control.status())
                .map_err(|e| HubError::Internal(e.to_string())),
            "session_status" => match self.agent.status() {
                Ok(status) => {
                    self.session.set_connected(true);
                    Ok(json!({"session":self.session.status(),"agent":status}))
                }
                Err(error) => {
                    self.session.set_connected(false);
                    Err(HubError::from(error))
                }
            },
            "session_set_agent_port" => {
                if caller.actor != ChannelActor::Human || !caller.trusted {
                    return Err(HubError::Permission(
                        "agent port change requires trusted human".into(),
                    ));
                }
                let p = parse_u16(req(args, "agent_port")?)?;
                self.agent.set_port(p);
                self.session.set_port(p);
                self.session.set_connected(false);
                Ok(json!({"agent_port":p.to_string(),"connected":false}))
            }
            "target_pause" => Ok(json!({"paused":self.agent.pause().map_err(HubError::from)?})),
            "target_resume" => Ok(json!({"running":self.agent.resume().map_err(HubError::from)?})),
            "target_step_into" | "target_step_over" => self
                .agent
                .step(parse_u32(req(args, "thread_id")?)?, name.ends_with("over"))
                .map_err(HubError::from),
            "breakpoint_set" => {
                let value = self
                    .agent
                    .breakpoint_set(parse_u64(req(args, "address")?)?)
                    .map_err(HubError::from)?;
                if let Some(id) = value
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|value| value.parse::<u32>().ok())
                {
                    self.breakpoint_owners
                        .lock()
                        .map_err(|_| HubError::Internal("breakpoint ownership poisoned".into()))?
                        .entry(id)
                        .or_default()
                        .insert(actor(caller).to_string());
                }
                Ok(value)
            }
            "breakpoint_remove" => {
                let id = parse_u32(req(args, "id")?)?;
                let inventory = self.agent.breakpoint_inventory().map_err(HubError::from)?;
                let has_callbacks = inventory["breakpoints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|breakpoint| {
                        breakpoint["id"]
                            .as_str()
                            .and_then(|value| value.parse::<u32>().ok())
                            == Some(id)
                            && breakpoint["callbacks"]
                                .as_array()
                                .map(|callbacks| !callbacks.is_empty())
                                .unwrap_or(false)
                    });
                if has_callbacks {
                    return Err(HubError::Validation(
                        "breakpoint has callback bindings; remove or edit the owning script first"
                            .into(),
                    ));
                }
                let value = self.agent.breakpoint_remove(id).map_err(HubError::from)?;
                self.breakpoint_owners
                    .lock()
                    .map_err(|_| HubError::Internal("breakpoint ownership poisoned".into()))?
                    .remove(&id);
                Ok(value)
            }
            "breakpoint_list" => self.agent.breakpoint_list().map_err(HubError::from),
            "breakpoint_inventory" => self.breakpoint_inventory(),
            "registers_get" => self
                .agent
                .registers_get(parse_u32(req(args, "thread_id")?)?)
                .map_err(HubError::from),
            "register_set" => self
                .agent
                .register_set(
                    parse_u32(req(args, "thread_id")?)?,
                    self.agent.register_id(req(args, "register")?)?,
                    parse_u64(req(args, "value")?)?,
                )
                .map_err(HubError::from),
            "memory_read" => self
                .agent
                .memory_read(
                    parse_u64(req(args, "address")?)?,
                    parse_u64(req(args, "size")?)?,
                )
                .map_err(HubError::from),
            "memory_write" => self
                .agent
                .memory_write(
                    parse_u64(req(args, "address")?)?,
                    &parse_hex(req(args, "data_hex")?)?,
                )
                .map_err(HubError::from),
            "disassemble" => self
                .agent
                .disassemble(
                    parse_u64(req(args, "address")?)?,
                    parse_u64(req(args, "count")?)?,
                )
                .map_err(HubError::from),
            "modules_list" => self.agent.modules().map_err(HubError::from),
            "module_exports" => {
                let module = bounded_module_name(req(args, "module")?)?;
                self.agent
                    .module_exports(&module_key(module))
                    .map_err(HubError::from)
            }
            "hook_targets_query" => self.hook_targets_query(args),
            "hook_monitor_apply" => self.hook_monitor_apply(caller, args),
            "hook_set" => {
                let address = parse_u64(req(args, "address")?)?;
                if address == 0 {
                    return Err(HubError::Validation("Hook address must be non-zero".into()));
                }
                let value = self.agent.hook_set(address).map_err(HubError::from)?;
                if value.get("hooked").and_then(Value::as_bool) == Some(false) {
                    return Err(HubError::Agent(
                        "Hook capacity is full (32768 points)".into(),
                    ));
                }
                self.hook_owners
                    .lock()
                    .map_err(|_| HubError::Internal("Hook ownership poisoned".into()))?
                    .entry(address)
                    .or_default()
                    .insert(actor(caller).to_string());
                Ok(value)
            }
            "hook_function_set" => {
                let address = parse_u64(req(args, "address")?)?;
                if address == 0 {
                    return Err(HubError::Validation("Hook address must be non-zero".into()));
                }
                let signature = parse_hook_signature(args)?;
                let layout = signature.capture_layout();
                self.agent
                    .hook_signature_set(
                        address,
                        layout.calling_convention,
                        layout.return_kind,
                        layout.parameter_count,
                        layout.float_parameter_mask,
                    )
                    .map_err(HubError::from)?;
                let value = self.agent.hook_function_set(address).map_err(|error| {
                    let _ = self.agent.hook_signature_remove(address);
                    HubError::from(error)
                })?;
                if value.get("hooked").and_then(Value::as_bool) == Some(false) {
                    let _ = self.agent.hook_signature_remove(address);
                    return Err(HubError::Agent(
                        "Hook capacity is full (32768 points)".into(),
                    ));
                }
                self.hook_signatures
                    .lock()
                    .map_err(|_| HubError::Internal("Hook signatures poisoned".into()))?
                    .insert(address, signature.clone());
                self.hook_owners
                    .lock()
                    .map_err(|_| HubError::Internal("Hook ownership poisoned".into()))?
                    .entry(address)
                    .or_default()
                    .insert(actor(caller).to_string());
                let mut value = value;
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "signature".into(),
                        signature.to_json(self.target_pointer_width()),
                    );
                    object.insert("signature_status".into(), Value::String("resolved".into()));
                }
                Ok(value)
            }
            "hook_signature_set" => {
                let address = parse_u64(req(args, "address")?)?;
                if address == 0 {
                    return Err(HubError::Validation("Hook address must be non-zero".into()));
                }
                let signature = parse_hook_signature(args)?;
                let layout = signature.capture_layout();
                self.agent
                    .hook_signature_set(
                        address,
                        layout.calling_convention,
                        layout.return_kind,
                        layout.parameter_count,
                        layout.float_parameter_mask,
                    )
                    .map_err(HubError::from)?;
                self.hook_signatures
                    .lock()
                    .map_err(|_| HubError::Internal("Hook signatures poisoned".into()))?
                    .insert(address, signature.clone());
                Ok(json!({
                    "address": format!("0x{address:x}"),
                    "signature_status": "resolved",
                    "signature": signature.to_json(self.target_pointer_width()),
                }))
            }
            "hook_signature_remove" => {
                let address = parse_u64(req(args, "address")?)?;
                self.agent
                    .hook_signature_remove(address)
                    .map_err(HubError::from)?;
                let removed = self
                    .hook_signatures
                    .lock()
                    .map_err(|_| HubError::Internal("Hook signatures poisoned".into()))?
                    .remove(&address)
                    .is_some();
                Ok(json!({
                    "address": format!("0x{address:x}"),
                    "signature_removed": removed,
                }))
            }
            "hook_remove" => {
                let address = parse_u64(req(args, "address")?)?;
                if self
                    .agent
                    .hook_callback_count(Some(address))
                    .map_err(HubError::from)?
                    != 0
                {
                    return Err(HubError::Validation(
                        "Hook has synchronous callback bindings; unload or edit the owning script first".into(),
                    ));
                }
                let value = self.agent.hook_remove(address).map_err(HubError::from)?;
                let _ = self.agent.hook_signature_remove(address);
                self.hook_signatures
                    .lock()
                    .map_err(|_| HubError::Internal("Hook signatures poisoned".into()))?
                    .remove(&address);
                self.hook_owners
                    .lock()
                    .map_err(|_| HubError::Internal("Hook ownership poisoned".into()))?
                    .remove(&address);
                Ok(value)
            }
            "hook_clear" => {
                let callback_count = self
                    .agent
                    .hook_callback_count(None)
                    .map_err(HubError::from)?;
                if callback_count != 0 {
                    return Err(HubError::Validation(format!(
                        "cannot clear Hooks while {callback_count} synchronous callback bindings are active"
                    )));
                }
                let value = self.agent.hook_clear().map_err(HubError::from)?;
                self.hook_owners
                    .lock()
                    .map_err(|_| HubError::Internal("Hook ownership poisoned".into()))?
                    .clear();
                self.hook_module_owners
                    .lock()
                    .map_err(|_| HubError::Internal("Hook module ownership poisoned".into()))?
                    .clear();
                self.hook_signatures
                    .lock()
                    .map_err(|_| HubError::Internal("Hook signatures poisoned".into()))?
                    .clear();
                Ok(value)
            }
            "hook_list" => self.agent.hook_list().map_err(HubError::from),
            "hook_inventory" => {
                let offset = parse_page_value(args, "offset", 0, 32768)?;
                let limit = parse_page_value(args, "limit", 1000, 4096)?;
                if limit == 0 {
                    return Err(HubError::Validation(
                        "Hook inventory limit must be 1..4096".into(),
                    ));
                }
                let function_log = match args.get("kind").and_then(Value::as_str).unwrap_or("all") {
                    "all" => None,
                    "api" => Some(true),
                    "instruction" => Some(false),
                    _ => {
                        return Err(HubError::Validation(
                            "Hook inventory kind must be all, api, or instruction".into(),
                        ))
                    }
                };
                self.hook_inventory(offset as usize, limit as usize, function_log)
            }
            "hook_monitor" => {
                let limit = args.get("limit").and_then(Value::as_str).unwrap_or("1024");
                let limit = limit.parse::<u64>().map_err(|_| {
                    HubError::Validation("Hook monitor limit must be decimal".into())
                })?;
                if limit == 0 || limit > 4096 {
                    return Err(HubError::Validation(
                        "Hook monitor limit must be 1..4096".into(),
                    ));
                }
                let before = args
                    .get("before")
                    .and_then(Value::as_str)
                    .unwrap_or("0")
                    .parse::<u64>()
                    .map_err(|_| {
                        HubError::Validation("Hook monitor before must be decimal".into())
                    })?;
                self.hook_monitor(limit, before)
            }
            "hook_events_query" | "hook_events_export" => {
                let before = crate::hook_query::requested_before(args)?;
                let source = self.hook_monitor(4096, before)?;
                if name == "hook_events_query" {
                    crate::hook_query::query(source, args)
                } else {
                    crate::hook_query::export(source, args)
                }
            }
            "event_index_query" => self.event_index_query(args),
            "event_index_export" => self.event_index_export(args),
            "trace_scope_query" => self.trace_scope_query(args),
            "trace_record_start" => self.trace_record_start(args),
            "trace_record_status" => self.trace_record_status(),
            "trace_record_stop" => self.trace_record_stop(),
            "trace_index_query" => self.trace_index_query(args),
            "trace_index_export" => self.trace_index_export(args),
            "hook_module" => {
                if caller.actor == ChannelActor::Ai {
                    return Err(HubError::Validation(
                        "AI module Hooks require hook_targets_query followed by hook_monitor_apply"
                            .into(),
                    ));
                }
                let module = bounded_module_name(req(args, "module")?)?;
                let module = module_key(module);
                let value = self.agent.hook_module(&module).map_err(HubError::from)?;
                self.hook_module_owners
                    .lock()
                    .map_err(|_| HubError::Internal("Hook module ownership poisoned".into()))?
                    .entry(module)
                    .or_default()
                    .insert(actor(caller).to_string());
                Ok(value)
            }
            "hook_range_preview" | "hook_range_set" => {
                let (start, end, kind_mask) = parse_hook_range(args)?;
                let apply = name == "hook_range_set";
                let value = self
                    .agent
                    .hook_range(start, end, kind_mask, apply)
                    .map_err(HubError::from)?;
                if !value
                    .get("complete")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    return Err(HubError::Validation(
                        "range scan stopped at undecodable or unreadable bytes; no Hooks were added"
                            .into(),
                    ));
                }
                if apply {
                    let addresses = value
                        .get("addresses")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(parse_value_u64)
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut owners = self
                        .hook_owners
                        .lock()
                        .map_err(|_| HubError::Internal("Hook ownership poisoned".into()))?;
                    for address in addresses {
                        owners
                            .entry(address)
                            .or_default()
                            .insert(actor(caller).to_string());
                    }
                }
                Ok(value)
            }
            "syscall_config_get" => self
                .syscall_config
                .lock()
                .map_err(|_| HubError::Internal("syscall configuration poisoned".into()))
                .map(|config| config.to_json()),
            "syscall_config_set" => {
                let enabled = args
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| HubError::Validation("enabled must be boolean".into()))?;
                let values = args
                    .get("numbers")
                    .and_then(Value::as_array)
                    .ok_or_else(|| HubError::Validation("numbers must be an array".into()))?;
                if values.len() > 4096 {
                    return Err(HubError::Validation(
                        "syscall number selection exceeds 4096 entries".into(),
                    ));
                }
                let mut unique = BTreeSet::new();
                for value in values {
                    let number = parse_value_u64(value)?;
                    if number > 0xfff {
                        return Err(HubError::Validation(
                            "syscall number must be in the native 0x000..0xfff range".into(),
                        ));
                    }
                    unique.insert(number as u32);
                }
                let numbers = unique.into_iter().collect::<Vec<_>>();
                let (
                    scope,
                    module,
                    module_base,
                    module_end,
                    rva_begin,
                    rva_end,
                    scope_start,
                    scope_end,
                ) = self.resolve_syscall_scope(args)?;
                let value = self
                    .agent
                    .syscall_config_set(enabled, &numbers, scope_start, scope_end)
                    .map_err(HubError::from)?;
                let next = SyscallConfig {
                    enabled,
                    numbers,
                    scope,
                    module,
                    module_base,
                    module_end,
                    rva_begin,
                    rva_end,
                    scope_start,
                    scope_end,
                };
                *self
                    .syscall_config
                    .lock()
                    .map_err(|_| HubError::Internal("syscall configuration poisoned".into()))? =
                    next.clone();
                let mut value = value;
                if let (Some(target), Value::Object(result)) =
                    (next.to_json().as_object().cloned(), &mut value)
                {
                    result.extend(target);
                }
                Ok(value)
            }
            "syscall_monitor" => {
                let limit = args
                    .get("limit")
                    .and_then(Value::as_str)
                    .unwrap_or("256")
                    .parse::<u64>()
                    .map_err(|_| {
                        HubError::Validation("syscall monitor limit must be decimal".into())
                    })?;
                if limit == 0 || limit > 512 {
                    return Err(HubError::Validation(
                        "syscall monitor limit must be 1..512".into(),
                    ));
                }
                self.agent.syscall_monitor(limit).map_err(HubError::from)
            }
            "memory_map" => self.agent.memory_map().map_err(HubError::from),
            "exception_monitor" => {
                let limit = args
                    .get("limit")
                    .and_then(Value::as_str)
                    .unwrap_or("256")
                    .parse::<u64>()
                    .map_err(|_| HubError::Validation("exception limit must be decimal".into()))?;
                if limit == 0 || limit > 1024 {
                    return Err(HubError::Validation(
                        "exception limit must be 1..1024".into(),
                    ));
                }
                self.agent.exception_monitor(limit).map_err(HubError::from)
            }
            "exception_policy_get" => self.agent.exception_policy_get().map_err(HubError::from),
            "exception_policy_set" => {
                let enabled = args
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| HubError::Validation("enabled must be boolean".into()))?;
                let code = parse_u32(req(args, "code")?)?;
                self.agent
                    .exception_policy_set(enabled, code)
                    .map_err(HubError::from)
            }
            "exception_inventory" => self.exception_inventory(),
            "threads_list" => self.agent.threads().map_err(HubError::from),
            "address_resolve" => {
                if let Some(n) = args.get("name").and_then(Value::as_str) {
                    self.agent.resolve_name(n).map_err(HubError::from)
                } else {
                    let a = args
                        .get("addresses")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            HubError::Validation(
                                "address_resolve requires addresses or name".into(),
                            )
                        })?
                        .iter()
                        .map(parse_value_u64)
                        .collect::<Result<Vec<_>, _>>()?;
                    self.agent.resolve(&a).map_err(HubError::from)
                }
            }
            "script_inject" | "script_replace" => {
                if caller.actor == ChannelActor::Ai {
                    validate_ai_breakpoint_descriptions(req(args, "source")?)?;
                }
                let kind = args.get("kind").and_then(Value::as_str);
                let r: ScriptRequest = serde_json::from_value(Value::Object(args.clone()))
                    .map_err(|e| HubError::Validation(e.to_string()))?;
                let v = if name == "script_inject" {
                    self.scripts.inject_kind_as(r, kind, actor(caller))
                } else {
                    self.scripts.replace_kind_as(r, kind, actor(caller))
                }
                .map_err(map_script_error)?;
                serde_json::to_value(v).map_err(|e| HubError::Internal(e.to_string()))
            }
            "script_start" => serde_json::to_value(
                self.scripts
                    .start(req(args, "name")?)
                    .map_err(map_script_error)?,
            )
            .map_err(|e| HubError::Internal(e.to_string())),
            "script_stop" => serde_json::to_value(
                self.scripts
                    .stop(req(args, "name")?)
                    .map_err(map_script_error)?,
            )
            .map_err(|e| HubError::Internal(e.to_string())),
            "script_remove" => self
                .scripts
                .remove(req(args, "name")?)
                .map_err(map_script_error),
            "script_list" => serde_json::to_value(self.scripts.list().map_err(map_script_error)?)
                .map_err(|e| HubError::Internal(e.to_string())),
            "script_get" => self
                .scripts
                .get(req(args, "name")?)
                .map_err(map_script_error),
            "script_status" => self
                .scripts
                .status(args.get("name").and_then(Value::as_str))
                .map_err(map_script_error),
            "script_output" => {
                let r: OutputRequest = serde_json::from_value(Value::Object(args.clone()))
                    .map_err(|e| HubError::Validation(e.to_string()))?;
                serde_json::to_value(self.scripts.output(r).map_err(map_script_error)?)
                    .map_err(|e| HubError::Internal(e.to_string()))
            }
            _ => Err(HubError::Validation(format!("unknown tool: {name}"))),
        }
    }
    fn activity_list(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let n = args
            .get("limit")
            .and_then(Value::as_str)
            .unwrap_or("50")
            .parse::<usize>()
            .map_err(|_| HubError::Validation("limit must be decimal".into()))?
            .min(100);
        Ok(json!({"activities":self.journal.list(n)}))
    }
    fn breakpoint_inventory(&self) -> Result<Value, HubError> {
        let mut value = self.agent.breakpoint_inventory().map_err(HubError::from)?;
        let owners = self
            .breakpoint_owners
            .lock()
            .map_err(|_| HubError::Internal("breakpoint ownership poisoned".into()))?
            .clone();
        let Some(breakpoints) = value.get_mut("breakpoints").and_then(Value::as_array_mut) else {
            return Ok(value);
        };
        for breakpoint in breakpoints {
            let id = breakpoint
                .get("id")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<u32>().ok());
            let plain_owners = id
                .and_then(|value| owners.get(&value))
                .map(|values| values.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let callbacks = breakpoint
                .get_mut("callbacks")
                .and_then(Value::as_array_mut);
            let callback_count = callbacks.as_ref().map(|values| values.len()).unwrap_or(0);
            if let Some(callbacks) = callbacks {
                for callback in callbacks {
                    let plugin = callback
                        .get("plugin")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if let Some((created_by, modified_by)) = self.scripts.provenance(&plugin) {
                        if let Some(object) = callback.as_object_mut() {
                            object.insert("owner".into(), Value::String(created_by.clone()));
                            object.insert("created_by".into(), Value::String(created_by));
                            object.insert("modified_by".into(), Value::String(modified_by));
                            object.insert("source_available".into(), Value::Bool(true));
                        }
                    } else if let Some(object) = callback.as_object_mut() {
                        object.insert("source_available".into(), Value::Bool(false));
                    }
                }
            }
            let kind = match (!plain_owners.is_empty(), callback_count > 0) {
                (true, true) => "mixed",
                (false, true) => "callback",
                (true, false) => "traditional",
                (false, false) => "external",
            };
            if let Some(object) = breakpoint.as_object_mut() {
                object.insert("plain_owners".into(), json!(plain_owners));
                object.insert("kind".into(), Value::String(kind.into()));
                object.insert(
                    "callback_count".into(),
                    Value::String(callback_count.to_string()),
                );
            }
        }
        Ok(value)
    }
    fn exception_inventory(&self) -> Result<Value, HubError> {
        let mut value = self.agent.exception_inventory().map_err(HubError::from)?;
        if let Some(interceptors) = value.get_mut("interceptors").and_then(Value::as_array_mut) {
            for interceptor in interceptors {
                let plugin = interceptor
                    .get("plugin")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if let Some((created_by, modified_by)) = self.scripts.provenance(&plugin) {
                    if let Some(object) = interceptor.as_object_mut() {
                        object.insert("owner".into(), Value::String(created_by.clone()));
                        object.insert("created_by".into(), Value::String(created_by));
                        object.insert("modified_by".into(), Value::String(modified_by));
                        object.insert("source_available".into(), Value::Bool(true));
                    }
                } else if let Some(object) = interceptor.as_object_mut() {
                    object.insert("source_available".into(), Value::Bool(false));
                }
            }
        }
        Ok(value)
    }
    fn target_pointer_width(&self) -> u32 {
        self.agent
            .status()
            .ok()
            .and_then(|status| {
                status
                    .get("pointer_width")
                    .and_then(|value| parse_value_u64(value).ok())
            })
            .unwrap_or(8) as u32
    }

    fn hook_inventory(
        &self,
        offset: usize,
        limit: usize,
        function_log: Option<bool>,
    ) -> Result<Value, HubError> {
        let mut value = self
            .agent
            .hook_inventory_page_filtered(offset, limit, function_log)
            .map_err(HubError::from)?;
        let pointer_width = self.target_pointer_width();
        let signatures = self
            .hook_signatures
            .lock()
            .map_err(|_| HubError::Internal("Hook signatures poisoned".into()))?
            .clone();
        let address_owners = self
            .hook_owners
            .lock()
            .map_err(|_| HubError::Internal("Hook ownership poisoned".into()))?
            .clone();
        let module_owners = self
            .hook_module_owners
            .lock()
            .map_err(|_| HubError::Internal("Hook module ownership poisoned".into()))?
            .clone();
        let Some(hooks) = value.get_mut("hooks").and_then(Value::as_array_mut) else {
            return Ok(value);
        };
        for hook in hooks {
            let address = hook
                .get("address")
                .and_then(|value| parse_value_u64(value).ok());
            let module = hook
                .get("module")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut owners = BTreeSet::new();
            if let Some(values) = address.and_then(|value| address_owners.get(&value)) {
                owners.extend(values.iter().cloned());
            }
            if let Some(values) = module_owners.get(&module_key(module)) {
                owners.extend(values.iter().cloned());
            }
            let callbacks = hook.get_mut("callbacks").and_then(Value::as_array_mut);
            let callback_count = callbacks.as_ref().map(|values| values.len()).unwrap_or(0);
            if let Some(callbacks) = callbacks {
                for callback in callbacks {
                    let plugin = callback
                        .get("plugin")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if let Some((created_by, modified_by)) = self.scripts.provenance(&plugin) {
                        if let Some(object) = callback.as_object_mut() {
                            object.insert("owner".into(), Value::String(created_by.clone()));
                            object.insert("created_by".into(), Value::String(created_by));
                            object.insert("modified_by".into(), Value::String(modified_by));
                            object.insert("source_available".into(), Value::Bool(true));
                        }
                    } else if let Some(object) = callback.as_object_mut() {
                        object.insert("source_available".into(), Value::Bool(false));
                    }
                }
            }
            let owner_list = owners.into_iter().collect::<Vec<_>>();
            let kind = match (!owner_list.is_empty(), callback_count > 0) {
                (true, true) => "mixed",
                (false, true) => "callback",
                (true, false) => "instruction",
                (false, false) => "external",
            };
            if let Some(object) = hook.as_object_mut() {
                object.insert("plain_owners".into(), json!(owner_list));
                object.insert("kind".into(), Value::String(kind.into()));
                object.insert(
                    "callback_count".into(),
                    Value::String(callback_count.to_string()),
                );
                let function_log = object
                    .get("function_log")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(signature) = address.and_then(|address| signatures.get(&address)) {
                    object.insert("signature".into(), signature.to_json(pointer_width));
                    object.insert("signature_status".into(), Value::String("resolved".into()));
                } else {
                    object.insert(
                        "signature_status".into(),
                        Value::String(
                            if function_log {
                                "missing"
                            } else {
                                "not_applicable"
                            }
                            .into(),
                        ),
                    );
                }
            }
        }
        Ok(value)
    }

    fn hook_monitor(&self, limit: u64, before: u64) -> Result<Value, HubError> {
        let mut value = self
            .agent
            .hook_monitor(limit, before)
            .map_err(HubError::from)?;
        let pointer_width = value
            .get("pointer_width")
            .and_then(|value| parse_value_u64(value).ok())
            .unwrap_or(8) as u32;
        let signatures = self
            .hook_signatures
            .lock()
            .map_err(|_| HubError::Internal("Hook signatures poisoned".into()))?
            .clone();
        let Some(events) = value.get_mut("events").and_then(Value::as_array_mut) else {
            return Ok(value);
        };
        for event in events {
            let Some(address) = event
                .get("address")
                .and_then(|value| parse_value_u64(value).ok())
            else {
                continue;
            };
            let signature_capture = event
                .get("signature_capture")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let function_hook = event
                .get("hook_type")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "api");
            let kind = event
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("entry")
                .to_string();
            let raw_arguments = event
                .get("arguments")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let raw_return = event.get("return_value").cloned();
            let Some(object) = event.as_object_mut() else {
                continue;
            };
            if let Some(signature) = function_hook.then(|| signatures.get(&address)).flatten() {
                if object.get("display").and_then(Value::as_str).is_none() {
                    object.insert("display".into(), Value::String(signature.function.clone()));
                }
                object.insert("symbol".into(), Value::String(signature.function.clone()));
                object.insert("signature".into(), signature.to_json(pointer_width));
                object.insert("signature_status".into(), Value::String("resolved".into()));
                if signature_capture {
                    object.insert("capture_status".into(), Value::String("signature".into()));
                    if kind == "return" {
                        object.insert(
                            "typed_return".into(),
                            signature.typed_return(raw_return.as_ref(), pointer_width),
                        );
                    } else {
                        object.insert(
                            "typed_arguments".into(),
                            Value::Array(signature.typed_arguments(&raw_arguments, pointer_width)),
                        );
                    }
                } else {
                    object.insert(
                        "capture_status".into(),
                        Value::String("pre_signature_raw_abi".into()),
                    );
                }
            } else {
                object.insert(
                    "signature_status".into(),
                    Value::String(
                        if function_hook {
                            "missing"
                        } else {
                            "not_applicable"
                        }
                        .into(),
                    ),
                );
                object.insert(
                    "capture_status".into(),
                    Value::String(
                        if function_hook {
                            "raw_abi"
                        } else {
                            "instruction"
                        }
                        .into(),
                    ),
                );
            }
        }
        Ok(value)
    }

    fn event_index_query(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let index = req(args, "index")?.trim().to_ascii_lowercase();
        if !matches!(index.as_str(), "api" | "syscall" | "address" | "thread") {
            return Err(HubError::Validation(
                "event index must be api, syscall, address, or thread".into(),
            ));
        }
        let key = req(args, "key")?.trim();
        if key.is_empty() || key.len() > 512 {
            return Err(HubError::Validation(
                "event index key must contain 1..512 bytes".into(),
            ));
        }
        let limit = req(args, "limit")?
            .parse::<usize>()
            .map_err(|_| HubError::Validation("event index limit must be decimal".into()))?;
        if limit == 0 || limit > 256 {
            return Err(HubError::Validation(
                "event index limit must be explicitly set to 1..256".into(),
            ));
        }
        let initial_before = args
            .get("before")
            .and_then(Value::as_str)
            .unwrap_or("0")
            .parse::<u64>()
            .map_err(|_| HubError::Validation("event index before must be decimal".into()))?;
        let payload = args
            .get("payload")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let fields = parse_event_index_fields(args)?;
        let phases = parse_event_index_phases(args)?;
        let module = args
            .get("module")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if index == "api" && module.is_none() {
            return Err(HubError::Validation(
                "api index queries require an explicit module".into(),
            ));
        }
        let numeric_key = match index.as_str() {
            "syscall" | "address" | "thread" => Some(parse_u64(key)?),
            _ => None,
        };
        if index == "syscall" && numeric_key.is_some_and(|value| value > 0xfff) {
            return Err(HubError::Validation(
                "syscall index key must be in the native 0x000..0xfff range".into(),
            ));
        }
        if index == "thread" && numeric_key.is_some_and(|value| value > u32::MAX as u64) {
            return Err(HubError::Validation("thread index key exceeds u32".into()));
        }
        let source = if index == "syscall" {
            "syscall"
        } else if index == "thread" {
            match args.get("source").and_then(Value::as_str).unwrap_or("hook") {
                "hook" => "hook",
                "syscall" => "syscall",
                _ => {
                    return Err(HubError::Validation(
                        "thread index source must be hook or syscall".into(),
                    ))
                }
            }
        } else {
            "hook"
        };

        let mut before = initial_before;
        let mut scanned = 0usize;
        let mut matches = Vec::with_capacity(limit);
        let mut last_match_sequence = None;
        let mut lane = Value::Null;
        let mut exhausted = false;
        while matches.len() < limit && scanned < 32768 {
            let page = if source == "syscall" {
                self.agent
                    .syscall_monitor_window(4096, before)
                    .map_err(HubError::from)?
            } else {
                self.hook_monitor(4096, before)?
            };
            let mut events = page
                .get("events")
                .and_then(Value::as_array)
                .cloned()
                .ok_or_else(|| {
                    HubError::Agent("Agent returned an invalid indexed event page".into())
                })?;
            if lane.is_null() {
                lane = event_index_lane_metadata(&page, source);
            }
            if events.is_empty() {
                exhausted = true;
                break;
            }
            events.sort_by_key(|event| event_index_sequence(event).unwrap_or(0));
            events.reverse();
            let oldest = events.last().and_then(event_index_sequence).unwrap_or(0);
            scanned += events.len();
            for mut event in events {
                if !event_index_matches(&event, &index, key, numeric_key, module, source, &phases) {
                    continue;
                }
                last_match_sequence = event_index_sequence(&event);
                if !payload {
                    strip_event_index_payload(&mut event);
                }
                if !fields.is_empty() {
                    event = project_event_index_fields(&event, &fields);
                }
                matches.push(event);
                if matches.len() == limit {
                    break;
                }
            }
            if matches.len() == limit {
                break;
            }
            if oldest <= 1 || scanned >= 32768 {
                exhausted = true;
                break;
            }
            before = oldest;
        }
        let next_before = if exhausted {
            None
        } else {
            last_match_sequence.or((before != 0).then_some(before))
        };
        Ok(json!({
            "index": index,
            "key": key,
            "module": module,
            "source": source,
            "payload": payload,
            "fields": fields,
            "requested_limit": limit.to_string(),
            "scanned": scanned.to_string(),
            "returned": matches.len().to_string(),
            "next_before": next_before.map(|value| value.to_string()),
            "exhausted": exhausted,
            "lane": lane,
            "events": matches,
        }))
    }

    fn event_index_export(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let result = self.event_index_query(args)?;
        let events = result
            .get("events")
            .and_then(Value::as_array)
            .ok_or_else(|| HubError::Internal("indexed event result has no events".into()))?;
        let format = args
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("jsonl");
        let (data, mime_type, extension) = match format {
            "json" => (
                serde_json::to_string_pretty(events)
                    .map_err(|error| HubError::Internal(error.to_string()))?,
                "application/json",
                "json",
            ),
            "jsonl" => {
                let mut data = String::new();
                for event in events {
                    data.push_str(
                        &serde_json::to_string(event)
                            .map_err(|error| HubError::Internal(error.to_string()))?,
                    );
                    data.push('\n');
                }
                (data, "application/x-ndjson", "jsonl")
            }
            "csv" => (
                event_index_csv(events, args)?,
                "text/csv; charset=utf-8",
                "csv",
            ),
            _ => {
                return Err(HubError::Validation(
                    "event index export format must be json, jsonl, or csv".into(),
                ))
            }
        };
        let requested_filename = args
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or("event-index")
            .trim();
        let filename = safe_event_index_filename(requested_filename, extension)?;
        match args
            .get("delivery")
            .and_then(Value::as_str)
            .unwrap_or("file")
        {
            "inline" => {
                if data.len() > 2 * 1024 * 1024 {
                    return Err(HubError::Validation(
                        "inline indexed export exceeds 2 MiB; use delivery=file".into(),
                    ));
                }
                Ok(json!({
                    "delivery":"inline",
                    "format":format,
                    "mime_type":mime_type,
                    "filename":filename,
                    "rows":events.len().to_string(),
                    "bytes":data.len().to_string(),
                    "data":data,
                }))
            }
            "file" => {
                let directory = std::env::temp_dir().join("pinbridge-hook-exports");
                std::fs::create_dir_all(&directory)
                    .map_err(|error| HubError::Internal(error.to_string()))?;
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|error| HubError::Internal(error.to_string()))?
                    .as_millis();
                let path = directory.join(format!("{stamp}-{}-{filename}", std::process::id()));
                std::fs::write(&path, data.as_bytes())
                    .map_err(|error| HubError::Internal(error.to_string()))?;
                Ok(json!({
                    "delivery":"file",
                    "format":format,
                    "mime_type":mime_type,
                    "filename":filename,
                    "path":path.to_string_lossy(),
                    "rows":events.len().to_string(),
                    "bytes":data.len().to_string(),
                }))
            }
            _ => Err(HubError::Validation(
                "event index export delivery must be file or inline".into(),
            )),
        }
    }

    fn trace_scope_query(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let requested_module = bounded_module_name(req(args, "module")?)?;
        let modules = self.agent.modules().map_err(HubError::from)?;
        let rows = modules
            .get("modules")
            .and_then(Value::as_array)
            .ok_or_else(|| HubError::Agent("Agent returned an invalid module list".into()))?;
        let module = rows
            .iter()
            .find(|row| {
                row.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| module_key(name) == module_key(requested_module))
            })
            .ok_or_else(|| {
                HubError::Validation(format!("module is not loaded: {requested_module}"))
            })?;
        let module_name = module
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(requested_module)
            .to_string();
        let module_base = module
            .get("base")
            .map(parse_value_u64)
            .transpose()?
            .ok_or_else(|| HubError::Agent("module base is missing".into()))?;
        let module_high = module
            .get("end")
            .map(parse_value_u64)
            .transpose()?
            .ok_or_else(|| HubError::Agent("module end is missing".into()))?;
        let module_end = module_high
            .checked_add(1)
            .ok_or_else(|| HubError::Validation("module end overflows the address space".into()))?;
        if module_end <= module_base {
            return Err(HubError::Agent(
                "Agent returned an invalid module range".into(),
            ));
        }
        let module_size = module_end - module_base;

        let kind_values = args
            .get("kinds")
            .and_then(Value::as_array)
            .ok_or_else(|| HubError::Validation("Trace kinds must be an array".into()))?;
        if kind_values.is_empty() || kind_values.len() > 8 {
            return Err(HubError::Validation(
                "Trace kinds must contain 1..8 entries".into(),
            ));
        }
        let mut kind_map = BTreeMap::new();
        for value in kind_values {
            let name = value
                .as_str()
                .ok_or_else(|| HubError::Validation("Trace kind must be a string".into()))?;
            let id = trace_kind_id(name)?;
            kind_map.entry(id).or_insert_with(|| name.to_string());
        }
        let kinds = kind_map.keys().copied().collect::<Vec<_>>();
        let kind_names = kind_map.into_values().collect::<Vec<_>>();

        let thread_values = args
            .get("threads")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if thread_values.len() > 64 {
            return Err(HubError::Validation(
                "Trace thread selection exceeds 64 entries".into(),
            ));
        }
        let mut thread_set = BTreeSet::new();
        for value in thread_values {
            let thread_id = parse_value_u64(value)?;
            let thread_id = u32::try_from(thread_id)
                .map_err(|_| HubError::Validation("Trace thread id is out of range".into()))?;
            thread_set.insert(thread_id);
        }
        let threads = thread_set.into_iter().collect::<Vec<_>>();
        if !threads.is_empty() {
            let live = self.agent.threads().map_err(HubError::from)?;
            let live_threads = live
                .get("threads")
                .and_then(Value::as_array)
                .ok_or_else(|| HubError::Agent("Agent returned an invalid thread list".into()))?
                .iter()
                .map(parse_value_u64)
                .collect::<Result<BTreeSet<_>, _>>()?;
            if let Some(missing) = threads
                .iter()
                .find(|thread| !live_threads.contains(&(**thread as u64)))
            {
                return Err(HubError::Validation(format!(
                    "Trace thread is not live: {missing}"
                )));
            }
        }

        let mut rva_ranges = Vec::new();
        if let Some(values) = args.get("ranges").and_then(Value::as_array) {
            if values.len() > 16 {
                return Err(HubError::Validation("Trace scope exceeds 16 ranges".into()));
            }
            for value in values {
                let range = value
                    .as_object()
                    .ok_or_else(|| HubError::Validation("Trace range must be an object".into()))?;
                let begin = parse_u64(req(range, "rva_begin")?)?;
                let end = parse_u64(req(range, "rva_end")?)?;
                if end <= begin {
                    return Err(HubError::Validation(
                        "Trace RVA end must be greater than RVA begin".into(),
                    ));
                }
                if end > module_size {
                    return Err(HubError::Validation(format!(
                        "Trace RVA range exceeds module size 0x{module_size:x}"
                    )));
                }
                rva_ranges.push((begin, end));
            }
        }
        if rva_ranges.is_empty() {
            rva_ranges.push((0, module_size));
        }
        rva_ranges.sort_unstable();
        let mut normalized = Vec::<(u64, u64)>::with_capacity(rva_ranges.len());
        for (begin, end) in rva_ranges {
            if let Some((_, previous_end)) = normalized.last_mut() {
                if begin <= *previous_end {
                    *previous_end = (*previous_end).max(end);
                    continue;
                }
            }
            normalized.push((begin, end));
        }
        let rva_ranges = normalized;
        let ranges = rva_ranges
            .iter()
            .map(|(begin, end)| {
                Ok((
                    module_base
                        .checked_add(*begin)
                        .ok_or_else(|| HubError::Validation("Trace RVA begin overflows".into()))?,
                    module_base
                        .checked_add(*end)
                        .ok_or_else(|| HubError::Validation("Trace RVA end overflows".into()))?,
                ))
            })
            .collect::<Result<Vec<_>, HubError>>()?;
        let digest = trace_scope_digest(&module_name, &ranges, &kinds, &threads);
        let id = format!(
            "tracesel-{:016x}",
            self.next_trace_scope_selection
                .fetch_add(1, Ordering::Relaxed)
        );
        let selection = TraceScopeSelection {
            module: module_name.clone(),
            module_base,
            module_end,
            ranges: ranges.clone(),
            rva_ranges: rva_ranges.clone(),
            kinds,
            kind_names: kind_names.clone(),
            threads: threads.clone(),
            digest: digest.clone(),
        };
        {
            let mut selections = self
                .trace_scope_selections
                .lock()
                .map_err(|_| HubError::Internal("Trace scope selections poisoned".into()))?;
            selections.insert(id.clone(), selection);
            while selections.len() > 64 {
                let Some(oldest) = selections.keys().next().cloned() else {
                    break;
                };
                selections.remove(&oldest);
            }
        }
        let range_json = trace_ranges_json(&ranges, &rva_ranges);
        Ok(json!({
            "selection_id": id,
            "selection_digest": digest,
            "selected_count": ranges.len().to_string(),
            "module": module_name,
            "module_base": format!("0x{module_base:x}"),
            "module_end": format!("0x{module_end:x}"),
            "module_size": format!("0x{module_size:x}"),
            "kinds": kind_names,
            "threads": threads.iter().map(|value| value.to_string()).collect::<Vec<_>>(),
            "thread_scope": if threads.is_empty() { "all" } else { "selected" },
            "ranges": range_json,
            "next_call": {
                "tool": "trace_record_start",
                "selection_id": id,
                "expected_count": ranges.len().to_string(),
                "selection_digest": digest,
            }
        }))
    }

    fn trace_record_start(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let selection_id = req(args, "selection_id")?;
        let expected_count = req(args, "expected_count")?
            .parse::<usize>()
            .map_err(|_| HubError::Validation("expected_count must be decimal".into()))?;
        let expected_digest = req(args, "selection_digest")?;
        let selection = self
            .trace_scope_selections
            .lock()
            .map_err(|_| HubError::Internal("Trace scope selections poisoned".into()))?
            .get(selection_id)
            .cloned()
            .ok_or_else(|| {
                HubError::Validation(
                    "Trace scope selection is unknown or expired; query it again".into(),
                )
            })?;
        if expected_count != selection.ranges.len() || expected_digest != selection.digest {
            return Err(HubError::Validation(format!(
                "Trace scope confirmation mismatch: current count={} digest={}",
                selection.ranges.len(),
                selection.digest
            )));
        }
        let requested_filename = args
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or("trace.pbtr");
        let filename = safe_trace_filename(requested_filename)?;
        let directory = std::env::temp_dir().join("pinbridge-traces");
        std::fs::create_dir_all(&directory)
            .map_err(|error| HubError::Internal(error.to_string()))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| HubError::Internal(error.to_string()))?
            .as_millis();
        let path = directory.join(format!("{stamp}-{}-{filename}", std::process::id()));
        let path_text = path.to_string_lossy().into_owned();
        let mut value = self
            .agent
            .trace_start_spec(
                &selection.kinds,
                &selection.ranges,
                &selection.threads,
                &path_text,
            )
            .map_err(HubError::from)?;
        let session = TraceSession {
            selection_id: selection_id.to_string(),
            selection_digest: selection.digest.clone(),
            module: selection.module.clone(),
            module_base: selection.module_base,
            module_end: selection.module_end,
            ranges: selection.ranges.clone(),
            rva_ranges: selection.rva_ranges.clone(),
            kind_names: selection.kind_names.clone(),
            threads: selection.threads.clone(),
            path: path_text,
            active: true,
            recorded: 0,
            dropped: 0,
            local_index: None,
        };
        *self
            .trace_session
            .lock()
            .map_err(|_| HubError::Internal("Trace session poisoned".into()))? =
            Some(session.clone());
        extend_trace_result(&mut value, &session);
        Ok(value)
    }

    fn trace_record_status(&self) -> Result<Value, HubError> {
        let session = self
            .trace_session
            .lock()
            .map_err(|_| HubError::Internal("Trace session poisoned".into()))?
            .clone();
        if let Some(session) = session.as_ref().filter(|session| !session.active) {
            let mut value = json!({
                "state": "complete",
                "active": false,
                "recorded": session.recorded.to_string(),
                "dropped": session.dropped.to_string(),
            });
            extend_trace_result(&mut value, session);
            return Ok(value);
        }
        let mut value = self.agent.trace_status().map_err(HubError::from)?;
        if let Some(session) = session {
            extend_trace_result(&mut value, &session);
        }
        Ok(value)
    }

    fn trace_record_stop(&self) -> Result<Value, HubError> {
        let mut value = self.agent.trace_stop().map_err(HubError::from)?;
        let recorded = value
            .get("recorded")
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let dropped = value
            .get("dropped")
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let session = {
            let mut guard = self
                .trace_session
                .lock()
                .map_err(|_| HubError::Internal("Trace session poisoned".into()))?;
            if let Some(session) = guard.as_mut() {
                session.active = false;
                session.recorded = recorded;
                session.dropped = dropped;
            }
            guard.clone()
        };
        if let Some(session) = session {
            extend_trace_result(&mut value, &session);
            match crate::trace_query::prepare(&session.path) {
                Ok(index) => {
                    if let Ok(mut guard) = self.trace_session.lock() {
                        if let Some(current) = guard.as_mut() {
                            current.local_index = Some(index.clone());
                        }
                    }
                    if let Some(object) = value.as_object_mut() {
                        object.insert("local_index".into(), index);
                    }
                }
                Err(error) => {
                    if let Some(object) = value.as_object_mut() {
                        object.insert("index_state".into(), Value::String("failed".into()));
                        object.insert("index_error".into(), Value::String(error.to_string()));
                    }
                }
            }
        }
        Ok(value)
    }

    fn trace_index_query(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let session = self
            .trace_session
            .lock()
            .map_err(|_| HubError::Internal("Trace session poisoned".into()))?
            .clone()
            .ok_or_else(|| {
                HubError::Validation(
                    "no Trace artifact exists in the current target session".into(),
                )
            })?;
        if session.active {
            return Err(HubError::Validation(
                "stop the Trace before reading its local database".into(),
            ));
        }
        crate::trace_query::query(&session.path, args)
    }

    fn trace_index_export(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let session = self
            .trace_session
            .lock()
            .map_err(|_| HubError::Internal("Trace session poisoned".into()))?
            .clone()
            .ok_or_else(|| {
                HubError::Validation(
                    "no Trace artifact exists in the current target session".into(),
                )
            })?;
        if session.active {
            return Err(HubError::Validation(
                "stop the Trace before exporting its local database".into(),
            ));
        }
        crate::trace_query::export(&session.path, args)
    }

    fn hook_targets_query(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let requested_module = bounded_module_name(req(args, "module")?)?;
        let module = module_key(requested_module);
        let symbol_pattern = args
            .get("symbol_pattern")
            .and_then(Value::as_str)
            .unwrap_or("*")
            .trim();
        if symbol_pattern.is_empty() || symbol_pattern.len() > 512 {
            return Err(HubError::Validation(
                "symbol_pattern must contain 1..512 bytes".into(),
            ));
        }
        let exports = self.agent.module_exports(&module).map_err(HubError::from)?;
        let rows = exports
            .get("exports")
            .and_then(Value::as_array)
            .ok_or_else(|| HubError::Agent("Agent returned an invalid export list".into()))?;
        let mut unique = BTreeMap::new();
        let mut matched_export_count = 0usize;
        for row in rows {
            let Some(name) = row.get("name").and_then(Value::as_str) else {
                continue;
            };
            if !crate::hook_query::wildcard_match(symbol_pattern, name) {
                continue;
            }
            matched_export_count += 1;
            let Some(address) = row.get("address").map(parse_value_u64).transpose()? else {
                continue;
            };
            if address != 0 {
                unique.entry(address).or_insert_with(|| name.to_string());
            }
        }
        let targets = unique.into_iter().collect::<Vec<_>>();
        let digest = hook_target_digest(&module, &targets);
        let id = format!(
            "hooksel-{:016x}",
            self.next_hook_target_selection
                .fetch_add(1, Ordering::Relaxed)
        );
        let selection = HookTargetSelection {
            module: module.clone(),
            digest: digest.clone(),
            targets: targets.clone(),
            export_count: rows.len(),
            matched_export_count,
        };
        {
            let mut selections = self
                .hook_target_selections
                .lock()
                .map_err(|_| HubError::Internal("Hook target selections poisoned".into()))?;
            selections.insert(id.clone(), selection);
            while selections.len() > 64 {
                let Some(oldest) = selections.keys().next().cloned() else {
                    break;
                };
                selections.remove(&oldest);
            }
        }
        let offset = parse_page_value(args, "offset", 0, 32768)? as usize;
        let limit = parse_page_value(args, "limit", 256, 4096)? as usize;
        if limit == 0 {
            return Err(HubError::Validation(
                "Hook target preview limit must be 1..4096".into(),
            ));
        }
        let start = offset.min(targets.len());
        let end = start.saturating_add(limit).min(targets.len());
        let preview = targets[start..end]
            .iter()
            .map(|(address, name)| json!({"address":format!("0x{address:x}"),"name":name}))
            .collect::<Vec<_>>();
        Ok(json!({
            "selection_id": id,
            "selection_digest": digest,
            "module": module,
            "symbol_pattern": symbol_pattern,
            "export_count": rows.len().to_string(),
            "matched_export_count": matched_export_count.to_string(),
            "selected_count": targets.len().to_string(),
            "deduplicated_aliases": matched_export_count.saturating_sub(targets.len()).to_string(),
            "offset": start.to_string(),
            "returned": preview.len().to_string(),
            "truncated": end < targets.len(),
            "targets": preview,
            "next_call": {
                "tool": "hook_monitor_apply",
                "selection_id": id,
                "expected_count": targets.len().to_string(),
                "selection_digest": digest,
                "mode": "monitor"
            }
        }))
    }

    fn hook_monitor_apply(
        &self,
        caller: Caller,
        args: &Map<String, Value>,
    ) -> Result<Value, HubError> {
        let selection_id = req(args, "selection_id")?;
        let expected_count = req(args, "expected_count")?
            .parse::<usize>()
            .map_err(|_| HubError::Validation("expected_count must be decimal".into()))?;
        let expected_digest = req(args, "selection_digest")?;
        if args
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("monitor")
            != "monitor"
        {
            return Err(HubError::Validation(
                "the first explicit Hook application mode is monitor".into(),
            ));
        }
        let selection = self
            .hook_target_selections
            .lock()
            .map_err(|_| HubError::Internal("Hook target selections poisoned".into()))?
            .get(selection_id)
            .cloned()
            .ok_or_else(|| {
                HubError::Validation(
                    "Hook target selection is unknown or expired; query it again".into(),
                )
            })?;
        if expected_count != selection.targets.len() || expected_digest != selection.digest {
            return Err(HubError::Validation(format!(
                "Hook target confirmation mismatch: current count={} digest={}",
                selection.targets.len(),
                selection.digest
            )));
        }
        let mut value = self
            .agent
            .hook_targets_apply(&selection.module, &selection.targets)
            .map_err(HubError::from)?;
        {
            let mut owners = self
                .hook_owners
                .lock()
                .map_err(|_| HubError::Internal("Hook ownership poisoned".into()))?;
            for (address, _) in &selection.targets {
                owners
                    .entry(*address)
                    .or_default()
                    .insert(actor(caller).to_string());
            }
        }
        self.hook_module_owners
            .lock()
            .map_err(|_| HubError::Internal("Hook module ownership poisoned".into()))?
            .entry(selection.module.clone())
            .or_default()
            .insert(actor(caller).to_string());
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "selection_id".into(),
                Value::String(selection_id.to_string()),
            );
            object.insert("selection_digest".into(), Value::String(selection.digest));
            object.insert(
                "selected_count".into(),
                Value::String(selection.targets.len().to_string()),
            );
            object.insert(
                "export_count".into(),
                Value::String(selection.export_count.to_string()),
            );
            object.insert(
                "matched_export_count".into(),
                Value::String(selection.matched_export_count.to_string()),
            );
            object.insert("mode".into(), Value::String("monitor".into()));
        }
        Ok(value)
    }
    fn audit_args(
        &self,
        args: &Map<String, Value>,
    ) -> Result<(Option<String>, Option<String>), HubError> {
        let purpose = match args.get("purpose") {
            None => None,
            Some(Value::String(value)) if value.len() <= 512 => Some(value.clone()),
            Some(Value::String(_)) => {
                return Err(HubError::Validation("purpose is too long".into()))
            }
            Some(_) => return Err(HubError::Validation("purpose must be string".into())),
        };
        let parent = match args.get("parent_operation_id") {
            None => None,
            Some(Value::String(value))
                if value.len() <= 32
                    && valid_operation_id(value)
                    && self.journal.contains(value) =>
            {
                Some(value.clone())
            }
            Some(Value::String(_)) => {
                return Err(HubError::Validation(
                    "parent operation is invalid or unknown".into(),
                ))
            }
            Some(_) => {
                return Err(HubError::Validation(
                    "parent_operation_id must be string".into(),
                ))
            }
        };
        Ok((purpose, parent))
    }
    fn activity_get(&self, args: &Map<String, Value>) -> Result<Value, HubError> {
        let id = req(args, "operation_id")?;
        self.journal
            .get(id)
            .map(|x| json!({"activity":x}))
            .ok_or_else(|| HubError::Validation("activity not found".into()))
    }
}
fn map_script_error(message: String) -> HubError {
    if message.starts_with("connection_failed:") {
        HubError::Connection(message)
    } else if message.starts_with("agent_failed:") {
        HubError::Agent(message)
    } else {
        HubError::Validation(message)
    }
}
fn actor(c: Caller) -> &'static str {
    match c.actor {
        ChannelActor::Human => "human",
        ChannelActor::Ai => "ai",
        ChannelActor::System => "system",
    }
}
fn parse_event_index_fields(args: &Map<String, Value>) -> Result<Vec<String>, HubError> {
    let Some(values) = args.get("fields") else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| HubError::Validation("event index fields must be an array".into()))?;
    if values.len() > 32 {
        return Err(HubError::Validation(
            "event index fields accepts at most 32 names".into(),
        ));
    }
    const ALLOWED: &[&str] = &[
        "sequence",
        "timestamp_unix_ns",
        "generation",
        "kind",
        "phase",
        "hook_type",
        "thread_id",
        "address",
        "module",
        "symbol",
        "display",
        "signature_capture",
        "signature_status",
        "capture_status",
        "argument_count",
        "arguments",
        "typed_arguments",
        "return_value",
        "typed_return",
        "errno",
        "number",
        "number_decimal",
    ];
    values
        .iter()
        .map(|value| {
            let field = value
                .as_str()
                .ok_or_else(|| HubError::Validation("event index fields must be strings".into()))?;
            if !ALLOWED.contains(&field) {
                return Err(HubError::Validation(format!(
                    "unsupported event index field: {field}"
                )));
            }
            Ok(field.to_string())
        })
        .collect()
}

fn parse_event_index_phases(args: &Map<String, Value>) -> Result<BTreeSet<String>, HubError> {
    let Some(values) = args.get("phases") else {
        return Ok(BTreeSet::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| HubError::Validation("event index phases must be an array".into()))?;
    let mut phases = BTreeSet::new();
    for value in values {
        let phase = value
            .as_str()
            .ok_or_else(|| HubError::Validation("event index phases must be strings".into()))?;
        if !matches!(phase, "hit" | "entry" | "return" | "exit") {
            return Err(HubError::Validation(format!(
                "unsupported event index phase: {phase}"
            )));
        }
        phases.insert(phase.to_string());
    }
    Ok(phases)
}

fn event_index_matches(
    event: &Value,
    index: &str,
    key: &str,
    numeric_key: Option<u64>,
    module: Option<&str>,
    _source: &str,
    phases: &BTreeSet<String>,
) -> bool {
    let phase = event
        .get("phase")
        .or_else(|| event.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !phases.is_empty() && !phases.contains(phase) {
        return false;
    }
    match index {
        "api" => {
            event.get("hook_type").and_then(Value::as_str) == Some("api")
                && event
                    .get("module")
                    .and_then(Value::as_str)
                    .is_some_and(|value| {
                        crate::hook_query::wildcard_match(module.unwrap_or("*"), value)
                    })
                && event
                    .get("symbol")
                    .and_then(Value::as_str)
                    .is_some_and(|value| crate::hook_query::wildcard_match(key, value))
        }
        "syscall" => {
            event
                .get("number_decimal")
                .or_else(|| event.get("number"))
                .and_then(|value| parse_value_u64(value).ok())
                == numeric_key
        }
        "address" => {
            event
                .get("address")
                .and_then(|value| parse_value_u64(value).ok())
                == numeric_key
        }
        "thread" => {
            event
                .get("thread_id")
                .and_then(|value| parse_value_u64(value).ok())
                == numeric_key
        }
        _ => false,
    }
}

fn strip_event_index_payload(event: &mut Value) {
    let Some(object) = event.as_object_mut() else {
        return;
    };
    for field in [
        "arguments",
        "typed_arguments",
        "return_value",
        "typed_return",
        "errno",
        "signature",
    ] {
        object.remove(field);
    }
}

fn project_event_index_fields(event: &Value, fields: &[String]) -> Value {
    let mut projected = Map::new();
    for field in fields {
        if let Some(value) = event.get(field) {
            projected.insert(field.clone(), value.clone());
        }
    }
    Value::Object(projected)
}

fn event_index_sequence(event: &Value) -> Option<u64> {
    event
        .get("sequence")
        .and_then(|value| parse_value_u64(value).ok())
}

fn event_index_lane_metadata(page: &Value, source: &str) -> Value {
    let fields = if source == "syscall" {
        [
            "ring_total",
            "ring_dropped",
            "ring_capacity",
            "history_overwritten",
        ]
    } else {
        [
            "lane_total",
            "lane_dropped",
            "capacity",
            "history_overwritten",
        ]
    };
    let mut lane = Map::new();
    for field in fields {
        if let Some(value) = page.get(field) {
            lane.insert(field.to_string(), value.clone());
        }
    }
    Value::Object(lane)
}

fn event_index_csv(events: &[Value], args: &Map<String, Value>) -> Result<String, HubError> {
    let mut fields = parse_event_index_fields(args)?;
    if fields.is_empty() {
        fields = [
            "sequence",
            "timestamp_unix_ns",
            "kind",
            "hook_type",
            "thread_id",
            "address",
            "module",
            "symbol",
            "number_decimal",
            "arguments",
            "return_value",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
    }
    let mut data = fields
        .iter()
        .map(|field| event_index_csv_cell(field))
        .collect::<Vec<_>>()
        .join(",");
    data.push_str("\r\n");
    for event in events {
        let row = fields
            .iter()
            .map(|field| {
                let value = match event.get(field) {
                    None | Some(Value::Null) => String::new(),
                    Some(Value::String(value)) => value.clone(),
                    Some(value) => serde_json::to_string(value).unwrap_or_default(),
                };
                event_index_csv_cell(&value)
            })
            .collect::<Vec<_>>()
            .join(",");
        data.push_str(&row);
        data.push_str("\r\n");
    }
    Ok(data)
}

fn event_index_csv_cell(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\r' | '\n'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn safe_event_index_filename(requested: &str, extension: &str) -> Result<String, HubError> {
    let stem = requested.trim().trim_end_matches(&format!(".{extension}"));
    if stem.is_empty()
        || stem.len() > 128
        || !stem.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(HubError::Validation(
            "event export filename must use 1..128 ASCII letters, digits, dot, dash, or underscore"
                .into(),
        ));
    }
    Ok(format!("{stem}.{extension}"))
}

fn trace_kind_id(name: &str) -> Result<u32, HubError> {
    match name {
        "exec" => Ok(9),
        "memory" => Ok(10),
        "branch" => Ok(4),
        "syscall" => Ok(5),
        "exception" => Ok(6),
        "registers" => Ok(13),
        "exec_plain" => Ok(3),
        "memory_plain" => Ok(2),
        _ => Err(HubError::Validation(format!(
            "unsupported Trace kind: {name}"
        ))),
    }
}

fn safe_trace_filename(requested: &str) -> Result<String, HubError> {
    let requested = requested.trim();
    let stem = requested
        .strip_suffix(".pbtr")
        .or_else(|| requested.strip_suffix(".PBTR"))
        .unwrap_or(requested);
    if stem.is_empty()
        || stem.len() > 128
        || !stem.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(HubError::Validation(
            "Trace filename must use 1..128 ASCII letters, digits, dot, dash, or underscore".into(),
        ));
    }
    Ok(format!("{stem}.pbtr"))
}

fn trace_scope_digest(
    module: &str,
    ranges: &[(u64, u64)],
    kinds: &[u32],
    threads: &[u32],
) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut digest = OFFSET;
    let mut update = |bytes: &[u8]| {
        for byte in bytes {
            digest ^= *byte as u64;
            digest = digest.wrapping_mul(PRIME);
        }
    };
    update(module.as_bytes());
    update(&[0]);
    for (begin, end) in ranges {
        update(&begin.to_le_bytes());
        update(&end.to_le_bytes());
    }
    update(&[0xff]);
    for kind in kinds {
        update(&kind.to_le_bytes());
    }
    update(&[0xfe]);
    for thread in threads {
        update(&thread.to_le_bytes());
    }
    format!("fnv1a64:{digest:016x}")
}

fn trace_ranges_json(ranges: &[(u64, u64)], rva_ranges: &[(u64, u64)]) -> Vec<Value> {
    ranges
        .iter()
        .zip(rva_ranges)
        .map(|((begin, end), (rva_begin, rva_end))| {
            json!({
                "begin": format!("0x{begin:x}"),
                "end": format!("0x{end:x}"),
                "rva_begin": format!("0x{rva_begin:x}"),
                "rva_end": format!("0x{rva_end:x}"),
                "size": format!("0x{:x}", end - begin),
            })
        })
        .collect()
}

fn extend_trace_result(value: &mut Value, session: &TraceSession) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let file = std::fs::metadata(&session.path).ok();
    object.insert(
        "selection_id".into(),
        Value::String(session.selection_id.clone()),
    );
    object.insert(
        "selection_digest".into(),
        Value::String(session.selection_digest.clone()),
    );
    object.insert("module".into(), Value::String(session.module.clone()));
    object.insert(
        "module_base".into(),
        Value::String(format!("0x{:x}", session.module_base)),
    );
    object.insert(
        "module_end".into(),
        Value::String(format!("0x{:x}", session.module_end)),
    );
    object.insert("kinds".into(), json!(session.kind_names));
    object.insert(
        "threads".into(),
        json!(session
            .threads
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()),
    );
    object.insert(
        "thread_scope".into(),
        Value::String(if session.threads.is_empty() {
            "all".into()
        } else {
            "selected".into()
        }),
    );
    object.insert(
        "ranges".into(),
        json!(trace_ranges_json(&session.ranges, &session.rva_ranges)),
    );
    object.insert("path".into(), Value::String(session.path.clone()));
    object.insert("file_exists".into(), Value::Bool(file.is_some()));
    object.insert(
        "file_bytes".into(),
        Value::String(file.map(|metadata| metadata.len()).unwrap_or(0).to_string()),
    );
    if let Some(index) = &session.local_index {
        object.insert("local_index".into(), index.clone());
    }
}

fn hook_target_digest(module: &str, targets: &[(u64, String)]) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut digest = OFFSET;
    let mut update = |bytes: &[u8]| {
        for byte in bytes {
            digest ^= *byte as u64;
            digest = digest.wrapping_mul(PRIME);
        }
    };
    update(module.as_bytes());
    update(&[0]);
    for (address, name) in targets {
        update(&address.to_le_bytes());
        update(name.as_bytes());
        update(&[0]);
    }
    format!("fnv1a64:{digest:016x}")
}
fn valid_operation_id(value: &str) -> bool {
    value.len() == 19
        && value.starts_with("op-")
        && value[3..].chars().all(|c| c.is_ascii_hexdigit())
}
fn is_write(n: &str) -> bool {
    matches!(
        n,
        "target_pause"
            | "target_resume"
            | "target_step_into"
            | "target_step_over"
            | "breakpoint_set"
            | "breakpoint_remove"
            | "hook_set"
            | "hook_function_set"
            | "hook_signature_set"
            | "hook_signature_remove"
            | "hook_remove"
            | "hook_clear"
            | "hook_module"
            | "hook_monitor_apply"
            | "hook_range_set"
            | "trace_record_start"
            | "trace_record_stop"
            | "syscall_config_set"
            | "register_set"
            | "memory_write"
            | "session_set_agent_port"
            | "script_inject"
            | "script_replace"
            | "script_start"
            | "script_stop"
            | "script_remove"
            | "exception_policy_set"
    )
}

/// MCP scripts must make callback intent visible at the call site. Hook
/// interceptors repeat the requirement inside the Agent API, so aliases
/// cannot bypass the native-control callback contract.
fn validate_ai_breakpoint_descriptions(source: &str) -> Result<(), HubError> {
    let code = mask_python_comments_and_strings(source);
    let bytes = code.as_bytes();
    let mut call_number = 0usize;
    for method in [b"breakpoint".as_slice(), b"intercept".as_slice()] {
        let mut cursor = 0usize;
        while let Some(open) = find_direct_pb_call(bytes, cursor, method) {
            call_number += 1;
            let Some(close) = find_matching_parenthesis(bytes, open) else {
                // Python compilation owns malformed syntax diagnostics.
                break;
            };
            let description_equals =
                find_top_level_keyword(&bytes[open + 1..close], b"description");
            let documented = description_equals
                .map(|equals| {
                    nonempty_python_string_literal(source.as_bytes(), open + 1 + equals + 1)
                })
                .unwrap_or(false);
            if !documented {
                let method = String::from_utf8_lossy(method);
                return Err(HubError::Validation(format!(
                    "MCP callback #{call_number} pb.{method} must include a non-empty literal description=; explain why it exists, which filter it uses, and what it may change"
                )));
            }
            cursor = close + 1;
        }
    }
    Ok(())
}

fn mask_python_comments_and_strings(source: &str) -> String {
    let input = source.as_bytes();
    let mut output = vec![b' '; input.len()];
    let mut index = 0usize;
    let mut quote = 0u8;
    let mut triple = false;
    let mut comment = false;
    while index < input.len() {
        let byte = input[index];
        if comment {
            if byte == b'\n' || byte == b'\r' {
                comment = false;
                output[index] = byte;
            }
            index += 1;
            continue;
        }
        if quote != 0 {
            if byte == b'\\' {
                index = (index + 2).min(input.len());
                continue;
            }
            if triple
                && byte == quote
                && input.get(index + 1) == Some(&quote)
                && input.get(index + 2) == Some(&quote)
            {
                index += 3;
                quote = 0;
                triple = false;
                continue;
            }
            if !triple && byte == quote {
                index += 1;
                quote = 0;
                continue;
            }
            index += 1;
            continue;
        }
        if byte == b'#' {
            comment = true;
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = byte;
            triple = input.get(index + 1) == Some(&byte) && input.get(index + 2) == Some(&byte);
            index += if triple { 3 } else { 1 };
            continue;
        }
        output[index] = byte;
        index += 1;
    }
    String::from_utf8(output).expect("masked Python source remains UTF-8")
}

fn find_direct_pb_call(code: &[u8], mut index: usize, method: &[u8]) -> Option<usize> {
    while index + 2 <= code.len() {
        if word_at(code, index, b"pb") {
            let mut next = skip_ascii_space(code, index + 2);
            if code.get(next) == Some(&b'.') {
                next = skip_ascii_space(code, next + 1);
                if word_at(code, next, method) {
                    next = skip_ascii_space(code, next + method.len());
                    if code.get(next) == Some(&b'(') {
                        return Some(next);
                    }
                }
            }
        }
        index += 1;
    }
    None
}

fn word_at(code: &[u8], index: usize, word: &[u8]) -> bool {
    code.get(index..index + word.len()) == Some(word)
        && (index == 0 || !is_python_ident(code[index - 1]))
        && code
            .get(index + word.len())
            .map(|byte| !is_python_ident(*byte))
            .unwrap_or(true)
}

fn is_python_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_ascii_space(code: &[u8], mut index: usize) -> usize {
    while code.get(index).map(|byte| byte.is_ascii_whitespace()) == Some(true) {
        index += 1;
    }
    index
}

fn find_matching_parenthesis(code: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in code[open..].iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_top_level_keyword(arguments: &[u8], keyword: &[u8]) -> Option<usize> {
    let mut round = 0usize;
    let mut square = 0usize;
    let mut curly = 0usize;
    let mut index = 0usize;
    while index < arguments.len() {
        match arguments[index] {
            b'(' => round += 1,
            b')' => round = round.saturating_sub(1),
            b'[' => square += 1,
            b']' => square = square.saturating_sub(1),
            b'{' => curly += 1,
            b'}' => curly = curly.saturating_sub(1),
            _ if round == 0 && square == 0 && curly == 0 && word_at(arguments, index, keyword) => {
                let equals = skip_ascii_space(arguments, index + keyword.len());
                if arguments.get(equals) == Some(&b'=') && arguments.get(equals + 1) != Some(&b'=')
                {
                    return Some(equals);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn nonempty_python_string_literal(source: &[u8], after_equals: usize) -> bool {
    let start = skip_ascii_space(source, after_equals);
    let Some(&quote) = source.get(start) else {
        return false;
    };
    if quote != b'\'' && quote != b'"' {
        return false;
    }
    // Keep MCP-authored metadata deliberately simple and auditable: one
    // ordinary literal, not an expression, format string, or variable.
    if source.get(start + 1) == Some(&quote) && source.get(start + 2) == Some(&quote) {
        return false;
    }
    let mut index = start + 1;
    let mut visible = false;
    while let Some(&byte) = source.get(index) {
        if byte == b'\\' {
            return false;
        }
        if byte == quote {
            return visible && index - (start + 1) <= 512;
        }
        if byte.is_ascii_control() {
            return false;
        }
        visible |= !byte.is_ascii_whitespace();
        index += 1;
    }
    false
}
fn error_kind(e: &HubError) -> &'static str {
    match e {
        HubError::Validation(_) => "validation",
        HubError::Permission(_) => "permission",
        HubError::Agent(_) => "agent",
        HubError::Connection(_) => "connection",
        HubError::Internal(_) => "internal",
        HubError::Operation { source, .. } => error_kind(source),
    }
}
fn resource_refs(n: &str, v: &Value) -> Value {
    match n {
        n if n.starts_with("script_") => {
            json!({"kind":"script","script_kind":v.get("kind"),"script_id":v.get("script_id"),"name":v.get("name"),"generation":v.get("generation"),"source_hash":v.get("source_hash"),"state":v.get("state")})
        }
        "breakpoint_set" | "breakpoint_remove" => {
            json!({"kind":"breakpoint","id":v.get("id"),"address":v.get("address")} )
        }
        "hook_set"
        | "hook_function_set"
        | "hook_signature_set"
        | "hook_signature_remove"
        | "hook_remove" => {
            json!({"kind":if n.starts_with("hook_signature") { "hook_signature" } else if n == "hook_function_set" { "function_call_hook" } else { "instruction_hook" },"address":v.get("address"),"hooked":v.get("hooked"),"function_log":v.get("function_log"),"signature_status":v.get("signature_status"),"signature_removed":v.get("signature_removed"),"removed":v.get("removed")})
        }
        "hook_module" => {
            json!({"kind":"dll_hook","module":v.get("module"),"armed":v.get("armed"),"exports":v.get("exports"),"capacity_full":v.get("capacity_full")})
        }
        "hook_range_set" => {
            json!({"kind":"instruction_range_hook","start":v.get("start"),"end":v.get("end"),"matched":v.get("matched"),"added":v.get("added"),"total_hooks":v.get("total_hooks"),"capacity_full":v.get("capacity_full")})
        }
        "syscall_config_set" => {
            json!({"kind":"syscall_hook","enabled":v.get("enabled"),"mode":v.get("mode"),"numbers":v.get("numbers")})
        }
        "trace_record_start" | "trace_record_stop" => {
            json!({"kind":"trace_recording","state":v.get("state"),"active":v.get("active"),"recorded":v.get("recorded"),"dropped":v.get("dropped"),"path":v.get("path"),"selection_id":v.get("selection_id")})
        }
        "memory_read" => json!({"kind":"memory","address":v.get("address"),"size":v.get("size")} ),
        "memory_write" => {
            json!({"kind":"memory_write","address":v.get("address"),"requested":v.get("requested"),"written":v.get("written")} )
        }
        "target_pause" | "target_resume" | "target_step_into" | "target_step_over" => {
            json!({"kind":"target_state","paused":v.get("paused"),"running":v.get("running"),"stopped":v.get("stopped"),"thread_id":v.get("thread_id")} )
        }
        _ => json!({"kind":"tool_result","tool":n}),
    }
}
fn parse_hook_signature(args: &Map<String, Value>) -> Result<HookSignature, HubError> {
    let prototype = req(args, "signature")?;
    let source = req(args, "signature_source")?;
    let confidence = req(args, "signature_confidence")?
        .parse::<u32>()
        .map_err(|_| HubError::Validation("signature_confidence must be decimal 0..100".into()))?;
    hook_signature::parse(prototype, source, confidence).map_err(HubError::Validation)
}
fn parse_hook_range(args: &Map<String, Value>) -> Result<(u64, u64, u32), HubError> {
    const KIND_ALL: u32 = 1 << 0;
    const KIND_CALL: u32 = 1 << 1;
    const KIND_SYSCALL: u32 = 1 << 2;
    const KIND_BRANCH: u32 = 1 << 3;
    const KIND_RETURN: u32 = 1 << 4;
    const MAX_RANGE_BYTES: u64 = 4 * 1024 * 1024;
    let start = parse_u64(req(args, "start")?)?;
    let end = parse_u64(req(args, "end")?)?;
    if start == 0 || end <= start {
        return Err(HubError::Validation(
            "Hook range must have non-zero start and end greater than start".into(),
        ));
    }
    if end - start > MAX_RANGE_BYTES {
        return Err(HubError::Validation(
            "Hook range exceeds the 4 MiB per-operation safety limit".into(),
        ));
    }
    let kinds = args
        .get("kinds")
        .and_then(Value::as_array)
        .ok_or_else(|| HubError::Validation("kinds must be an array".into()))?;
    if kinds.is_empty() {
        return Err(HubError::Validation(
            "at least one instruction kind must be selected".into(),
        ));
    }
    let mut mask = 0u32;
    for kind in kinds {
        mask |= match kind.as_str() {
            Some("all") => KIND_ALL,
            Some("call") => KIND_CALL,
            Some("syscall") => KIND_SYSCALL,
            Some("branch") => KIND_BRANCH,
            Some("return") => KIND_RETURN,
            _ => {
                return Err(HubError::Validation(
                    "instruction kind must be all, call, syscall, branch, or return".into(),
                ))
            }
        };
    }
    Ok((start, end, mask))
}
fn req<'a>(a: &'a Map<String, Value>, k: &str) -> Result<&'a str, HubError> {
    a.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| HubError::Validation(format!("{k} must be string")))
}
fn bounded_module_name(value: &str) -> Result<&str, HubError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(HubError::Validation("module must not be empty".into()));
    }
    if value.len() > 1024 {
        return Err(HubError::Validation("module is too long".into()));
    }
    Ok(value)
}
fn module_key(value: &str) -> String {
    value
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase()
}
fn parse_u64(x: &str) -> Result<u64, HubError> {
    let x = x.trim();
    if let Some(hex) = x.strip_prefix("0x").or_else(|| x.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else {
        x.parse()
    }
    .map_err(|_| HubError::Validation(format!("invalid integer: {x}")))
}
fn parse_u32(x: &str) -> Result<u32, HubError> {
    parse_u64(x)?
        .try_into()
        .map_err(|_| HubError::Validation("integer out of range".into()))
}
fn parse_u16(x: &str) -> Result<u16, HubError> {
    parse_u64(x)?
        .try_into()
        .map_err(|_| HubError::Validation("port out of range".into()))
}
fn parse_value_u64(v: &Value) -> Result<u64, HubError> {
    parse_u64(
        v.as_str()
            .ok_or_else(|| HubError::Validation("address must be string".into()))?,
    )
}
fn parse_page_value(
    args: &Map<String, Value>,
    name: &str,
    default: u64,
    maximum: u64,
) -> Result<u64, HubError> {
    let raw = args.get(name).and_then(Value::as_str);
    let value = raw
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| HubError::Validation(format!("Hook inventory {name} must be decimal")))?
        .unwrap_or(default);
    if value > maximum {
        return Err(HubError::Validation(format!(
            "Hook inventory {name} exceeds {maximum}"
        )));
    }
    Ok(value)
}
fn parse_hex(x: &str) -> Result<Vec<u8>, HubError> {
    if !x.len().is_multiple_of(2) {
        return Err(HubError::Validation("data_hex must be even".into()));
    }
    (0..x.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&x[i..i + 2], 16)
                .map_err(|_| HubError::Validation("invalid data_hex".into()))
        })
        .collect()
}
fn parse_mode(v: Option<&Value>) -> Result<ControlMode, HubError> {
    match v.and_then(Value::as_str) {
        Some("ai_read_only") => Ok(ControlMode::AiReadOnly),
        Some("ai_assist") => Ok(ControlMode::AiAssist),
        Some("ai_autonomous") => Ok(ControlMode::AiAutonomous),
        _ => Err(HubError::Validation("invalid mode".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentApi, AgentError};
    use crate::ipc::{spawn_listener, IpcClient, IpcHello, IpcRequest, IpcResponse};
    use std::net::TcpListener;
    use std::sync::Arc;
    #[derive(Clone)]
    struct Fake;
    impl AgentApi for Fake {
        fn set_port(&self, _: u16) {}
        fn port(&self) -> u16 {
            1
        }
        fn status(&self) -> Result<Value, AgentError> {
            Ok(json!({"connected":true}))
        }
        fn pause(&self) -> Result<bool, AgentError> {
            Ok(true)
        }
        fn resume(&self) -> Result<bool, AgentError> {
            Ok(true)
        }
        fn memory_read(&self, _: u64, _: u64) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn memory_write(&self, _: u64, _: &[u8]) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn breakpoint_set(&self, _: u64) -> Result<Value, AgentError> {
            Ok(json!({"id":"7","address":"0x10"}))
        }
        fn breakpoint_remove(&self, _: u32) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn breakpoint_list(&self) -> Result<Value, AgentError> {
            Ok(json!({
                "stopped": false,
                "hit_thread_id": u32::MAX.to_string(),
                "hit_address": "0x0",
                "stop_generation": "0",
                "breakpoints": [{"id":"7","address":"0x10","hits":"0","callbacks":[]}]
            }))
        }
        fn registers_get(&self, _: u32) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn register_set(&self, _: u32, _: u32, _: u64) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn threads(&self) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn modules(&self) -> Result<Value, AgentError> {
            Ok(json!({"modules":[{
                "base":"0x140000000",
                "end":"0x14001ffff",
                "is_main":true,
                "name":"C:\\fixtures\\sample.exe"
            }]}))
        }
        fn module_exports(&self, module: &str) -> Result<Value, AgentError> {
            Ok(json!({
                "module": module,
                "count": "3",
                "exports": [
                    {"address":"0x100","name":"NtOpenFile"},
                    {"address":"0x100","name":"NtOpenFileAlias"},
                    {"address":"0x200","name":"ZwClose"}
                ]
            }))
        }
        fn hook_targets_apply(
            &self,
            module: &str,
            targets: &[(u64, String)],
        ) -> Result<Value, AgentError> {
            Ok(json!({
                "module": module,
                "requested": targets.len().to_string(),
                "armed": targets.len().to_string(),
                "total_hooks": targets.len().to_string(),
                "capacity_full": false
            }))
        }
        fn hook_monitor(&self, _: u64, before: u64) -> Result<Value, AgentError> {
            Ok(json!({
                "lane_total":"3",
                "lane_dropped":"0",
                "history_overwritten":"0",
                "next_cursor":"3",
                "capacity":"32768",
                "pointer_width":"8",
                "window_before":before.to_string(),
                "events":[
                    {"sequence":"1","timestamp_unix_ns":"100","kind":"entry","hook_type":"api","thread_id":"7","address":"0x100","module":"ntdll.dll","symbol":"NtOpenFile","display":"ntdll.dll!NtOpenFile","signature_capture":false,"arguments":["0x1"]},
                    {"sequence":"2","timestamp_unix_ns":"125","kind":"return","hook_type":"api","thread_id":"7","address":"0x100","module":"ntdll.dll","symbol":"NtOpenFile","display":"ntdll.dll!NtOpenFile","signature_capture":false,"arguments":[],"return_value":"0x0"},
                    {"sequence":"3","timestamp_unix_ns":"130","kind":"hit","hook_type":"instruction","thread_id":"8","address":"0x200","module":"sample.exe","symbol":null,"display":null,"signature_capture":false,"arguments":[]}
                ]
            }))
        }
        fn syscall_monitor_window(&self, _: u64, before: u64) -> Result<Value, AgentError> {
            Ok(json!({
                "ring_total":"2",
                "ring_dropped":"0",
                "ring_capacity":"32768",
                "history_overwritten":"0",
                "window_before":before.to_string(),
                "events":[
                    {"sequence":"1","timestamp_unix_ns":"200","generation":"1","thread_id":"9","number":"0x32","number_decimal":"50","phase":"entry","kind":"entry","hook_type":"syscall","arguments":["0xaa"]},
                    {"sequence":"2","timestamp_unix_ns":"225","generation":"1","thread_id":"9","number":"0x32","number_decimal":"50","phase":"exit","kind":"return","hook_type":"syscall","arguments":[],"return_value":"0x0","errno":"0x0"}
                ]
            }))
        }
        fn hook_inventory(&self) -> Result<Value, AgentError> {
            Ok(json!({
                "capacity": "32768",
                "hooks": [
                    {"address":"0x10", "function_log":false, "callbacks":[]},
                    {"address":"0x20", "function_log":true, "callbacks":[]}
                ]
            }))
        }
        fn trace_start_spec(
            &self,
            _: &[u32],
            _: &[(u64, u64)],
            _: &[u32],
            _: &str,
        ) -> Result<Value, AgentError> {
            Ok(json!({"state":"recording","active":true,"recorded":"0","dropped":"0"}))
        }
        fn trace_status(&self) -> Result<Value, AgentError> {
            Ok(json!({"state":"recording","active":true,"recorded":"12","dropped":"0"}))
        }
        fn trace_stop(&self) -> Result<Value, AgentError> {
            Ok(json!({"state":"complete","active":false,"recorded":"12","dropped":"0"}))
        }
        fn step(&self, _: u32, _: bool) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn disassemble(&self, _: u64, _: u64) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn resolve(&self, _: &[u64]) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn resolve_name(&self, _: &str) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn script_load(&self, _: &str, _: &str) -> Result<u32, AgentError> {
            Ok(1)
        }
        fn script_unload(&self, _: &str) -> Result<(), AgentError> {
            Ok(())
        }
        fn script_list(&self) -> Result<Vec<crate::AgentScript>, AgentError> {
            Ok(vec![])
        }
        fn script_output(
            &self,
            _: u64,
            _: u32,
        ) -> Result<(u64, Vec<crate::AgentOutputLine>), AgentError> {
            Ok((0, vec![]))
        }
        fn events_newest(&self, _limit: u64) -> Result<Value, AgentError> {
            Ok(
                json!({"next":u64::MAX.to_string(),"events":[{"sequence":u64::MAX.to_string(),"kind":"7","thread_id":u32::MAX.to_string(),"address":"0xffffffffffffffff","arg0":"0xffffffffffffffff"}]}),
            )
        }
    }
    #[test]
    fn ai_write_is_denied_until_handoff() {
        let h = HubService::new(Fake);
        let a = Map::new();
        assert!(matches!(
            h.call(Caller::AI, "target_pause", &a),
            Err(HubError::Operation { source, .. }) if matches!(*source, HubError::Permission(_))
        ));
    }
    #[test]
    fn successful_write_has_operation_id_and_resource_refs() {
        let h = HubService::new(Fake);
        h.control
            .handoff(Caller::TRUSTED_HUMAN, ControlMode::AiAutonomous)
            .unwrap();
        let mut args = Map::new();
        args.insert("address".into(), Value::String("0x10".into()));
        let value = h.call(Caller::AI, "breakpoint_set", &args).unwrap();
        assert!(value.get("operation_id").and_then(Value::as_str).is_some());
        assert!(h.journal.list(1)[0].resource_refs.get("address").is_some());
    }
    #[test]
    fn breakpoint_inventory_reports_traditional_creator() {
        let h = HubService::new(Fake);
        let mut args = Map::new();
        args.insert("address".into(), Value::String("0x10".into()));
        h.call(Caller::TRUSTED_HUMAN, "breakpoint_set", &args)
            .unwrap();

        let value = h
            .call(Caller::SYSTEM, "breakpoint_inventory", &Map::new())
            .unwrap();
        assert_eq!(value["breakpoints"][0]["kind"], "traditional");
        assert_eq!(value["breakpoints"][0]["plain_owners"][0], "human");
    }
    #[test]
    fn hook_inventory_strictly_separates_instruction_and_api_points() {
        let h = HubService::new(Fake);
        let mut instruction = Map::new();
        instruction.insert("kind".into(), Value::String("instruction".into()));
        let value = h
            .call(Caller::SYSTEM, "hook_inventory", &instruction)
            .unwrap();
        assert_eq!(value["count"], "1");
        assert_eq!(value["hooks"][0]["address"], "0x10");

        let mut api = Map::new();
        api.insert("kind".into(), Value::String("api".into()));
        let value = h.call(Caller::SYSTEM, "hook_inventory", &api).unwrap();
        assert_eq!(value["count"], "1");
        assert_eq!(value["hooks"][0]["address"], "0x20");
    }
    #[test]
    fn explicit_hook_target_snapshot_must_be_confirmed_before_apply() {
        let h = HubService::new(Fake);
        let mut query = Map::new();
        query.insert("module".into(), Value::String("ntdll.dll".into()));
        query.insert("symbol_pattern".into(), Value::String("Nt*".into()));
        let selection = h
            .call(Caller::SYSTEM, "hook_targets_query", &query)
            .unwrap();
        assert_eq!(selection["selected_count"], "1");
        assert_eq!(selection["matched_export_count"], "2");
        assert_eq!(selection["deduplicated_aliases"], "1");

        let mut apply = Map::new();
        apply.insert("selection_id".into(), selection["selection_id"].clone());
        apply.insert("expected_count".into(), Value::String("2".into()));
        apply.insert(
            "selection_digest".into(),
            selection["selection_digest"].clone(),
        );
        assert!(h
            .call(Caller::TRUSTED_HUMAN, "hook_monitor_apply", &apply)
            .is_err());
        apply.insert("expected_count".into(), Value::String("1".into()));
        let applied = h
            .call(Caller::TRUSTED_HUMAN, "hook_monitor_apply", &apply)
            .unwrap();
        assert_eq!(applied["armed"], "1");
        assert_eq!(applied["mode"], "monitor");
    }

    #[test]
    fn trace_scope_is_resolved_then_explicitly_confirmed() {
        let h = HubService::new(Fake);
        let mut query = Map::new();
        query.insert("module".into(), json!("sample.exe"));
        query.insert("kinds".into(), json!(["exec", "memory", "branch"]));
        query.insert(
            "ranges".into(),
            json!([
                {"rva_begin":"0x100","rva_end":"0x200"},
                {"rva_begin":"0x180","rva_end":"0x300"}
            ]),
        );
        let selection = h.call(Caller::SYSTEM, "trace_scope_query", &query).unwrap();
        assert_eq!(selection["selected_count"], "1");
        assert_eq!(selection["ranges"][0]["begin"], "0x140000100");
        assert_eq!(selection["ranges"][0]["end"], "0x140000300");

        let mut start = Map::new();
        start.insert("selection_id".into(), selection["selection_id"].clone());
        start.insert("expected_count".into(), json!("2"));
        start.insert(
            "selection_digest".into(),
            selection["selection_digest"].clone(),
        );
        assert!(h
            .call(Caller::TRUSTED_HUMAN, "trace_record_start", &start)
            .is_err());

        start.insert("expected_count".into(), json!("1"));
        start.insert("filename".into(), json!("mcp-scope.pbtr"));
        let started = h
            .call(Caller::TRUSTED_HUMAN, "trace_record_start", &start)
            .unwrap();
        assert_eq!(started["state"], "recording");
        assert!(started["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("mcp-scope.pbtr")));

        let status = h
            .call(Caller::SYSTEM, "trace_record_status", &Map::new())
            .unwrap();
        assert_eq!(status["recorded"], "12");
        assert_eq!(status["selection_id"], selection["selection_id"]);

        let stopped = h
            .call(Caller::TRUSTED_HUMAN, "trace_record_stop", &Map::new())
            .unwrap();
        assert_eq!(stopped["state"], "complete");
        assert_eq!(stopped["recorded"], "12");
    }

    #[test]
    fn trace_scope_rejects_out_of_module_rvas_and_unsafe_names() {
        let h = HubService::new(Fake);
        let query = [
            ("module".into(), json!("sample.exe")),
            ("kinds".into(), json!(["exec"])),
            (
                "ranges".into(),
                json!([{"rva_begin":"0x100","rva_end":"0x30000"}]),
            ),
        ]
        .into_iter()
        .collect();
        assert!(matches!(
            h.call(Caller::SYSTEM, "trace_scope_query", &query),
            Err(HubError::Validation(message)) if message.contains("module size")
        ));
        assert!(safe_trace_filename("../escape.pbtr").is_err());
    }

    #[test]
    fn hook_event_query_pairs_calls_and_exports_csv() {
        let h = HubService::new(Fake);
        let mut query = Map::new();
        query.insert("layout".into(), Value::String("calls".into()));
        query.insert("order".into(), Value::String("asc".into()));
        query.insert("hook_types".into(), json!(["api"]));
        let calls = h.call(Caller::SYSTEM, "hook_events_query", &query).unwrap();
        assert_eq!(calls["matched_calls"], "1");
        assert_eq!(calls["calls"][0]["status"], "paired");
        assert_eq!(calls["calls"][0]["duration_ns"], "25");

        query.insert("format".into(), Value::String("csv".into()));
        let export = h
            .call(Caller::SYSTEM, "hook_events_export", &query)
            .unwrap();
        assert_eq!(export["format"], "csv");
        assert!(export["data"]
            .as_str()
            .unwrap()
            .contains("ntdll.dll!NtOpenFile"));
    }
    #[test]
    fn mcp_event_indices_target_one_api_or_syscall_without_a_raw_window() {
        let h = HubService::new(Fake);
        let mut api = Map::new();
        api.insert("index".into(), Value::String("api".into()));
        api.insert("key".into(), Value::String("NtOpenFile".into()));
        api.insert("module".into(), Value::String("ntdll.dll".into()));
        api.insert("limit".into(), Value::String("2".into()));
        let indexed = h.call(Caller::SYSTEM, "event_index_query", &api).unwrap();
        assert_eq!(indexed["returned"], "2");
        assert!(indexed["events"][0].get("arguments").is_none());

        let mut syscall = Map::new();
        syscall.insert("index".into(), Value::String("syscall".into()));
        syscall.insert("key".into(), Value::String("50".into()));
        syscall.insert("limit".into(), Value::String("2".into()));
        syscall.insert("payload".into(), Value::Bool(true));
        let indexed = h
            .call(Caller::SYSTEM, "event_index_query", &syscall)
            .unwrap();
        assert_eq!(indexed["returned"], "2");
        assert_eq!(indexed["events"][0]["number_decimal"], "50");

        syscall.insert("format".into(), Value::String("jsonl".into()));
        syscall.insert("delivery".into(), Value::String("inline".into()));
        let exported = h
            .call(Caller::SYSTEM, "event_index_export", &syscall)
            .unwrap();
        assert_eq!(exported["rows"], "2");
        assert!(exported["data"]
            .as_str()
            .unwrap()
            .contains("number_decimal"));
    }
    #[test]
    fn invalid_purpose_does_not_create_in_progress_activity() {
        let h = HubService::new(Fake);
        let mut args = Map::new();
        args.insert("purpose".into(), Value::String("x".repeat(513)));
        assert!(matches!(
            h.call(Caller::AI, "session_status", &args),
            Err(HubError::Validation(_))
        ));
        assert_eq!(h.journal.list(10).len(), 0);
    }
    #[test]
    fn mcp_breakpoint_callbacks_require_one_literal_description_each() {
        assert!(validate_ai_breakpoint_descriptions(
            "pb.breakpoint(0x10, first, description=\"checks the entry guard\")\n\
             pb.breakpoint(0x20, second, description='records the decoded state')"
        )
        .is_ok());
        assert!(validate_ai_breakpoint_descriptions(
            "# pb.breakpoint(0, ignored)\ntext = \"pb.breakpoint(0, ignored)\""
        )
        .is_ok());

        let missing = validate_ai_breakpoint_descriptions(
            "pb.breakpoint(0x10, first, description=\"documented\")\n\
             pb.breakpoint(0x20, second)",
        )
        .unwrap_err();
        assert!(missing.to_string().contains("callback #2"));
        assert!(validate_ai_breakpoint_descriptions(
            "pb.breakpoint(0x10, first, description=\"   \")"
        )
        .is_err());
        assert!(validate_ai_breakpoint_descriptions(
            "pb.breakpoint(0x10, first, description=DESCRIPTION)"
        )
        .is_err());
        assert!(validate_ai_breakpoint_descriptions(
            "pb.intercept('hook.entry', cb, address=0x10, description='inspect API entry arguments')"
        )
        .is_ok());
        assert!(validate_ai_breakpoint_descriptions(
            "pb.intercept('hook.entry', cb, address=0x10)"
        )
        .is_err());
    }
    #[test]
    fn mcp_accepts_the_real_hook_callback_fixture_source() {
        let source = include_str!("../../../../fixtures/hook_python_demo/hook_intercept.py");
        validate_ai_breakpoint_descriptions(source).unwrap();
    }
    #[test]
    fn ai_cannot_change_agent_port_in_autonomous_mode() {
        let h = HubService::new(Fake);
        h.control
            .handoff(Caller::TRUSTED_HUMAN, ControlMode::AiAutonomous)
            .unwrap();
        let mut args = Map::new();
        args.insert("agent_port".into(), Value::String("99".into()));
        assert!(matches!(
            h.call(Caller::AI, "session_set_agent_port", &args),
            Err(HubError::Operation { .. })
        ));
    }
    #[test]
    fn system_poll_reads_do_not_grow_journal_but_ai_reads_do() {
        let h = HubService::new(Fake);
        for _ in 0..100 {
            h.call(Caller::SYSTEM, "control_status", &Map::new())
                .unwrap();
            h.call(Caller::SYSTEM, "session_status", &Map::new())
                .unwrap();
            h.call(Caller::SYSTEM, "breakpoint_list", &Map::new())
                .unwrap();
        }
        assert_eq!(h.journal.list(1000).len(), 0);
        h.call(Caller::AI, "control_status", &Map::new()).unwrap();
        assert_eq!(h.journal.list(1).len(), 1);
    }
    #[test]
    fn system_event_snapshot_is_bounded_precise_and_not_a_tool() {
        let h = HubService::new(Fake);
        let mut args = Map::new();
        args.insert("limit".into(), Value::String("24".into()));
        let value = h.call(Caller::SYSTEM, "events_newest", &args).unwrap();
        assert_eq!(value["next"], u64::MAX.to_string());
        assert_eq!(value["events"][0]["sequence"], u64::MAX.to_string());
        assert_eq!(h.journal.list(1).len(), 0);
        args.insert("limit".into(), Value::String("25".into()));
        assert!(matches!(
            h.call(Caller::SYSTEM, "events_newest", &args),
            Err(HubError::Validation(_))
        ));
        assert!(matches!(
            h.call(Caller::AI, "events_newest", &Map::new()),
            Err(HubError::Permission(_))
        ));
    }

    #[test]
    fn syscall_scope_resolves_whole_module_and_rva_range() {
        let hub = HubService::new(Fake);
        let mut module = Map::new();
        module.insert("scope".into(), json!("module"));
        module.insert("module".into(), json!("sample.exe"));
        let resolved = hub.resolve_syscall_scope(&module).unwrap();
        assert_eq!(resolved.0, "module");
        assert_eq!(resolved.2, 0x140000000);
        assert_eq!(resolved.3, 0x140020000);
        assert_eq!(resolved.6, 0x140000000);
        assert_eq!(resolved.7, 0x140020000);

        module.insert("scope".into(), json!("rva"));
        module.insert("rva_begin".into(), json!("0x120"));
        module.insert("rva_end".into(), json!("0x480"));
        let resolved = hub.resolve_syscall_scope(&module).unwrap();
        assert_eq!(resolved.0, "rva");
        assert_eq!(resolved.4, 0x120);
        assert_eq!(resolved.5, 0x480);
        assert_eq!(resolved.6, 0x140000120);
        assert_eq!(resolved.7, 0x140000480);
    }

    #[test]
    fn syscall_scope_rejects_rva_outside_module() {
        let hub = HubService::new(Fake);
        let args = [
            ("scope".into(), json!("rva")),
            ("module".into(), json!("sample.exe")),
            ("rva_begin".into(), json!("0x100")),
            ("rva_end".into(), json!("0x30000")),
        ]
        .into_iter()
        .collect();
        assert!(matches!(
            hub.resolve_syscall_scope(&args),
            Err(HubError::Validation(message)) if message.contains("module size")
        ));
    }

    #[test]
    fn denied_write_keeps_ipc_connection_available_for_followup_calls() {
        let service = Arc::new(HubService::new(Fake));
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handler_service = service.clone();
        let server = spawn_listener(
            listener,
            "human-secret-123456".into(),
            "ai-secret-123456".into(),
            move |caller, request: IpcRequest| match handler_service.call(
                caller,
                &request.method,
                &request.params,
            ) {
                Ok(result) => IpcResponse {
                    id: request.id,
                    ok: true,
                    operation_id: result
                        .get("operation_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    result: Some(result),
                    error: None,
                },
                Err(error) => IpcResponse {
                    id: request.id,
                    ok: false,
                    operation_id: error.operation_id().map(str::to_owned),
                    result: None,
                    error: Some(error.to_string()),
                },
            },
        )
        .unwrap();
        let mut client = IpcClient::connect(
            address,
            IpcHello {
                channel: "ai".into(),
                secret: "ai-secret-123456".into(),
            },
        )
        .unwrap();

        let status = client
            .call(IpcRequest {
                id: json!("status-1"),
                method: "control_status".into(),
                params: Map::new(),
            })
            .unwrap();
        assert!(status.ok);

        let denied = client
            .call(IpcRequest {
                id: json!("write-1"),
                method: "breakpoint_set".into(),
                params: [("address".into(), json!("0x10"))].into_iter().collect(),
            })
            .unwrap();
        assert!(!denied.ok);
        assert!(denied
            .error
            .as_deref()
            .is_some_and(|message| message.contains("permission_denied")));
        assert!(denied
            .operation_id
            .as_deref()
            .is_some_and(|id| id.starts_with("op-")));

        let followup = client
            .call(IpcRequest {
                id: json!("status-2"),
                method: "control_status".into(),
                params: Map::new(),
            })
            .unwrap();
        assert!(followup.ok);
        server.stop();
    }
}
