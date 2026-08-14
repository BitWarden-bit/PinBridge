use std::env;
use std::path::PathBuf;

fn main() {
    // Directory containing pinbridge.lib (produced by Build-Pin.ps1).
    // Override with PINBRIDGE_LIB_DIR when linking against a non-default layout.
    let dir = env::var("PINBRIDGE_LIB_DIR").unwrap_or_else(|_| {
        let mut path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        path.push("..");
        path.push("..");
        path.push("..");
        path.push("build");
        path.push("pin");
        path.push("x64");
        path.push("Release");
        path.to_string_lossy().into_owned()
    });
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=dylib=pinbridge");
    println!("cargo:rerun-if-env-changed=PINBRIDGE_LIB_DIR");
}
