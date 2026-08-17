use pinbridge_mcp::{hub::IpcHubClient, transport::run_stdio, Server};
use std::sync::Arc;

fn main() {
    let (endpoint, credential) = parse_config().unwrap_or_else(|error| {
        eprintln!("pinbridge-mcp: {error}");
        std::process::exit(2);
    });
    let hub = IpcHubClient::new(endpoint, credential).unwrap_or_else(|error| {
        eprintln!("pinbridge-mcp: {error}");
        std::process::exit(2);
    });
    if let Err(error) = run_stdio(&Server::new(Arc::new(hub))) {
        eprintln!("pinbridge-mcp: stdio error: {error}");
        std::process::exit(1);
    }
}

fn parse_config() -> Result<(String, String), String> {
    let mut args = std::env::args().skip(1);
    let mut endpoint = None;
    while let Some(arg) = args.next() {
        if arg == "--hub-endpoint" {
            endpoint = Some(args.next().ok_or("--hub-endpoint requires a value")?);
        } else if arg == "--help" {
            println!("Usage: pinbridge-mcp --hub-endpoint ENDPOINT\nUses the shared pinbridge Hub; it never connects to or launches an Agent.");
            std::process::exit(0);
        } else {
            return Err(format!("unknown argument: {arg}"));
        }
    }
    let endpoint = endpoint
        .or_else(|| std::env::var("PINBRIDGE_HUB_ENDPOINT").ok())
        .ok_or_else(|| {
            "explicit --hub-endpoint or PINBRIDGE_HUB_ENDPOINT is required".to_string()
        })?;
    let credential = std::env::var("PINBRIDGE_HUB_AI_SECRET").map_err(|_| {
        "PINBRIDGE_HUB_AI_SECRET is required; credentials are not accepted on the CLI".to_string()
    })?;
    Ok((endpoint, credential))
}
