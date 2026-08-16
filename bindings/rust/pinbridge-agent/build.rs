use std::path::{Path, PathBuf};

fn main() {
    pinbridge_tool::emit_link_flags();
    stage_bridge_dll();
    // The embedded CPython import must be invisible to Pin's tool loader
    // (same proven trick as WS2_32): delay-load hides it from the loader's
    // import walk; the scripting thread preloads python310.dll with the real
    // OS loader before the first pyo3 call, so the delay-load then resolves
    // to the already-loaded module — no CRT delay-load hook needed.
    // Gated on the `scripting` feature: a native-only build must not link or
    // stage Python at all.
    if std::env::var("CARGO_FEATURE_SCRIPTING").is_ok() {
        println!("cargo:rustc-link-arg=/DELAYLOAD:python310.dll");
        stage_python_distribution();
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
/// fallback). If an embeddable distribution also supplies python310.zip,
/// stage the standard library beside it. The agent runs fine without either
/// file (scripting reports "python unavailable"), so copy failures warn.
#[allow(dead_code)] // unreachable in the no-`scripting` build
fn stage_python_distribution() {
    println!("cargo:rerun-if-env-changed=PINBRIDGE_PYTHON_DIST");
    println!("cargo:rerun-if-env-changed=PYTHON_SYS_EXECUTABLE");
    let source = match locate_python_dll() {
        Some(source) => source,
        None => {
            println!(
                "cargo:warning=matching {} python310.dll not found; scripting will be unavailable in this build's output dir",
                target_arch_name()
            );
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
    if newer(&source, &dest) {
        if let Err(error) = std::fs::copy(&source, &dest) {
            println!("cargo:warning=failed to stage python310.dll: {error}");
        }
    }
    let Some(source_dir) = source.parent() else {
        return;
    };
    let source_zip = source_dir.join("python310.zip");
    if source_zip.exists() {
        let dest_zip = profile_dir.join("python310.zip");
        println!("cargo:rerun-if-changed={}", source_zip.display());
        if newer(&source_zip, &dest_zip) {
            if let Err(error) = std::fs::copy(&source_zip, &dest_zip) {
                println!("cargo:warning=failed to stage python310.zip: {error}");
            }
        }
    }
}

/// Finds the python310.dll of a CPython 3.10 install:
///   1. in $PINBRIDGE_PYTHON_DIST,
///   2. next to $PYTHON_SYS_EXECUTABLE when set,
///   3. next to the first architecture-matching `python` on PATH,
///   4. standard per-user install dirs.
/// Every candidate's PE Machine field must match the Rust target; this keeps
/// an x86 build from silently copying the host's 64-bit Python DLL.
#[allow(dead_code)] // only reachable from stage_python_distribution
fn locate_python_dll() -> Option<PathBuf> {
    let beside_exe = |exe: &str| -> Option<PathBuf> {
        let dll = PathBuf::from(exe).parent()?.join("python310.dll");
        python_dll_matches_target(&dll).then_some(dll)
    };
    if let Ok(dir) = std::env::var("PINBRIDGE_PYTHON_DIST") {
        let dll = PathBuf::from(dir.trim()).join("python310.dll");
        if python_dll_matches_target(&dll) {
            return Some(dll);
        }
        println!(
            "cargo:warning=PINBRIDGE_PYTHON_DIST has no {} python310.dll: {}",
            target_arch_name(),
            dll.display()
        );
    }
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
        let base = PathBuf::from(local).join("Programs").join("Python");
        let installs: &[&str] = if expected_pe_machine() == Some(0x014c) {
            &["Python310-32", "Python310"]
        } else {
            &["Python310", "Python310-64"]
        };
        for install in installs {
            let dll = base.join(install).join("python310.dll");
            if python_dll_matches_target(&dll) {
                return Some(dll);
            }
        }
    }
    None
}

#[allow(dead_code)]
fn target_arch_name() -> &'static str {
    if expected_pe_machine() == Some(0x014c) {
        "x86"
    } else {
        "x64"
    }
}

#[allow(dead_code)]
fn expected_pe_machine() -> Option<u16> {
    match std::env::var("CARGO_CFG_TARGET_ARCH").ok()?.as_str() {
        "x86" => Some(0x014c),
        "x86_64" => Some(0x8664),
        _ => None,
    }
}

#[allow(dead_code)]
fn python_dll_matches_target(path: &Path) -> bool {
    let Some(expected) = expected_pe_machine() else {
        return path.exists();
    };
    pe_machine(path) == Some(expected)
}

#[allow(dead_code)]
fn pe_machine(path: &Path) -> Option<u16> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 0x40 || &bytes[..2] != b"MZ" {
        return None;
    }
    let pe_offset = u32::from_le_bytes(bytes.get(0x3c..0x40)?.try_into().ok()?) as usize;
    if bytes.get(pe_offset..pe_offset.checked_add(6)?)?.get(..4)? != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes(
        bytes.get(pe_offset + 4..pe_offset + 6)?.try_into().ok()?,
    ))
}
