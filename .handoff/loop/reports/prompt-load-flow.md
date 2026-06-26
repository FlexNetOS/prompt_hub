# prompt_hub — how prompts are loaded (decision-grade, evidence-verified)

**Question:** Map the exact end-to-end prompt-LOAD flow, with adversarially-verified evidence.
**Confidence: HIGH.** 6 dimensions, 24 material claims verified; 3 overclaims caught & downgraded by the gate. 0 unrefuted-but-asserted claims.

## Verdict
A prompt load is one of two paths, both fail-closed on RBAC then a **direct, uncached** libsql read:
- **Exact (by UUID):** `get_by_id` → `Storage::get_prompt` → `row_to_prompt`.
- **Search (by role+intent):** `get` → `HybridEngine::search` (FTS5 ⊕ embedding cosine) → top-1.
Rendering is a separate `render_prompt` = exact-load + presence-only var gate + feature-selected template engine over `user_template` only. No cache, no hooks, no audit, no sync fire on any read — only a metric.

## The two load paths (verified)
```
get_by_id(uuid, id)                      get(role, intent, id)
  authorize_action(Read)  hub.rs:1014      authorize_action(Read)  hub.rs:987
  metrics.record_request()                 metrics.record_request()
  storage.get_prompt(uuid) :374            SearchFilters{role:Some} :991  (role is DEAD — see below)
    acquire() semaphore :146               search_engine.search(intent,..) :995  ← NOT PromptHub::search(1117)
    SELECT 19 cols WHERE id=?1               HybridEngine: FAST ⊕ SMART via tokio::join! :1146
      AND deleted_at IS NULL :383             FAST: JOIN prompts_fts MATCH ?1 ... ORDER BY rank :154-178
    row_to_prompt() :1550                     SMART: JOIN embeddings + cosine :944-1024
  Option<Prompt>                            merge FAST*0.4 + SMART*0.6, in-mem paginate :1107-1186
                                           results.items.next() → top-1  hub.rs:999
```

## Confirmed findings worth knowing (all CONFIRMED vs source)
1. **`role` is silently ignored in role+intent lookup.** `get` sets `SearchFilters.role` but no engine reads it; `target_roles` isn't even in the FTS index (0001_initial.sql:71-77). `get(role,intent)` ranks by intent text only. *Doc-vs-signature defect.*
2. **The entire hook subsystem is dormant.** `trigger_pre/post_execute` have zero production callers (only `#[cfg(test)]`); `JunieHook` is registered (hub.rs:496) but never fired. Reads (and writes) run no hooks.
3. **Reads are uncached & side-effect-free.** `Storage` has 4 fields, none a cache; reads emit no audit/sync — only `record_request()`. Writes (`register`) do the full `sanitize→insert→index→log_audit→sync.broadcast`. Read/write asymmetry is the defining trait.
4. **`row_to_prompt` is structurally infallible but lossy.** Its `Result` never errs: corrupt columns silently coerce (nil UUID, `0.0.0` version, `Draft`/`General`, `Utc::now()` timestamps, author `name="unknown"`). Sibling decoders `row_to_version`/`row_to_audit_entry` *do* `Err` on bad ids — inconsistent failure policy.
5. **The "connection pool" is one shared libsql connection** cloned per `acquire()`; the semaphore bounds concurrency only (required so `:memory:` DBs don't get a fresh empty db).
6. **RBAC bypass surface (not an exploit).** `storage()` is a public `Arc<Storage>`; CLI `Tokens` reads via it, skipping the façade's RBAC + audit. Not escalation (CLI identity `local_operator` is Admin-capable) — a consistency/missing-audit gap maintainers already closed on the HTTP side (routes.rs:306-309). HTTP itself uses a fixed `default_agent()` [Read,Write], so its Read check is a no-op.
7. **Render path:** presence-only required-vars gate (null/empty values pass); only `user_template` rendered, never `system_prompt`; default Handlebars runs strict-mode + `no_escape` (raw substitution — intentional, output is LLM-bound not DOM).

## Overclaims the verify gate caught (NOT report facts)
- git-kb call graph "`search@1117 called@995`" — **wrong** (symbol-resolution artifact; two `search` symbols).
- "write-path SQL injection" — **not exploitable** (interpolated id is a typed `Uuid`; only style/footgun).
- Handlebars "injection vulnerability" — **intentional design** for LLM output.

## Gaps (out of scope of "load one prompt", not examined)
- [!] `list_prompts` / batch & paginated listing read path.
- [!] Version-history reads (`get_version`/`rollback` load path) — `row_to_version` only spot-checked.
- [!] `qdrant`-feature search engine variant (default is Hybrid; qdrant arm not traced).
