use crate::agent::{AgentApi, AgentError, AgentScript};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Mutex;

pub const MAX_SCRIPT_NAME_BYTES: usize = 256;
pub const MAX_SCRIPT_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_LIMIT: u32 = 1024;
pub const MAX_OUTPUT_TEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScriptKind {
    Module,
    Callback,
}

impl ScriptKind {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("module") {
            "module" => Ok(Self::Module),
            "callback" => Ok(Self::Callback),
            _ => Err("kind must be module or callback".into()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Callback => "callback",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScriptRequest {
    pub name: String,
    pub source: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct OutputRequest {
    #[serde(default)]
    pub cursor: String,
    #[serde(default = "default_limit")]
    pub limit: String,
}
fn default_limit() -> String {
    "256".into()
}
#[derive(Clone, Debug, Serialize)]
pub struct ScriptResource {
    pub script_id: String,
    pub name: String,
    pub kind: String,
    pub generation: String,
    pub source_hash: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration: Option<Registration>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Registration {
    pub state: String,
    pub delivered: String,
    pub dropped: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct OutputPage {
    pub next_cursor: String,
    pub lines: Vec<OutputLine>,
}
#[derive(Clone, Debug, Serialize)]
pub struct OutputLine {
    pub seq: String,
    pub plugin: String,
    pub line: String,
}
#[derive(Clone)]
struct Record {
    script_id: String,
    kind: ScriptKind,
    generation: u64,
    source_hash: String,
    source: String,
    created_by: String,
    modified_by: String,
    agent_id: Option<String>,
    state: String,
}
pub struct ScriptService<A: AgentApi> {
    agent: A,
    mutation: Mutex<()>,
    records: Mutex<BTreeMap<String, Record>>,
    next: Mutex<u64>,
}
impl<A: AgentApi> ScriptService<A> {
    pub fn new(agent: A) -> Self {
        Self {
            agent,
            mutation: Mutex::new(()),
            records: Mutex::new(BTreeMap::new()),
            next: Mutex::new(1),
        }
    }
    fn validate(r: &ScriptRequest) -> Result<(), String> {
        if r.name.is_empty() {
            return Err("name must not be empty".into());
        }
        if r.name.len() > MAX_SCRIPT_NAME_BYTES {
            return Err("name exceeds 256 bytes".into());
        }
        if r.name.chars().any(|c| c.is_control()) || r.name.as_bytes().contains(&0) {
            return Err("name contains control character".into());
        }
        if r.source.is_empty() {
            return Err("source must not be empty".into());
        }
        if r.source.len() > MAX_SCRIPT_SOURCE_BYTES {
            return Err("source exceeds 1 MiB".into());
        }
        if r.source.as_bytes().contains(&0) {
            return Err("source contains NUL".into());
        }
        Ok(())
    }
    fn apply(
        &self,
        r: ScriptRequest,
        replace: bool,
        actor: &str,
        requested_kind: Option<&str>,
    ) -> Result<ScriptResource, String> {
        Self::validate(&r)?;
        let requested_kind = requested_kind
            .map(|kind| ScriptKind::parse(Some(kind)))
            .transpose()?;
        // Keep the Agent mutation and the corresponding local bookkeeping in
        // one transaction.  The Agent transport gate alone cannot prevent a
        // remove from landing between script_load and records.insert.
        let _mutation = self
            .mutation
            .lock()
            .map_err(|_| "script mutation state poisoned")?;
        // Do not hold the bookkeeping lock while making an Agent request.  In
        // particular, a restarted Hub has no local row yet, so a replacement
        // must consult the Agent before deciding that the script is missing.
        let old = self
            .records
            .lock()
            .map_err(|_| "script state poisoned")?
            .get(&r.name)
            .cloned();
        if let (Some(old), Some(requested)) = (&old, requested_kind) {
            if old.kind != requested {
                return Err(format!(
                    "script kind mismatch: {} is {}, not {}",
                    r.name,
                    old.kind.as_str(),
                    requested.as_str()
                ));
            }
        }
        let kind = old
            .as_ref()
            .map(|record| record.kind)
            .or(requested_kind)
            .unwrap_or(ScriptKind::Module);

        // Updating a stopped module saves a new source generation without
        // implicitly executing it. Callback scripts have binding-scoped
        // lifetimes and therefore retain the historical hot-reload behavior.
        if replace
            && kind == ScriptKind::Module
            && old.as_ref().is_some_and(|record| record.state == "stopped")
        {
            let old = old.expect("checked above");
            let rec = Record {
                script_id: old.script_id,
                kind,
                generation: old.generation + 1,
                source_hash: hash(&r.source),
                source: r.source,
                created_by: old.created_by,
                modified_by: actor.to_string(),
                agent_id: None,
                state: "stopped".into(),
            };
            let out = resource(&r.name, &rec, None);
            self.records
                .lock()
                .map_err(|_| "script state poisoned")?
                .insert(r.name, rec);
            return Ok(out);
        }
        let agent_exists = if replace && old.is_none() {
            self.agent
                .script_list()
                .map_err(format_agent)?
                .into_iter()
                .any(|script| script.name == r.name)
        } else {
            false
        };
        if replace && old.is_none() && !agent_exists {
            return Err("script_replace requires an existing control-plane or Agent script".into());
        }
        let id = self
            .agent
            .script_load(&r.name, &r.source)
            .map_err(format_agent)?;
        let mut rows = self.records.lock().map_err(|_| "script state poisoned")?;
        // Re-read after the Agent call so a concurrent local operation wins
        // its established identity/generation.  Recovered rows use a stable
        // name-derived id because the Agent only exposes script names.
        let current = rows.get(&r.name).cloned();
        let existing = current.clone().or_else(|| old.clone());
        let (sid, g, created_by) = if let Some(o) = existing {
            (o.script_id, o.generation + 1, o.created_by)
        } else if agent_exists {
            (format!("agent:{}", r.name), 1, "unknown".to_string())
        } else {
            let mut n = self.next.lock().map_err(|_| "script id poisoned")?;
            let x = format!("script-{n}");
            *n += 1;
            (x, 1, actor.to_string())
        };
        let rec = Record {
            script_id: sid,
            kind,
            generation: g,
            source_hash: hash(&r.source),
            source: r.source.clone(),
            created_by,
            modified_by: actor.to_string(),
            agent_id: Some(id.to_string()),
            state: if replace {
                "replacement_staged".into()
            } else {
                "load_staged".into()
            },
        };
        let out = resource(&r.name, &rec, None);
        rows.insert(r.name, rec);
        Ok(out)
    }
    pub fn inject(&self, r: ScriptRequest) -> Result<ScriptResource, String> {
        self.inject_as(r, "system")
    }
    pub fn replace(&self, r: ScriptRequest) -> Result<ScriptResource, String> {
        self.replace_as(r, "system")
    }
    pub fn inject_as(&self, r: ScriptRequest, actor: &str) -> Result<ScriptResource, String> {
        self.apply(r, false, actor, None)
    }
    pub fn replace_as(&self, r: ScriptRequest, actor: &str) -> Result<ScriptResource, String> {
        self.apply(r, true, actor, None)
    }
    pub fn inject_kind_as(
        &self,
        r: ScriptRequest,
        kind: Option<&str>,
        actor: &str,
    ) -> Result<ScriptResource, String> {
        self.apply(r, false, actor, kind)
    }
    pub fn replace_kind_as(
        &self,
        r: ScriptRequest,
        kind: Option<&str>,
        actor: &str,
    ) -> Result<ScriptResource, String> {
        self.apply(r, true, actor, kind)
    }
    pub fn stop(&self, name: &str) -> Result<ScriptResource, String> {
        validate_name(name)?;
        let _mutation = self
            .mutation
            .lock()
            .map_err(|_| "script mutation state poisoned")?;
        let current = self
            .records
            .lock()
            .map_err(|_| "script state poisoned")?
            .get(name)
            .cloned()
            .ok_or_else(|| {
                "module source unavailable: script was not loaded through this Hub".to_string()
            })?;
        if current.kind != ScriptKind::Module {
            return Err("callback scripts are stopped by removing their owning binding".into());
        }
        if current.state == "stopped" {
            return Ok(resource(name, &current, None));
        }
        self.agent.script_unload(name).map_err(format_agent)?;
        let mut rows = self.records.lock().map_err(|_| "script state poisoned")?;
        let record = rows
            .get_mut(name)
            .ok_or_else(|| "script disappeared during stop".to_string())?;
        record.state = "stopped".into();
        record.agent_id = None;
        Ok(resource(name, record, None))
    }
    pub fn start(&self, name: &str) -> Result<ScriptResource, String> {
        validate_name(name)?;
        let _mutation = self
            .mutation
            .lock()
            .map_err(|_| "script mutation state poisoned")?;
        let current = self
            .records
            .lock()
            .map_err(|_| "script state poisoned")?
            .get(name)
            .cloned()
            .ok_or_else(|| {
                "module source unavailable: script was not loaded through this Hub".to_string()
            })?;
        if current.kind != ScriptKind::Module {
            return Err("callback scripts start with their owning binding".into());
        }
        if current.state != "stopped" {
            return Err("module is already active".into());
        }
        let id = self
            .agent
            .script_load(name, &current.source)
            .map_err(format_agent)?;
        let mut rows = self.records.lock().map_err(|_| "script state poisoned")?;
        let record = rows
            .get_mut(name)
            .ok_or_else(|| "script disappeared during start".to_string())?;
        record.state = "load_staged".into();
        record.agent_id = Some(id.to_string());
        Ok(resource(name, record, None))
    }
    pub fn remove(&self, name: &str) -> Result<Value, String> {
        validate_name(name)?;
        let _mutation = self
            .mutation
            .lock()
            .map_err(|_| "script mutation state poisoned")?;
        let stopped = self
            .records
            .lock()
            .map_err(|_| "script state poisoned")?
            .get(name)
            .is_some_and(|record| record.state == "stopped");
        if !stopped {
            self.agent.script_unload(name).map_err(format_agent)?;
        }
        let mut rows = self.records.lock().map_err(|_| "script state poisoned")?;
        if let Some(r) = rows.remove(name) {
            Ok(
                json!({"script_id":r.script_id,"name":name,"generation":r.generation.to_string(),"source_hash":r.source_hash,"state":"remove_accepted"}),
            )
        } else {
            Ok(
                json!({"script_id":format!("agent:{name}"),"name":name,"generation":"0","source_hash":"unknown","state":"remove_accepted"}),
            )
        }
    }
    pub fn list(&self) -> Result<Vec<ScriptResource>, String> {
        let agent = self.agent.script_list().map_err(format_agent)?;
        let rows = self.records.lock().map_err(|_| "script state poisoned")?;
        let mut out = Vec::new();
        for a in agent {
            let reg = Some(registration(&a));
            if let Some(r) = rows.get(&a.name) {
                out.push(resource(&a.name, r, reg))
            } else {
                out.push(ScriptResource {
                    script_id: format!("agent:{}", a.name),
                    name: a.name,
                    kind: "unknown".into(),
                    generation: "0".into(),
                    source_hash: "unknown".into(),
                    state: "agent_reported".into(),
                    created_by: None,
                    modified_by: None,
                    agent_id: None,
                    registration: reg,
                })
            }
        }
        for (n, r) in rows.iter() {
            if !out.iter().any(|x| x.name == *n) {
                out.push(resource(n, r, None))
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
    pub fn get(&self, name: &str) -> Result<Value, String> {
        validate_name(name)?;
        let rows = self.records.lock().map_err(|_| "script state poisoned")?;
        let record = rows.get(name).ok_or_else(|| {
            "script source unavailable: script was not loaded through this Hub".to_string()
        })?;
        let mut value = serde_json::to_value(resource(name, record, None))
            .map_err(|error| error.to_string())?;
        if let Value::Object(ref mut object) = value {
            object.insert("source".into(), Value::String(record.source.clone()));
            object.insert("source_available".into(), Value::Bool(true));
        }
        Ok(value)
    }
    pub fn provenance(&self, name: &str) -> Option<(String, String)> {
        self.records
            .lock()
            .ok()?
            .get(name)
            .map(|record| (record.created_by.clone(), record.modified_by.clone()))
    }
    pub fn status(&self, name: Option<&str>) -> Result<Value, String> {
        let rows = self.list()?;
        if let Some(n) = name {
            validate_name(n)?;
            rows.into_iter()
                .find(|r| r.name == n)
                .map(|r| serde_json::to_value(r).unwrap())
                .ok_or_else(|| "script not found".into())
        } else {
            Ok(json!({"scripts":rows}))
        }
    }
    pub fn output(&self, r: OutputRequest) -> Result<OutputPage, String> {
        let after = parse(&r.cursor, "cursor")?;
        let limit = u32::try_from(parse(&r.limit, "limit")?)
            .map_err(|_| "limit exceeds u32".to_string())?;
        if limit == 0 || limit > MAX_OUTPUT_LIMIT {
            return Err("limit must be 1..1024".into());
        }
        let (next, lines) = self
            .agent
            .script_output(after, limit)
            .map_err(format_agent)?;
        let mut out = Vec::new();
        for l in lines.into_iter().take(limit as usize) {
            if l.plugin.len() > MAX_SCRIPT_NAME_BYTES || l.line.len() > MAX_OUTPUT_TEXT_BYTES {
                return Err("agent returned oversized script output".into());
            }
            out.push(OutputLine {
                seq: l.seq.to_string(),
                plugin: l.plugin,
                line: l.line,
            })
        }
        Ok(OutputPage {
            next_cursor: next.to_string(),
            lines: out,
        })
    }
}
fn format_agent(e: AgentError) -> String {
    match e {
        AgentError::Connection(x) => format!("connection_failed: {x}"),
        AgentError::Operation(x) => format!("agent_failed: {x}"),
    }
}
fn parse(v: &str, n: &str) -> Result<u64, String> {
    if v.trim().is_empty() {
        Ok(0)
    } else {
        v.trim()
            .parse()
            .map_err(|_| format!("{n} must be unsigned decimal string"))
    }
}
fn validate_name(n: &str) -> Result<(), String> {
    if n.is_empty()
        || n.len() > MAX_SCRIPT_NAME_BYTES
        || n.chars().any(|c| c.is_control())
        || n.as_bytes().contains(&0)
    {
        Err("invalid script name".into())
    } else {
        Ok(())
    }
}
fn hash(s: &str) -> String {
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn registration(a: &AgentScript) -> Registration {
    Registration {
        state: a.state.to_string(),
        delivered: a.delivered.to_string(),
        dropped: a.dropped.to_string(),
    }
}
fn resource(n: &str, r: &Record, reg: Option<Registration>) -> ScriptResource {
    ScriptResource {
        script_id: r.script_id.clone(),
        name: n.into(),
        kind: r.kind.as_str().into(),
        generation: r.generation.to_string(),
        source_hash: r.source_hash.clone(),
        state: r.state.clone(),
        created_by: Some(r.created_by.clone()),
        modified_by: Some(r.modified_by.clone()),
        agent_id: r.agent_id.clone(),
        registration: reg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentApi, AgentError, AgentOutputLine};
    use std::sync::{mpsc::Sender, Arc, Barrier, Mutex};
    #[derive(Clone)]
    struct Fake {
        loads: Arc<Mutex<u32>>,
        agent_scripts: Arc<Mutex<Vec<AgentScript>>>,
        load_started: Option<Arc<Barrier>>,
        load_release: Option<Arc<Barrier>>,
        unload_notify: Option<Sender<()>>,
    }
    impl Fake {
        fn new() -> Self {
            Self {
                loads: Arc::new(Mutex::new(0)),
                agent_scripts: Arc::new(Mutex::new(Vec::new())),
                load_started: None,
                load_release: None,
                unload_notify: None,
            }
        }
    }
    impl AgentApi for Fake {
        fn set_port(&self, _: u16) {}
        fn port(&self) -> u16 {
            1
        }
        fn status(&self) -> Result<Value, AgentError> {
            Ok(json!({}))
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
            *self.loads.lock().unwrap() += 1;
            if let Some(barrier) = &self.load_started {
                barrier.wait();
                self.load_release.as_ref().unwrap().wait();
            }
            Ok(1)
        }
        fn script_unload(&self, _: &str) -> Result<(), AgentError> {
            if let Some(notify) = &self.unload_notify {
                notify.send(()).unwrap();
            }
            Ok(())
        }
        fn script_list(&self) -> Result<Vec<AgentScript>, AgentError> {
            Ok(self.agent_scripts.lock().unwrap().clone())
        }
        fn script_output(&self, _: u64, _: u32) -> Result<(u64, Vec<AgentOutputLine>), AgentError> {
            Ok((u64::MAX, vec![]))
        }
    }
    #[test]
    fn generation_and_hash() {
        let s = ScriptService::new(Fake::new());
        let a = s
            .inject(ScriptRequest {
                name: "a.py".into(),
                source: "x".into(),
            })
            .unwrap();
        let b = s
            .replace(ScriptRequest {
                name: "a.py".into(),
                source: "y".into(),
            })
            .unwrap();
        assert_eq!(a.generation, "1");
        assert_eq!(b.generation, "2");
        assert_ne!(a.source_hash, b.source_hash)
    }
    #[test]
    fn source_never_in_resource() {
        let s = ScriptService::new(Fake::new());
        let r = s
            .inject(ScriptRequest {
                name: "a.py".into(),
                source: "SECRET".into(),
            })
            .unwrap();
        assert!(!serde_json::to_string(&r).unwrap().contains("SECRET"));
    }

    #[test]
    fn source_get_preserves_creator_and_modifier() {
        let s = ScriptService::new(Fake::new());
        s.inject_as(
            ScriptRequest {
                name: "callback.py".into(),
                source: "return_stay = True".into(),
            },
            "ai",
        )
        .unwrap();
        s.replace_as(
            ScriptRequest {
                name: "callback.py".into(),
                source: "return_stay = False".into(),
            },
            "human",
        )
        .unwrap();

        let value = s.get("callback.py").unwrap();
        assert_eq!(value["source"], "return_stay = False");
        assert_eq!(value["created_by"], "ai");
        assert_eq!(value["modified_by"], "human");
        assert_eq!(s.provenance("callback.py").unwrap().0, "ai");
    }

    #[test]
    fn replace_recovers_agent_script_after_local_restart() {
        let fake = Fake::new();
        fake.agent_scripts.lock().unwrap().push(AgentScript {
            name: "a.py".into(),
            state: 1,
            delivered: 0,
            dropped: 0,
        });
        let s = ScriptService::new(fake);
        let r = s
            .replace(ScriptRequest {
                name: "a.py".into(),
                source: "recovered".into(),
            })
            .unwrap();

        assert_eq!(r.script_id, "agent:a.py");
        assert_eq!(r.generation, "1");
    }

    #[test]
    fn replace_rejects_script_missing_locally_and_on_agent() {
        let s = ScriptService::new(Fake::new());
        let error = s
            .replace(ScriptRequest {
                name: "missing.py".into(),
                source: "x".into(),
            })
            .unwrap_err();

        assert!(error.contains("control-plane or Agent"));
    }

    #[test]
    fn mutation_transaction_serializes_load_and_remove() {
        let load_started = Arc::new(Barrier::new(2));
        let load_release = Arc::new(Barrier::new(2));
        let (unload_tx, unload_rx) = std::sync::mpsc::channel();
        let fake = Fake {
            loads: Arc::new(Mutex::new(0)),
            agent_scripts: Arc::new(Mutex::new(Vec::new())),
            load_started: Some(load_started.clone()),
            load_release: Some(load_release.clone()),
            unload_notify: Some(unload_tx),
        };
        let service = Arc::new(ScriptService::new(fake));
        let loader = service.clone();
        let load_thread = std::thread::spawn(move || {
            loader
                .inject(ScriptRequest {
                    name: "race.py".into(),
                    source: "x".into(),
                })
                .unwrap();
        });
        load_started.wait();

        let remover = service.clone();
        let remove_started = Arc::new(Barrier::new(2));
        let remove_started_thread = remove_started.clone();
        let remove_thread = std::thread::spawn(move || {
            remove_started_thread.wait();
            remover.remove("race.py").unwrap();
        });
        remove_started.wait();

        assert!(unload_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        load_release.wait();
        load_thread.join().unwrap();
        remove_thread.join().unwrap();
        assert!(unload_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .is_ok());
    }

    #[test]
    fn output_limit_rejects_u32_overflow() {
        let s = ScriptService::new(Fake::new());
        let error = s
            .output(OutputRequest {
                cursor: "0".into(),
                limit: "4294967297".into(),
            })
            .unwrap_err();
        assert!(error.contains("exceeds u32"));
    }

    #[test]
    fn script_kinds_are_explicit_and_default_to_module() {
        let s = ScriptService::new(Fake::new());
        let module = s
            .inject(ScriptRequest {
                name: "analysis.py".into(),
                source: "x = 1".into(),
            })
            .unwrap();
        let callback = s
            .inject_kind_as(
                ScriptRequest {
                    name: "breakpoint_callback.py".into(),
                    source: "x = 2".into(),
                },
                Some("callback"),
                "human",
            )
            .unwrap();

        assert_eq!(module.kind, "module");
        assert_eq!(callback.kind, "callback");
        assert!(s
            .stop("breakpoint_callback.py")
            .unwrap_err()
            .contains("binding"));
    }

    #[test]
    fn stopped_module_retains_source_and_can_restart() {
        let fake = Fake::new();
        let loads = fake.loads.clone();
        let s = ScriptService::new(fake);
        s.inject(ScriptRequest {
            name: "workflow.py".into(),
            source: "version = 1".into(),
        })
        .unwrap();

        let stopped = s.stop("workflow.py").unwrap();
        assert_eq!(stopped.state, "stopped");
        assert_eq!(s.get("workflow.py").unwrap()["source"], "version = 1");

        let saved = s
            .replace(ScriptRequest {
                name: "workflow.py".into(),
                source: "version = 2".into(),
            })
            .unwrap();
        assert_eq!(saved.state, "stopped");
        assert_eq!(*loads.lock().unwrap(), 1);

        let started = s.start("workflow.py").unwrap();
        assert_eq!(started.state, "load_staged");
        assert_eq!(*loads.lock().unwrap(), 2);
        assert_eq!(s.get("workflow.py").unwrap()["source"], "version = 2");
    }

    #[test]
    fn explicit_kind_cannot_reclassify_existing_script() {
        let s = ScriptService::new(Fake::new());
        s.inject_kind_as(
            ScriptRequest {
                name: "focused.py".into(),
                source: "x = 1".into(),
            },
            Some("callback"),
            "human",
        )
        .unwrap();

        let error = s
            .replace_kind_as(
                ScriptRequest {
                    name: "focused.py".into(),
                    source: "x = 2".into(),
                },
                Some("module"),
                "human",
            )
            .unwrap_err();
        assert!(error.contains("kind mismatch"));
    }
}
