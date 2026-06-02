fn main() {
    // Rerun this script only when it changes.
    println!("cargo:rerun-if-changed=build.rs");

    // Declare the `ffi` feature so `cfg(feature = "ffi")` does not warn
    // in downstream crates or CI.
    println!("cargo:rustc-check-cfg=cfg(feature, values(\"ffi\"))");

    // Note: Rust 2024 Edition requires rustc >= 1.85.0.
    // The workspace rust-version is set to 1.91.1 in Cargo.toml.
    // Any toolchain older than the declared `rust-version` will produce
    // a clear Cargo error at build time, so no manual check is required.
}
