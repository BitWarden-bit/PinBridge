use std::env;
use std::path::PathBuf;

fn main() {
    // Directory containing pinbridge.lib (produced by Build-Pin.ps1).
    // Override with PINBRIDGE_LIB_DIR when linking against a non-default layout.
    let dir = env::var("PINBRIDGE_LIB_DIR").unwrap_or_else(|_| {
        // Build-Pin.ps1 lays out the bridge as build/pin/<arch>/<config>/,
        // where ia32 = x86 targets and x64 = x86_64 (kept as the default).
        let arch = match env::var("CARGO_CFG_TARGET_ARCH").ok().as_deref() {
            Some("x86") => "ia32",
            _ => "x64",
        };
        // Cargo's PROFILE is lowercase (debug/release); the bridge layout uses
        // the MSBuild-style Debug/Release capitalization.
        let config = match env::var("PROFILE").ok().as_deref() {
            Some("debug") => "Debug",
            _ => "Release",
        };
        let mut path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        path.push("..");
        path.push("..");
        path.push("..");
        path.push("build");
        path.push("pin");
        path.push(arch);
        path.push(config);
        path.to_string_lossy().into_owned()
    });
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=dylib=pinbridge");
    println!("cargo:rerun-if-env-changed=PINBRIDGE_LIB_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");
    println!("cargo:rerun-if-env-changed=PROFILE");
}
