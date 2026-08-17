use serde::Serialize;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, Serialize)]
pub struct SessionStatus {
    pub connected: bool,
    pub agent_port: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}
#[derive(Clone)]
pub struct Session {
    inner: Arc<RwLock<SessionStatus>>,
}
impl Session {
    pub fn new(port: u16) -> Self {
        Self {
            inner: Arc::new(RwLock::new(SessionStatus {
                connected: false,
                agent_port: port.to_string(),
                pid: None,
                target: None,
            })),
        }
    }
    pub fn status(&self) -> SessionStatus {
        self.inner.read().expect("session poisoned").clone()
    }
    pub fn set_port(&self, port: u16) {
        self.inner.write().expect("session poisoned").agent_port = port.to_string();
    }
    pub fn set_connected(&self, yes: bool) {
        self.inner.write().expect("session poisoned").connected = yes;
    }
    pub fn set_target(&self, target: Option<String>) {
        self.inner.write().expect("session poisoned").target = target;
    }
}
