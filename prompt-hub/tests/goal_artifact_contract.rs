//! Additive RED suite — ADR-0007 goal-artifact emission contract.
//!
//! Convergence contract (docs/plans/lifeos-meta-front-door.md:81,123,147 +
//! docs/adr/0007-plugin-system.md): prompt_hub is the prompt/intent
//! source-of-truth that hands a **provenance-stamped GOAL ARTIFACT** to
//! `rusty-idd`, which then drives the OpenSpec/spec/task lifecycle. The plan
//! requires the handoff to be a **stable-schema envelope** carrying
//! **source-citation provenance** (`[L#] [E#] [W#] …` keys,
//! lifeos-meta-front-door.md:10-11) so rusty-idd can consume it deterministically
//! and bind it to a change.
//!
//! GREEN since the `goal_artifact` module landed: `GoalArtifact::from_prompt` /
//! `::from_intent` emit the stable envelope (schema_version + provenance with
//! `[P#]`/`[I#]` source citations + producer/consumer binding). This suite is
//! now the pinned convergence contract — a regression in the envelope shape
//! turns it RED again. (It began life as the additive RED suite whose flip to
//! GREEN was the acceptance signal.)

use prompt_hub::models::*;
use prompt_hub::{GoalArtifact, HubConfig, PromptHub};
use serde_json::Value;
use std::collections::HashMap;
use tempfile::TempDir;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// A benign prompt that passes the injection sanitizer (no jailbreak keywords).
fn sample_prompt(name: &str) -> Prompt {
    let mut p = Prompt::new(
        name,
        "You are a helpful planning assistant for build tasks.",
    );
    p.domain = Domain::General;
    p.tags = vec!["planning".to_string()];
    p.target_roles = vec![Role::Developer];
    p
}

/// A sample classified intent (the "goal" prompt_hub would hand to rusty-idd).
fn sample_intent() -> Intent {
    Intent {
        raw_text: "Add a provenance-stamped goal artifact emitter".to_string(),
        domain: Domain::General,
        role: Role::Developer,
        task_type: TaskType::Create,
        complexity: Complexity::Simple,
        urgency: Urgency::Medium,
        extracted_entities: HashMap::new(),
    }
}

/// The goal-artifact emission for a prompt record (ADR-0007 convergence
/// contract): the stable envelope rusty-idd consumes.
fn current_emission(prompt: &Prompt) -> Value {
    serde_json::to_value(GoalArtifact::from_prompt(prompt)).expect("GoalArtifact derives Serialize")
}

// ── Contract: stable schema version ─────────────────────────────────────────

#[test]
fn goal_artifact_declares_stable_schema_version() {
    // rusty-idd must consume the artifact deterministically across prompt_hub
    // versions (lifeos-meta-front-door.md:147 "stable schema"). The envelope
    // therefore MUST carry a top-level `schema_version`.
    let artifact = current_emission(&sample_prompt("schema-version-case"));
    assert!(
        artifact
            .get("schema_version")
            .and_then(Value::as_str)
            .is_some(),
        "ADR-0007 goal-artifact contract: emission must carry a stable \
         top-level `schema_version` string for rusty-idd to consume; the bare \
         serialized Prompt has none. Emitted keys: {:?}",
        artifact.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
}

// ── Contract: provenance block present ──────────────────────────────────────

#[test]
fn goal_artifact_carries_provenance_block() {
    // lifeos-meta-front-door.md:10-11 — "Provenance of every claim". The
    // artifact MUST carry a `provenance` object, not just prompt content.
    let artifact = current_emission(&sample_prompt("provenance-case"));
    assert!(
        artifact
            .get("provenance")
            .map(Value::is_object)
            .unwrap_or(false),
        "ADR-0007 goal-artifact contract: emission must carry a `provenance` \
         object; the bare serialized Prompt has no provenance. Emitted keys: {:?}",
        artifact.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
}

// ── Contract: provenance lists source citations ─────────────────────────────

#[test]
fn goal_artifact_provenance_lists_source_citations() {
    // The handoff must carry source citations ([L#] [E#] [W#] …,
    // lifeos-meta-front-door.md:10-11,123 "with source citations") so rusty-idd
    // can trace each goal claim. Assert a non-empty `provenance.sources` array.
    let artifact = current_emission(&sample_prompt("citations-case"));
    let sources = artifact
        .get("provenance")
        .and_then(|p| p.get("sources"))
        .and_then(Value::as_array);
    assert!(
        sources.map(|s| !s.is_empty()).unwrap_or(false),
        "ADR-0007 goal-artifact contract: `provenance.sources` must be a \
         non-empty citation list; absent in the bare Prompt emission. \
         provenance = {:?}",
        artifact.get("provenance")
    );
}

// ── Contract: producer + consumer binding ───────────────────────────────────

#[test]
fn goal_artifact_identifies_producer_and_targets_rusty_idd() {
    // The envelope must self-describe its producer (`prompt_hub`) and its
    // consumer binding so rusty-idd knows it owns the downstream lifecycle
    // (lifeos-meta-front-door.md:35-36, 81 "prompt_hub → rusty-idd").
    let artifact = current_emission(&sample_prompt("routing-case"));
    let produced_by = artifact
        .get("provenance")
        .and_then(|p| p.get("produced_by"))
        .and_then(Value::as_str);
    let target = artifact.get("target").and_then(Value::as_str);
    assert_eq!(
        produced_by,
        Some("prompt_hub"),
        "ADR-0007 goal-artifact contract: emission must stamp \
         `provenance.produced_by = \"prompt_hub\"`; got {produced_by:?}"
    );
    assert_eq!(
        target,
        Some("rusty-idd"),
        "ADR-0007 goal-artifact contract: emission must bind \
         `target = \"rusty-idd\"` as the consumer; got {target:?}"
    );
}

// ── Contract: it is a GOAL envelope, not a bare prompt record ────────────────

#[test]
fn goal_artifact_envelope_wraps_the_goal_payload() {
    // The artifact rusty-idd consumes is a GOAL envelope: it carries the goal
    // (intent) payload + the originating prompt id under stable keys, not a raw
    // prompt row (lifeos-meta-front-door.md:147 "planning outputs create/select
    // OpenSpec changes").
    let intent = sample_intent();
    // The intent is emitted wrapped in the goal-artifact envelope, bound to its
    // originating prompt record.
    let emitted = serde_json::to_value(GoalArtifact::from_intent(&intent, "prompt-000"))
        .expect("GoalArtifact derives Serialize");
    let has_envelope = emitted.get("artifact_kind").and_then(Value::as_str)
        == Some("goal_artifact")
        && emitted.get("goal").is_some()
        && emitted.get("origin_prompt_id").is_some();
    assert!(
        has_envelope,
        "ADR-0007 goal-artifact contract: intent must be emitted as a \
         `artifact_kind = \"goal_artifact\"` envelope carrying `goal` + \
         `origin_prompt_id`; the bare Intent is not an envelope. Emitted keys: {:?}",
        emitted.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
}

// ── Contract: hub round-trip emission honors the contract ───────────────────

#[tokio::test]
async fn registered_prompt_emits_contract_compliant_goal_artifact() {
    // True public-API round-trip: register through the hub, retrieve via search,
    // then assert the retrieved record can be emitted as a contract-compliant
    // goal artifact. This guards the integration path
    // (prompt_hub register → emit → rusty-idd), which is wholly untested today.
    let tmp = TempDir::new().unwrap();
    let hub = PromptHub::new(tmp.path().join("ga.db").as_path(), HubConfig::default())
        .await
        .expect("hub init");
    let identity = AgentIdentity::default(); // Read + Write — sufficient to register

    let prompt = sample_prompt("roundtrip-emit");
    let id = hub.register(prompt, &identity).await.expect("register");

    let results = hub
        .search(
            "roundtrip-emit",
            SearchMode::Fast,
            SearchFilters::default(),
            Pagination::default(),
        )
        .await
        .expect("search");
    let stored = &results
        .items
        .first()
        .expect("registered prompt should be retrievable")
        .prompt;
    assert_eq!(
        stored.id, id,
        "round-trip should return the registered prompt"
    );

    let artifact = current_emission(stored);
    let schema_ok = artifact
        .get("schema_version")
        .and_then(Value::as_str)
        .is_some();
    let provenance_ok = artifact
        .get("provenance")
        .and_then(|p| p.get("sources"))
        .and_then(Value::as_array)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    assert!(
        schema_ok && provenance_ok,
        "ADR-0007 goal-artifact contract: a hub-registered prompt must emit a \
         goal artifact with `schema_version` + non-empty `provenance.sources` \
         for rusty-idd; got schema_version_present={schema_ok}, \
         provenance_sources_present={provenance_ok}. Emitted keys: {:?}",
        artifact.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
}

// ── Contract: schema is stable across prompt versions (golden/stability) ────

#[test]
fn goal_artifact_schema_is_stable_across_versions() {
    // Differential/stability check: two different prompt versions must emit the
    // SAME `schema_version` so rusty-idd's consumer is version-pinned, not
    // coupled to prompt content. Absent today → no stable schema to pin.
    let mut v1 = sample_prompt("stable-a");
    v1.version = semver::Version::new(0, 1, 0);
    let mut v2 = sample_prompt("stable-b");
    v2.version = semver::Version::new(0, 2, 0);

    let s1 = current_emission(&v1)
        .get("schema_version")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let s2 = current_emission(&v2)
        .get("schema_version")
        .and_then(Value::as_str)
        .map(str::to_owned);

    assert!(
        s1.is_some() && s1 == s2,
        "ADR-0007 goal-artifact contract: emissions must carry a stable, \
         identical `schema_version` across prompt versions; got {s1:?} vs {s2:?}"
    );
}
