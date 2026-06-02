#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod analytics;
pub mod audit;
pub mod auth;
pub mod budget;
pub mod canary;
pub mod circuit_breaker;
pub mod config;
pub mod confidence;
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
pub mod moderation;
pub mod models;
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
pub use models::*;
pub use models::UserProfile;

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
        // Just verify modules compile
        assert!(true);
    }
}
