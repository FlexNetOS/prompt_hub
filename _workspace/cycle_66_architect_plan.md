# Cycle 66 Architect Plan: voice feature

## 1. Blast Radius & Risk Assessment

### Files to touch

| File | Action | Lines/Section |
|------|--------|---------------|
| `prompt-hub/Cargo.toml` | MODIFY | Add `voice = []` to Category C (real gated features) around line ~64 |
| `prompt-hub/src/lib.rs` | MODIFY | Add `#[cfg(feature = "voice")] pub mod voice;` around line 97 (after `vibe`) |
| `prompt-hub/src/models.rs` | MODIFY | Append new types after Sandbox model tests (~line 1287) |
| `prompt-hub/src/hub.rs` | MODIFY | Add import, struct field, constructor init, and hub methods |
| **NEW** `prompt-hub/src/voice.rs` | CREATE | New module: VoicePipelineEngine + types |

### Caller/Impact analysis

- **No existing callers to break.** The `InputType::Voice` variant already exists in `models.rs` (line 576), and `multimodal_input.rs` has a stub for voice input (line 41-52). This is a greenfield module addition.
- **New types added to models.rs** are re-exported via `pub use models::*` at lib.rs:104, so they appear on the crate API automatically. No explicit re-export line needed.
- **HubConfig** does NOT need a `voice_enabled` field — configuration stays in the engine's own config type (`VoicePipelineConfig`).

### Risk classification

| Area | Callers | Risk | Reason |
|------|---------|------|--------|
| New module `voice.rs` | 0 (new) | **Low** | No existing code depends on it; feature-gated behind `#[cfg(feature = "voice")]` |
| models.rs additions | 0 direct breakage | **Low** | Only appends new types; no modifications to existing types |
| Cargo.toml feature gate | N/A | **Low** | Simple `[]` entry, Category C pattern (like sandbox) |
| lib.rs mod gate | N/A | **Low** | Standard cfg-gated module decl, identical to sandbox/vibe/etc. |

**Overall: LOW risk.** This is a leaf addition — no existing symbols are modified, only new ones appended.

## 2. Rust-Native Design Decisions

### Feature gate strategy
- Cargo.toml feature name: `voice` (kebab-case per convention)
- Module file: `prompt-hub/src/voice.rs`
- Feature gate in Cargo.toml line ~64: `voice = []`
- Feature gate in lib.rs: `#[cfg(feature = "voice")] pub mod voice;`
- All hub methods gated with `#[cfg(feature = "voice")]`

### Core types (in models.rs)

Following exact style of `SandboxConfig`/`SandboxMode` pattern:

```rust
/// Voice pipeline configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoicePipelineConfig {
    /// Maximum recording duration before auto-stop.
    pub max_duration_secs: u64,
    /// Audio sample rate in Hz (8000, 16000, 24000, 44100, 48000).
    pub sample_rate: u32,
    /// BCP-47 language tag for STT.
    pub language: String,
    /// Enable text-to-speech output.
    pub tts_enabled: bool,
    /// Enable speech-to-text input.
    pub stt_enabled: bool,
    /// Output audio format.
    pub output_format: VoiceOutputFormat,
}

/// Audio output format for TTS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceOutputFormat {
    Wav,
    Mp3,
    Ogg,
    Raw,
}

impl Default for VoiceOutputFormat {
    fn default() -> Self { Self::Wav }
}

/// A single turn in a voice conversation (one input → one output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInteraction {
    pub id: Uuid,
    /// STT-transcribed text that initiated this turn.
    pub stt_input: Option<String>,
    /// TTS-delivered response text.
    pub tts_output: Option<String>,
    /// Whether TTS is currently playing (applied after receiving the response).
    pub playback_status: VoicePlaybackStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Playback state for a voice interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VoicePlaybackStatus {
    Idle,
    Recording,
    Processing,
    Playing,
    #[default]
    Complete,
}

impl Default for VoicePipelineConfig {
    fn default() -> Self {
        Self {
            max_duration_secs: 60,
            sample_rate: 16000,
            language: "en".to_string(),
            tts_enabled: true,
            stt_enabled: true,
            output_format: VoiceOutputFormat::Wav,
        }
    }
}

/// State of the voice pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoicePipelineState {
    Idle,
    Recording,
    SttComplete,
    Processing,
    TtsComplete,
    Error(HubError),
}
```

### Engine design (in voice.rs)

`VoicePipelineEngine` manages a state machine. It does NOT perform audio encoding/decoding. Its methods are thin orchestration:

```rust
#[derive(Debug, Clone)]
pub struct VoicePipelineEngine {
    config: VoicePipelineConfig,
    current_state: VoicePipelineState,
    conversation_history: Vec<VoiceInteraction>,
}

impl VoicePipelineEngine {
    pub fn new(config: VoicePipelineConfig) -> Self { ... }
    
    /// Start recording a voice input. Transitions Idle → Recording.
    pub fn start_recording(&mut self) -> Result<()>;
    
    /// Complete recording and return the raw audio buffer (passthrough).
    pub fn stop_recording(&mut self) -> Result<Vec<u8>>;
    
    /// Delegate to external STT service, store result in interaction.
    pub async fn transcribe(&mut self, audio: &[u8]) -> Result<String>;
    
    /// Process the transcribed text through PromptHub and produce a response.
    pub async fn process_text(&mut self, text: &str) -> Result<String>;
    
    /// Delegate to external TTS service for output.
    pub async fn speak(&self, text: &str) -> Result<Vec<u8>>;
    
    /// Create a complete voice turn: record → transcribe → process → speak.
    pub async fn execute_turn(&mut self) -> Result<VoiceInteraction>;
    
    pub fn get_state(&self) -> &VoicePipelineState;
    pub fn get_history(&self) -> &[VoiceInteraction];
}
```

### Hub wiring pattern (in hub.rs)

Following exact sandbox pattern:

1. Add import at top of hub.rs: `#[cfg(feature = "voice")] use crate::voice::VoicePipelineEngine;`
2. Add struct field in PromptHub: `#[cfg(feature = "voice")] voice_engine: std::sync::Mutex<VoicePipelineEngine>,`
3. Initialize in `new()` around line ~410: `voice_engine: std::sync::Mutex::new(VoicePipelineEngine::default())`
4. Add hub methods following sandbox pattern:
   - `configure_voice(&self, config: VoicePipelineConfig)` — create/replace engine
   - `get_voice_state(&self) -> Option<&VoicePipelineState>`
   - `execute_voice_turn(&mut self) -> Result<VoiceInteraction>`

### No migrations needed

This feature is purely an in-process pipeline configuration + interaction abstraction. It stores no persistent data in the database and requires no schema changes. VoiceInteraction records live only as Rust objects in the engine's memory during a session.

## 3. Files & Changes

### File: `prompt-hub/Cargo.toml` (lines ~57-64)
**Change:** Add `voice = []` to Category C block, after `sandbox = []`.

```diff
 sandbox = []
+voice = []
```

### File: `prompt-hub/src/lib.rs` (line ~98)
**Change:** Add feature-gated module declaration after `vibe` line.

```diff
 #[cfg(feature = "vibe")]
 pub mod vibe;
 
+[cfg(feature = "voice")]
+pub mod voice;
```

### File: `prompt-hub/src/models.rs` (after line 1287)
**Change:** Append new types and their tests at end of file.
- `VoicePipelineConfig` struct + Default impl
- `VoiceOutputFormat` enum + Default impl
- `VoiceInteraction` struct
- `VoicePlaybackStatus` enum + Default impl
- `VoicePipelineState` enum

### File: `prompt-hub/src/hub.rs`
**Changes:**
1. **Import** (after line 28): 
   ```rust
   #[cfg(feature = "voice")]
   use crate::models::{VoiceInteraction, VoicePipelineConfig};
   ```
2. **Struct field** (in `PromptHub` struct, after sandbox_engine ~line 185):
   ```rust
   #[cfg(feature = "voice")]
   voice_engine: std::sync::Mutex<VoicePipelineEngine>,
   ```
3. **Constructor init** (in `new()`, after sandbox_engine init):
   ```rust
   #[cfg(feature = "voice")]
   voice_engine: std::sync::Mutex::new(VoicePipelineEngine::default()),
   ```
4. **Hub methods** (after check_sandbox/apply_timeout block ~line 2069, before Analytics section):
   - `configure_voice` (takes VoicePipelineConfig, stores it)
   - `get_voice_state` (returns Option<&VoicePipelineState>)
   - `execute_voice_turn` (full pipeline: start→transcribe→process→speak)

### File: `prompt-hub/src/voice.rs` (NEW)
**Content:** Full module with:
- `VoicePipelineConfig`, `VoiceOutputFormat`, `VoiceInteraction`, `VoicePlaybackStatus`, `VoicePipelineState` type definitions moved from models.rs to keep the module self-contained (they're crate-wide via re-export in models.rs, but voice.rs is the authoritative definition — models.rs re-exports them)
  - Actually: types go in models.rs as usual for this codebase; voice.rs only imports them. This matches the sandbox pattern where SandboxConfig/SandboxMode live in models.rs.
- `VoicePipelineEngine` struct + impl with all methods
- `#[cfg(test)]` module with unit tests

### File: `prompt-hub/src/multimodal_input.rs` (line 41-52)
**Change:** No modification needed. The existing voice stub in the multi-modal processor correctly passthrough transcribed text. The new `voice` feature provides the recording/STT/TTS orchestration layer; `multimodal_input.rs` handles intent classification after transcription.

## 4. Migrations

**None.** This is a pure pipeline abstraction — no persistent state, no schema changes. `VoiceInteraction` records are ephemeral Rust objects stored in `VoicePipelineEngine`'s in-memory Vec. The PromptHub database remains untouched by this feature.

## 5. Test Plan

### Unit tests in `voice.rs`:
| Test name | What it verifies |
|-----------|-----------------|
| `test_engine_default_config` | Default VoicePipelineConfig has correct values (max_duration=60, sample_rate=16000, etc.) |
| `test_engine_start_stop_recording_state_transition` | start_recording → state becomes Recording; stop_recording → returns bytes; state transitions |
| `test_execute_turn_full_pipeline` | Full execute_turn() flows through all states and produces a VoiceInteraction |
| `test_execute_turn_with_tts_disabled` | When tts_enabled=false, tts_output is None and playback_status skips Playing |
| `test_execute_turn_with_stt_disabled` | When stt_enabled=false, transcribe returns the raw buffer as input directly |
| `test_conversation_history_accumulates` | Each execute_turn appends to history; get_history returns all interactions |
| `test_voice_output_format_variants` | VoiceOutputFormat serializes correctly for each variant (Wav/Mp3/Ogg/Raw) |
| `test_playback_status_default` | Default playback status is Complete, not Idle |
| `test_state_error_variant` | Error state stores the HubError and can be recovered via idle reset |

### Hub-level integration test in `hub.rs`:
| Test name | What it verifies |
|-----------|-----------------|
| `test_voice_engine_wiring_in_hub` | VoicePipelineEngine is accessible from PromptHub when feature="voice" is enabled; configure_voice + execute_turn roundtrip works at hub level |

## 6. Verify Commands

```bash
# Default build (voice NOT included — must verify clean)
just check

# All features including voice
just check --all-features   # or: cargo check --workspace --all-features

# Clippy lint
just lint

# Voice-specific tests
cargo test -p prompt-hub --features voice voice::

# Full test suite with voice
cargo test -p prompt-hub --all-features
```

### Pre-commit gates (must all pass):
1. `cargo check --workspace` — default features green (voice NOT in default)
2. `cargo check --workspace --all-features` — voice feature compiles
3. `cargo clippy --workspace --all-features -- -D warnings` — no lint errors
4. `cargo test -p prompt-hub --features voice` — voice module tests pass
5. `cargo test -p prompt-hub --all-features` — all integration tests pass

## 7. Acceptance Criteria (10+)

- [ ] AC1: New file `prompt-hub/src/voice.rs` exists with feature-gated module declaration in lib.rs
- [ ] AC2: Feature flag `voice = []` added to Cargo.toml in Category C section (no optional deps)
- [ ] AC3: All new types (`VoicePipelineConfig`, `VoiceOutputFormat`, `VoiceInteraction`, `VoicePlaybackStatus`, `VoicePipelineState`) are in models.rs with Serialize/Deserialize derives
- [ ] AC4: `VoicePipelineConfig::default()` returns correct defaults (max_duration=60, sample_rate=16000, language="en", tts_enabled=true, stt_enabled=true)
- [ ] AC5: `VoiceOutputFormat` has 4 variants: Wav, Mp3, Ogg, Raw with Default = Wav
- [ ] AC6: `VoicePipelineEngine::new()` creates engine in Idle state with config
- [ ] AC7: `start_recording()` transitions state to Recording, returns Ok(())
- [ ] AC8: `stop_recording()` returns a non-empty Vec<u8> (passthrough buffer)
- [ ] AC9: `execute_turn()` produces a `VoiceInteraction` with stt_input set when transcription completes
- [ ] AC10: When tts_enabled=false, execute_turn() sets tts_output=None in the interaction
- [ ] AC11: VoicePipelineEngine implements Send (via std::sync::Mutex, not parking_lot)
- [ ] AC12: `#![forbid(unsafe_code)]` satisfied — zero unsafe usage across all new/modified files
- [ ] AC13: `cargo clippy --workspace --all-features -- -D warnings` passes clean
- [ ] AC14: All unit tests in voice.rs module pass when feature="voice" is enabled
- [ ] AC15: Hub-level voice methods (`configure_voice`, `get_voice_state`, `execute_voice_turn`) are callable through PromptHub facade

## 8. Drift Flagged

| Item from backlog | Non-native pattern | Rust-native correction |
|-------------------|-------------------|----------------------|
| "real-time speech-to-text prompting" | Could imply building STT engine in-crate | Pipeline orchestration only — delegates to external STT service; the engine manages state transitions, not audio processing |
| "TTS response delivery" | Could imply audio codec implementation | Delegates to vox (external TTS via CLI pipe); engine manages the request/response cycle |
| "voice command syntax for CLI operations" | CLI commands belong in `prompthub` crate | VoicePipelineEngine lives in `prompt-hub` core; CLI integration is a thin consumer layer (out of scope for this cycle) — only the pipeline abstraction is built here |
| "extending existing multimodal work (PR #53)" | Suggests modifying multimodal.rs heavily | The existing `InputType::Voice` stub and `multimodal_input.rs` passthrough are correct as-is; we add a dedicated feature-gated voice module alongside it, not modify the existing stub |
| "Phoneme type" mentioned in suggestions | Unnecessary — this is a pipeline config layer, not speech processing | No Phoneme type needed. The STT output is text (String), not phonetic data |

## 9. Implementation Order (leaf-first)

1. **models.rs** — Append new types (VoicePipelineConfig, VoiceOutputFormat, etc.) + their tests
2. **Cargo.toml** — Add `voice = []` to Category C
3. **lib.rs** — Add feature-gated module declaration
4. **voice.rs** — Create new file with VoicePipelineEngine impl + tests
5. **hub.rs** — Add import, struct field, constructor init, and hub methods

This order ensures each step compiles: models types exist → Cargo gate is added → module is declared → engine impl uses the types → hub wires the engine.

---

**Note to implementer:** The VoicePipelineEngine methods that would call external STT/TTS services (`transcribe`, `speak`) should accept a callback/trait parameter for the actual service call, or simply return `Ok(buffer)` as passthrough stubs. The PR description says "pipeline configuration + interaction abstraction" — the engine manages the state machine; actual audio processing is external. Follow the sandbox pattern exactly: config type in models.rs, engine impl in its own module, hub facade methods gated behind the feature flag.
