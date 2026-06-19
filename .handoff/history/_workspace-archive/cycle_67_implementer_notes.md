# Cycle 67 Implementer Notes: `local-llm` Feature

## What Changed

### New Files
| File | Description |
|------|-------------|
| `prompt-hub/src/local_llm/mod.rs` | Module root with re-exports |
| `prompt-hub/src/local_llm/engine.rs` | `LocalModelEngine` — config CRUD, health checks, inference dispatch (9 tests) |
| `prompt-hub/src/local_llm/inference.rs` | `LocalInferenceClient` — Ollama/llamafile protocol mapping + types (4 tests) |

### Modified Files
| File | Change |
|------|--------|
| `prompt-hub/Cargo.toml` | Added `local-llm = ["dep:reqwest"]` to Category C features (after `voice`) |
| `prompt-hub/src/models.rs` | Added 4 types: `LocalProviderKind` enum + Display, `LocalModelConfig` struct + builder impl, `LocalModelHealth` enum + `is_healthy()`, `ModelInfo` struct (~75 lines total) |
| `prompt-hub/src/lib.rs` | Added `#[cfg(feature = "local-llm")] pub mod local_llm;` |
| `prompt-hub/src/hub.rs` | Added field `local_model_config: Arc<Mutex<Vec<LocalModelConfig>>>`, constructor init, two public methods (`configure_local_model`, `local_model_health`), internal helper `health_check_local`. Also added `info!` log line. |

## Verification Results

| Gate | Result |
|------|--------|
| `cargo check -p prompt-hub` (default, no local-llm) | PASS |
| `cargo check -p prompt-hub --features local-llm` | PASS |
| `cargo clippy -p prompt-hub --all-targets --all-features -- -D warnings` | PASS (no issues) |
| `cargo fmt -- --check` | PASS (no changes needed) |
| `cargo check --workspace` | PASS |
| `cargo test -p prompt-hub --all-features -- local_llm` | 13/13 passed |

### Test Coverage (13 tests)
**engine.rs (9):**
- `test_engine_new_defaults` — zero configs on construction
- `test_add_and_remove_config` — CRUD lifecycle
- `test_duplicate_rejected` — duplicate model name guard
- `test_health_check_no_configs` — no panics with empty configs
- `test_list_models_empty` — empty Vec, not error
- `test_default_config_values` — temperature=0.7, top_p=0.9, max_tokens=2048
- `test_config_builders` — `.with_*()` chains override defaults
- `test_generate_no_config_error` — error when no config registered
- `test_multiple_configs` — two configs can coexist

**inference.rs (4):**
- `test_local_provider_display` — "ollama", "llamafile", "whisper-cpp"
- `test_inference_request_serialization` — JSON round-trip, skip_serializing_if
- `test_default_false_stream` — stream=false present in output
- `test_health_is_healthy` — matches only Healthy variant

## Deviations from Architect Plan (Cycle 67)

| Item | Planner Said | Actually Done | Reason |
|------|-------------|---------------|--------|
| File naming | `client.rs` | `inference.rs` | User instructions specified `inference.rs`; followed those |
| Feature gate | `local_llm = []` | `local-llm = ["dep:reqwest"]` | Needs reqwest dep to compile; kebab-case per existing convention |
| Hub field type | `Arc<LocalModelEngine>` | `Arc<Mutex<Vec<LocalModelConfig>>>` | User instructions specified this simpler approach (direct config list) |
| Hub methods | 2 (`configure_local_model`, `local_model_health`) | Same + internal `health_check_local` helper | Helper needed to avoid reqwest dep at non-feature-gated scope |
| LocalProviderKind in models.rs | Planner debated it; decided on models.rs after all | In models.rs (always available) | Shared type, not feature-specific — follows Vendor pattern |
| Default max_tokens | 512 (spec) vs 2048 (plan) | 2048 | Following the Rust-native design decisions in the plan's types section |
| Drift on "embed LLMs" | Planner flagged | Noted: local-llm is HTTP client only, no FFI/weights | Acknowledged and followed |

## Notes for Follow-ups
- No integration tests with wiremock (would require adding `wiremock` as dev-dep)
- The hub field is a bare `Vec<LocalModelConfig>` in a Mutex — the engine module exists but isn't wired into hub.rs yet. This matches the user's instructions exactly.
- `LocalModelHealth::Degraded` is a unit variant (no String payload). If reason tracking is needed later, change to `Degraded(String)`.
