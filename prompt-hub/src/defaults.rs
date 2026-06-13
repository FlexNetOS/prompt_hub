#![forbid(unsafe_code)]

use crate::auth::{Action, RbacAuthManager};
use crate::error::Result;
use crate::hub::PromptHub;
use crate::models::{AgentIdentity, Pagination, Prompt, Role};

/// The built-in base templates as `(prompt name, primary target role,
/// system prompt)` triples — the canonical seed set for a fresh store.
fn base_template_specs() -> [(&'static str, Role, &'static str); 6] {
    [
        (
            "base_orchestrator",
            Role::Orchestrator,
            templates::BASE_ORCHESTRATOR,
        ),
        ("base_architect", Role::Architect, templates::BASE_ARCHITECT),
        (
            "base_implementer",
            Role::Implementer,
            templates::BASE_IMPLEMENTER,
        ),
        ("base_critic", Role::Critic, templates::BASE_CRITIC),
        ("base_reviewer", Role::Reviewer, templates::BASE_REVIEWER),
        (
            "handoff_standard",
            Role::Orchestrator,
            templates::HANDOFF_STANDARD,
        ),
    ]
}

/// Build the default base [`Prompt`]s (in memory, not yet persisted).
///
/// Each template becomes a [`Prompt`] tagged `default`/`base-template` with its
/// primary role in `target_roles`, so the seeded prompts are discoverable
/// through the normal role-filtered search/`get` path.
pub fn default_prompts() -> Vec<Prompt> {
    base_template_specs()
        .into_iter()
        .map(|(name, role, content)| {
            let mut prompt = Prompt::new(name, content);
            prompt.target_roles = vec![role];
            prompt.tags = vec!["default".to_string(), "base-template".to_string()];
            prompt
        })
        .collect()
}

/// Seed the database with the base role templates on first init.
///
/// Registers every template from [`default_prompts`] that is not already
/// present (matched by name), going through the hub's normal RBAC + sanitize +
/// audit [`PromptHub::register`] path. The caller's *identity* must hold the
/// `Write` capability.
///
/// **Idempotent:** templates whose name already exists in the store are
/// skipped, so calling this repeatedly is safe — the second call seeds nothing.
///
/// # Returns
/// The number of templates newly inserted by this call.
///
/// # Errors
/// - [`crate::error::HubError::Unauthorized`] if *identity* lacks `Write`.
/// - any storage/sanitize error surfaced by [`PromptHub::register`].
pub async fn seed_database(hub: &PromptHub, identity: &AgentIdentity) -> Result<usize> {
    // Authorize Write up front, unconditionally. Otherwise an unauthorized
    // caller hitting an already-seeded store would skip every `register` (the
    // only other auth point) and silently get `Ok(0)` — i.e. the Write gate
    // would hold only when there was something to insert. Enforce it before
    // the existence probe so seeding is a real authorization boundary.
    RbacAuthManager::authorize_action(identity, Action::Write)?;

    // Names already present, so seeding stays idempotent. A large per_page
    // pulls the (small) base set in one page.
    let existing: std::collections::HashSet<String> = hub
        .list(Pagination {
            page: 1,
            per_page: 1000,
        })
        .await?
        .items
        .into_iter()
        .map(|p| p.name)
        .collect();

    let mut seeded = 0;
    for prompt in default_prompts() {
        if existing.contains(&prompt.name) {
            continue;
        }
        hub.register(prompt, identity).await?;
        seeded += 1;
    }
    Ok(seeded)
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
    use crate::config::HubConfig;
    use crate::models::Capability;
    use std::path::Path;

    fn seeder_identity() -> AgentIdentity {
        AgentIdentity {
            id: uuid::Uuid::new_v4(),
            name: "seeder".to_string(),
            capabilities: vec![Capability::Read, Capability::Write],
            token_hash: String::new(),
            specialization_score: 0.0,
        }
    }

    async fn hub() -> PromptHub {
        PromptHub::new(Path::new(":memory:"), HubConfig::default())
            .await
            .expect("in-memory hub constructs")
    }

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

    #[test]
    fn default_prompts_cover_all_base_templates() {
        let prompts = default_prompts();
        assert_eq!(prompts.len(), 6, "all six base templates are built");
        for prompt in &prompts {
            assert!(!prompt.system_prompt.is_empty());
            assert_eq!(prompt.target_roles.len(), 1, "each carries a primary role");
            assert!(prompt.tags.iter().any(|t| t == "base-template"));
        }
    }

    #[tokio::test]
    async fn seed_database_inserts_all_base_templates() {
        let hub = hub().await;
        let identity = seeder_identity();

        let seeded = seed_database(&hub, &identity).await.unwrap();
        assert_eq!(seeded, 6, "a fresh store gets all six templates");

        let listed = hub.list(Pagination::default()).await.unwrap();
        assert!(
            listed.total >= 6,
            "seeded templates are persisted, got total {}",
            listed.total
        );
    }

    #[tokio::test]
    async fn seed_database_is_idempotent() {
        let hub = hub().await;
        let identity = seeder_identity();

        assert_eq!(seed_database(&hub, &identity).await.unwrap(), 6);
        // Second run finds everything already present and inserts nothing.
        assert_eq!(
            seed_database(&hub, &identity).await.unwrap(),
            0,
            "re-seeding is a no-op"
        );
    }

    #[tokio::test]
    async fn seed_database_requires_write() {
        let hub = hub().await;
        // Anonymous identity lacks Write → rejected up front.
        let err = seed_database(&hub, &AgentIdentity::default())
            .await
            .expect_err("seeding without Write is rejected");
        assert!(matches!(err, crate::error::HubError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn seed_database_requires_write_even_when_already_seeded() {
        // Regression for PHTASK-0044: on an ALREADY-seeded store the per-template
        // `register` calls are all skipped, so the only check left is the up-front
        // authorize. Without it an unauthorized caller silently got `Ok(0)`.
        let hub = hub().await;
        let identity = seeder_identity();
        assert_eq!(seed_database(&hub, &identity).await.unwrap(), 6);

        let err = seed_database(&hub, &AgentIdentity::default())
            .await
            .expect_err("seeding a seeded store without Write is still rejected");
        assert!(matches!(err, crate::error::HubError::Unauthorized(_)));
    }
}
