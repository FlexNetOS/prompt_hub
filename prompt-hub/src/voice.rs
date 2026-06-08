//! Voice pipeline orchestration for in-process STT/TTS hand-off.
//!
//! Provides a finite-state machine that manages a voice conversation turn:
//! `Idle → Recording → SttComplete → Processing → TtsComplete`. No actual
//! audio processing is performed — methods are thin passthroughs designed for
//! external service integration.

#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{
    VoiceInteraction, VoiceOutputFormat, VoicePipelineConfig, VoicePipelineState,
    VoicePlaybackStatus,
};
use chrono::Utc;
use uuid::Uuid;

/// In-memory voice pipeline engine managing an FSM-driven conversation turn.
#[derive(Debug)]
pub struct VoicePipelineEngine {
    config: VoicePipelineConfig,
    current_state: VoicePipelineState,
    conversation_history: Vec<VoiceInteraction>,
}

impl Default for VoicePipelineEngine {
    fn default() -> Self {
        Self::new(VoicePipelineConfig::default())
    }
}

impl VoicePipelineEngine {
    /// Create a new engine with the given configuration, starting in Idle state.
    pub fn new(config: VoicePipelineConfig) -> Self {
        Self {
            config,
            current_state: VoicePipelineState::Idle,
            conversation_history: Vec::new(),
        }
    }

    /// Return a reference to the current FSM state.
    pub fn get_state(&self) -> &VoicePipelineState {
        &self.current_state
    }

    /// Return a slice of the full interaction history.
    pub fn get_history(&self) -> &[VoiceInteraction] {
        &self.conversation_history
    }

    /// Start recording a voice input. Transitions Idle → Recording.
    ///
    /// # Errors
    /// Returns `HubError::InvalidInput` if the pipeline is not in Idle or STT
    /// is disabled in configuration.
    pub fn start_recording(&mut self) -> Result<()> {
        if !self.config.stt_enabled {
            return Err(HubError::InvalidInput(
                "STT is disabled in voice pipeline config".to_string(),
            ));
        }

        match &self.current_state {
            VoicePipelineState::Idle => {
                self.current_state = VoicePipelineState::Recording;
                Ok(())
            }
            other => Err(HubError::InvalidInput(format!(
                "cannot start recording from state {:?}; expected Idle",
                other
            ))),
        }
    }

    /// Stop recording and return the raw audio buffer. Transitions Recording → SttComplete.
    ///
    /// The returned bytes are a passthrough stub — in production this would be
    /// the captured audio sample data fed to an STT service.
    ///
    /// # Errors
    /// Returns `HubError::InvalidInput` if not in Recording state.
    pub fn stop_recording(&mut self) -> Result<Vec<u8>> {
        match &self.current_state {
            VoicePipelineState::Recording => {
                let audio_buffer = b"passthrough-audio-buffer".to_vec();
                self.current_state = VoicePipelineState::SttComplete;
                Ok(audio_buffer)
            }
            other => Err(HubError::InvalidInput(format!(
                "cannot stop recording from state {:?}; expected Recording",
                other
            ))),
        }
    }

    /// Delegate transcription to an external STT service, storing the result.
    ///
    /// # Arguments
    /// * `audio` — Raw audio bytes produced by `stop_recording()`.
    ///
    /// # Errors
    /// Returns `HubError::InvalidInput` if not in SttComplete state or text is empty.
    pub async fn transcribe(&mut self, audio: &[u8]) -> Result<String> {
        match &self.current_state {
            VoicePipelineState::SttComplete => {}
            other => {
                return Err(HubError::InvalidInput(format!(
                    "cannot transcribe from state {:?}; expected SttComplete",
                    other
                )));
            }
        }

        // Passthrough stub: in production this calls the STT service.
        let text = String::from_utf8_lossy(audio).to_string();
        if text.is_empty() {
            return Err(HubError::InvalidInput(
                "transcription produced empty text".to_string(),
            ));
        }

        Ok(text)
    }

    /// Process the transcribed text through PromptHub and produce a response.
    /// Transitions SttComplete → Processing → TtsComplete internally.
    ///
    /// # Arguments
    /// * `text` — The STT-transcribed text to process.
    ///
    /// # Errors
    /// Returns `HubError::InvalidInput` if not in the correct state.
    pub async fn process_text(&mut self, text: &str) -> Result<String> {
        match &self.current_state {
            VoicePipelineState::SttComplete => {}
            other => {
                return Err(HubError::InvalidInput(format!(
                    "cannot process from state {:?}; expected SttComplete",
                    other
                )));
            }
        }

        self.current_state = VoicePipelineState::Processing;

        // Passthrough stub: in production this would call the prompt hub
        // API to get a response for the transcribed text.
        let response = format!("TTS-processed response for: {}", text);

        self.current_state = VoicePipelineState::TtsComplete;

        Ok(response)
    }

    /// Delegate TTS synthesis for output.
    ///
    /// # Arguments
    /// * `text` — The response text to synthesize.
    ///
    /// # Errors
    /// Returns `HubError::InvalidInput` if not in TtsComplete state or TTS is disabled.
    pub async fn speak(&self, text: &str) -> Result<Vec<u8>> {
        match &self.current_state {
            VoicePipelineState::TtsComplete => {}
            other => {
                return Err(HubError::InvalidInput(format!(
                    "cannot synthesize from state {:?}; expected TtsComplete",
                    other
                )));
            }
        }

        if !self.config.tts_enabled {
            return Err(HubError::InvalidInput(
                "TTS is disabled in voice pipeline config".to_string(),
            ));
        }

        // Passthrough stub: returns a synthesized audio buffer.
        Ok(text.as_bytes().to_vec())
    }

    /// Execute a complete voice turn: start → stop → transcribe → process → speak.
    /// Creates a `VoiceInteraction` record and appends it to conversation history.
    ///
    /// # Arguments
    /// * `prompt_text` — The prompt text to use when TTS is disabled (stt_passthrough mode).
    ///
    /// # Errors
    /// Returns `HubError` at the first failure in the pipeline chain.
    pub async fn execute_turn(&mut self, prompt_text: &str) -> Result<VoiceInteraction> {
        // Phase 1: recording (skip if STT is disabled — passthrough mode)
        let stt_text = if self.config.stt_enabled {
            self.start_recording()?;
            let audio = self.stop_recording()?;
            match &self.current_state {
                VoicePipelineState::SttComplete => self.transcribe(&audio).await?,
                _ => {
                    return Err(HubError::InvalidInput(
                        "pipeline not in SttComplete".to_string(),
                    ));
                }
            }
        } else if matches!(self.current_state, VoicePipelineState::Idle) {
            // Skip recording entirely when STT disabled
            prompt_text.to_string()
        } else {
            return Err(HubError::InvalidInput(
                "cannot execute turn in passthrough mode from non-Idle state".to_string(),
            ));
        };

        // Phase 2: process & TTS
        let (tts_output, playback_status) = if self.config.tts_enabled {
            let response = self.process_text(&stt_text).await?;
            let _audio = self.speak(&response).await?;
            (Some(response), VoicePlaybackStatus::Playing)
        } else {
            (None, VoicePlaybackStatus::Complete)
        };

        let interaction = VoiceInteraction {
            id: Uuid::new_v4(),
            stt_input: Some(stt_text),
            tts_output,
            playback_status,
            created_at: Utc::now(),
        };

        self.conversation_history.push(interaction.clone());

        // Reset to idle after the turn completes.
        self.current_state = VoicePipelineState::Idle;

        Ok(interaction)
    }

    /// Reset the pipeline back to Idle state, clearing any error state.
    pub fn reset(&mut self) {
        self.current_state = VoicePipelineState::Idle;
    }

    /// Get the current voice pipeline configuration.
    pub fn config(&self) -> &VoicePipelineConfig {
        &self.config
    }

    /// Replace the engine's configuration and return the old one.
    pub fn configure(&mut self, new_config: VoicePipelineConfig) -> VoicePipelineConfig {
        std::mem::replace(&mut self.config, new_config)
    }

    /// Get the voice output format from config.
    pub fn get_output_format(&self) -> &VoiceOutputFormat {
        &self.config.output_format
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod voice_tests {
    use super::*;
    use crate::models::{VoiceOutputFormat, VoicePipelineConfig};

    fn test_engine() -> VoicePipelineEngine {
        VoicePipelineEngine::default()
    }

    #[test]
    fn test_engine_default_creates_idle() {
        let engine = test_engine();
        assert!(matches!(engine.get_state(), &VoicePipelineState::Idle));
    }

    #[test]
    fn test_start_recording_transitions() {
        let mut engine = test_engine();
        assert!(matches!(engine.start_recording(), Ok(())));
        assert!(matches!(engine.get_state(), &VoicePipelineState::Recording));
    }

    #[test]
    fn test_stop_recording_from_idle_rejected() {
        let mut engine = test_engine();
        let err = engine.stop_recording().unwrap_err();
        assert!(matches!(err, HubError::InvalidInput(_)));
    }

    #[test]
    fn test_complete_stt_from_recording() {
        let mut engine = test_engine();
        engine.start_recording().unwrap();
        let audio = engine.stop_recording().unwrap();
        assert!(!audio.is_empty());
        assert!(matches!(
            engine.get_state(),
            &VoicePipelineState::SttComplete
        ));
    }

    #[test]
    fn test_process_and_transcribe_returns_text() {
        let mut engine = test_engine();
        engine.start_recording().unwrap();
        engine.stop_recording().unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime");
        let response = rt.block_on(engine.process_text("hello world")).unwrap();
        assert!(response.contains("TTS-processed"));
    }

    #[test]
    fn test_execute_turn_full_pipeline() {
        let mut engine = test_engine();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime");
        let interaction = rt.block_on(engine.execute_turn("fallback prompt")).unwrap();
        assert!(matches!(
            &interaction.playback_status,
            VoicePlaybackStatus::Playing
        ));
        assert!(interaction.stt_input.is_some());
    }

    #[test]
    fn test_reset_returns_to_idle() {
        let mut engine = test_engine();
        engine.current_state = VoicePipelineState::Recording;
        engine.reset();
        assert!(matches!(engine.get_state(), &VoicePipelineState::Idle));
    }

    #[test]
    fn test_wrong_state_rejected() {
        let mut engine = test_engine();
        // Try to transcribe without recording first.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime");
        let err = rt.block_on(engine.transcribe(b"hello")).unwrap_err();
        assert!(matches!(err, HubError::InvalidInput(_)));

        // Try to process without stopping recording.
        let mut engine2 = test_engine();
        engine2.current_state = VoicePipelineState::Recording;
        let err = rt.block_on(engine2.process_text("hello")).unwrap_err();
        assert!(matches!(err, HubError::InvalidInput(_)));
    }

    #[test]
    fn test_voice_config_default_values() {
        let config = VoicePipelineConfig::default();
        assert_eq!(config.max_duration_secs, 60);
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.language, "en");
        assert!(config.tts_enabled);
        assert!(config.stt_enabled);
        assert!(matches!(config.output_format, VoiceOutputFormat::Wav));
    }

    #[test]
    fn test_output_format_enum_variants() {
        let wav: VoiceOutputFormat = serde_json::from_str("\"Wav\"").unwrap();
        assert!(matches!(wav, VoiceOutputFormat::Wav));
        let mp3: VoiceOutputFormat = serde_json::from_str("\"Mp3\"").unwrap();
        assert!(matches!(mp3, VoiceOutputFormat::Mp3));
    }

    #[test]
    fn test_voice_interaction_serialization() {
        let interaction = VoiceInteraction {
            id: Uuid::new_v4(),
            stt_input: Some("hello".to_string()),
            tts_output: Some("world".to_string()),
            playback_status: VoicePlaybackStatus::Complete,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&interaction).unwrap();
        let restored: VoiceInteraction = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.stt_input, Some("hello".to_string()));
        assert_eq!(restored.tts_output, Some("world".to_string()));
    }

    #[test]
    fn test_multiple_interactions_history() {
        let mut engine = test_engine();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime");
        for _i in 0..3 {
            let interaction = rt.block_on(engine.execute_turn("prompt")).unwrap();
            assert_eq!(
                interaction.stt_input,
                Some("passthrough-audio-buffer".to_string())
            );
            let _ = interaction;
        }
        assert_eq!(engine.get_history().len(), 3);
    }

    #[test]
    fn test_stt_disabled_blocks_recording() {
        let config = VoicePipelineConfig {
            stt_enabled: false,
            ..VoicePipelineConfig::default()
        };
        let mut engine = VoicePipelineEngine::new(config);
        let err = engine.start_recording().unwrap_err();
        assert!(matches!(err, HubError::InvalidInput(_)));
        assert!(matches!(engine.get_state(), &VoicePipelineState::Idle));
    }

    #[test]
    fn test_tts_disabled_blocks_response() {
        let config = VoicePipelineConfig {
            tts_enabled: false,
            ..VoicePipelineConfig::default()
        };
        let mut engine = VoicePipelineEngine::new(config);
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime");
        let interaction = rt.block_on(engine.execute_turn("test")).unwrap();
        assert!(interaction.tts_output.is_none());
        assert!(matches!(
            interaction.playback_status,
            VoicePlaybackStatus::Complete
        ));
    }

    #[test]
    fn test_execute_turn_with_tts_disabled() {
        let config = VoicePipelineConfig {
            tts_enabled: false,
            stt_enabled: true,
            ..VoicePipelineConfig::default()
        };
        let mut engine = VoicePipelineEngine::new(config);
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime");
        let interaction = rt.block_on(engine.execute_turn("prompt")).unwrap();
        assert!(interaction.tts_output.is_none());
        assert!(matches!(
            interaction.playback_status,
            VoicePlaybackStatus::Complete
        ));
    }

    #[test]
    fn test_config_replace_returns_old() {
        let mut engine = test_engine();
        let old = engine.configure(VoicePipelineConfig {
            max_duration_secs: 120,
            ..VoicePipelineConfig::default()
        });
        assert_eq!(old.max_duration_secs, 60);
        assert_eq!(engine.config().max_duration_secs, 120);
    }

    #[test]
    fn test_get_output_format() {
        let engine = test_engine();
        assert!(matches!(
            engine.get_output_format(),
            &VoiceOutputFormat::Wav
        ));
    }

    #[test]
    fn test_execute_turn_with_stt_passthrough() {
        let config = VoicePipelineConfig {
            stt_enabled: false,
            tts_enabled: false,
            ..VoicePipelineConfig::default()
        };
        let mut engine = VoicePipelineEngine::new(config);
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime");
        let interaction = rt.block_on(engine.execute_turn("my-prompt")).unwrap();
        // When STT disabled, stt_input should be the prompt_text passed in.
        assert!(interaction.stt_input.is_some());
    }
}
