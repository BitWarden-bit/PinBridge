use crate::control::{Caller, ChannelActor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize = MAX_FRAME_BYTES;
pub const MIN_SECRET_BYTES: usize = 16;
pub const MAX_SECRET_BYTES: usize = 4096;
pub const MAX_CONNECTIONS: usize = 32;
#[derive(Debug)]
pub enum IpcError {
    Io(io::Error),
    FrameTooLarge,
    InvalidFrame(String),
    Unauthorized,
}
impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::FrameTooLarge => write!(f, "frame exceeds limit"),
            Self::InvalidFrame(e) => write!(f, "invalid frame: {e}"),
            Self::Unauthorized => write!(f, "unauthorized"),
        }
    }
}
impl From<io::Error> for IpcError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IpcHello {
    pub channel: String,
    pub secret: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IpcRequest {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Map<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IpcResponse {
    pub id: Value,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}
pub fn authenticate(
    hello: &IpcHello,
    human_secret: &str,
    ai_secret: &str,
) -> Result<Caller, IpcError> {
    if hello.channel == "human" && constant_eq(&hello.secret, human_secret) {
        Ok(Caller {
            actor: ChannelActor::Human,
            trusted: true,
        })
    } else if hello.channel == "ai" && constant_eq(&hello.secret, ai_secret) {
        Ok(Caller {
            actor: ChannelActor::Ai,
            trusted: false,
        })
    } else {
        Err(IpcError::Unauthorized)
    }
}
pub fn validate_secrets(human: &str, ai: &str) -> Result<(), IpcError> {
    if human.len() < MIN_SECRET_BYTES
        || ai.len() < MIN_SECRET_BYTES
        || human.len() > MAX_SECRET_BYTES
        || ai.len() > MAX_SECRET_BYTES
        || constant_eq(human, ai)
    {
        return Err(IpcError::Unauthorized);
    }
    Ok(())
}
fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.as_bytes()
            .iter()
            .zip(b.as_bytes())
            .fold(0u8, |d, (x, y)| d | (x ^ y))
            == 0
}
pub fn read_frame<R: Read>(r: &mut R) -> Result<Value, IpcError> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge);
    }
    let mut b = vec![0; n];
    r.read_exact(&mut b)?;
    serde_json::from_slice(&b).map_err(|e| IpcError::InvalidFrame(e.to_string()))
}
pub fn write_frame<W: Write>(w: &mut W, value: &Value) -> Result<(), IpcError> {
    let b = serde_json::to_vec(value).map_err(|e| IpcError::InvalidFrame(e.to_string()))?;
    if b.len() > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge);
    }
    w.write_all(&(b.len() as u32).to_le_bytes())?;
    w.write_all(&b)?;
    w.flush()?;
    Ok(())
}
pub struct IpcClient {
    stream: TcpStream,
}
impl IpcClient {
    pub fn connect<A: ToSocketAddrs>(addr: A, hello: IpcHello) -> Result<Self, IpcError> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        write_frame(&mut stream, &serde_json::to_value(hello).unwrap())?;
        Ok(Self { stream })
    }
    pub fn call(&mut self, request: IpcRequest) -> Result<IpcResponse, IpcError> {
        let request_id = request.id.clone();
        write_frame(&mut self.stream, &serde_json::to_value(request).unwrap())?;
        let v = read_frame(&mut self.stream)?;
        let response: IpcResponse =
            serde_json::from_value(v).map_err(|e| IpcError::InvalidFrame(e.to_string()))?;
        if response.id != request_id {
            return Err(IpcError::InvalidFrame("response id mismatch".into()));
        }
        Ok(response)
    }
}

/// Reusable loopback server for both the headless binary and embedded Tauri.
/// Each connection has a bounded worker and idle read/write timeouts, so one
/// adapter cannot block the other. Agent operations remain globally serial in
/// `AgentConnection`.
pub struct IpcServerHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}
impl IpcServerHandle {
    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
    pub fn stop(mut self) {
        self.shutdown();
    }
}
impl Drop for IpcServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}
pub fn spawn_listener<F>(
    listener: TcpListener,
    human_secret: String,
    ai_secret: String,
    handler: F,
) -> Result<IpcServerHandle, IpcError>
where
    F: Fn(Caller, IpcRequest) -> IpcResponse + Send + Sync + 'static,
{
    validate_secrets(&human_secret, &ai_secret)?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let handler = Arc::new(handler);
    let join = thread::spawn(move || {
        let active = Arc::new(AtomicUsize::new(0));
        while !stop_thread.load(Ordering::Acquire) {
            let stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(_) => break,
            };
            // The listening socket is nonblocking so the accept loop can
            // observe shutdown. On Windows accepted sockets inherit that
            // mode; workers use a request/response loop and must block while
            // waiting for the next frame instead of treating WouldBlock as a
            // disconnected client.
            if stream.set_nonblocking(false).is_err() {
                drop(stream);
                continue;
            }
            if active.load(Ordering::Acquire) >= MAX_CONNECTIONS {
                drop(stream);
                continue;
            }
            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
            active.fetch_add(1, Ordering::AcqRel);
            let active_worker = active.clone();
            let handler_worker = handler.clone();
            let human = human_secret.clone();
            let ai = ai_secret.clone();
            thread::spawn(move || {
                serve_connection(stream, &human, &ai, handler_worker);
                active_worker.fetch_sub(1, Ordering::AcqRel);
            });
        }
    });
    Ok(IpcServerHandle {
        stop,
        join: Some(join),
    })
}
fn serve_connection<F>(mut stream: TcpStream, human: &str, ai: &str, handler: Arc<F>)
where
    F: Fn(Caller, IpcRequest) -> IpcResponse + Send + Sync + 'static,
{
    let hello = match read_frame(&mut stream).and_then(|v| {
        serde_json::from_value::<IpcHello>(v).map_err(|e| IpcError::InvalidFrame(e.to_string()))
    }) {
        Ok(v) => v,
        Err(_) => return,
    };
    let caller = match authenticate(&hello, human, ai) {
        Ok(v) => v,
        Err(_) => return,
    };
    loop {
        let request = match read_frame(&mut stream).and_then(|v| {
            serde_json::from_value::<IpcRequest>(v)
                .map_err(|e| IpcError::InvalidFrame(e.to_string()))
        }) {
            Ok(v) => v,
            Err(_) => return,
        };
        let response = handler(caller, request);
        if write_frame(
            &mut stream,
            &serde_json::to_value(response).unwrap_or(Value::Null),
        )
        .is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auth_binds_channel_to_secret() {
        let h = IpcHello {
            channel: "human".into(),
            secret: "h".into(),
        };
        assert!(authenticate(&h, "h", "a").unwrap().trusted);
        let forged = IpcHello {
            channel: "human".into(),
            secret: "a".into(),
        };
        assert!(authenticate(&forged, "h", "a").is_err());
    }
    #[test]
    fn frame_limit() {
        let mut v = Vec::new();
        v.extend_from_slice(&(MAX_FRAME_BYTES as u32 + 1).to_le_bytes());
        assert!(matches!(
            read_frame(&mut std::io::Cursor::new(v)),
            Err(IpcError::FrameTooLarge)
        ));
    }
    #[test]
    fn weak_or_equal_secrets_are_rejected() {
        assert!(validate_secrets("short", "another").is_err());
        assert!(validate_secrets("0123456789abcdef", "0123456789abcdef").is_err());
        assert!(validate_secrets("0123456789abcdef", "fedcba9876543210").is_ok());
        assert!(validate_secrets(&"x".repeat(4097), "0123456789abcdef").is_err());
    }
    #[test]
    fn idle_connection_does_not_block_second_client() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = spawn_listener(
            listener,
            "human-secret-012345".into(),
            "ai-secret-01234567".into(),
            |_caller, request| IpcResponse {
                id: request.id,
                ok: true,
                result: Some(serde_json::json!({"ok":true})),
                error: None,
                operation_id: None,
            },
        )
        .unwrap();
        let _idle = IpcClient::connect(
            address,
            IpcHello {
                channel: "ai".into(),
                secret: "ai-secret-01234567".into(),
            },
        )
        .unwrap();
        let mut active = IpcClient::connect(
            address,
            IpcHello {
                channel: "ai".into(),
                secret: "ai-secret-01234567".into(),
            },
        )
        .unwrap();
        let response = active
            .call(IpcRequest {
                id: serde_json::json!(1),
                method: "ping".into(),
                params: Map::new(),
            })
            .unwrap();
        assert!(response.ok);
        drop(active);
        drop(_idle);
        drop(handle);
    }

    #[test]
    fn client_rejects_mismatched_response_id() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = spawn_listener(
            listener,
            "human-secret-123456".into(),
            "ai-secret-123456".into(),
            |_caller, _request| IpcResponse {
                id: Value::String("wrong-id".into()),
                ok: true,
                result: Some(serde_json::json!({})),
                error: None,
                operation_id: None,
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
        assert!(matches!(
            client.call(IpcRequest {
                id: Value::String("request-id".into()),
                method: "ping".into(),
                params: Map::new(),
            }),
            Err(IpcError::InvalidFrame(message)) if message == "response id mismatch"
        ));
        handle.stop();
    }

    #[test]
    fn persistent_connection_handles_three_calls() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = spawn_listener(
            listener,
            "human-secret-123456".into(),
            "ai-secret-123456".into(),
            |_caller, request| IpcResponse {
                id: request.id,
                ok: true,
                result: Some(serde_json::json!({"method":"ok"})),
                error: None,
                operation_id: Some("op-0000000000000001".into()),
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
        for id in 1..=3 {
            let response = client
                .call(IpcRequest {
                    id: serde_json::json!(id),
                    method: "control_status".into(),
                    params: Map::new(),
                })
                .unwrap();
            assert!(response.ok);
            assert_eq!(response.id, serde_json::json!(id));
        }
        handle.stop();
    }
}
