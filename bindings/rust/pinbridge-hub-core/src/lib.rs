//! The single-owner domain for PinBridge desktop and AI adapters.
//!
//! `pinbridge-hub-core` owns policy, journal, scripts, session state, and the
//! serialized Agent transport.  MCP and Tauri are deliberately adapters: a
//! caller identity is supplied by their trusted IPC entrance, never by a
//! JSON argument.

pub mod activities;
pub mod agent;
pub mod control;
pub mod ipc;
pub mod script_service;
pub mod service;
pub mod session;

pub use agent::{AgentApi, AgentConnection, AgentOutputLine, AgentScript};
pub use control::{Caller, ChannelActor, ControlMode, ControlState};
pub use ipc::{IpcError, IpcHello, IpcRequest, IpcResponse, MAX_FRAME_BYTES};
pub use service::{HubError, HubService};
