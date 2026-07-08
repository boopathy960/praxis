use std::env;
use std::path::PathBuf;

/// Point the linker at our AArch64 layout with an absolute path, so the build
/// works regardless of the directory cargo invokes the linker from.
fn main() {
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let script = dir.join("linker.ld");
    println!("cargo:rustc-link-arg-bins=-T{}", script.display());
    println!("cargo:rerun-if-changed=linker.ld");
}
