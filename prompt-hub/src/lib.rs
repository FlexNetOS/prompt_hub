#![forbid(unsafe_code)]
//! Production-ready prompt management for LLM agent swarms. Rust 2024 Edition.
//!
//! ```ignore
//! use prompt_hub::{HubConfig, PromptHub};
//! use std::path::Path;
//!
//! let hub = PromptHub::new(Path::new("prompthub.db"), HubConfig::default()).await?;
//! let prompt = prompt_hub::models::Prompt::new("hello", "Hello, world!");
//! hub.register(prompt.clone(), &Default::default()).await?;
//! let results = hub.search("hello", /* mode */, /* filters */, /* pagination */).await?;
//! println!("Found {} prompt(s)", results.items.len());
//! ```
// This crate is still being built out: many modules are scaffolded ahead of the
// features that will wire them in, so dead-code is expected for now. The search
// and storage traits intentionally use native `async fn` (Rust 2024 Edition, no
// async_trait crate); `Arc<dyn SearchEngine>` is supported via boxed-future
// methods where object-safety is required.
#![allow(dead_code, async_fn_in_trait, unused_assignments)]
#![doc = include_str!("../README.md")]

pub mod analytics;
pub mod audit;
pub mod auth;
#[cfg(feature = "beta-program")]
pub mod beta_program;
#[cfg(feature = "budget")]
pub mod budget;
#[cfg(feature = "canary")]
pub mod canary;
#[cfg(feature = "circuit-breaker")]
pub mod circuit_breaker;
#[cfg(feature = "confidence")]
pub mod confidence;
pub mod config;
pub mod context_gatherer;
#[cfg(feature = "cost")]
pub mod cost;
#[cfg(feature = "cost-limits")]
pub mod cost_limits;
pub mod defaults;
pub mod diff;
pub mod error;
pub mod evolution;
#[cfg(feature = "fallback")]
pub mod fallback;
#[cfg(feature = "retention")]
pub mod garbage_collector;
pub mod health;
pub mod hooks;
pub mod hub;
#[cfg(feature = "i18n")]
pub mod i18n;
pub mod junie;
#[cfg(feature = "learn")]
pub mod learn;
pub mod lineage;
pub mod load_balancer;
pub mod lock;
pub mod metrics;
pub mod models;
#[cfg(feature = "moderation")]
pub mod moderation;
#[cfg(feature = "multi-provider")]
pub mod multi_provider;
#[cfg(feature = "multimodal")]
pub mod multimodal;
pub mod multimodal_input;
pub mod plugins;
pub mod pollination;
#[cfg(feature = "preview")]
pub mod preview;
#[cfg(feature = "privacy")]
pub mod privacy;
pub mod provider_health;
pub mod quality_gate;
#[cfg(feature = "quota")]
pub mod quota;
#[cfg(feature = "retention")]
pub mod retention;
#[cfg(feature = "rollback")]
pub mod rollback;
pub mod sanitize;
// satisfaction is deeply wired into PromptHub struct — kept always-in for now.
pub mod satisfaction;
pub mod search;
pub mod shutdown;
pub mod storage;
pub mod summarizer;
pub mod swarm;
pub mod sync;
pub mod templates;
pub mod tokens;
#[cfg(feature = "vibe")]
pub mod vibe;

// Re-export commonly used types
pub use config::HubConfig;
pub use error::{HubError, Result};
pub use hub::PromptHub;
pub use models::UserProfile;
pub use models::*;

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn test_re_exports_exist() {
        // Verify all re-exported types are accessible
        let _: models::Status = models::Status::Active;
        let _: models::Domain = models::Domain::General;
        let _: models::Role = models::Role::Developer;
    }

    #[test]
    fn test_module_declarations() {
        // Compilation of this test module is the assertion: if the module
        // declarations or imports above break, this test fails to build.
    }
}
