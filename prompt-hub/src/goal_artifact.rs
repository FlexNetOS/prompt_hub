//! ADR-0007 goal-artifact emission — the prompt_hub → rusty-idd front-door seam.
//!
//! prompt_hub is the prompt/intent source-of-truth; rusty-idd owns the
//! downstream OpenSpec/spec/task lifecycle (docs/plans/lifeos-meta-front-door.md
//! §81/123/147). The handoff is a **stable-schema envelope** carrying
//! **source-citation provenance** so rusty-idd can consume it deterministically
//! and bind it to a change. Contract pinned by
//! `tests/goal_artifact_contract.rs` (formerly the additive RED suite).

use crate::models::{Intent, Prompt};
use serde::{Deserialize, Serialize};

/// Stable envelope schema version consumed by rusty-idd. Bump ONLY with a
/// coordinated consumer change; it is deliberately decoupled from prompt
/// versions (see `goal_artifact_schema_is_stable_across_versions`).
pub const GOAL_ARTIFACT_SCHEMA_VERSION: &str = "1.0.0";
/// The bound consumer of goal artifacts (lifeos-meta-front-door.md §35-36).
pub const GOAL_ARTIFACT_TARGET: &str = "rusty-idd";
/// `artifact_kind` discriminant for goal envelopes.
pub const GOAL_ARTIFACT_KIND: &str = "goal_artifact";

/// One provenance citation (`[P#]`-style keys per lifeos-meta-front-door.md
/// §10-11: every claim carried by the artifact is traceable to a source).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCitation {
    /// Citation key, e.g. `P1` (prompt record), `I1` (classified intent).
    pub key: String,
    /// Human-traceable description of the source.
    pub description: String,
}

/// Provenance block: who produced the artifact and from which sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalProvenance {
    /// Always `"prompt_hub"` — the producer identity rusty-idd verifies.
    pub produced_by: String,
    /// Producer crate version (informational; NOT the envelope schema).
    pub producer_version: String,
    /// Non-empty source-citation list backing the goal's claims.
    pub sources: Vec<SourceCitation>,
}

/// The stable goal envelope handed to rusty-idd.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalArtifact {
    /// Stable envelope schema version (`GOAL_ARTIFACT_SCHEMA_VERSION`).
    pub schema_version: String,
    /// Envelope discriminant: `"goal_artifact"`.
    pub artifact_kind: String,
    /// Bound consumer: `"rusty-idd"`.
    pub target: String,
    /// The prompt record this goal originated from.
    pub origin_prompt_id: String,
    /// The goal payload (serialized `Intent` or `Prompt` model).
    pub goal: serde_json::Value,
    /// Producer identity + source citations.
    pub provenance: GoalProvenance,
}

impl GoalArtifact {
    fn envelope(
        origin_prompt_id: String,
        goal: serde_json::Value,
        sources: Vec<SourceCitation>,
    ) -> Self {
        Self {
            schema_version: GOAL_ARTIFACT_SCHEMA_VERSION.to_string(),
            artifact_kind: GOAL_ARTIFACT_KIND.to_string(),
            target: GOAL_ARTIFACT_TARGET.to_string(),
            origin_prompt_id,
            goal,
            provenance: GoalProvenance {
                produced_by: "prompt_hub".to_string(),
                producer_version: env!("CARGO_PKG_VERSION").to_string(),
                sources,
            },
        }
    }

    /// Emit a goal artifact for a stored prompt record.
    ///
    /// The prompt itself is the goal payload; provenance cites the prompt
    /// record (`P1`) so rusty-idd can trace the claim back to the hub row.
    pub fn from_prompt(prompt: &Prompt) -> Self {
        let sources = vec![SourceCitation {
            key: "P1".to_string(),
            description: format!(
                "prompt_hub prompt record {} (name: {}, version: {})",
                prompt.id, prompt.name, prompt.version
            ),
        }];
        let goal = serde_json::to_value(prompt)
            .expect("Prompt is a plain serde model; serialization is infallible");
        Self::envelope(prompt.id.to_string(), goal, sources)
    }

    /// Emit a goal artifact for a classified intent, bound to the prompt it
    /// originated from.
    pub fn from_intent(intent: &Intent, origin_prompt_id: &str) -> Self {
        let sources = vec![
            SourceCitation {
                key: "I1".to_string(),
                description: format!(
                    "classified intent (task_type: {:?}, domain: {:?}): {}",
                    intent.task_type, intent.domain, intent.raw_text
                ),
            },
            SourceCitation {
                key: "P1".to_string(),
                description: format!("originating prompt_hub record {origin_prompt_id}"),
            },
        ];
        let goal = serde_json::to_value(intent)
            .expect("Intent is a plain serde model; serialization is infallible");
        Self::envelope(origin_prompt_id.to_string(), goal, sources)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_envelope_carries_contract_fields() {
        let p = Prompt::new("unit", "system");
        let v = serde_json::to_value(GoalArtifact::from_prompt(&p)).unwrap();
        assert_eq!(
            v["schema_version"].as_str(),
            Some(GOAL_ARTIFACT_SCHEMA_VERSION)
        );
        assert_eq!(v["target"].as_str(), Some(GOAL_ARTIFACT_TARGET));
        assert_eq!(v["provenance"]["produced_by"].as_str(), Some("prompt_hub"));
        assert!(!v["provenance"]["sources"].as_array().unwrap().is_empty());
        assert_eq!(
            v["origin_prompt_id"].as_str(),
            Some(p.id.to_string().as_str())
        );
    }

    #[test]
    fn intent_envelope_is_a_goal_envelope() {
        let i = Intent::default();
        let v = serde_json::to_value(GoalArtifact::from_intent(&i, "prompt-1")).unwrap();
        assert_eq!(v["artifact_kind"].as_str(), Some(GOAL_ARTIFACT_KIND));
        assert!(v.get("goal").is_some());
        assert_eq!(v["origin_prompt_id"].as_str(), Some("prompt-1"));
    }
}
