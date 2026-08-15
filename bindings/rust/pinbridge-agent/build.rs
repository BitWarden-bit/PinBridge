use std::path::{Path, PathBuf};

fn main() {
    pinbridge_tool::emit_link_flags();
    stage_bridge_dll();
    // The embedded CPython import must be invisible to Pin's tool loader
    // (same proven trick as WS2_32): delay-load hides it from the loader's
    // import walk; the scripting thread preloads python310.dll with the real
    // OS loader before the first pyo3 call, so the delay-load then resolves
    // to the already-loaded module — no CRT delay-load hook needed.
    // Gated on the `scripting` feature: the x86 no-Python build must not
    // link or stage Python at all.
    if std::env::var("CARGO_FEATURE_SCRIPTING").is_ok() {
        println!("cargo:rustc-link-arg=/DELAYLOAD:python310.dll");
        stage_python_dll();
    }
}

/// The agent cdylib imports pinbridge.dll (the C++ ABI bridge), and the
/// Windows loader resolves it next to the agent DLL at Pin load time. Cargo
/// never learns about that dependency, so a fresh/stale target dir misses it
/// ("pinbridge.dll missing next to ...pinbridge_agent.dll"). Copy the MSVC
/// output for the Rust target architecture next to the agent whenever it is
/// newer; prefer the Release bridge, fall back to Debug.
fn stage_bridge_dll() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = match manifest.ancestors().nth(3) {
        Some(p) => p.to_path_buf(), // pinbridge-agent -> rust -> bindings -> repo root
        None => return,
    };
    let pin_arch = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86") {
        "ia32"
    } else {
        // Preserve the existing x64 behavior for x86_64 and unknown hosts.
        "x64"
    };
    let candidates = [
        root.join(format!(r"build\pin\{pin_arch}\Release\pinbridge.dll")),
        root.join(format!(r"build\pin\{pin_arch}\Debug\pinbridge.dll")),
    ];
    let source = match candidates.iter().find(|p| p.exists()) {
        Some(p) => p,
        None => return, // bridge not built yet; launch.rs reports it clearly
    };
    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out -> target/<profile>
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile_dir = match out_dir.ancestors().nth(3) {
        Some(p) => p.to_path_buf(),
        None => return,
    };
    let dest = profile_dir.join("pinbridge.dll");
    println!("cargo:rerun-if-changed={}", source.display());
    if !newer(source, &dest) {
        return;
    }
    if let Err(error) = std::fs::copy(source, &dest) {
        // Never fail the build for staging; the launcher validates presence.
        println!("cargo:warning=failed to stage pinbridge.dll: {error}");
    }
}

fn newer(source: &Path, dest: &Path) -> bool {
    let at = source.metadata().and_then(|m| m.modified()).ok();
    let bt = dest.metadata().and_then(|m| m.modified()).ok();
    match (at, bt) {
        (Some(a), Some(b)) => a > b,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Deployment for the embedded scripting host: python310.dll sits next to
/// the agent DLL (the scripting thread preloads it from there, PATH as
/// fallback). Stage a copy into the profile dir like the bridge DLL. The
/// agent runs fine without it (scripting reports "python unavailable"), so
/// every failure here is a warning, never a build error.
#[allow(dead_code)] // unreachable in the no-`scripting` build
fn stage_python_dll() {
    let source = match locate_python_dll() {
        Some(source) => source,
        None => {
            println!("cargo:warning=python310.dll not found on this machine; scripting will be unavailable in this build's output dir");
            return;
        }
    };
    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out -> target/<profile>
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile_dir = match out_dir.ancestors().nth(3) {
        Some(p) => p.to_path_buf(),
        None => return,
    };
    let dest = profile_dir.join("python310.dll");
    println!("cargo:rerun-if-changed={}", source.display());
    if !newer(&source, &dest) {
        return;
    }
    if let Err(error) = std::fs::copy(&source, &dest) {
        println!("cargo:warning=failed to stage python310.dll: {error}");
    }
}

/// Finds the python310.dll of a CPython 3.10 install:
///   1. next to $PYTHON_SYS_EXECUTABLE when set,
///   2. next to the first `python` on PATH (`where python`),
///   3. the standard per-user install dir.
#[allow(dead_code)] // only reachable from stage_python_dll
fn locate_python_dll() -> Option<PathBuf> {
    let beside_exe = |exe: &str| -> Option<PathBuf> {
        let dll = PathBuf::from(exe).parent()?.join("python310.dll");
        dll.exists().then_some(dll)
    };
    if let Ok(exe) = std::env::var("PYTHON_SYS_EXECUTABLE") {
        if let Some(dll) = beside_exe(exe.trim()) {
            return Some(dll);
        }
    }
    if let Ok(output) = std::process::Command::new("where").arg("python").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if let Some(dll) = beside_exe(line.trim()) {
                    return Some(dll);
                }
            }
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let dll = PathBuf::from(local)
            .join("Programs")
            .join("Python")
            .join("Python310")
            .join("python310.dll");
        if dll.exists() {
            return Some(dll);
        }
    }
    None
}
