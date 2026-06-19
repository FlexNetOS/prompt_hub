#![forbid(unsafe_code)]

use crate::models::{AgentIdentity, Capability, Role};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Junie {
    pub identity: AgentIdentity,
}

impl Junie {
    /// Create a new Junie agent instance.
    pub fn new() -> Self {
        Self {
            identity: AgentIdentity {
                id: Uuid::new_v4(),
                name: "Junie".to_string(),
                capabilities: vec![Capability::Read, Capability::Write, Capability::Execute],
                token_hash: "junie-core-token".to_string(),
                specialization_score: 1.0,
            },
        }
    }

    /// Return the primary role of Junie.
    pub fn role(&self) -> Role {
        Role::Junie
    }

    /// A default system prompt for Junie when acting as an orchestrator.
    pub fn system_prompt(&self) -> &'static str {
        "You are Junie, the primary orchestrator for the PromptHub ecosystem. \
         Your goal is to coordinate tasks, ensure code quality, and manage prompt lifecycles \
         efficiently. You follow Rust 2024 standards and prioritize safety and performance."
    }
}

impl Default for Junie {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_junie_identity() {
        let junie = Junie::new();
        assert_eq!(junie.identity.name, "Junie");
        assert_eq!(junie.role(), Role::Junie);
    }
}
