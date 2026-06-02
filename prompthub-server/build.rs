use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Generate a static openapi.json so the `OPENAPI_SPEC` constant in
    // openapi.rs can embed it at compile time via `include_str!`.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let spec_path = out_dir.join("openapi.json");

    let spec = serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "PromptHub API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Production-ready prompt management for LLM agent swarms"
        },
        "servers": [
            { "url": "http://localhost:8080", "description": "Local development" }
        ],
        "paths": {},
        "components": {
            "schemas": {}
        }
    });

    fs::write(&spec_path, serde_json::to_string_pretty(&spec).unwrap())
        .expect("failed to write openapi.json");

    // Re-run when Cargo.toml changes so version updates are reflected.
    println!("cargo:rerun-if-changed=Cargo.toml");
}
