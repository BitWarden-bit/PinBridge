use crate::{
    activities::Journal,
    agent::{AgentApi, AgentError},
    control::{Caller, ChannelActor, ControlMode, ControlState},
    script_service::{OutputRequest, ScriptRequest, ScriptService},
    session::Session,
};
use serde_json::{json, Map, Value};

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
    "registers_get",
    "register_set",
    "memory_read",
    "memory_write",
    "disassemble",
    "modules_list",
    "threads_list",
    "address_resolve",
    "activity_list",
    "activity_get",
    "script_inject",
    "script_replace",
    "script_remove",
    "script_list",
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
        }
    }
    pub fn set_target(&self, target: Option<String>) {
        self.session.set_target(target)
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
            "breakpoint_set" => self
                .agent
                .breakpoint_set(parse_u64(req(args, "address")?)?)
                .map_err(HubError::from),
            "breakpoint_remove" => self
                .agent
                .breakpoint_remove(parse_u32(req(args, "id")?)?)
                .map_err(HubError::from),
            "breakpoint_list" => self.agent.breakpoint_list().map_err(HubError::from),
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
                let r: ScriptRequest = serde_json::from_value(Value::Object(args.clone()))
                    .map_err(|e| HubError::Validation(e.to_string()))?;
                let v = if name == "script_inject" {
                    self.scripts.inject(r)
                } else {
                    self.scripts.replace(r)
                }
                .map_err(map_script_error)?;
                serde_json::to_value(v).map_err(|e| HubError::Internal(e.to_string()))
            }
            "script_remove" => self
                .scripts
                .remove(req(args, "name")?)
                .map_err(map_script_error),
            "script_list" => serde_json::to_value(self.scripts.list().map_err(map_script_error)?)
                .map_err(|e| HubError::Internal(e.to_string())),
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
            | "register_set"
            | "memory_write"
            | "session_set_agent_port"
            | "script_inject"
            | "script_replace"
            | "script_remove"
    )
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
            json!({"kind":"script","script_id":v.get("script_id"),"name":v.get("name"),"generation":v.get("generation"),"source_hash":v.get("source_hash"),"state":v.get("state")})
        }
        "breakpoint_set" | "breakpoint_remove" => {
            json!({"kind":"breakpoint","id":v.get("id"),"address":v.get("address")} )
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
fn req<'a>(a: &'a Map<String, Value>, k: &str) -> Result<&'a str, HubError> {
    a.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| HubError::Validation(format!("{k} must be string")))
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
            Ok(json!({}))
        }
        fn breakpoint_remove(&self, _: u32) -> Result<Value, AgentError> {
            Ok(json!({}))
        }
        fn breakpoint_list(&self) -> Result<Value, AgentError> {
            Ok(json!({}))
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
            Ok(json!({}))
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
