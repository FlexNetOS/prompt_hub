# Verdicts (adversarial verify gate)

## D2 — CONFIRMED 5/5
- D2-3 CONFIRMED: get calls self.search_engine.search at hub.rs:995-998; PromptHub::search(1117) NOT called by get. git-kb callgraph edge "search@1117 called@995" REFUTED (symbol-resolution artifact: trait vs inherent method).
- D2 dead-role-filter CONFIRMED: filters.role set hub.rs:991-994, read by NO engine. Fast search.rs:95-109,169-174 (domain/tags/status only); Smart 971-976 (domain/status); Hybrid 1146-1149 forwards. FTS indexes name/system_prompt/tags only (migrations/0001_initial.sql:71-77); target_roles NOT indexed; p.target_roles in SELECT only for row_to_prompt reconstruction. => get(role,intent) ranks by intent text only; role silently discarded.
- D2-6/12 CONFIRMED: HybridEngine::new built unconditionally hub.rs:395, default in both cfg arms 402-405; SmartEngine no feature gate (search.rs:800-861), only OrtEmbedder behind smart-ort (822-835) else HashEmbedder.
- D2-11 CONFIRMED: score 1.0 hardcoded search.rs:194; ORDER BY rank search.rs:176; total=items.len 199 (not real COUNT).
- D2-13/14 CONFIRMED: SMART JOIN embeddings search.rs:965 + cosine 996-999, in-mem skip/take 1004-1016; Hybrid merge *0.4 fast/*0.6 smart 1114-1123, in-mem paginate 1173-1178, tokio::join! + swallow errors.

## D4/D6 — CONFIRMED 5/5
- D6-5 CONFIRMED: trigger_pre/post_execute 3 callers each, ALL #[cfg(test)] (hooks.rs:225-307); JunieHook registered hub.rs:496, never fired in prod. (call-graph + grep agree.)
- D4-7/8 CONFIRMED: storage() pub Arc<Storage> hub.rs:624; Storage::get_prompt no authz storage.rs:374-398; CLI Tokens main.rs:212 bypasses get_by_id RBAC.
- D4-7 framing CONFIRMED: cli_identity=local_operator (identity.rs:20-26) = [Read,Write,Admin] (models.rs:178-186) => NOT escalation, just consistency+missing-audit gap.
- D4-10 CONFIRMED: default_agent() caps [Read,Write] routes.rs:150-158 => Read check no-op for server identity.
- D6-1/2/6 CONFIRMED: no cache in PromptHub(hub.rs:232-314)/Storage(storage.rs:45-57); read=authz+metric+fetch only, no audit/sync; register(hub.rs:933-956) does log_audit(Created)+sync.broadcast(PromptAdded).

## D1/D5/D3 — CONFIRMED 14, QUALIFIED 2
- D1-8..17 CONFIRMED: row_to_prompt structurally infallible (Result misleading); all unwrap_or*; storage.rs:1550-1622. Coercions: nil UUID(1572), ver 0.0.0(1574), Draft/General(1575,1586), JSON empty/default(1578-1597), author name="unknown"+nil id(1599-1604), ts->Utc::now()(1557-63,1605-06). CONTRAST row_to_version/audit DO Err on bad ids (1637-41,1663-68) — same-crate inconsistent failure policy.
- D1-5 CONFIRMED: WHERE id=?1 AND deleted_at IS NULL; no-row Ok(None) storage.rs:383,395-396.
- D1-7a CONFIRMED: read binds params!() storage.rs:384.
- D1-7b CONFIRMED(literal): update_prompt format!() interpolates id storage.rs:472-476.
- D1-7c QUALIFIED / NOT exploitable: id is typed Uuid (sig storage.rs:402), Display=[0-9a-f-] only; column values still bound params_from_iter(478). Style defect + latent footgun, NOT a vuln. *** DO NOT report as vulnerability ***
- D5-2/3 CONFIRMED: single conn cloned per acquire (54,156), semaphore concurrency only (138,147-160).
- D5-6 CONFIRMED: 4 fields, no cache (45-57).
- D3-6 CONFIRMED: presence-only gate hub.rs:1437-1442.
- D3-8 CONFIRMED: only user_template rendered, system_prompt never referenced hub.rs:1453.
- D3-12 CONFIRMED(mechanism)/QUALIFIED(framing): no_escape raw subst templates.rs:69 + strict_mode(64); rationale 65-68 = LLM-bound output intentional, not DOM. Raw subst real; "vulnerability" is design-context judgment.

# REFUTED OVERCLAIMS (gate caught these — reported as caveats not facts)
1. git-kb call graph edge "search@1117 called@995" — REFUTED (symbol-resolution artifact).
2. write-path SQL injection "vulnerability" — DOWNGRADED to non-exploitable style defect.
3. Handlebars "injection surface" — DOWNGRADED to intentional design (contextual).
