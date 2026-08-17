//! Backend lifecycle: the TUI can spawn `pin.exe -t pinbridge_agent.dll --
//! <target>` itself, wait for the query port, and reap the child on exit.
//!
//! Architecture selection is PE-driven: `auto` reads the target's DOS/COFF/
//! optional headers (`arch::detect_pe_arch`) and picks the `ia32` or
//! `intel64` Pin runtime; `x86`/`x64` override it explicitly. File names are
//! never consulted, and a missing arch-specific agent/runtime/bridge is a
//! hard, descriptive error — no i686 support is ever faked.

use crate::arch::{detect_pe_arch, Arch};
use std::io::{Error, ErrorKind, Result as IoResult};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

#[cfg(windows)]
mod target_process {
    use core::ffi::c_void;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    const PROCESS_TERMINATE: u32 = 0x0001;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    static HANDLES: OnceLock<Mutex<HashMap<u32, usize>>> = OnceLock::new();

    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> *mut c_void;
        fn TerminateProcess(process: *mut c_void, exit_code: u32) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    pub fn remember(backend_pid: u32, target_pid: u32) {
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, target_pid) };
        if handle.is_null() {
            return;
        }
        let mut handles = HANDLES
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(previous) = handles.insert(backend_pid, handle as usize) {
            unsafe {
                CloseHandle(previous as *mut c_void);
            }
        }
    }

    pub fn terminate(backend_pid: u32) {
        let Some(handles) = HANDLES.get() else {
            return;
        };
        let handle = handles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&backend_pid);
        let Some(handle) = handle else {
            return;
        };
        unsafe {
            let handle = handle as *mut c_void;
            let _ = TerminateProcess(handle, 1);
            let _ = WaitForSingleObject(handle, 5_000);
            let _ = CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
mod target_process {
    pub fn remember(_backend_pid: u32, _target_pid: u32) {}
    pub fn terminate(_backend_pid: u32) {}
}

pub struct BackendConfig {
    pub pin_exe: PathBuf,
    pub agent_dll: PathBuf,
    pub arch: Arch,
    pub port: u16,
    /// Plant a one-shot breakpoint on the main module's entry point before
    /// the first instruction runs (debugger-style "break at entry").
    pub entry_bp: bool,
    /// Run the target natively with Pin probes instead of JIT translation.
    /// This is the compatibility mode for protectors which depend on native
    /// exception/single-step semantics.
    pub probe_mode: bool,
}

/// The resolved backend for one launch: which architecture, which Pin
/// runtime, and which agent DLL. Produced by [`resolve_backend`].
#[derive(Debug)]
pub struct ResolvedBackend {
    pub arch: Arch,
    pub pin_exe: PathBuf,
    pub agent_dll: PathBuf,
}

/// Launch-time facts recorded for a session: architecture, pointer width and
/// the ABI reported by the agent's PING (if it answered before the caller
/// moved on). Wire/PBTR remain backward compatible — these are additive.
#[derive(Copy, Clone, Debug)]
pub struct LaunchMetadata {
    pub arch: Arch,
    pub pointer_width: u32,
    pub abi: Option<(u32, u32)>,
}

/// Resolves pin.exe from --pin, $PIN_EXE, $PIN_ROOT, then auto-discovery
/// (walk up from the executable looking for `*/runtime/pin/<runtime>/pin.exe`
/// or `*/pin/<runtime>/bin/pin.exe` kits). `arch` selects the `ia32` or
/// `intel64` kit directory.
pub fn resolve_pin(flag: Option<&str>, arch: Arch) -> IoResult<PathBuf> {
    if let Some(value) = flag {
        return Ok(PathBuf::from(value));
    }
    if let Ok(value) = std::env::var("PIN_EXE") {
        return Ok(PathBuf::from(value));
    }
    if let Ok(root) = std::env::var("PIN_ROOT") {
        return Ok(PathBuf::from(root)
            .join(arch.runtime_dir())
            .join("bin")
            .join("pin.exe"));
    }
    if let Some(found) = discover_pin(arch) {
        return Ok(found);
    }
    Err(Error::new(
        ErrorKind::NotFound,
        format!(
            "pin.exe for arch {} not specified: pass --pin <path>, or set PIN_EXE / PIN_ROOT \
             (expects {}/bin/pin.exe)",
            arch.as_str(),
            arch.runtime_dir()
        ),
    ))
}

/// Upwards search from the current exe: at each ancestor directory look for
/// `<dir>/runtime/pin/<runtime>/bin/pin.exe`, `<dir>/pin/<runtime>/bin/pin.exe`
/// and one level of children (`<dir>/<child>/runtime/pin/...`). Finds a Pin
/// kit sitting anywhere alongside the app without hardcoded paths.
fn discover_pin(arch: Arch) -> Option<PathBuf> {
    let runtime = arch.runtime_dir();
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();
    for _ in 0..8 {
        let candidates = [
            dir.join("runtime").join("pin").join(runtime).join("bin").join("pin.exe"),
            dir.join("pin").join(runtime).join("bin").join("pin.exe"),
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
                    .join(runtime)
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

/// Default agent DLL for an architecture, next to the launcher executable.
/// The intel64 agent keeps its historical name; the ia32 agent lives in an
/// `ia32/` subdirectory (its own `pinbridge.dll` cannot share a directory
/// with the intel64 one). Override with `--agent`.
pub fn default_agent_dll(arch: Arch) -> IoResult<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "exe has no parent dir"))?;
    match arch {
        Arch::X64 => Ok(dir.join("pinbridge_agent.dll")),
        Arch::X86 => Ok(dir.join("ia32").join("pinbridge_agent.dll")),
    }
}

/// Verifies the three files an arch-specific backend needs and reports which
/// one is missing, always naming the architecture so an ia32 request can never
/// silently degrade into the intel64 kit (or vice versa).
fn validate_backend_files(arch: Arch, pin_exe: &Path, agent_dll: &Path) -> Result<(), String> {
    if !pin_exe.exists() {
        return Err(format!(
            "Pin runtime for arch {} is missing: {} (expected a {}/bin/pin.exe kit; \
             architecture is never guessed from the file name)",
            arch.as_str(),
            pin_exe.display(),
            arch.runtime_dir()
        ));
    }
    if !agent_dll.exists() {
        return Err(format!(
            "pinbridge agent for arch {} is missing: {} (no {} agent build is present; \
             i686 support is not faked)",
            arch.as_str(),
            agent_dll.display(),
            arch.as_str()
        ));
    }
    let agent_dir = agent_dll
        .parent()
        .ok_or_else(|| "agent DLL has no parent dir".to_string())?;
    if !agent_dir.join("pinbridge.dll").exists() {
        return Err(format!(
            "pinbridge.dll for arch {} is missing next to {} (build build\\pin\\{}\\Release\\pinbridge.dll)",
            arch.as_str(),
            agent_dll.display(),
            arch.runtime_dir()
        ));
    }
    Ok(())
}

/// Resolves the architecture for a launch: an explicit `--arch x86|x64` wins,
/// otherwise the first target argument's PE headers decide. An unreadable or
/// non-PE target is an error, never a filename-based guess.
pub fn resolve_target_arch(explicit: Option<Arch>, target: &[String]) -> Result<Arch, String> {
    if let Some(arch) = explicit {
        return Ok(arch);
    }
    let exe = target
        .first()
        .ok_or_else(|| "no target executable to detect architecture for".to_string())?;
    detect_pe_arch(Path::new(exe))
}

/// Full resolution chain (architecture → Pin runtime → agent DLL) with
/// existence checks. Pure and testable: no process is spawned.
pub fn resolve_backend(
    options: &LaunchOptions,
    target: &[String],
) -> Result<ResolvedBackend, String> {
    let arch = resolve_target_arch(options.arch, target)?;
    let pin_exe = resolve_pin(options.pin.as_deref(), arch).map_err(|e| e.to_string())?;
    let agent_dll = match &options.agent {
        Some(path) => PathBuf::from(path),
        None => default_agent_dll(arch).map_err(|e| e.to_string())?,
    };
    validate_backend_files(arch, &pin_exe, &agent_dll)?;
    Ok(ResolvedBackend {
        arch,
        pin_exe,
        agent_dll,
    })
}

pub fn spawn_backend(config: &BackendConfig, target: &[String]) -> IoResult<Child> {
    validate_backend_files(config.arch, &config.pin_exe, &config.agent_dll)
        .map_err(|message| Error::new(ErrorKind::NotFound, message))?;
    // The agent DLL's directory must also contain pinbridge.dll (its import);
    // use it as the child working directory so Windows finds it.
    let agent_dir = config
        .agent_dll
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "agent DLL has no parent dir"))?;

    let mut command = Command::new(&config.pin_exe);
    configure_execution_mode(&mut command, config.probe_mode);
    command
        .arg("-t")
        .arg(&config.agent_dll)
        .arg("--")
        .args(target)
        .current_dir(agent_dir)
        .env("PINBRIDGE_AGENT_PORT", config.port.to_string());
    if config.entry_bp && !config.probe_mode {
        command.env("PINBRIDGE_ENTRY_BP", "1");
    } else {
        // An explicit raw-run request must not inherit a debugger setting
        // from the parent process.
        command.env_remove("PINBRIDGE_ENTRY_BP");
    }
    command.spawn().map_err(|error| {
        Error::new(
            error.kind(),
            format!("failed to spawn {}: {error}", config.pin_exe.display()),
        )
    })
}

fn configure_execution_mode(command: &mut Command, probe_mode: bool) {
    if probe_mode {
        command.arg("-probe").arg("1");
    }
}

/// Polls until the agent's query port accepts connections (the agent binds it
/// from a Pin internal thread shortly after the tool starts).
pub fn wait_for_port(port: u16, timeout: Duration) -> IoResult<()> {
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

/// Wait for the actual PE-entry stop. A listening query port only proves
/// that the control plane is alive; the application may still be starting.
pub fn wait_for_entry_stop(port: u16, timeout: Duration) -> IoResult<(u32, u64)> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(mut client) = crate::client::Client::connect(port) {
            if let Ok((stopped, tid, address, expected)) = client.entry_stop_status() {
                let at_entry = expected
                    .map(|entry| entry != 0 && address == entry)
                    .unwrap_or(address != 0);
                if stopped && at_entry {
                    return Ok((tid, address));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(Error::new(
        ErrorKind::TimedOut,
        format!("target did not stop at its PE entry within {}s", timeout.as_secs()),
    ))
}

/// Best-effort teardown. Probe-mode Pin can exit without taking its natively
/// running target with it, so terminate the exact process handle captured
/// from the authenticated agent PING before reaping the Pin launcher.
pub fn kill_backend(child: &mut Child) {
    target_process::terminate(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

pub struct LaunchOptions {
    pub pin: Option<String>,
    pub agent: Option<String>,
    /// Explicit architecture override; `None` means auto-detect from the
    /// target's PE headers.
    pub arch: Option<Arch>,
    pub port: u16,
    /// Stop at the main module entry point on launch.
    pub entry_bp: bool,
    /// Prefer target compatibility over instruction-level JIT features.
    /// Probe mode implicitly disables the entry breakpoint.
    pub probe_mode: bool,
}

/// Full launch chain for a target: resolve pin, spawn backend, wait for the
/// query port. `options.port == 0` picks a free port automatically (multiple
/// concurrent sessions would otherwise collide on the default port).
/// Returns the owned child (reap with kill_backend) and the port in use.
pub fn launch_for_target(
    options: &LaunchOptions,
    target: &[String],
    timeout: Duration,
) -> IoResult<(Child, u16)> {
    launch_for_target_full(options, target, timeout).map(|(child, port, _meta)| (child, port))
}

/// [`launch_for_target`] plus the launch metadata (arch / pointer width /
/// ABI) for callers that want to record or display it.
pub fn launch_for_target_full(
    options: &LaunchOptions,
    target: &[String],
    timeout: Duration,
) -> IoResult<(Child, u16, LaunchMetadata)> {
    let resolved = resolve_backend(options, target).map_err(Error::other)?;
    let port = if options.port == 0 {
        pick_free_port()?
    } else {
        options.port
    };
    let backend = BackendConfig {
        pin_exe: resolved.pin_exe,
        agent_dll: resolved.agent_dll,
        arch: resolved.arch,
        port,
        entry_bp: options.entry_bp,
        probe_mode: options.probe_mode,
    };
    let mut child = spawn_backend(&backend, target)?;
    if let Err(error) = wait_for_port(port, timeout) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    // Best-effort ABI read before the caller takes over the control plane.
    let ping = crate::client::Client::connect(port)
        .and_then(|mut client| client.ping())
        .ok();
    let abi = ping.map(|(major, minor, _pid, _total)| (major, minor));
    if options.probe_mode {
        if let Some((_major, _minor, target_pid, _total)) = ping {
            target_process::remember(child.id(), target_pid);
        }
    }
    if options.entry_bp && !options.probe_mode {
        if let Err(error) = wait_for_entry_stop(port, timeout) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    }
    let metadata = LaunchMetadata {
        arch: resolved.arch,
        pointer_width: resolved.arch.pointer_width(),
        abi,
    };
    Ok((child, port, metadata))
}

/// Binds port 0 and hands back the OS-assigned free port.
fn pick_free_port() -> IoResult<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> LaunchOptions {
        LaunchOptions {
            pin: None,
            agent: None,
            arch: None,
            port: 9001,
            entry_bp: true,
            probe_mode: false,
        }
    }

    #[test]
    fn probe_mode_is_explicit_and_disables_entry_wait() {
        let mut opts = options();
        opts.probe_mode = true;
        assert!(opts.probe_mode);
        assert!(opts.entry_bp);
        assert!(!(opts.entry_bp && !opts.probe_mode));

        let mut command = Command::new("pin.exe");
        configure_execution_mode(&mut command, true);
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(arguments, ["-probe", "1"]);
    }

    #[test]
    fn resolve_target_arch_prefers_explicit() {
        // An explicit override must win even over a non-existent target.
        assert_eq!(
            resolve_target_arch(Some(Arch::X86), &[]).unwrap(),
            Arch::X86
        );
        assert_eq!(
            resolve_target_arch(Some(Arch::X64), &["nonexistent.exe".into()]).unwrap(),
            Arch::X64
        );
    }

    #[test]
    fn resolve_target_arch_requires_target_in_auto() {
        let err = resolve_target_arch(None, &[]).unwrap_err();
        assert!(err.contains("no target"), "unexpected error: {err}");
    }

    #[test]
    fn resolve_target_arch_errors_on_non_pe() {
        let err = resolve_target_arch(None, &["Cargo.toml".into()]).unwrap_err();
        assert!(err.contains("MZ") || err.contains("read"), "unexpected error: {err}");
    }

    #[test]
    fn resolve_backend_reports_missing_x86_agent() {
        // No ia32 agent/pinbridge is staged in this repo, so an explicit x86
        // request must fail with a descriptive error, not silently fall back.
        let mut opts = options();
        opts.arch = Some(Arch::X86);
        let err = resolve_backend(&opts, &["target.exe".into()]).unwrap_err();
        assert!(err.contains("x86"), "unexpected error: {err}");
    }

    #[test]
    fn default_agent_dll_uses_ia32_subdir_for_x86() {
        // The current exe dir varies per test binary; assert only the tail.
        let path = default_agent_dll(Arch::X86).unwrap();
        let text = path.to_string_lossy().replace('\\', "/");
        assert!(text.ends_with("ia32/pinbridge_agent.dll"), "unexpected: {text}");
        let path64 = default_agent_dll(Arch::X64).unwrap();
        let text64 = path64.to_string_lossy().replace('\\', "/");
        assert!(text64.ends_with("pinbridge_agent.dll"), "unexpected: {text64}");
    }
}
