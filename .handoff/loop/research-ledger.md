# Research Ledger — prompt_hub prompt-load flow

Status legend: `- [ ]` unexamined · `- [~]` analyzed/unverified · `- [x]` verified · `- [!]` blocked

## Dimensions
- [x] D1 by-uuid-read-path — get_by_id → storage.get_prompt → row_to_prompt; gates + soft-delete
- [x] D2 search-backed-read-path — get → PromptHub::search(1117) → Fast/Smart/Hybrid engine SQL
- [x] D3 render-templating-path — render_prompt: load + required_vars gate + engine.render
- [x] D4 gates-rbac-asymmetry — authorize_action on get/get_by_id vs raw storage CLI bypass
- [x] D5 storage-internals — acquire() semaphore pool, shared conn, :memory: reuse, hydration parsing
- [x] D6 caching-and-hooks-on-read — is there ANY read cache? do HookRegistry/sync/metrics fire on read?

## Claims → see findings/<Dn>.md   Verdicts → see findings/verdicts.md
