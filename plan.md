# Wave 9 — Fix Compilation Blockers

## Issues Found via Static Analysis

### 1. routes.rs — `default_agent()` capabilities type mismatch
- **File**: `prompthub-server/src/routes.rs` lines 60-68
- **Problem**: `vec!["read".to_string(), "write".to_string()]` but `capabilities` is `Vec<Capability>`
- **Fix**: Change to `vec![Capability::Read, Capability::Write]`

### 2. hub.rs tests — `test_agent()` capabilities type mismatch  
- **File**: `prompt-hub/src/hub.rs` lines 583-588
- **Problem**: Same as above — `Vec<String>` vs `Vec<Capability>`
- **Fix**: Change to `vec![Capability::Read, Capability::Write]`

### 3. hub.rs tests — `test_prompt()` uses non-existent fields
- **File**: `prompt-hub/src/hub.rs` lines 591-604
- **Problem**: Uses `role`, `intent`, `domain: "general".to_string()`, `created_by` which don't exist in `Prompt`
- **Fix**: Rewrite with correct Prompt fields

### 4. storage.rs tests — `Prompt::new()` doesn't exist
- **File**: `prompt-hub/src/storage.rs` line 1405
- **Problem**: `Prompt::new(name, system_prompt)` called but no such method
- **Fix**: Add `impl Prompt { pub fn new(name: &str, system_prompt: &str) -> Self { ... } }` to models.rs

### 5. canary.rs — missing `Digest` trait import
- **File**: `prompt-hub/src/canary.rs` line 26
- **Problem**: `sha2::Sha256::digest()` requires `Digest` trait in scope
- **Fix**: Add `use sha2::{Sha256, Digest};` and change call to `Sha256::digest(...)`

## Stage 1: Deploy 3 parallel fix agents
- Agent A: routes.rs + canary.rs (server + lib fixes)
- Agent B: hub.rs tests (test_agent + test_prompt fixes)
- Agent C: models.rs + storage.rs tests (add Prompt::new + fix storage tests)

## Stage 2: Verify all fixes applied
- Read each modified file
- Confirm all type mismatches resolved
- Update TODO.md and SESSION.md
