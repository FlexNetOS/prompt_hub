# Cycle 67 Architect Plan: `local-llm` Feature (P1 Recovery)

## 1. Blast Radius & Risk Assessment

### Files to Touch
| File | Change Type | Risk |
|------|------------|------|
| `prompt-hub/Cargo.toml` | Add feature + dep entries | **Low** — additive only |
| `prompt-hub/src/lib.rs` | Conditional `mod local_llm;` gate | **Low** — one-line cfg addition |
| `prompt-hub/src/models.rs` | Add `LocalModelConfig`, `ProviderKind`, `ModelStatus`, `ModelInfo` structs + derive impls | **Low** — no existing callers to break |
| `prompt-hub/src/local_llm/engine.rs` | New file — engine module with traits/types | **Low** — new code, no callers |
| `prompt-hub/src/local_llm/client.rs` | New file — HTTP client for local providers | **Low** — new code, no callers |
| `prompt-hub/src/local_llm/mod.rs` | New file — module root with re-exports | **Low** — new code, no callers |
| `prompt-hub/src/hub.rs` | Add `local_llm` field to `PromptHub` + builder methods | **Medium** — changes public façade, adds 2–3 public methods |
| `prompt-hub/Cargo.toml` `[dev-dependencies]` | No new deps needed (uses existing `reqwest` if present, else pure std `ureq`) | — |

### Caller/Impact Analysis
- **No existing callers** for any new symbol. This is a greenfield feature with zero blast radius on existing code paths.
- The only risk surface is `hub.rs` wiring: adding fields/methods to `PromptHub` doesn't break callers because we only add new public methods (no signature changes on existing ones).
- Feature flag `local_llm` means this code is **invisible** unless explicitly enabled — no default-build impact.

---

## 3. Files & Changes (exact line references)

### File 1: `prompt-hub/Cargo.toml` (features section, ~line 50+)
**Change**: Add feature gate entry.

```diff
 # ... after existing stub features around line 48 ...
+[features]
+...existing entries...
+local-llm = []
```

### File 2: `prompt-hub/src/lib.rs` (~line 90, after `load_balancer`)
**Change**: Add conditional module declaration (alphabetical order within the cfg section).

```diff
 #[cfg(feature = "learn")]
 pub mod learn;
 pub mod lineage;
+#[cfg(feature = "local-llm")]
+pub mod local_llm;
 pub mod load_balancer;
```

### File 3: `prompt-hub/src/models.rs` (~line 900, after `LLMProvider`)
**Change**: Insert new types (see design section above for exact content).
- `LocalProviderKind` enum (~10 lines)
- `LocalModelConfig` struct (~25 lines + impl block ~40 lines)
- `LocalModelHealth` enum (~8 lines)
- `ModelInfo` struct (~10 lines)

### File 4: `prompt-hub/src/local_llm/mod.rs` (NEW FILE)
**Change**: Module root with re-exports.

```rust
//! Local LLM inference — configuration, health checking, and model management for on-device
//! deployment scenarios.
//!
//! Provides a lightweight client that talks to local inference servers
//! (Ollama, llamafile, whisper.cpp) via their HTTP APIs. No model weights are embedded.

#![forbid(unsafe_code)]

mod engine;
mod inference;

pub use engine::LocalModelEngine;
pub use inference::{InferenceOptions, InferenceRequest, LocalProviderKind};
pub use crate::models::{LocalModelConfig, LocalModelHealth, ModelInfo};
```

### File 5: `prompt-hub/src/local_llm/engine.rs` (NEW FILE)
**Change**: Core engine with config management + health checking. (~200 lines)
- `LocalModelEngine` struct — holds `Vec<LocalModelConfig>`, `Arc<Mutex<Vec<ModelInfo>>>`, `reqwest::Client`
- Methods: `new()`, `add_config()`, `remove_config()`, `get_configs()`
- `refresh_health()` — async, probes each config's base URL `/api/tags` (Ollama) or `/v1/models` (OpenAI-compatible)
- `list_models()` — delegates to the first available config
- `generate()` — dispatches an inference request and returns raw JSON string

### File 6: `prompt-hub/src/local_llm/inference.rs` (NEW FILE)  
**Change**: HTTP client that maps providers to API protocols. (~150 lines)
- `LocalProviderKind` enum + Display impl (moved from models.rs for cfg-gating, or kept in models.rs and re-exported)
  - **Decision**: Keep `LocalProviderKind` in `models.rs` (always-available type, follows `Vendor` pattern) — do NOT gate it behind the feature. This is a model type shared across features.
- `InferenceRequest` struct — prompt + options as JSON-serializable payload
- `InferenceOptions` — temperature, top_p, max_tokens override for a single request
- `LocalInferenceClient` — stateless HTTP client with methods:
  - `health_check(base_url) -> async Result<LocalModelHealth>`
  - `list_models(base_url, provider) -> async Result<Vec<ModelInfo>>`
  - `generate(base_url, provider, request) -> async Result<String>`

### File 7: `prompt-hub/src/hub.rs` (~line 55, imports section)
**Change**: Add cfg-gated import after `sandbox` import.

```diff
 #[cfg(feature = "sandbox")]
 use crate::models::{Sandbox, SandboxConfig, SandboxMode};
+#[cfg(feature = "local-llm")]
+use crate::local_llm::{LocalModelConfig, LocalModelEngine, ModelInfo, LocalModelHealth};
```

### File 8: `prompt-hub/src/hub.rs` (~line 192, struct fields)
**Change**: Add field after the `voice_engine` line.

```diff
     #[cfg(feature = "voice")]
     voice_engine: std::sync::Arc<std::sync::Mutex<VoicePipelineEngine>>,
+    #[cfg(feature = "local-llm")]
+    local_llm_engine: std::sync::Arc<LocalModelEngine>,
```

### File 9: `prompt-hub/src/hub.rs` (~line 291, constructor — hub struct init)
**Change**: Add conditional construction after voice_engine.

```diff
             #[cfg(feature = "voice")]
             voice_engine: Arc::new(
                 std::sync::Mutex::new(VoicePipelineEngine::default()),
             ),
+            #[cfg(feature = "local-llm")]
+            local_llm_engine: Arc::new(LocalModelEngine::new()),
```

### File 10: `prompt-hub/src/hub.rs` (~line 580+, impl block public methods)
**Change**: Add two new public methods after the voice/sandbox section (~line 2600+).

```rust
    /// Register a local model endpoint for on-device inference.
    #[cfg(feature = "local-llm")]
    pub fn configure_local_model(&mut self, config: LocalModelConfig) {
        self.local_llm_engine.add_config(config);
    }

    /// Check the health of all configured local model endpoints.
    #[cfg(feature = "local-llm")]
    pub async fn local_model_health(&self) -> Vec<(String, LocalModelHealth)> {
        self.local_llm_engine.refresh_health().await
    }
```

---

## 4. Migrations

**None required.** This feature adds no new database schema. The `LLMProvider` struct already exists in models.rs (line 851) as the existing cloud-provider config pattern. Local model configs are managed purely in-memory via the `LocalModelEngine`. If persistence is needed later, it can reuse the existing `config/` module's TOML serialization or add a migration row to the existing storage table.

---

## 5. Test Plan

### Unit tests (in each new file)

**`local_llm/inference.rs`:**
| Test | What it verifies |
|------|-----------------|
| `test_local_provider_display` | Display impl for Ollama/Llamafile/WhisperCPP variants |
| `test_inference_request_serialization` | JSON round-trip of InferenceRequest struct |
| `test_config_builders` | Builder pattern: `LocalModelConfig::new()` with `.with_*()` chains produce expected defaults |

**`local_llm/engine.rs`:**
| Test | What it verifies |
|------|-----------------|
| `test_engine_new_defaults` | Engine starts empty, http_client initialized |
| `test_add_and_remove_config` | Add config → get_configs returns 1 item → remove → returns empty |
| `test_list_models_empty` | list_models on no configured endpoint returns empty Vec (not error) |

**Note**: No real HTTP calls — health checks are stubbed with mocked `reqwest::Client` or the test uses a local test server via `wiremock`.

### Integration test

**`prompt-hub/tests/test_local_llm.rs`** (new file):
| Test | What it verifies |
|------|-----------------|
| `test_hub_configure_local_model` | Call hub.configure_local_model() → engine has the config |
| `test_hub_local_model_health` | With wiremock server at configured URL, health check returns Healthy |
| `test_full_inference_flow` | Wiremock responds with a synthetic JSON body → generate() returns it unchanged |

---

## 6. Verify Commands

```bash
# Feature-gated check (default build must stay green)
rtk cargo check -p prompt-hub --features local-llm

# Check without the feature (must compile cleanly too)
rtk cargo check -p prompt-hub

# All features
just check

# Run tests for this feature only
rtk cargo test -p prompt-hub --features local-llm -- test_local_llm

# Clippy (must be clean with -D warnings)
just lint

# Formatting
just fmt
```

---

## 7. Acceptance Criteria (12 criteria)

1. [ ] `cargo check -p prompt-hub` (default features, **without** `local-llm`) compiles cleanly — feature-gated code is invisible when not enabled.
2. [ ] `cargo check -p prompt-hub --features local-llm` compiles cleanly.
3. [ ] `just lint` (clippy with `-D warnings`) passes on the new module and hub wiring.
4. [ ] `just fmt` reports no changes needed for the new files.
5. [ ] `LocalModelConfig::new()` creates a valid config with correct defaults (temperature=0.7, top_p=0.9, max_tokens=2048).
6. [ ] `LocalModelConfig` with `.with_temperature(1.0)` produces a struct where temperature is exactly 1.0.
7. [ ] `InferenceOptions` serializes to JSON that the Ollama `/api/generate` endpoint would accept (field names match provider spec).
8. [ ] `LocalProviderKind::Display` outputs `"ollama"`, `"llamafile"`, `"whisper-cpp"` respectively.
9. [ ] `LocalModelEngine::new()` produces an engine with zero configs and an initialized reqwest client.
10. [ ] `local_model_health()` returns empty Vec when no configs exist (no panics, no errors).
11. [ ] With a wiremock server at the configured base_url responding to `/api/tags`, `list_models()` returns a non-empty result.
12. [ ] `generate()` returns a JSON string matching the provider's response format.

---

## 8. Drift Flagged

| Item from backlog | Drift type | Rust-native translation |
|---|---|---|
| "embed lightweight LLMs" | **DRIFT**: Embedding model weights into the binary would require static linking of C/C++ code (llama.cpp has unsafe FFI). This crate is `#![forbid(unsafe_code)]`. | Local-llm is a **configuration + health + HTTP client** layer only. Model inference is delegated to running local servers via HTTP. No weights are embedded, no FFI used. |
| "Ollama/Llama.cpp integration" | Partial drift: Llama.cpp directly requires C compilation + unsafe. | Use llamafile (which provides an OpenAI-compatible HTTP server) instead — the HTTP API is identical to Ollama's for our purposes. Both map to the same `InferenceClient` protocol layer. |
| "on-device prompt generation" | This is product context, not implementation guidance. OK as-is. | N/A |
| "edge deployment scenarios" | Product scope description. OK as-is. | N/A |

---

## 9. Implementation Order (leaf-first)

1. **`prompt-hub/src/models.rs`** — Add `LocalModelConfig`, `LocalModelHealth`, `ModelInfo` types (no module deps, purely additive)
2. **`prompt-hub/Cargo.toml`** — Add `local-llm = []` feature entry
3. **`prompt-hub/src/local_llm/mod.rs`** — Module root (new file)
4. **`prompt-hub/src/local_llm/inference.rs`** — Types + HTTP client (depends only on models.rs + reqwest)
5. **`prompt-hub/src/local_llm/engine.rs`** — Engine with config management (depends on inference.rs + models.rs)
6. **`prompt-hub/src/lib.rs`** — Add `#[cfg(feature = "local-llm")] pub mod local_llm;` declaration
7. **`prompt-hub/src/hub.rs`** — Add field to struct, constructor initialization, two public methods
8. **`prompt-hub/tests/test_local_llm.rs`** — Integration tests with wiremock

Each step must compile clean (`cargo check`) before proceeding to the next.

---

## 2. Rust-Native Design Decisions

### Feature Gate Strategy
- **Feature flag**: `local-llm` (kebab-case per existing convention: `cost-limits`, `beta-program`)
- **Module path**: `crate::local_llm` — a directory module (like `sandbox.rs` was originally) containing:
  - `mod.rs` — types + re-exports
  - `engine.rs` — `LocalModelEngine` (config management + health checks)
  - `client.rs` — `LocalInferenceClient` (HTTP calls to local endpoints)
- **reqwest dependency**: `reqwest` is already in the workspace with `rustls-tls` + `json`. We add it as an optional dep gated by `local-llm` in `prompt-hub/Cargo.toml` — but since it's at workspace level without feature gating, we can simply reference it via `dep:reqwest` or use it directly. The safest path: keep the current workspace-level availability (no new Cargo.toml change for deps), only add a `[features]` entry for `local-llm = ["reqwest"]`.

### Core Types (local_llm/inference.rs — feature-gated, NOT models.rs)

**Decision**: `LocalProviderKind` and all inference-related types live in `local_llm/inference.rs`, NOT models.rs. They are only needed when the feature is enabled, following the pattern of `Vendor` (in gated `multi_provider.rs`). Only data types that cross features go in models.rs: `LocalModelConfig`, `LocalModelHealth`, `ModelInfo`.

```rust
// In models.rs — near the LLMProvider struct (~line 851)

/// Local inference providers we can target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalProviderKind {
    Ollama,
    Llamafile,
    WhisperCPP,
}

impl std::fmt::Display for LocalProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalProviderKind::Ollama => write!(f, "ollama"),
            LocalProviderKind::Llamafile => write!(f, "llamafile"),
            LocalProviderKind::WhisperCPP => write!(f, "whisper-cpp"),
        }
    }
}

/// Configuration for a single local model endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelConfig {
    /// Provider type (determines API protocol).
    pub provider: LocalProviderKind,
    /// Base URL of the local inference server
    /// (e.g. "http://localhost:11434", "http://localhost:8081/v1").
    pub base_url: String,
    /// Model identifier as understood by the provider
    /// (e.g. "llama3.2", "mistral-nemo:latest").
    pub model_name: String,
    /// Sampling temperature [0.0, 2.0].
    pub temperature: f32,
    /// Top-p sampling threshold.
    pub top_p: f32,
    /// Maximum number of tokens in the response.
    pub max_tokens: u32,
}

impl LocalModelConfig {
    pub fn new(
        provider: LocalProviderKind,
        base_url: &str,
        model_name: &str,
    ) -> Self {
        Self {
            provider,
            base_url: base_url.to_string(),
            model_name: model_name.to_string(),
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 2048,
        }
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = top_p;
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

/// Health status of a local model instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalModelHealth {
    Healthy,
    Degraded,
    Unavailable,
}

impl LocalModelHealth {
    pub fn is_healthy(&self) -> bool {
        matches!(self, LocalModelHealth::Healthy)
    }
}

/// Information about a model available on a local endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub format: String,
    pub size_bytes: u64,
    pub status: LocalModelHealth,
    pub downloaded_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

### Engine Design — `LocalModelEngine` (engine.rs)

Analogous to `VoicePipelineEngine` / `SandboxEngine`:

```rust
pub struct LocalModelEngine {
    /// User-configured model configurations.
    configs: Vec<LocalModelConfig>,
    /// Cached model registry from health checks.
    models: Arc<Mutex<Vec<ModelInfo>>>,
    http_client: reqwest::Client,
}

impl LocalModelEngine {
    pub fn new() -> Self { ... }
    pub fn add_config(&mut self, config: LocalModelConfig) -> &LocalModelConfig { ... }
    pub fn remove_config(&mut self, model_name: &str) -> Option<LocalModelConfig> { ... }
    pub fn get_configs(&self) -> &[LocalModelConfig] { ... }
    
    /// Probe the health of each configured endpoint. Updates internal state.
    pub async fn refresh_health(&self) -> Vec<(String, LocalModelHealth)> { ... }
    
    /// List models available on the first configured endpoint (Ollama-style /api/tags).
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> { ... }
    
    /// Build and dispatch an inference request using the configured model.
    /// Returns raw JSON response from the provider — caller deserializes.
    pub async fn generate(&self, prompt: &str, options: Option<InferenceOptions>) 
        -> Result<String> { ... }
}
```

### Inference Client (`client.rs`)

Thin HTTP client that maps each `LocalProviderKind` to its specific API protocol:

- **Ollama**: POST `{base_url}/api/generate` with JSON body `{"model": ..., "prompt": ..., "stream": false, "options": {...}}`
- **Llamafile**: POST `{base_url}/v1/completions` — OpenAI-compatible format
- **WhisperCPP**: POST `{base_url}/v1/audio/transcriptions` — multipart form (STT endpoint)

This is a pure HTTP layer — no unsafe, no FFI. All protocol knowledge lives in the client.

### Hub Integration

In `hub.rs`:
1. Add a `#[cfg(feature = "local-llm")]` field: `local_llm_engine: std::sync::Arc<LocalModelEngine>`
2. In `PromptHub::new()`, conditionally construct it (empty by default if no configs)
3. Add two public methods on `PromptHub`:
   - `pub fn configure_local_model(&mut self, config: LocalModelConfig)` — register a local endpoint
   - `pub async fn local_model_health(&self) -> ...` — delegate to engine

No changes to existing method signatures — purely additive.

