//! TEAS front-door emitter (DOMAIN_MODEL seam S1).
//!
//! Maps prompt_hub's [`Intent`] + [`ExecutionPlan`] into one or more canonical
//! `handoff.task.v1` WorkOrders, so human intent entering the prompt_hub front
//! door becomes governed task-graph rows.
//!
//! The emitted WorkOrders are strongly typed ([`EmittedWorkOrder`]) and, when
//! serialized, contain ONLY the fields defined by the canonical
//! `task_graph.schema.json` (embedded below via `include_str!`). Optional fields
//! use `skip_serializing_if` so absent values never appear — required to satisfy
//! the schema's `additionalProperties: false`.
//!
//! prompt_hub defines its OWN emission type here (DOMAIN_MODEL 9.6 — types
//! converge at consolidation); this module intentionally does not depend on the
//! `handoff` or `rvagent-engine` crates.

#![forbid(unsafe_code)]

use serde::Serialize;

use crate::models::{Complexity, ExecutionPlan, ExecutionStep, Intent, Urgency};

/// The canonical task-graph JSON Schema, embedded at build time.
///
/// Emitted WorkOrders validate against this contract.
pub const TASK_GRAPH_SCHEMA: &str = include_str!("../schema/task_graph.schema.json");

/// Schema tag pinning the shared kernel version.
const SCHEMA_TAG: &str = "handoff.task.v1";

/// `IntentLock` value object: blake3 hashes pinning the immutable contract
/// surface of a WorkOrder. A verifier recomputes these; any mismatch is drift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntentLock {
    /// `blake3:<hex>` of the objective string.
    pub objective_hash: String,
    /// `blake3:<hex>` of the newline-joined path_scope.
    pub path_scope_hash: String,
    /// `blake3:<hex>` of the newline-joined acceptance_criteria.
    pub acceptance_hash: String,
}

/// A single emitted WorkOrder (one canonical `handoff.task.v1` task-graph row).
///
/// Field set is a subset of the canonical schema. Optional fields are elided
/// from the serialized JSON when empty/absent so the output never carries a key
/// the schema forbids (`additionalProperties: false`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EmittedWorkOrder {
    /// Schema tag, always `SCHEMA_TAG` (`handoff.task.v1`).
    pub schema: String,
    /// Stable identity, e.g. `TASK-0001`. Matches `^[A-Z]*TASK-[A-Z0-9][A-Z0-9-]*$`.
    pub id: String,
    /// Short human-readable title.
    pub title: String,
    /// Proof-oriented objective (what "done" means for this step).
    pub objective: String,
    /// Card-declared status; the front door emits `backlog`.
    pub status: String,
    /// Priority `P0`..`P3`, mapped from intent urgency/complexity.
    pub priority: String,
    /// Paths this WorkOrder may touch (may be empty).
    pub path_scope: Vec<String>,
    /// Proof-of-done criteria; always at least one item.
    pub acceptance_criteria: Vec<String>,
    /// Dependency TASK ids (translated from step dependency references).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    /// Blocking TASK ids (mirror of `dependencies` for this seam).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
    /// Computed blake3 intent lock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_lock: Option<IntentLock>,
    /// Proof gate; always `true` when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_required: Option<bool>,
}

/// Maps an [`Intent`] + [`ExecutionPlan`] into governed WorkOrders — one per
/// [`ExecutionStep`], preserving the plan's (toposorted) step order.
///
/// Determinism: WorkOrder ids are `TASK-<NNNN>` from the 1-based step position;
/// dependency references are resolved against step ids (falling back to 1-based
/// position) and rewritten to the corresponding TASK ids.
///
/// No panics: this function performs no fallible I/O and never unwraps.
pub fn emit_work_orders(intent: &Intent, plan: &ExecutionPlan) -> Vec<EmittedWorkOrder> {
    let priority = map_priority(intent);

    // Map each step's `id` (and its 1-based position) to its TASK id so we can
    // rewrite dependency references regardless of whether a plan references
    // steps by their `id` field or by positional index.
    let task_id_for_position: Vec<String> = (1..=plan.steps.len()).map(task_id).collect();

    plan.steps
        .iter()
        .enumerate()
        .map(|(idx, step)| {
            let position = idx + 1;
            let id = task_id(position);

            let title = short_title(step, position);
            let objective = objective_for(step, position);
            // ExecutionStep carries no file-scope in the current domain model,
            // so path_scope is emitted empty (the schema permits an empty array).
            let path_scope: Vec<String> = Vec::new();
            let acceptance_criteria = acceptance_for(step, position);

            let dependencies = resolve_dependencies(step, &plan.steps, &task_id_for_position);
            let blocked_by = dependencies.clone();

            let intent_lock = Some(IntentLock {
                objective_hash: blake3_tag(objective.as_bytes()),
                path_scope_hash: blake3_tag(path_scope.join("\n").as_bytes()),
                acceptance_hash: blake3_tag(acceptance_criteria.join("\n").as_bytes()),
            });

            EmittedWorkOrder {
                schema: SCHEMA_TAG.to_string(),
                id,
                title,
                objective,
                status: "backlog".to_string(),
                priority: priority.to_string(),
                path_scope,
                acceptance_criteria,
                dependencies,
                blocked_by,
                intent_lock,
                proof_required: Some(true),
            }
        })
        .collect()
}

/// Deterministic TASK id from a 1-based position, zero-padded to 4 digits.
///
/// Matches the schema id regex `^[A-Z]*TASK-[A-Z0-9][A-Z0-9-]*$`.
fn task_id(position: usize) -> String {
    format!("TASK-{position:04}")
}

/// Maps intent urgency (primary) and complexity (tie-break) to a `P0`..`P3`
/// priority. Falls back to `P2` for the neutral case.
fn map_priority(intent: &Intent) -> &'static str {
    match intent.urgency {
        Urgency::Critical => "P0",
        Urgency::High => "P1",
        Urgency::Medium => {
            // Escalate a medium-urgency but research/complex request one notch.
            match intent.complexity {
                Complexity::Complex | Complexity::Research => "P1",
                _ => "P2",
            }
        }
        Urgency::Low => "P3",
    }
}

/// Builds a short, non-empty title for a step.
fn short_title(step: &ExecutionStep, position: usize) -> String {
    let source = first_non_empty(&[&step.action, &step.description]);
    match source {
        Some(text) => truncate_words(text, 72),
        None => format!("Step {position}"),
    }
}

/// Builds a non-empty objective for a step.
fn objective_for(step: &ExecutionStep, position: usize) -> String {
    match first_non_empty(&[&step.description, &step.action]) {
        Some(text) => text.trim().to_string(),
        None => format!("Complete step {position} and pass verification"),
    }
}

/// Builds a non-empty acceptance-criteria list for a step (schema requires >=1).
fn acceptance_for(step: &ExecutionStep, position: usize) -> Vec<String> {
    match first_non_empty(&[&step.description, &step.action]) {
        Some(text) => vec![format!(
            "{} — completes and verification passes",
            text.trim()
        )],
        None => vec![format!("Step {position} completes and verification passes")],
    }
}

/// Resolves a step's dependency references to the corresponding TASK ids.
///
/// A reference resolves against a step's `id` field first; failing that, it is
/// treated as a 1-based positional index into the plan. Unresolvable references
/// are skipped (no fabricated ids). Order is preserved and duplicates removed.
fn resolve_dependencies(
    step: &ExecutionStep,
    steps: &[ExecutionStep],
    task_id_for_position: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for dep in &step.dependencies {
        let resolved = steps
            .iter()
            .position(|s| s.id == *dep)
            .and_then(|pos| task_id_for_position.get(pos).cloned())
            .or_else(|| {
                // Fall back to 1-based positional interpretation.
                dep.checked_sub(1)
                    .and_then(|pos| task_id_for_position.get(pos).cloned())
            });
        if let Some(task) = resolved
            && !out.contains(&task)
        {
            out.push(task);
        }
    }
    out
}

/// Returns the first trimmed, non-empty string from the candidates.
fn first_non_empty<'a>(candidates: &[&'a String]) -> Option<&'a str> {
    candidates.iter().map(|s| s.trim()).find(|s| !s.is_empty())
}

/// Truncates `text` to at most `max_chars` characters on a word boundary,
/// collapsing internal whitespace. Never splits a UTF-8 code point.
fn truncate_words(text: &str, max_chars: usize) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut result = String::new();
    for word in collapsed.split(' ') {
        // +1 accounts for the joining space (skipped for the first word).
        let extra = if result.is_empty() { 0 } else { 1 };
        if result.chars().count() + extra + word.chars().count() > max_chars {
            break;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    if result.is_empty() {
        // Single word longer than max_chars: hard-truncate on a char boundary.
        result = collapsed.chars().take(max_chars).collect();
    }
    result
}

/// Computes the `blake3:<hex>` tag for the given bytes.
fn blake3_tag(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Domain, Role, TaskType};
    use jsonschema::Validator;
    use std::collections::HashMap;

    fn sample_intent() -> Intent {
        Intent {
            raw_text: "Build a login flow with tests".to_string(),
            domain: Domain::General,
            role: Role::Developer,
            task_type: TaskType::Create,
            complexity: Complexity::Complex,
            urgency: Urgency::High,
            extracted_entities: HashMap::new(),
        }
    }

    fn sample_plan() -> ExecutionPlan {
        ExecutionPlan {
            title: "Login flow".to_string(),
            description: "Implement and verify a login flow".to_string(),
            steps: vec![
                ExecutionStep {
                    id: 1,
                    description: "Design the authentication data model".to_string(),
                    action: "design".to_string(),
                    dependencies: vec![],
                    estimated_duration_secs: 600,
                },
                ExecutionStep {
                    id: 2,
                    description: "Implement the login handler".to_string(),
                    action: "implement".to_string(),
                    dependencies: vec![1],
                    estimated_duration_secs: 1200,
                },
                ExecutionStep {
                    id: 3,
                    description: "Write integration tests for login".to_string(),
                    action: "test".to_string(),
                    dependencies: vec![1, 2],
                    estimated_duration_secs: 900,
                },
            ],
            total_estimated_duration_secs: 2700,
        }
    }

    fn compiled_schema() -> Validator {
        let schema: serde_json::Value =
            serde_json::from_str(TASK_GRAPH_SCHEMA).expect("schema parses");
        jsonschema::validator_for(&schema).expect("schema compiles")
    }

    #[test]
    fn emits_one_work_order_per_step() {
        let orders = emit_work_orders(&sample_intent(), &sample_plan());
        assert_eq!(orders.len(), 3, "one WorkOrder per ExecutionStep");
        assert_eq!(orders[0].id, "TASK-0001");
        assert_eq!(orders[1].id, "TASK-0002");
        assert_eq!(orders[2].id, "TASK-0003");
    }

    #[test]
    fn every_work_order_validates_against_canonical_schema() {
        let validator = compiled_schema();
        let orders = emit_work_orders(&sample_intent(), &sample_plan());
        assert!(!orders.is_empty(), "at least one WorkOrder emitted");
        for order in &orders {
            let value = serde_json::to_value(order).expect("serializes");
            let errors: Vec<String> = validator
                .iter_errors(&value)
                .map(|e| e.to_string())
                .collect();
            assert!(
                errors.is_empty(),
                "WorkOrder {} failed schema validation: {:?}\njson: {}",
                order.id,
                errors,
                serde_json::to_string_pretty(&value).unwrap_or_default()
            );
        }
    }

    #[test]
    fn dependencies_translated_to_task_ids_preserving_toposort() {
        let orders = emit_work_orders(&sample_intent(), &sample_plan());
        assert!(orders[0].dependencies.is_empty());
        assert_eq!(orders[1].dependencies, vec!["TASK-0001".to_string()]);
        assert_eq!(
            orders[2].dependencies,
            vec!["TASK-0001".to_string(), "TASK-0002".to_string()]
        );
        // blocked_by mirrors dependencies for this seam.
        assert_eq!(orders[2].blocked_by, orders[2].dependencies);
    }

    #[test]
    fn intent_lock_has_blake3_hashes_and_proof_required() {
        let orders = emit_work_orders(&sample_intent(), &sample_plan());
        for order in &orders {
            let lock = order.intent_lock.as_ref().expect("intent_lock present");
            for hash in [
                &lock.objective_hash,
                &lock.path_scope_hash,
                &lock.acceptance_hash,
            ] {
                assert!(hash.starts_with("blake3:"), "hash prefixed: {hash}");
                assert_eq!(hash.len(), "blake3:".len() + 64, "64 hex chars: {hash}");
            }
            assert_eq!(order.proof_required, Some(true));
        }
    }

    #[test]
    fn priority_maps_from_urgency() {
        let mut intent = sample_intent();
        intent.urgency = Urgency::Critical;
        let orders = emit_work_orders(&intent, &sample_plan());
        assert_eq!(orders[0].priority, "P0");

        intent.urgency = Urgency::Low;
        let orders = emit_work_orders(&intent, &sample_plan());
        assert_eq!(orders[0].priority, "P3");
    }

    #[test]
    fn serialized_json_has_no_extra_keys() {
        let orders = emit_work_orders(&sample_intent(), &sample_plan());
        let value = serde_json::to_value(&orders[0]).expect("serializes");
        let obj = value.as_object().expect("object");
        let allowed = [
            "schema",
            "id",
            "title",
            "objective",
            "status",
            "priority",
            "path_scope",
            "acceptance_criteria",
            "dependencies",
            "blocked_by",
            "intent_lock",
            "proof_required",
        ];
        for key in obj.keys() {
            assert!(allowed.contains(&key.as_str()), "unexpected key: {key}");
        }
        // id matches the canonical regex shape.
        assert!(orders[0].id.starts_with("TASK-"));
        assert!(!orders[0].acceptance_criteria.is_empty());
    }
}
