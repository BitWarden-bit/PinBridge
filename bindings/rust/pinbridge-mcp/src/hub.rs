//! Stable boundary between the MCP adapter and the shared control hub.
//! The concrete IPC implementation belongs to `pinbridge-hub`; this crate
//! never opens an Agent connection or owns target/session state.

use serde_json::Value;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

// The Hub closes an IPC worker after 30 seconds without a request. Reopen a
// quiet connection before that boundary so the next MCP tool call does not
// have to fail once merely to discover that its socket has expired.
const IPC_RECONNECT_IDLE: Duration = Duration::from_secs(25);

#[derive(Clone, Debug)]
pub struct HubResult {
    pub value: Value,
    pub operation_id: Option<String>,
}

#[derive(Clone, Debug)]
pub enum HubError {
    Unavailable(String),
    Execution {
        message: String,
        operation_id: Option<String>,
    },
}

pub trait HubClient: Send + Sync {
    fn call(&self, tool: &str, arguments: &Value) -> Result<HubResult, HubError>;
}

pub struct UnavailableHub {
    pub endpoint: String,
    pub credential_configured: bool,
}

impl HubClient for UnavailableHub {
    fn call(&self, _tool: &str, _arguments: &Value) -> Result<HubResult, HubError> {
        Err(HubError::Unavailable(format!(
            "Hub unavailable at {}",
            self.endpoint
        )))
    }
}

impl<T: HubClient + ?Sized> HubClient for Arc<T> {
    fn call(&self, tool: &str, arguments: &Value) -> Result<HubResult, HubError> {
        (**self).call(tool, arguments)
    }
}

/// Client for the Hub's length-delimited local IPC. The AI secret is held
/// only in memory and is sent solely in the authentication hello frame.
pub struct IpcHubClient {
    endpoint: Vec<SocketAddr>,
    credential: String,
    connection: Mutex<ConnectionState>,
    next_request_id: AtomicU64,
}

struct ConnectionState {
    client: Option<pinbridge_hub_core::ipc::IpcClient>,
    last_activity: Option<Instant>,
}

impl IpcHubClient {
    pub fn new(endpoint: String, credential: String) -> Result<Self, String> {
        let endpoint = resolve_loopback_endpoint(&endpoint)?;
        if credential.len() < 16 {
            return Err("PINBRIDGE_HUB_AI_SECRET is too short".into());
        }
        if credential.len() > 4096 {
            return Err("PINBRIDGE_HUB_AI_SECRET is too long".into());
        }
        Ok(Self {
            endpoint,
            credential,
            connection: Mutex::new(ConnectionState {
                client: None,
                last_activity: None,
            }),
            next_request_id: AtomicU64::new(1),
        })
    }
}

fn resolve_loopback_endpoint(endpoint: &str) -> Result<Vec<SocketAddr>, String> {
    if endpoint.trim().is_empty() {
        return Err("Hub endpoint must not be empty".into());
    }
    let addresses = endpoint
        .to_socket_addrs()
        .map_err(|_| "Hub endpoint must resolve to loopback".to_string())?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| !address.ip().is_loopback()) {
        return Err("Hub endpoint must resolve only to loopback addresses".into());
    }
    Ok(addresses)
}

impl HubClient for IpcHubClient {
    fn call(&self, tool: &str, arguments: &Value) -> Result<HubResult, HubError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| HubError::Unavailable("Hub connection lock poisoned".into()))?;
        if connection
            .last_activity
            .is_some_and(|last| last.elapsed() >= IPC_RECONNECT_IDLE)
        {
            connection.client = None;
            connection.last_activity = None;
        }
        if connection.client.is_none() {
            let client = pinbridge_hub_core::ipc::IpcClient::connect(
                self.endpoint.as_slice(),
                pinbridge_hub_core::ipc::IpcHello {
                    channel: "ai".into(),
                    secret: self.credential.clone(),
                },
            )
            .map_err(|e| HubError::Unavailable(format!("Hub unavailable: {e}")))?;
            connection.client = Some(client);
        }
        let params = arguments.as_object().cloned().unwrap_or_default();
        let request_id = format!(
            "mcp-{}",
            self.next_request_id.fetch_add(1, Ordering::Relaxed)
        );
        let request = pinbridge_hub_core::ipc::IpcRequest {
            id: Value::String(request_id.clone()),
            method: tool.into(),
            params,
        };
        let response = match connection
            .client
            .as_mut()
            .expect("connection initialized")
            .call(request)
        {
            Ok(response) => {
                connection.last_activity = Some(Instant::now());
                response
            }
            Err(error) => {
                connection.client = None;
                connection.last_activity = None;
                return Err(HubError::Unavailable(format!("Hub unavailable: {error}")));
            }
        };
        if response.id != Value::String(request_id) {
            connection.client = None;
            connection.last_activity = None;
            return Err(HubError::Unavailable("Hub response id mismatch".into()));
        }
        let response_operation_id = response.operation_id;
        if !response.ok {
            return Err(HubError::Execution {
                message: response
                    .error
                    .unwrap_or_else(|| "Hub operation failed".into()),
                operation_id: response_operation_id,
            });
        }
        let value = response.result.unwrap_or(Value::Null);
        let operation_id = response_operation_id.or_else(|| {
            value
                .get("operation_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        Ok(HubResult {
            value,
            operation_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinbridge_hub_core::ipc::{
        read_frame, spawn_listener, write_frame, IpcHello, IpcRequest, IpcResponse,
    };
    use serde_json::json;
    use std::net::TcpListener;

    #[test]
    fn credentials_are_nonempty_and_bounded() {
        assert!(IpcHubClient::new("127.0.0.1:1".into(), "".into()).is_err());
        assert!(IpcHubClient::new("127.0.0.1:1".into(), "x".repeat(4097)).is_err());
    }

    #[test]
    fn endpoint_must_resolve_only_to_loopback() {
        assert!(IpcHubClient::new("192.0.2.1:9444".into(), "x".repeat(16)).is_err());
        assert!(IpcHubClient::new("0.0.0.0:9444".into(), "x".repeat(16)).is_err());
        assert!(IpcHubClient::new("127.0.0.1:9444".into(), "x".repeat(16)).is_ok());
        assert!(IpcHubClient::new("[::1]:9444".into(), "x".repeat(16)).is_ok());
    }

    #[test]
    fn persistent_ipc_ids_operation_ids_and_reconnect() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let hello: IpcHello = serde_json::from_value(read_frame(&mut stream).unwrap()).unwrap();
            assert_eq!(hello.channel, "ai");
            assert_eq!(hello.secret, "ai-secret-123456789");
            for n in 1..=2 {
                let request: IpcRequest =
                    serde_json::from_value(read_frame(&mut stream).unwrap()).unwrap();
                write_frame(
                    &mut stream,
                    &serde_json::to_value(IpcResponse {
                        id: request.id,
                        ok: true,
                        result: Some(json!({"ok":true,"n":n})),
                        error: None,
                        operation_id: Some(format!("op-{n}")),
                    })
                    .unwrap(),
                )
                .unwrap();
            }
        });
        let client = IpcHubClient::new(endpoint.clone(), "ai-secret-123456789".into()).unwrap();
        let first = client.call("session_status", &json!({})).unwrap();
        let second = client.call("session_status", &json!({})).unwrap();
        assert_eq!(first.operation_id.as_deref(), Some("op-1"));
        assert_eq!(second.operation_id.as_deref(), Some("op-2"));
        thread.join().unwrap();
        let unavailable = client.call("session_status", &json!({}));
        assert!(matches!(unavailable, Err(HubError::Unavailable(_))));
        let listener = TcpListener::bind(endpoint.as_str()).unwrap();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let hello: IpcHello = serde_json::from_value(read_frame(&mut stream).unwrap()).unwrap();
            assert_eq!(hello.channel, "ai");
            let request: IpcRequest =
                serde_json::from_value(read_frame(&mut stream).unwrap()).unwrap();
            write_frame(
                &mut stream,
                &serde_json::to_value(IpcResponse {
                    id: request.id,
                    ok: true,
                    result: Some(json!({"ok":true})),
                    error: None,
                    operation_id: Some("op-3".into()),
                })
                .unwrap(),
            )
            .unwrap();
        });
        let reconnected = client.call("session_status", &json!({})).unwrap();
        assert_eq!(reconnected.operation_id.as_deref(), Some("op-3"));
        thread.join().unwrap();
    }

    #[test]
    fn idle_ipc_connection_is_refreshed_before_the_next_request() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let thread = std::thread::spawn(move || {
            for n in 1..=2 {
                let (mut stream, _) = listener.accept().unwrap();
                let hello: IpcHello =
                    serde_json::from_value(read_frame(&mut stream).unwrap()).unwrap();
                assert_eq!(hello.channel, "ai");
                let request: IpcRequest =
                    serde_json::from_value(read_frame(&mut stream).unwrap()).unwrap();
                write_frame(
                    &mut stream,
                    &serde_json::to_value(IpcResponse {
                        id: request.id,
                        ok: true,
                        result: Some(json!({"n":n})),
                        error: None,
                        operation_id: Some(format!("op-{n}")),
                    })
                    .unwrap(),
                )
                .unwrap();
            }
        });
        let client = IpcHubClient::new(endpoint, "ai-secret-123456789".into()).unwrap();
        assert_eq!(
            client.call("session_status", &json!({})).unwrap().value["n"],
            1
        );
        {
            let mut connection = client.connection.lock().unwrap();
            connection.last_activity = Some(Instant::now() - IPC_RECONNECT_IDLE);
        }
        assert_eq!(
            client.call("session_status", &json!({})).unwrap().value["n"],
            2
        );
        thread.join().unwrap();
    }

    #[test]
    fn mismatched_response_id_is_rejected_and_connection_dropped() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let handle = spawn_listener(
            listener,
            "human-secret-123456".into(),
            "ai-secret-123456789".into(),
            |_caller, _request| IpcResponse {
                id: Value::String("wrong-id".into()),
                ok: true,
                result: Some(json!({})),
                error: None,
                operation_id: Some("op-wrong".into()),
            },
        )
        .unwrap();
        let client = IpcHubClient::new(endpoint, "ai-secret-123456789".into()).unwrap();
        assert!(
            matches!(client.call("session_status", &json!({})), Err(HubError::Unavailable(message)) if message.contains("id mismatch"))
        );
        handle.stop();
    }
}
