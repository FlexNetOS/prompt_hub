//! Integration test for the `PromptHub::get` / `get_by_id` retrieval flow:
//! RBAC authorization → storage/intent lookup → audit trail (PHTASK-0039).
//!
//! `get()` and `get_by_id()` gate every read on [`RbacAuthManager`] Read
//! authorization before touching the search engine or storage. These tests
//! exercise the full path end to end against a `:memory:` hub: the deny path
//! (caller without `Read`), the allow + lookup path, the intent-search path,
//! and the audit trail recorded by registration.

use prompt_hub::config::HubConfig;
use prompt_hub::error::HubError;
use prompt_hub::hub::PromptHub;
use prompt_hub::models::{AgentIdentity, Capability, Pagination, Prompt, Role};
use std::path::Path;
use uuid::Uuid;

/// A caller holding `Read` + `Write` — the minimum to register and then read.
fn reader_writer() -> AgentIdentity {
    AgentIdentity {
        id: Uuid::new_v4(),
        name: "reader-writer".to_string(),
        capabilities: vec![Capability::Read, Capability::Write],
        token_hash: String::new(),
        specialization_score: 0.0,
    }
}

/// An explicitly capability-less identity — must be denied every read.
/// (Built directly rather than via `AgentIdentity::default()`, which now carries
/// Read+Write by owner decision; PHTASK-0040.)
fn anonymous() -> AgentIdentity {
    AgentIdentity {
        capabilities: Vec::new(),
        ..AgentIdentity::default()
    }
}

async fn hub() -> PromptHub {
    PromptHub::new(Path::new(":memory:"), HubConfig::default())
        .await
        .expect("in-memory hub constructs")
}

async fn register_dev_prompt(hub: &PromptHub, identity: &AgentIdentity) -> Uuid {
    let mut prompt = Prompt::new(
        "rbac-flow-prompt",
        "Help the developer refactor and test the authentication module.",
    );
    prompt.target_roles = vec![Role::Developer];
    let id = prompt.id;
    hub.register(prompt, identity)
        .await
        .expect("authorized register succeeds");
    id
}

#[tokio::test]
async fn get_denies_caller_without_read() {
    let hub = hub().await;
    // Auth is checked before any search work, so this is deterministic even
    // with an empty store.
    let err = hub
        .get(Role::Developer, "anything", &anonymous())
        .await
        .expect_err("anonymous caller is rejected");
    assert!(
        matches!(err, HubError::Unauthorized(_)),
        "expected Unauthorized, got {err:?}"
    );
}

#[tokio::test]
async fn get_by_id_denies_caller_without_read() {
    let hub = hub().await;
    let err = hub
        .get_by_id(Uuid::new_v4(), &anonymous())
        .await
        .expect_err("anonymous caller is rejected");
    assert!(
        matches!(err, HubError::Unauthorized(_)),
        "expected Unauthorized, got {err:?}"
    );
}

#[tokio::test]
async fn get_by_id_returns_registered_prompt_for_authorized() {
    let hub = hub().await;
    let identity = reader_writer();
    let id = register_dev_prompt(&hub, &identity).await;

    // Auth check passes → storage lookup returns the exact prompt.
    let found = hub
        .get_by_id(id, &identity)
        .await
        .expect("authorized lookup succeeds")
        .expect("the registered prompt exists");
    assert_eq!(found.id, id);
    assert_eq!(found.name, "rbac-flow-prompt");
}

#[tokio::test]
async fn get_by_id_returns_none_for_unknown_id() {
    let hub = hub().await;
    let identity = reader_writer();
    // Authorized, but no such prompt → Ok(None), not an error.
    let found = hub
        .get_by_id(Uuid::new_v4(), &identity)
        .await
        .expect("authorized lookup succeeds");
    assert!(found.is_none(), "unknown id must yield None");
}

#[tokio::test]
async fn get_finds_prompt_by_role_and_intent() {
    let hub = hub().await;
    let identity = reader_writer();
    let id = register_dev_prompt(&hub, &identity).await;

    // Full flow: auth (Read) → intent search filtered by role.
    let found = hub
        .get(
            Role::Developer,
            "refactor the authentication module",
            &identity,
        )
        .await
        .expect("authorized intent lookup succeeds");

    match found {
        Some(p) => assert_eq!(p.id, id, "get() must return the matching prompt"),
        None => panic!("get() should find the registered Developer prompt by intent"),
    }
}

#[tokio::test]
async fn registration_writes_a_queryable_audit_trail() {
    let hub = hub().await;
    let identity = reader_writer();
    let id = register_dev_prompt(&hub, &identity).await;

    // The create→audit→retrieve loop: the registration must leave an audit
    // entry that the trail query surfaces for that prompt id.
    let trail = hub
        .audit_trail(id, Pagination::default())
        .await
        .expect("audit trail query succeeds");
    assert!(
        trail.total >= 1,
        "registering a prompt should record at least one audit entry, got {}",
        trail.total
    );
}
