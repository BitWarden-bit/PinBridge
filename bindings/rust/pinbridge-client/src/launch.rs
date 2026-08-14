//! Backend lifecycle: the TUI can spawn `pin.exe -t pinbridge_agent.dll --
//! <target>` itself, wait for the query port, and reap the child on exit.

use std::io::{Error, ErrorKind, Result};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub struct BackendConfig {
    pub pin_exe: PathBuf,
    pub agent_dll: PathBuf,
    pub port: u16,
    /// Plant a one-shot breakpoint on the main module's entry point before
    /// the first instruction runs (debugger-style "break at entry").
    pub entry_bp: bool,
}

/// Resolves pin.exe from --pin, $PIN_EXE, $PIN_ROOT, then auto-discovery
/// (walk up from the executable looking for */runtime/pin or */pin kits).
pub fn resolve_pin(flag: Option<&str>) -> Result<PathBuf> {
    if let Some(value) = flag {
        return Ok(PathBuf::from(value));
    }
    if let Ok(value) = std::env::var("PIN_EXE") {
        return Ok(PathBuf::from(value));
    }
    if let Ok(root) = std::env::var("PIN_ROOT") {
        return Ok(PathBuf::from(root).join("intel64").join("bin").join("pin.exe"));
    }
    if let Some(found) = discover_pin() {
        return Ok(found);
    }
    Err(Error::new(
        ErrorKind::NotFound,
        "pin.exe not specified: pass --pin <path>, or set PIN_EXE / PIN_ROOT",
    ))
}

/// Upwards search from the current exe: at each ancestor directory look for
/// `<dir>/runtime/pin/intel64/bin/pin.exe`, `<dir>/pin/intel64/bin/pin.exe`
/// and one level of children (`<dir>/<child>/runtime/pin/...`). Finds a Pin
/// kit sitting anywhere alongside the app without hardcoded paths.
fn discover_pin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();
    for _ in 0..8 {
        let candidates = [
            dir.join("runtime").join("pin").join("intel64").join("bin").join("pin.exe"),
            dir.join("pin").join("intel64").join("bin").join("pin.exe"),
        ];
        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate);
            }
        }
        if let Ok(children) = std::fs::read_dir(&dir) {
            for child in children.flatten() {
                let candidate = child
                    .path()
                    .join("runtime")
                    .join("pin")
                    .join("intel64")
                    .join("bin")
                    .join("pin.exe");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
        if !dir.pop() {
            return None;
        }
    }
    None
}

/// Default agent DLL: next to the TUI executable.
pub fn default_agent_dll() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "exe has no parent dir"))?;
    Ok(dir.join("pinbridge_agent.dll"))
}

pub fn spawn_backend(config: &BackendConfig, target: &[String]) -> Result<Child> {
    if !config.agent_dll.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("agent DLL missing: {}", config.agent_dll.display()),
        ));
    }
    // The agent DLL's directory must also contain pinbridge.dll (its import);
    // use it as the child working directory so Windows finds it.
    let agent_dir = config
        .agent_dll
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "agent DLL has no parent dir"))?;
    if !agent_dir.join("pinbridge.dll").exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("pinbridge.dll missing next to {}", config.agent_dll.display()),
        ));
    }

    let mut command = Command::new(&config.pin_exe);
    command
        .arg("-t")
        .arg(&config.agent_dll)
        .arg("--")
        .args(target)
        .current_dir(agent_dir)
        .env("PINBRIDGE_AGENT_PORT", config.port.to_string());
    if config.entry_bp {
        command.env("PINBRIDGE_ENTRY_BP", "1");
    }
    command.spawn().map_err(|error| {
        Error::new(
            error.kind(),
            format!("failed to spawn {}: {error}", config.pin_exe.display()),
        )
    })
}

/// Polls until the agent's query port accepts connections (the agent binds it
/// from a Pin internal thread shortly after the tool starts).
pub fn wait_for_port(port: u16, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(Error::new(
        ErrorKind::TimedOut,
        format!("agent did not open port {port} within {}s", timeout.as_secs()),
    ))
}

/// Best-effort teardown: kill pin, which takes the target with it.
pub fn kill_backend(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub struct LaunchOptions {
    pub pin: Option<String>,
    pub agent: Option<String>,
    pub port: u16,
    /// Stop at the main module entry point on launch.
    pub entry_bp: bool,
}

/// Full launch chain for a target: resolve pin, spawn backend, wait for the
/// query port. `options.port == 0` picks a free port automatically (multiple
/// concurrent sessions would otherwise collide on the default port).
/// Returns the owned child (reap with kill_backend) and the port in use.
pub fn launch_for_target(
    options: &LaunchOptions,
    target: &[String],
    timeout: Duration,
) -> Result<(Child, u16)> {
    let pin_exe = resolve_pin(options.pin.as_deref())?;
    let agent_dll = match &options.agent {
        Some(path) => PathBuf::from(path),
        None => default_agent_dll()?,
    };
    let port = if options.port == 0 {
        pick_free_port()?
    } else {
        options.port
    };
    let backend = BackendConfig {
        pin_exe,
        agent_dll,
        port,
        entry_bp: options.entry_bp,
    };
    let mut child = spawn_backend(&backend, target)?;
    if let Err(error) = wait_for_port(port, timeout) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok((child, port))
}

/// Binds port 0 and hands back the OS-assigned free port.
fn pick_free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

