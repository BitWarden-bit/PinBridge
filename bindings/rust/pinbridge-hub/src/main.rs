use pinbridge_hub_core::{
    agent::AgentConnection,
    ipc::{spawn_listener, IpcRequest, IpcResponse},
    HubService,
};
use std::net::TcpListener;
use std::sync::Arc;

mod shutdown;
use shutdown::{install_console_handler, Shutdown};

fn env_required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        eprintln!("pinbridge-hub: {name} is required");
        std::process::exit(2)
    })
}
fn main() {
    let mut listen = None;
    let mut agent_port = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => listen = args.next().and_then(|v| v.parse().ok()),
            "--agent-port" => agent_port = args.next().and_then(|v| v.parse().ok()),
            "--help" => {
                println!("Usage: pinbridge-hub --agent-port PORT [--listen PORT]");
                return;
            }
            _ => {
                eprintln!("unknown argument: {arg}");
                std::process::exit(2);
            }
        }
    }
    let agent_port = agent_port
        .or_else(|| {
            std::env::var("PINBRIDGE_AGENT_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or_else(|| {
            eprintln!("--agent-port or PINBRIDGE_AGENT_PORT is required");
            std::process::exit(2)
        });
    let listen = listen
        .or_else(|| {
            std::env::var("PINBRIDGE_HUB_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(9444);
    let human = env_required("PINBRIDGE_HUB_HUMAN_SECRET");
    let ai = env_required("PINBRIDGE_HUB_AI_SECRET");
    let shutdown = Shutdown::new();
    let _console_handler = install_console_handler(&shutdown).unwrap_or_else(|error| {
        eprintln!("pinbridge-hub: console shutdown setup failed: {error}");
        std::process::exit(1)
    });
    let service = Arc::new(HubService::new(AgentConnection::new(agent_port)));
    let listener = TcpListener::bind(("127.0.0.1", listen)).unwrap_or_else(|e| {
        eprintln!("pinbridge-hub: listen failed: {e}");
        std::process::exit(1)
    });
    let service_handler = service.clone();
    let server =
        spawn_listener(
            listener,
            human,
            ai,
            move |caller, request: IpcRequest| match service_handler.call(
                caller,
                &request.method,
                &request.params,
            ) {
                Ok(result) => {
                    let operation_id = result
                        .get("operation_id")
                        .and_then(|v| v.as_str())
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
            },
        )
        .unwrap_or_else(|error| {
            eprintln!("pinbridge-hub: IPC setup failed: {error}");
            std::process::exit(1)
        });
    shutdown.wait();
    server.stop();
}
