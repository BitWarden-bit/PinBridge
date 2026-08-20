#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Trusted human adapter for the PinBridge Hub.
//!
//! The Tauri process owns the embedded `HubService`.  UI commands never keep
//! an Agent client and never connect to the Agent directly; Hub's typed human
//! caller is the policy boundary for every operation.

use pinbridge_client::launch;
use pinbridge_hub_core::ipc::{
    spawn_listener, validate_secrets, IpcRequest, IpcResponse, IpcServerHandle,
};
use pinbridge_hub_core::{AgentConnection, Caller, HubService};
use pinbridge_proto as proto;
use serde_json::{json, Map, Value};
use std::net::TcpListener;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, RunEvent, State};

const POLL_PERIOD: Duration = Duration::from_millis(250);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

struct Config {
    port: u16,
    hub_listen: u16,
    pin: Option<String>,
    agent: Option<String>,
    target: Vec<String>,
}

fn parse_args() -> Config {
    let default_hub_port = std::env::var("PINBRIDGE_HUB_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9444);
    let mut c = Config {
        port: proto::DEFAULT_PORT,
        hub_listen: default_hub_port,
        pin: None,
        agent: None,
        target: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                if let Some(v) = args.next() {
                    c.port = v.parse().unwrap_or(proto::DEFAULT_PORT);
                }
            }
            "--hub-listen" | "--listen" => {
                if let Some(v) = args.next() {
                    c.hub_listen = v.parse().unwrap_or(c.hub_listen);
                }
            }
            "--pin" => c.pin = args.next(),
            "--agent" => c.agent = args.next(),
            "--" => c.target.extend(args.by_ref()),
            other => c.target.push(other.to_string()),
        }
    }
    c
}

struct AppState {
    port: Mutex<u16>,
    pin: Option<String>,
    agent: Option<String>,
    hub: Arc<HubService<AgentConnection>>,
    ai_adapter_available: bool,
    ipc: Mutex<Option<IpcServerHandle>>,
    backend: Arc<Mutex<Option<Child>>>,
    target: Mutex<Option<String>>,
    /// Operator released the workspace while the local Pin backend and target
    /// remain alive. The owned Child handle is deliberately retained so the
    /// same UI can either reattach or explicitly terminate it later.
    released: Mutex<bool>,
}

fn map_args(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

fn human_call(state: &AppState, name: &str, args: Map<String, Value>) -> Result<Value, String> {
    state
        .hub
        .call(Caller::TRUSTED_HUMAN, name, &args)
        .map_err(|e| e.to_string())
}

fn system_call(state: &AppState, name: &str, args: Map<String, Value>) -> Result<Value, String> {
    state
        .hub
        .call(Caller::SYSTEM, name, &args)
        .map_err(|e| e.to_string())
}

fn parse_addr(text: &str) -> Result<u64, String> {
    let s = text.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|_| format!("bad address: {text}"))
    } else {
        s.parse::<u64>().map_err(|_| format!("bad number: {text}"))
    }
}

fn decimal(v: u64) -> Value {
    Value::String(v.to_string())
}

fn parse_agent_endpoint(address: &str) -> Result<u16, String> {
    let text = address
        .trim()
        .strip_prefix("tcp://")
        .unwrap_or(address.trim());
    let (host, port_text) = text
        .rsplit_once(':')
        .ok_or_else(|| "Agent address must be host:port".to_string())?;
    let host = host.trim_matches(['[', ']']);
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return Err("PinBridge Agent only accepts loopback attachments".into());
    }
    let port = port_text
        .parse::<u16>()
        .map_err(|_| format!("invalid Agent port: {port_text}"))?;
    if port == 0 {
        return Err("Agent port must be greater than zero".into());
    }
    Ok(port)
}

fn string_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string field: {key}"))
}
fn decimal_field(v: &Value, key: &str) -> Result<u64, String> {
    string_field(v, key)?
        .parse()
        .map_err(|_| format!("invalid decimal field: {key}"))
}
fn parse_bp_id(text: &str) -> Result<u32, String> {
    text.trim()
        .parse()
        .map_err(|_| "breakpoint id must be a decimal u32".into())
}
fn decimal_or(v: &Value, key: &str, fallback: u64) -> u64 {
    v.get(key)
        .and_then(Value::as_str)
        .and_then(|x| x.parse().ok())
        .unwrap_or(fallback)
}
fn decimal_string_or(v: &Value, key: &str, fallback: u64) -> Value {
    match v.get(key) {
        Some(Value::String(value)) => Value::String(value.clone()),
        Some(Value::Number(value)) => Value::String(value.to_string()),
        _ => decimal(fallback),
    }
}
fn decimal_strings(v: &Value, key: &str) -> Vec<Value> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|xs| {
            xs.iter()
                .filter_map(|x| match x {
                    Value::String(value) => Some(Value::String(value.clone())),
                    Value::Number(value) => Some(Value::String(value.to_string())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tauri::command]
fn cmd_session(state: State<'_, Arc<AppState>>) -> serde_json::Value {
    let owned = state.backend.lock().map(|g| g.is_some()).unwrap_or(false);
    let released = state.released.lock().map(|value| *value).unwrap_or(false);
    let status = system_call(&state, "session_status", Map::new()).ok();
    let connected = status
        .as_ref()
        .and_then(|v| v.get("session"))
        .and_then(|v| v.get("connected"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let target = state
        .target
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .or_else(|| {
            status
                .as_ref()
                .and_then(|v| v.get("session"))
                .and_then(|v| v.get("target"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let port = state.port.lock().map(|value| *value).unwrap_or(0);
    json!({
        "running": !released && (owned || connected),
        "released": released,
        "address": format!("127.0.0.1:{port}"),
        "target": target,
        "backend_owned": owned,
    })
}

fn launch_blocking(
    state: &AppState,
    target: String,
    pin: Option<String>,
    entry_bp: Option<bool>,
    probe_mode: Option<bool>,
    arguments: Option<Vec<String>>,
) -> Result<(), String> {
    if state.backend.lock().map_err(|e| e.to_string())?.is_some() {
        return Err("backend already running; kill it first".into());
    }
    // A new target starts in Manual. Hub changes the mode before launch; it
    // does not restart or otherwise control the target through this call.
    let _ = human_call(&state, "control_takeover_manual", Map::new());
    let options = launch::LaunchOptions {
        pin: pin.or_else(|| state.pin.clone()),
        agent: state.agent.clone(),
        arch: None,
        port: 0,
        entry_bp: entry_bp.unwrap_or(true),
        probe_mode: probe_mode.unwrap_or(false),
        show_target_console: true,
    };
    let mut command = vec![target.clone()];
    command.extend(arguments.unwrap_or_default());
    let (child, port) = launch::launch_for_target(&options, &command, STARTUP_TIMEOUT)
        .map_err(|e| e.to_string())?;
    let connected = human_call(
        &state,
        "session_set_agent_port",
        map_args([("agent_port", decimal(port as u64))]),
    );
    if let Err(e) = connected {
        let mut c = child;
        launch::kill_backend(&mut c);
        return Err(e);
    }
    *state.backend.lock().map_err(|e| e.to_string())? = Some(child);
    *state.port.lock().map_err(|e| e.to_string())? = port;
    *state.target.lock().map_err(|e| e.to_string())? = Some(target.clone());
    *state.released.lock().map_err(|e| e.to_string())? = false;
    state.hub.set_target(Some(target));
    Ok(())
}

#[tauri::command]
async fn cmd_launch(
    state: State<'_, Arc<AppState>>,
    target: String,
    pin: Option<String>,
    entry_bp: Option<bool>,
    probe_mode: Option<bool>,
    arguments: Option<Vec<String>>,
) -> Result<(), String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        launch_blocking(&state, target, pin, entry_bp, probe_mode, arguments)
    })
    .await
    .map_err(|error| format!("launch worker failed: {error}"))?
}

#[tauri::command]
fn cmd_attach_agent(
    state: State<'_, Arc<AppState>>,
    address: String,
) -> Result<serde_json::Value, String> {
    let port = parse_agent_endpoint(&address)?;
    let backend_owned = state.backend.lock().map_err(|e| e.to_string())?.is_some();
    let released = *state.released.lock().map_err(|e| e.to_string())?;
    let current_port = *state.port.lock().map_err(|e| e.to_string())?;
    if backend_owned && (!released || port != current_port) {
        return Err(if released {
            format!(
                "a released local backend is still alive at 127.0.0.1:{current_port}; reattach it or terminate it before attaching another Agent"
            )
        } else {
            "a locally launched backend is already attached; release or terminate it first".into()
        });
    }
    let previous = *state.port.lock().map_err(|e| e.to_string())?;
    human_call(
        &state,
        "session_set_agent_port",
        map_args([("agent_port", decimal(port as u64))]),
    )?;
    *state.port.lock().map_err(|e| e.to_string())? = port;
    let session = system_call(&state, "session_status", Map::new());
    match session {
        Ok(value) => {
            let target = value
                .get("session")
                .and_then(|item| item.get("target"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            *state.target.lock().map_err(|e| e.to_string())? = target.clone();
            *state.released.lock().map_err(|e| e.to_string())? = false;
            state.hub.set_target(target);
            Ok(value)
        }
        Err(error) => {
            let _ = human_call(
                &state,
                "session_set_agent_port",
                map_args([("agent_port", decimal(previous as u64))]),
            );
            *state.port.lock().map_err(|e| e.to_string())? = previous;
            Err(format!("could not attach to Agent at {address}: {error}"))
        }
    }
}

#[tauri::command]
fn cmd_environment(state: State<'_, Arc<AppState>>) -> serde_json::Value {
    let pin = state
        .pin
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| launch::resolve_pin(None, pinbridge_client::Arch::X64).ok());
    let agent = state
        .agent
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| launch::default_agent_dll(pinbridge_client::Arch::X64).ok());
    json!({
        "pin": {
            "available": pin.as_ref().is_some_and(|path| path.is_file()),
            "path": pin.map(|path| path.display().to_string())
        },
        "agent": {
            "available": agent.as_ref().is_some_and(|path| path.is_file()),
            "path": agent.as_ref().map(|path| path.display().to_string())
        },
        "python": {
            "available": agent.as_ref().is_some_and(|path| path.is_file()),
            "detail": "validated by the embedded Agent at launch"
        },
        "hub": {
            "available": state.ai_adapter_available,
            "detail": if state.ai_adapter_available { "embedded Hub ready" } else { "AI IPC adapter unavailable" }
        }
    })
}

#[tauri::command]
fn cmd_kill_backend(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    // Hub takeover blocks AI writes before attempting the pause. The child is
    // then reaped; a disconnected Agent is not treated as an exit request.
    let _ = human_call(&state, "control_takeover_manual", Map::new());
    if let Some(child) = state.backend.lock().map_err(|e| e.to_string())?.as_mut() {
        launch::kill_backend(child);
    }
    *state.backend.lock().map_err(|e| e.to_string())? = None;
    *state.target.lock().map_err(|e| e.to_string())? = None;
    *state.released.lock().map_err(|e| e.to_string())? = false;
    let port = *state.port.lock().map_err(|e| e.to_string())?;
    let _ = human_call(
        &state,
        "session_set_agent_port",
        map_args([("agent_port", decimal(port as u64))]),
    );
    state.hub.set_target(None);
    Ok(())
}

#[tauri::command]
fn cmd_release_session(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let owned = state.backend.lock().map_err(|e| e.to_string())?.is_some();
    let session = system_call(&state, "session_status", Map::new())?;
    let connected = session
        .get("session")
        .and_then(|value| value.get("connected"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !owned && !connected {
        return Err("no live local backend or Agent connection to release".into());
    }
    // A released workspace must never leave AI autonomous writes enabled.
    let _ = human_call(&state, "control_takeover_manual", Map::new());
    *state.released.lock().map_err(|e| e.to_string())? = true;
    let port = *state.port.lock().map_err(|e| e.to_string())?;
    let target = state.target.lock().map_err(|e| e.to_string())?.clone();
    Ok(json!({
        "released": true,
        "address": format!("127.0.0.1:{port}"),
        "target": target,
        "backend_owned": owned,
        "detail": "workspace control released; target and Pin backend remain alive"
    }))
}

#[tauri::command]
fn cmd_control(state: State<'_, Arc<AppState>>, action: String) -> Result<bool, String> {
    let name = match action.as_str() {
        "stop" => "target_pause",
        "resume" => "target_resume",
        _ => return Err("bad action".into()),
    };
    let value = human_call(&state, name, Map::new())?;
    Ok(value
        .get("paused")
        .or_else(|| value.get("running"))
        .and_then(Value::as_bool)
        .unwrap_or(true))
}

#[tauri::command]
fn cmd_step(state: State<'_, Arc<AppState>>, tid: u32, over: bool) -> Result<bool, String> {
    let name = if over {
        "target_step_over"
    } else {
        "target_step_into"
    };
    let value = human_call(&state, name, map_args([("thread_id", decimal(tid as u64))]))?;
    Ok(value
        .get("stopped")
        .and_then(Value::as_bool)
        .unwrap_or(true))
}

#[tauri::command]
fn cmd_bp_set(state: State<'_, Arc<AppState>>, address: String) -> Result<u32, String> {
    let value = human_call(
        &state,
        "breakpoint_set",
        map_args([("address", decimal(parse_addr(&address)?))]),
    )?;
    decimal_field(&value, "id").and_then(|x| {
        x.try_into()
            .map_err(|_| "breakpoint id out of range".into())
    })
}

#[tauri::command]
fn cmd_bp_remove(state: State<'_, Arc<AppState>>, id: String) -> Result<u32, String> {
    let id = parse_bp_id(&id)?;
    let value = human_call(
        &state,
        "breakpoint_remove",
        map_args([("id", decimal(id as u64))]),
    )?;
    decimal_field(&value, "id").and_then(|x| {
        x.try_into()
            .map_err(|_| "breakpoint id out of range".into())
    })
}

#[tauri::command]
fn cmd_bp_list(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let value = human_call(&state, "breakpoint_list", Map::new())?;
    Ok(normalize_bp(&value))
}

#[tauri::command]
async fn cmd_breakpoint_inventory(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        system_call(&state, "breakpoint_inventory", Map::new())
    })
    .await
    .map_err(|error| format!("breakpoint inventory worker failed: {error}"))?
}

#[tauri::command]
fn cmd_script_get(state: State<'_, Arc<AppState>>, name: String) -> Result<Value, String> {
    human_call(
        &state,
        "script_get",
        map_args([("name", Value::String(name))]),
    )
}

#[tauri::command]
fn cmd_script_list(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    system_call(&state, "script_list", Map::new())
}

#[tauri::command]
fn cmd_script_inject(
    state: State<'_, Arc<AppState>>,
    name: String,
    source: String,
    kind: Option<String>,
) -> Result<Value, String> {
    let mut args = map_args([
        ("name", Value::String(name)),
        ("source", Value::String(source)),
    ]);
    if let Some(kind) = kind {
        args.insert("kind".into(), Value::String(kind));
    }
    human_call(&state, "script_inject", args)
}

#[tauri::command]
fn cmd_script_replace(
    state: State<'_, Arc<AppState>>,
    name: String,
    source: String,
    kind: Option<String>,
) -> Result<Value, String> {
    let mut args = map_args([
        ("name", Value::String(name)),
        ("source", Value::String(source)),
    ]);
    if let Some(kind) = kind {
        args.insert("kind".into(), Value::String(kind));
    }
    human_call(&state, "script_replace", args)
}

#[tauri::command]
fn cmd_script_start(state: State<'_, Arc<AppState>>, name: String) -> Result<Value, String> {
    human_call(
        &state,
        "script_start",
        map_args([("name", Value::String(name))]),
    )
}

#[tauri::command]
fn cmd_script_stop(state: State<'_, Arc<AppState>>, name: String) -> Result<Value, String> {
    human_call(
        &state,
        "script_stop",
        map_args([("name", Value::String(name))]),
    )
}

#[tauri::command]
fn cmd_script_remove(state: State<'_, Arc<AppState>>, name: String) -> Result<Value, String> {
    human_call(
        &state,
        "script_remove",
        map_args([("name", Value::String(name))]),
    )
}

#[tauri::command]
fn cmd_script_output(
    state: State<'_, Arc<AppState>>,
    cursor: String,
    limit: String,
) -> Result<Value, String> {
    system_call(
        &state,
        "script_output",
        map_args([
            ("cursor", Value::String(cursor)),
            ("limit", Value::String(limit)),
        ]),
    )
}

fn normalize_bp(value: &Value) -> Value {
    let rows = value.get("breakpoints").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().map(|b| json!({
        "id": decimal_string_or(&b, "id", 0), "address": b.get("address").cloned().unwrap_or_else(|| json!("0x0")), "hits": decimal_string_or(&b, "hits", 0),
    })).collect::<Vec<_>>();
    json!({ "stopped": value.get("stopped").and_then(Value::as_bool).unwrap_or(false), "hit_tid": decimal_string_or(value, "hit_thread_id", u32::MAX as u64), "hit_addr": value.get("hit_address").cloned().unwrap_or_else(|| json!("0x0")), "stop_gen": decimal_string_or(value, "stop_generation", 0), "breakpoints": rows })
}

#[tauri::command]
fn cmd_modules(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let value = human_call(&state, "modules_list", Map::new())?;
    Ok(Value::Array(value.get("modules").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().map(|m| json!({
        "low": m.get("base").cloned().unwrap_or_else(|| json!("0x0")), "high": m.get("end").cloned().unwrap_or_else(|| json!("0x0")), "main": m.get("is_main").and_then(Value::as_bool).unwrap_or(false), "name": m.get("name").cloned().unwrap_or_else(|| json!("")),
    })).collect()))
}

#[tauri::command]
fn cmd_module_exports(state: State<'_, Arc<AppState>>, module: String) -> Result<Value, String> {
    system_call(
        &state,
        "module_exports",
        map_args([("module", Value::String(module))]),
    )
}

#[tauri::command]
fn cmd_hook_set(state: State<'_, Arc<AppState>>, address: String) -> Result<Value, String> {
    human_call(
        &state,
        "hook_set",
        map_args([("address", decimal(parse_addr(&address)?))]),
    )
}

#[tauri::command]
fn cmd_hook_function_set(
    state: State<'_, Arc<AppState>>,
    address: String,
    signature: String,
    signature_source: String,
    signature_confidence: String,
) -> Result<Value, String> {
    human_call(
        &state,
        "hook_function_set",
        map_args([
            ("address", decimal(parse_addr(&address)?)),
            ("signature", Value::String(signature)),
            ("signature_source", Value::String(signature_source)),
            ("signature_confidence", Value::String(signature_confidence)),
        ]),
    )
}

#[tauri::command]
fn cmd_hook_signature_set(
    state: State<'_, Arc<AppState>>,
    address: String,
    signature: String,
    signature_source: String,
    signature_confidence: String,
) -> Result<Value, String> {
    human_call(
        &state,
        "hook_signature_set",
        map_args([
            ("address", decimal(parse_addr(&address)?)),
            ("signature", Value::String(signature)),
            ("signature_source", Value::String(signature_source)),
            ("signature_confidence", Value::String(signature_confidence)),
        ]),
    )
}

#[tauri::command]
fn cmd_hook_signature_remove(
    state: State<'_, Arc<AppState>>,
    address: String,
) -> Result<Value, String> {
    human_call(
        &state,
        "hook_signature_remove",
        map_args([("address", decimal(parse_addr(&address)?))]),
    )
}

#[tauri::command]
fn cmd_hook_remove(state: State<'_, Arc<AppState>>, address: String) -> Result<Value, String> {
    human_call(
        &state,
        "hook_remove",
        map_args([("address", decimal(parse_addr(&address)?))]),
    )
}

#[tauri::command]
fn cmd_hook_clear(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    human_call(&state, "hook_clear", Map::new())
}

#[tauri::command]
fn cmd_hook_inventory(
    state: State<'_, Arc<AppState>>,
    offset: u64,
    limit: u64,
    kind: Option<String>,
) -> Result<Value, String> {
    let mut args = map_args([
        ("offset", Value::String(offset.to_string())),
        ("limit", Value::String(limit.to_string())),
    ]);
    if let Some(kind) = kind {
        args.insert("kind".into(), Value::String(kind));
    }
    system_call(&state, "hook_inventory", args)
}

#[tauri::command]
fn cmd_hook_list(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    system_call(&state, "hook_list", Map::new())
}

#[tauri::command]
fn cmd_hook_monitor(
    state: State<'_, Arc<AppState>>,
    limit: u64,
    before: Option<String>,
) -> Result<Value, String> {
    let mut args = map_args([("limit", decimal(limit.clamp(1, 4096)))]);
    if let Some(before) = before.filter(|value| value != "0" && !value.is_empty()) {
        args.insert("before".into(), Value::String(before));
    }
    system_call(&state, "hook_monitor", args)
}

#[tauri::command]
fn cmd_hook_events_query(state: State<'_, Arc<AppState>>, query: Value) -> Result<Value, String> {
    let args = query
        .as_object()
        .cloned()
        .ok_or_else(|| "Hook event query must be an object".to_string())?;
    system_call(&state, "hook_events_query", args)
}

#[tauri::command]
fn cmd_hook_events_export(state: State<'_, Arc<AppState>>, query: Value) -> Result<Value, String> {
    let args = query
        .as_object()
        .cloned()
        .ok_or_else(|| "Hook event export query must be an object".to_string())?;
    system_call(&state, "hook_events_export", args)
}

#[tauri::command]
async fn cmd_trace_scope_query(
    state: State<'_, Arc<AppState>>,
    query: Value,
) -> Result<Value, String> {
    let args = query
        .as_object()
        .cloned()
        .ok_or_else(|| "Trace scope query must be an object".to_string())?;
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || system_call(&state, "trace_scope_query", args))
        .await
        .map_err(|error| format!("Trace scope worker failed: {error}"))?
}

#[tauri::command]
async fn cmd_trace_record_start(
    state: State<'_, Arc<AppState>>,
    query: Value,
) -> Result<Value, String> {
    let args = query
        .as_object()
        .cloned()
        .ok_or_else(|| "Trace start request must be an object".to_string())?;
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || human_call(&state, "trace_record_start", args))
        .await
        .map_err(|error| format!("Trace start worker failed: {error}"))?
}

#[tauri::command]
async fn cmd_trace_record_status(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        system_call(&state, "trace_record_status", Map::new())
    })
    .await
    .map_err(|error| format!("Trace status worker failed: {error}"))?
}

#[tauri::command]
async fn cmd_trace_record_stop(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        human_call(&state, "trace_record_stop", Map::new())
    })
    .await
    .map_err(|error| format!("Trace stop worker failed: {error}"))?
}

#[tauri::command]
async fn cmd_trace_index_query(
    state: State<'_, Arc<AppState>>,
    query: Value,
) -> Result<Value, String> {
    let args = query
        .as_object()
        .cloned()
        .ok_or_else(|| "Trace index query must be an object".to_string())?;
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || system_call(&state, "trace_index_query", args))
        .await
        .map_err(|error| format!("Trace query worker failed: {error}"))?
}

#[tauri::command]
async fn cmd_trace_index_export(
    state: State<'_, Arc<AppState>>,
    query: Value,
) -> Result<Value, String> {
    let args = query
        .as_object()
        .cloned()
        .ok_or_else(|| "Trace index export must be an object".to_string())?;
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || system_call(&state, "trace_index_export", args))
        .await
        .map_err(|error| format!("Trace export worker failed: {error}"))?
}

#[tauri::command]
fn cmd_hook_module(state: State<'_, Arc<AppState>>, module: String) -> Result<Value, String> {
    human_call(
        &state,
        "hook_module",
        map_args([("module", Value::String(module))]),
    )
}

#[tauri::command]
fn cmd_hook_range_preview(
    state: State<'_, Arc<AppState>>,
    start: String,
    end: String,
    kinds: Vec<String>,
) -> Result<Value, String> {
    system_call(
        &state,
        "hook_range_preview",
        map_args([
            ("start", decimal(parse_addr(&start)?)),
            ("end", decimal(parse_addr(&end)?)),
            (
                "kinds",
                Value::Array(kinds.into_iter().map(Value::String).collect()),
            ),
        ]),
    )
}

#[tauri::command]
fn cmd_hook_range_set(
    state: State<'_, Arc<AppState>>,
    start: String,
    end: String,
    kinds: Vec<String>,
) -> Result<Value, String> {
    human_call(
        &state,
        "hook_range_set",
        map_args([
            ("start", decimal(parse_addr(&start)?)),
            ("end", decimal(parse_addr(&end)?)),
            (
                "kinds",
                Value::Array(kinds.into_iter().map(Value::String).collect()),
            ),
        ]),
    )
}

#[tauri::command]
fn cmd_syscall_config_get(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    system_call(&state, "syscall_config_get", Map::new())
}

#[tauri::command]
fn cmd_syscall_config_set(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
    numbers: Vec<String>,
    scope: String,
    module: String,
    rva_begin: String,
    rva_end: String,
) -> Result<Value, String> {
    human_call(
        &state,
        "syscall_config_set",
        map_args([
            ("enabled", Value::Bool(enabled)),
            (
                "numbers",
                Value::Array(numbers.into_iter().map(Value::String).collect()),
            ),
            ("scope", Value::String(scope)),
            ("module", Value::String(module)),
            ("rva_begin", Value::String(rva_begin)),
            ("rva_end", Value::String(rva_end)),
        ]),
    )
}

#[tauri::command]
fn cmd_syscall_monitor(state: State<'_, Arc<AppState>>, limit: u64) -> Result<Value, String> {
    system_call(
        &state,
        "syscall_monitor",
        map_args([("limit", decimal(limit.clamp(1, 512)))]),
    )
}

#[tauri::command]
fn cmd_memory_map(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    human_call(&state, "memory_map", Map::new())
}

#[tauri::command]
fn cmd_exception_monitor(state: State<'_, Arc<AppState>>, limit: u64) -> Result<Value, String> {
    system_call(
        &state,
        "exception_monitor",
        map_args([("limit", decimal(limit.clamp(1, 1024)))]),
    )
}

#[tauri::command]
fn cmd_exception_policy_get(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    system_call(&state, "exception_policy_get", Map::new())
}

#[tauri::command]
fn cmd_exception_policy_set(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
    code: String,
) -> Result<Value, String> {
    let code = parse_addr(&code)?;
    let code = u32::try_from(code).map_err(|_| "exception code exceeds 32 bits".to_string())?;
    human_call(
        &state,
        "exception_policy_set",
        map_args([
            ("enabled", Value::Bool(enabled)),
            ("code", decimal(code as u64)),
        ]),
    )
}

#[tauri::command]
fn cmd_exception_inventory(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    system_call(&state, "exception_inventory", Map::new())
}

#[tauri::command]
fn cmd_threads(state: State<'_, Arc<AppState>>) -> Result<Vec<u32>, String> {
    let value = human_call(&state, "threads_list", Map::new())?;
    value
        .get("threads")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("invalid threads response"))?
        .iter()
        .map(|x| {
            x.as_str()
                .ok_or_else(|| String::from("thread id is not decimal"))
                .and_then(|s| {
                    s.parse()
                        .map_err(|_| String::from("thread id out of range"))
                })
        })
        .collect()
}

#[tauri::command]
fn cmd_context(state: State<'_, Arc<AppState>>, tid: u32) -> Result<Value, String> {
    let value = human_call(
        &state,
        "registers_get",
        map_args([("thread_id", decimal(tid as u64))]),
    )?;
    Ok(Value::Array(value.get("registers").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().map(|r| json!({ "reg": decimal_or(&r, "id", 0), "value": r.get("value").cloned().unwrap_or_else(|| json!("0x0")) })).collect()))
}

#[tauri::command]
fn cmd_setreg(
    state: State<'_, Arc<AppState>>,
    tid: u32,
    reg: u32,
    value: String,
) -> Result<(), String> {
    human_call(
        &state,
        "register_set",
        map_args([
            ("thread_id", decimal(tid as u64)),
            ("register", decimal(reg as u64)),
            ("value", decimal(parse_addr(&value)?)),
        ]),
    )?;
    Ok(())
}

#[tauri::command]
fn cmd_read_mem(
    state: State<'_, Arc<AppState>>,
    address: String,
    size: u64,
) -> Result<String, String> {
    let value = human_call(
        &state,
        "memory_read",
        map_args([
            ("address", decimal(parse_addr(&address)?)),
            ("size", decimal(size)),
        ]),
    )?;
    Ok(string_field(&value, "data_hex")?.to_owned())
}

#[tauri::command]
fn cmd_write_mem(
    state: State<'_, Arc<AppState>>,
    address: String,
    data: String,
) -> Result<u64, String> {
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err("hex string needs an even length".into());
    }
    let value = human_call(
        &state,
        "memory_write",
        map_args([
            ("address", decimal(parse_addr(&address)?)),
            ("data_hex", Value::String(cleaned)),
        ]),
    )?;
    decimal_field(&value, "written")
}

fn instruction_rows(value: Value) -> Result<Value, String> {
    Ok(Value::Array(value.get("instructions").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().map(|r| json!({
        "address": r.get("address").cloned().unwrap_or_else(|| json!("0x0")), "bytes": r.get("bytes_hex").cloned().unwrap_or_else(|| json!("")), "kind": decimal_or(&r, "kind", 0), "text": r.get("text").cloned().unwrap_or_else(|| json!("")), "target": r.get("target").cloned().unwrap_or_else(|| json!("0x0")),
    })).collect()))
}

#[tauri::command]
fn cmd_disasm(
    state: State<'_, Arc<AppState>>,
    address: String,
    count: u64,
) -> Result<Value, String> {
    instruction_rows(human_call(
        &state,
        "disassemble",
        map_args([
            ("address", decimal(parse_addr(&address)?)),
            ("count", decimal(count)),
        ]),
    )?)
}

#[tauri::command]
fn cmd_disasm_up(
    state: State<'_, Arc<AppState>>,
    address: String,
    count: u64,
) -> Result<Value, String> {
    let target = parse_addr(&address)?;
    let count = count.clamp(1, 512);
    let base = target.saturating_sub(count * 15);
    for shift in 0..15u64 {
        let value = human_call(
            &state,
            "disassemble",
            map_args([
                ("address", decimal(base + shift)),
                ("count", decimal(count + 16)),
            ]),
        )?;
        let rows = value
            .get("instructions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(pos) = rows.iter().position(|r| {
            r.get("address")
                .and_then(Value::as_str)
                .and_then(|s| parse_addr(s).ok())
                == Some(target)
        }) {
            let start = pos.saturating_sub(count as usize);
            return instruction_rows(json!({ "instructions": rows[start..pos].to_vec() }));
        }
    }
    cmd_disasm(state, format!("0x{base:x}"), count)
}

#[tauri::command]
fn cmd_resolve(state: State<'_, Arc<AppState>>, addresses: Vec<String>) -> Result<Value, String> {
    let parsed = addresses
        .iter()
        .map(|s| parse_addr(s).map(decimal))
        .collect::<Result<Vec<_>, _>>()?;
    let value = human_call(
        &state,
        "address_resolve",
        map_args([("addresses", Value::Array(parsed))]),
    )?;
    Ok(Value::Array(
        value
            .get("resolutions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.get("display").cloned().unwrap_or_else(|| json!("<null>")))
            .collect(),
    ))
}

#[tauri::command]
fn control_status(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let mut value = system_call(&state, "control_status", Map::new())?;
    if let Value::Object(ref mut status) = value {
        status.insert(
            "ai_adapter_available".into(),
            Value::Bool(state.ai_adapter_available),
        );
    }
    Ok(value)
}

#[tauri::command]
fn control_handoff_to_ai(
    state: State<'_, Arc<AppState>>,
    mode: Option<String>,
) -> Result<Value, String> {
    if !state.ai_adapter_available {
        return Err(
            "AI control adapter unavailable; configure both Hub IPC secrets before handoff".into(),
        );
    }
    let mode = mode.unwrap_or_else(|| "ai_autonomous".into());
    human_call(
        &state,
        "control_handoff_to_ai",
        map_args([("mode", Value::String(mode))]),
    )
}

#[tauri::command]
fn control_takeover_manual(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    human_call(&state, "control_takeover_manual", Map::new())
}

#[tauri::command]
fn session_status(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    system_call(&state, "session_status", Map::new())
}

#[tauri::command]
fn activity_list(state: State<'_, Arc<AppState>>, limit: String) -> Result<Value, String> {
    human_call(
        &state,
        "activity_list",
        map_args([("limit", Value::String(limit))]),
    )
}

#[tauri::command]
fn activity_get(state: State<'_, Arc<AppState>>, operation_id: String) -> Result<Value, String> {
    human_call(
        &state,
        "activity_get",
        map_args([("operation_id", Value::String(operation_id))]),
    )
}

fn snapshot_json(agent: &Value, bp: &Value, rate: u64, events: &Value) -> Value {
    let abi = agent
        .get("abi")
        .cloned()
        .unwrap_or_else(|| json!({"major":"0","minor":"0"}));
    let bps = bp.get("breakpoints").cloned().unwrap_or_else(|| json!([]));
    json!({
        "connected": true, "abi": [decimal_string_or(&abi, "major", 0), decimal_string_or(&abi, "minor", 0)], "pid": decimal_string_or(agent, "pid", 0),
        "total": decimal_string_or(agent, "total_events", 0), "dropped": decimal_string_or(agent, "dropped_events", 0), "capacity": decimal_string_or(agent, "ring_capacity", 0), "kinds": decimal_strings(agent, "kind_counts"), "rate": decimal(rate),
        "stopped": bp.get("stopped").and_then(Value::as_bool).unwrap_or(false), "hit_tid": decimal_string_or(bp, "hit_tid", u32::MAX as u64), "hit_addr": bp.get("hit_addr").cloned().unwrap_or_else(|| json!("0x0")), "stop_gen": decimal_string_or(bp, "stop_gen", 0), "bps": bps, "events": events.get("events").cloned().unwrap_or_else(|| json!([])),
    })
}

fn poller(handle: tauri::AppHandle, state: Arc<AppState>) {
    let mut last: Option<(u64, Instant)> = None;
    loop {
        let result = (|| {
            let session = system_call(&state, "session_status", Map::new())?;
            let agent = session
                .get("agent")
                .ok_or_else(|| "session response missing agent".to_string())?;
            let bp = normalize_bp(&system_call(&state, "breakpoint_list", Map::new())?);
            let events = system_call(
                &state,
                "events_newest",
                map_args([("limit", Value::String("24".into()))]),
            )?;
            let total = decimal_or(agent, "total_events", 0);
            let now = Instant::now();
            let rate = last
                .map(|(old, at)| {
                    if total >= old {
                        ((total - old) as f64 / now.duration_since(at).as_secs_f64()) as u64
                    } else {
                        0
                    }
                })
                .unwrap_or(0);
            last = Some((total, now));
            Ok::<_, String>(snapshot_json(agent, &bp, rate, &events))
        })();
        match result {
            Ok(value) => {
                if handle.emit("snapshot", value).is_err() {
                    return;
                }
            }
            Err(_) => {
                let _ = handle.emit("snapshot", json!({"connected":false}));
            }
        }
        std::thread::sleep(POLL_PERIOD);
    }
}

fn configured_ipc_secrets(
    human: Option<String>,
    ai: Option<String>,
) -> Result<Option<(String, String)>, String> {
    match (human, ai) {
        (Some(human), Some(ai)) => validate_secrets(&human, &ai)
            .map(|()| Some((human, ai)))
            .map_err(|_| "Hub IPC secrets are invalid".to_string()),
        _ => Ok(None),
    }
}

fn ipc_response(
    hub: &HubService<AgentConnection>,
    caller: Caller,
    request: IpcRequest,
) -> IpcResponse {
    match hub.call(caller, &request.method, &request.params) {
        Ok(result) => {
            let operation_id = result
                .get("operation_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            IpcResponse {
                id: request.id,
                ok: true,
                result: Some(result),
                error: None,
                operation_id,
            }
        }
        Err(error) => IpcResponse {
            id: request.id,
            ok: false,
            result: None,
            error: Some(error.to_string()),
            operation_id: error.operation_id().map(str::to_owned),
        },
    }
}

fn main() {
    let config = parse_args();
    let ipc_secrets = match configured_ipc_secrets(
        std::env::var("PINBRIDGE_HUB_HUMAN_SECRET").ok(),
        std::env::var("PINBRIDGE_HUB_AI_SECRET").ok(),
    ) {
        Ok(Some(secrets)) => Some(secrets),
        Ok(None) => {
            eprintln!(
                "pinbridge-ui: embedded Hub IPC disabled; set both PINBRIDGE_HUB_HUMAN_SECRET and PINBRIDGE_HUB_AI_SECRET"
            );
            None
        }
        Err(error) => {
            eprintln!("pinbridge-ui: embedded Hub IPC disabled: {error}");
            None
        }
    };
    let backend: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let initial_target = (!config.target.is_empty()).then(|| config.target.join(" "));
    let mut port = config.port;
    if !config.target.is_empty() {
        let options = launch::LaunchOptions {
            pin: config.pin.clone(),
            agent: config.agent.clone(),
            arch: None,
            port: 0,
            entry_bp: true,
            probe_mode: false,
            show_target_console: true,
        };
        match launch::launch_for_target(&options, &config.target, STARTUP_TIMEOUT) {
            Ok((child, p)) => {
                port = p;
                *backend.lock().unwrap() = Some(child);
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        }
    }
    let hub = Arc::new(HubService::new(AgentConnection::new(port)));
    hub.set_target(initial_target.clone());
    if !config.target.is_empty() {
        if let Err(e) = hub.call(
            Caller::TRUSTED_HUMAN,
            "session_set_agent_port",
            &map_args([("agent_port", decimal(port as u64))]),
        ) {
            if let Some(child) = backend.lock().unwrap().as_mut() {
                launch::kill_backend(child);
            }
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    let state = Arc::new(AppState {
        port: Mutex::new(port),
        pin: config.pin,
        agent: config.agent,
        hub,
        ai_adapter_available: ipc_secrets.is_some(),
        ipc: Mutex::new(None),
        backend: Arc::clone(&backend),
        target: Mutex::new(initial_target),
        released: Mutex::new(false),
    });
    let poller_state = Arc::clone(&state);
    let setup_state = Arc::clone(&state);
    let exit_state = Arc::clone(&state);
    let listen_port = config.hub_listen;
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            if let Some((human_secret, ai_secret)) = ipc_secrets {
                let listener = TcpListener::bind(("127.0.0.1", listen_port)).map_err(|error| {
                    std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!(
                            "embedded Hub IPC listener 127.0.0.1:{listen_port} failed: {error}"
                        ),
                    )
                })?;
                let hub = setup_state.hub.clone();
                let server =
                    spawn_listener(listener, human_secret, ai_secret, move |caller, request| {
                        ipc_response(&hub, caller, request)
                    })
                    .map_err(|error| {
                        std::io::Error::other(format!("embedded Hub IPC startup failed: {error}"))
                    })?;
                *setup_state
                    .ipc
                    .lock()
                    .map_err(|_| std::io::Error::other("embedded Hub IPC state lock poisoned"))? =
                    Some(server);
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || poller(handle, poller_state));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd_session,
            cmd_launch,
            cmd_attach_agent,
            cmd_environment,
            cmd_kill_backend,
            cmd_release_session,
            cmd_control,
            cmd_step,
            cmd_bp_set,
            cmd_bp_remove,
            cmd_bp_list,
            cmd_breakpoint_inventory,
            cmd_script_get,
            cmd_script_list,
            cmd_script_inject,
            cmd_script_replace,
            cmd_script_start,
            cmd_script_stop,
            cmd_script_remove,
            cmd_script_output,
            cmd_modules,
            cmd_module_exports,
            cmd_hook_set,
            cmd_hook_function_set,
            cmd_hook_signature_set,
            cmd_hook_signature_remove,
            cmd_hook_remove,
            cmd_hook_clear,
            cmd_hook_list,
            cmd_hook_inventory,
            cmd_hook_monitor,
            cmd_hook_events_query,
            cmd_hook_events_export,
            cmd_trace_scope_query,
            cmd_trace_record_start,
            cmd_trace_record_status,
            cmd_trace_record_stop,
            cmd_trace_index_query,
            cmd_trace_index_export,
            cmd_hook_module,
            cmd_hook_range_preview,
            cmd_hook_range_set,
            cmd_syscall_config_get,
            cmd_syscall_config_set,
            cmd_syscall_monitor,
            cmd_memory_map,
            cmd_exception_monitor,
            cmd_exception_policy_get,
            cmd_exception_policy_set,
            cmd_exception_inventory,
            cmd_threads,
            cmd_context,
            cmd_setreg,
            cmd_read_mem,
            cmd_write_mem,
            cmd_disasm,
            cmd_disasm_up,
            cmd_resolve,
            control_status,
            control_handoff_to_ai,
            control_takeover_manual,
            session_status,
            activity_list,
            activity_get,
        ])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(move |_handle, event| {
            if let RunEvent::Exit = event {
                if let Some(server) = exit_state.ipc.lock().unwrap().take() {
                    server.stop();
                }
                let released = exit_state
                    .released
                    .lock()
                    .map(|value| *value)
                    .unwrap_or(false);
                if !released {
                    if let Some(child) = backend.lock().unwrap().as_mut() {
                        launch::kill_backend(child);
                    }
                } else {
                    // Dropping a Windows Child handle does not terminate the
                    // process. Do not call kill_backend after explicit release.
                    backend.lock().unwrap().take();
                }
            }
        }),
        Err(error) => {
            if let Some(child) = backend.lock().unwrap().as_mut() {
                launch::kill_backend(child);
            }
            eprintln!("pinbridge-ui: Tauri startup failed (embedded Hub listener was not started): {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinbridge_hub_core::ipc::{IpcClient, IpcHello};
    use std::net::TcpListener;

    #[test]
    fn ui_addresses_cross_hub_as_decimal_strings() {
        assert_eq!(parse_addr("0x1234").unwrap(), 0x1234);
        assert_eq!(decimal(parse_addr("0x1234").unwrap()), json!("4660"));
    }

    #[test]
    fn agent_attach_endpoint_is_loopback_only() {
        assert_eq!(parse_agent_endpoint("127.0.0.1:9011").unwrap(), 9011);
        assert_eq!(parse_agent_endpoint("tcp://localhost:9123").unwrap(), 9123);
        assert_eq!(parse_agent_endpoint("[::1]:9444").unwrap(), 9444);
        assert!(parse_agent_endpoint("0.0.0.0:9011").is_err());
        assert!(parse_agent_endpoint("example.com:9011").is_err());
        assert!(parse_agent_endpoint("127.0.0.1:0").is_err());
    }

    #[test]
    fn breakpoint_remove_accepts_decimal_string_ids() {
        assert_eq!(parse_bp_id("2").unwrap(), 2);
        assert_eq!(parse_bp_id(" 4294967295 ").unwrap(), u32::MAX);
        assert!(parse_bp_id("0x2").is_err());
        assert!(parse_bp_id("not-an-id").is_err());
    }

    #[test]
    fn missing_ipc_secrets_keep_manual_adapter_available() {
        assert!(configured_ipc_secrets(None, None).unwrap().is_none());
        assert!(
            configured_ipc_secrets(Some("human-secret-123456".into()), None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn invalid_ipc_secrets_disable_only_the_ai_adapter() {
        assert!(
            configured_ipc_secrets(Some("short".into()), Some("another-short".into())).is_err()
        );
        let configured = configured_ipc_secrets(
            Some("human-secret-123456".into()),
            Some("ai-secret-12345678".into()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(configured.0, "human-secret-123456");
        assert_eq!(configured.1, "ai-secret-12345678");
    }

    #[test]
    fn breakpoint_contract_is_normalized_without_numberifying_addresses() {
        let raw = json!({"stopped":true,"hit_thread_id":"7","hit_address":"0xffffffffffffffff","stop_generation":"9","breakpoints":[{"id":"2","address":"0x100000000","hits":"3"}]});
        let value = normalize_bp(&raw);
        assert_eq!(value["hit_addr"], "0xffffffffffffffff");
        assert_eq!(value["breakpoints"][0]["address"], "0x100000000");
        assert_eq!(value["hit_tid"], "7");
        assert_eq!(value["stop_gen"], "9");
        assert_eq!(value["breakpoints"][0]["id"], "2");
        assert_eq!(value["breakpoints"][0]["hits"], "3");
    }

    #[test]
    fn snapshot_keeps_counters_and_events_as_decimal_text() {
        let agent = json!({
            "pid":"18446744073709551615",
            "total_events":"9007199254740993",
            "dropped_events":"7",
            "ring_capacity":"24",
            "kind_counts":["1","2"]
        });
        let bp =
            normalize_bp(&json!({"hit_thread_id":"99","stop_generation":"12","breakpoints":[]}));
        let events = json!({"next":"9007199254740994","events":[{
            "sequence":"9007199254740993","kind":"1","thread_id":"99",
            "address":"0xffffffffffffffff","arg0":"0x1"
        }]});
        let value = snapshot_json(&agent, &bp, 3, &events);
        assert_eq!(value["pid"], "18446744073709551615");
        assert_eq!(value["total"], "9007199254740993");
        assert_eq!(value["rate"], "3");
        assert_eq!(value["stop_gen"], "12");
        assert_eq!(value["events"][0]["sequence"], "9007199254740993");
        assert_eq!(value["events"][0]["address"], "0xffffffffffffffff");
    }

    #[test]
    fn disassembly_contract_keeps_target_as_text() {
        let rows = instruction_rows(json!({"instructions":[{"address":"0x100000000","bytes_hex":"90","kind":"0","text":"nop","target":"0xffffffffffffffff"}]})).unwrap();
        assert_eq!(rows[0]["address"], "0x100000000");
        assert_eq!(rows[0]["target"], "0xffffffffffffffff");
    }

    #[test]
    fn embedded_ipc_observes_the_same_hub_control_and_journal() {
        let hub = Arc::new(HubService::new(AgentConnection::new(1)));
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = listener.local_addr().unwrap();
        let handler_hub = hub.clone();
        let server = spawn_listener(
            listener,
            "human-secret-123456".into(),
            "ai-secret-123456".into(),
            move |caller, request| ipc_response(&handler_hub, caller, request),
        )
        .unwrap();

        hub.call(
            Caller::TRUSTED_HUMAN,
            "control_handoff_to_ai",
            &map_args([("mode", json!("ai_autonomous"))]),
        )
        .unwrap();
        let successful = ipc_response(
            &hub,
            Caller::TRUSTED_HUMAN,
            IpcRequest {
                id: json!(0),
                method: "control_handoff_to_ai".into(),
                params: map_args([("mode", json!("ai_assist"))]),
            },
        );
        assert!(successful
            .operation_id
            .as_deref()
            .is_some_and(|id| id.starts_with("op-")));
        hub.call(
            Caller::TRUSTED_HUMAN,
            "control_handoff_to_ai",
            &map_args([("mode", json!("ai_autonomous"))]),
        )
        .unwrap();
        let journal_before_poll = hub.journal.list(100).len();
        for _ in 0..3 {
            hub.call(Caller::SYSTEM, "control_status", &Map::new())
                .unwrap();
            hub.call(Caller::SYSTEM, "session_status", &Map::new())
                .unwrap_err();
            hub.call(Caller::SYSTEM, "breakpoint_list", &Map::new())
                .unwrap_err();
        }
        assert_eq!(hub.journal.list(100).len(), journal_before_poll);
        let mut client = IpcClient::connect(
            endpoint,
            IpcHello {
                channel: "ai".into(),
                secret: "ai-secret-123456".into(),
            },
        )
        .unwrap();
        let status = client
            .call(IpcRequest {
                id: json!(1),
                method: "control_status".into(),
                params: Map::new(),
            })
            .unwrap();
        assert_eq!(status.result.unwrap()["mode"], "ai_autonomous");
        let mut activity_client = IpcClient::connect(
            endpoint,
            IpcHello {
                channel: "ai".into(),
                secret: "ai-secret-123456".into(),
            },
        )
        .unwrap();
        let activities = activity_client
            .call(IpcRequest {
                id: json!(2),
                method: "activity_list".into(),
                params: map_args([("limit", json!("100"))]),
            })
            .unwrap();
        assert!(activities.result.unwrap()["activities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["action"] == "control_handoff_to_ai"));
        let denied = ipc_response(
            &hub,
            Caller::AI,
            IpcRequest {
                id: json!(3),
                method: "control_handoff_to_ai".into(),
                params: map_args([("mode", json!("ai_assist"))]),
            },
        );
        assert!(!denied.ok);
        assert!(denied
            .operation_id
            .as_deref()
            .is_some_and(|id| id.starts_with("op-")));
        server.stop();
    }
}
