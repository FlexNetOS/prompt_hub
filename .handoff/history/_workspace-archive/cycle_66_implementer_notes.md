# Cycle 66 Implementer Notes: voice feature

## Summary

Implemented the `voice` feature for prompt_hub as a pure in-process pipeline orchestration abstraction. No database migrations needed — all state is ephemeral Rust objects managed by an FSM-driven `VoicePipelineEngine`.

## Changes Made

### Files Created
| File | Lines | Description |
|------|-------|-------------|
| `prompt-hub/src/voice.rs` | ~470 | New module: `VoicePipelineEngine` with 18 unit tests |
| `prompt-hub/tests/test_voice.rs` | ~65 | Hub-level integration test |

### Files Modified
| File | Change |
|------|--------|
| `prompt-hub/Cargo.toml` | Added `voice = []` to Category C (after sandbox, line 65) |
| `prompt-hub/src/lib.rs` | Added `#[cfg(feature = "voice")] pub mod voice;` gate (line 98) |
| `prompt-hub/src/models.rs` | Appended 5 new types at crate module level (~100 lines): `VoiceOutputFormat`, `VoicePipelineConfig`, `VoicePlaybackStatus`, `VoiceInteraction`, `VoicePipelineState` |
| `prompt-hub/src/hub.rs` | Added import (line 29-31), struct field (line 190-191), constructor init (line 287-288), and 6 public methods (lines 2081-2145) behind `#[cfg(feature = "voice")]` |

## Deviations from Architect Plan

### 1. `VoicePipelineState::Error(HubError)` variant removed
The architect plan included an `Error(HubError)` variant in `VoicePipelineState`. This was not possible because `HubError` doesn't implement `PartialEq`, which is required by the derive on `VoicePipelineState`. The architect plan also had this state as `#[derive(Debug, Clone, PartialEq, Eq)]` — I removed `Eq` (matching what was needed), but the Error variant still prevents `PartialEq` because it holds a non-eq-able type. Removed the variant entirely and used `#[derive(Default)]` with `#[default] Idle` to satisfy clippy's `derivable_impls` lint instead of manual `impl Default`.

### 2. Hub engine uses `Arc<Mutex<VoicePipelineEngine>>` instead of bare `Mutex`
The architect plan specified `std::sync::Mutex<VoicePipelineEngine>` as the struct field type. This caused a clippy `await_holding_lock` error when exposing an async hub method (`execute_voice_turn`) because the lock must be held across an await point. Fixed by wrapping in `Arc` and using `std::thread::spawn` + `tokio::sync::oneshot` channel to avoid holding the lock across the async boundary. This matches the pattern used elsewhere in the crate for similar cases (e.g., sandbox uses `Arc<SandboxEngine>`).

### 3. `execute_turn` handles STT-disabled passthrough
The architect plan showed `start_recording()` followed by `stop_recording()` unconditionally, then conditionally checking `stt_enabled`. This would fail when both STT and TTS are disabled because `start_recording()` rejects the operation. Fixed by skipping the recording phase entirely when `stt_enabled=false` and current state is Idle (passthrough mode).

### 4. Voice types placed at crate module level in models.rs
The architect plan suggested appending types after "Sandbox model tests" but I placed them *before* the `mod model_tests { ... }` block so they're actually visible as public crate types. Placing them inside the test module (as the line number hint would suggest) would have made them inaccessible to all other modules.

## Verification Results

| Gate | Result |
|------|--------|
| `cargo check -p prompt-hub` (default, voice OFF) | PASS |
| `cargo check --workspace --all-features` (voice ON) | PASS |
| `cargo clippy --workspace --all-features -- -D warnings` | PASS (no issues) |
| `cargo test -p prompt-hub --all-features voice::` | 18 passed |
| `test_voice_engine_wiring_in_hub` | 1 passed |
| `cargo test --workspace --all-features` | 793 passed, 2 ignored (pre-existing) |

## Test Coverage

### Unit tests in voice.rs (18 total):
- `test_engine_default_creates_idle` — default state is Idle
- `test_start_recording_transitions` — start -> Recording
- `test_stop_recording_from_idle_rejected` — wrong-state rejection
- `test_complete_stt_from_recording` — recording -> SttComplete + non-empty buffer
- `test_process_and_transcribe_returns_text` — full FSM transition with text response
- `test_execute_turn_full_pipeline` — complete turn with TTS enabled (Playing status)
- `test_reset_returns_to_idle` — reset clears any state
- `test_wrong_state_rejected` — transcribe/process from wrong states
- `test_voice_config_default_values` — all default values verified
- `test_output_format_enum_variants` — Wav/Mp3 serialization round-trip
- `test_voice_interaction_serialization` — full JSON round-trip
- `test_multiple_interactions_history` — history accumulation (3 turns)
- `test_stt_disabled_blocks_recording` — STT disabled blocks start_recording
- `test_tts_disabled_blocks_response` — TTS disabled => tts_output=None, Complete status
- `test_execute_turn_with_tts_disabled` — full turn with TTS off
- `test_config_replace_returns_old` — configure returns previous config
- `test_get_output_format` — output format accessor
- `test_execute_turn_with_stt_passthrough` — STT disabled passthrough mode

### Hub-level integration test:
- `test_voice_engine_wiring_in_hub` — full lifecycle: create -> configure -> execute_turn -> history -> reset -> format access

## Follow-ups

1. CLI integration: The `prompthub` binary should gain voice subcommands (e.g., `prompthub voice record`) as a thin consumer layer over the core engine.
2. Server HTTP endpoint: The `prompthub-server` axum layer should expose `/voice/turn` for external TTS/STT service integration.
3. External service callbacks: The passthrough methods (`transcribe`, `speak`) accept no callback/trait parameter yet — this is by design per the plan ("pipeline configuration + interaction abstraction"), but should be addressed when STT/TTS services are integrated.
4. Consider adding `#[test]` for `VoiceInteraction` with all None variants (no stt_input, no tts_output).
