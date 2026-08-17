//! MCP control plane for an already-running pinbridge agent.
//!
//! This crate deliberately contains no target-launching code and no copy of
//! the agent wire protocol.  `pinbridge-client` remains the sole gateway to
//! the binary protocol.

pub mod hub;
pub mod server;
pub mod tools;
pub mod transport;

pub use hub::{HubClient, HubError, HubResult};
pub use server::Server;
