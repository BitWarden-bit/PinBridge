use crate::server::Server;
use serde_json::Value;
use std::io::{self, BufRead, Write};

pub fn run_stdio(server: &Server) -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let response = serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {error}")}});
                writeln!(stdout, "{}", response)?;
                stdout.flush()?;
                continue;
            }
        };
        if let Some(response) = server.handle(request) {
            writeln!(stdout, "{}", response)?;
            stdout.flush()?;
        }
    }
    Ok(())
}
