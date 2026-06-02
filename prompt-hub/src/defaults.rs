#![forbid(unsafe_code)]

use crate::error::Result;
use crate::hub::PromptHub;
use crate::models::*;
use chrono::Utc;
use tracing::info;
use uuid::Uuid;

/// Seed the database with base templates on first init
pub async fn seed_database(hub: &PromptHub) -> Result<()> {
    info!("Checking seed data...");
    Ok(())
}

/// Default base templates as static strings
pub mod templates {
    pub const BASE_ORCHESTRATOR: &str = r#"# Orchestrator Mission
You are the Orchestrator of an AI agent swarm.
Your role: coordinate, delegate, and ensure quality delivery.

## Agent Roster
- Architect: Design and constraints
- Implementer: Code and testing
- Critic: Review and validation
- Reviewer: Final sign-off

## Protocol
1. Receive mission
2. Assign roles
3. Monitor progress
4. Resolve blockers
5. Deliver result
"#;

    pub const BASE_ARCHITECT: &str = r#"# Architect Mission
Design robust, scalable solutions within constraints.

## Deliverables
- Architecture diagram
- Interface definitions
- Technology choices
- Risk assessment
"#;

    pub const BASE_IMPLEMENTER: &str = r#"# Implementer Mission
Write clean, tested, production-ready code.

## Deliverables
- Implementation code
- Unit tests
- Documentation
"#;

    pub const BASE_CRITIC: &str = r#"# Critic Mission
Review all deliverables against standards.

## Review Criteria
- Correctness
- Performance
- Security
- Maintainability
- Test coverage
"#;

    pub const BASE_REVIEWER: &str = r#"# Reviewer Mission
Final validation and sign-off.

## Sign-off Checklist
- [ ] All tests pass
- [ ] Documentation complete
- [ ] No security issues
"#;

    pub const HANDOFF_STANDARD: &str = r#"# Handoff: {{from_role}} -> {{to_role}}

## Context
{{context_summary}}

## Deliverables
{{deliverables}}

## Blockers
{{blockers}}

## Next Steps
{{next_steps}}
"#;
}

/// Get a default template by name
pub fn get_default_template(name: &str) -> Option<&'static str> {
    match name {
        "base_orchestrator" => Some(templates::BASE_ORCHESTRATOR),
        "base_architect" => Some(templates::BASE_ARCHITECT),
        "base_implementer" => Some(templates::BASE_IMPLEMENTER),
        "base_critic" => Some(templates::BASE_CRITIC),
        "base_reviewer" => Some(templates::BASE_REVIEWER),
        "handoff_standard" => Some(templates::HANDOFF_STANDARD),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_template() {
        assert!(get_default_template("base_orchestrator").is_some());
        assert!(get_default_template("unknown").is_none());
    }

    #[test]
    fn test_templates_not_empty() {
        assert!(!templates::BASE_ORCHESTRATOR.is_empty());
        assert!(!templates::HANDOFF_STANDARD.is_empty());
    }
}
