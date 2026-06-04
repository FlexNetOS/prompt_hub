#![forbid(unsafe_code)]
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
pub mod budget;
pub mod canary;
pub mod circuit_breaker;
pub mod confidence;
pub mod config;
pub mod context_gatherer;
pub mod cost;
pub mod defaults;
pub mod diff;
pub mod error;
pub mod evolution;
pub mod fallback;
pub mod garbage_collector;
pub mod health;
pub mod hub;
pub mod i18n;
pub mod learn;
pub mod lineage;
pub mod load_balancer;
pub mod lock;
pub mod metrics;
pub mod models;
pub mod moderation;
pub mod multimodal;
pub mod multimodal_input;
pub mod plugins;
pub mod pollination;
pub mod preview;
pub mod privacy;
pub mod provider_health;
pub mod quality_gate;
pub mod quota;
pub mod retention;
pub mod rollback;
pub mod sanitize;
pub mod satisfaction;
pub mod search;
pub mod shutdown;
pub mod storage;
pub mod summarizer;
pub mod swarm;
pub mod sync;
pub mod templates;
pub mod tokens;
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
