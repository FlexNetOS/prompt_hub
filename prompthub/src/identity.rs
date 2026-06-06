#![forbid(unsafe_code)]
//! The acting identity for the local `prompthub` CLI.
//!
//! Every CLI command operates on the local on-disk store as its owner, so it
//! runs as a trusted [local operator](prompt_hub::models::AgentIdentity::local_operator)
//! — Read + Write + Admin — rather than the capability-less `anonymous`
//! default (which made every mutating command fail with `Unauthorized`).
//!
//! The library's RBAC is unchanged and remains the enforcement point; this only
//! selects which identity the CLI presents. The display name (recorded in the
//! audit log) can be overridden with the `PROMPTHUB_AGENT` environment variable.

use prompt_hub::models::AgentIdentity;

/// Environment variable overriding the local operator's display name.
const AGENT_NAME_ENV: &str = "PROMPTHUB_AGENT";

/// The identity the CLI acts as. Defaults to `local-operator`; the name can be
/// overridden via `PROMPTHUB_AGENT` for clearer audit attribution.
pub fn cli_identity() -> AgentIdentity {
    let name = std::env::var(AGENT_NAME_ENV)
        .ok()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "local-operator".to_string());
    AgentIdentity::local_operator(name)
}
