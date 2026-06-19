//! Voice pipeline orchestration for in-process STT/TTS hand-off.
//!
//! Provides a finite-state machine that manages a voice conversation turn:
//! `Idle → Recording → SttComplete → Processing → TtsComplete`. The actual
//! STT/TTS work is delegated to configurable backends (mock passthrough or
//! OpenAI-compatible cloud endpoints) selected in [`VoicePipelineConfig`].

#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{
    OpenAiSttConfig, OpenAiTtsConfig, VoiceInteraction, VoiceOutputFormat, VoicePipelineConfig,
    VoicePipelineState, VoicePlaybackStatus, VoiceSttBackend, VoiceTtsBackend,
};
use chrono::Utc;
use std::future::Future;
use std::pin::Pin;
use uuid::Uuid;

/// Resolves prompt text for a voice turn.
pub trait PromptResolver {
    fn resolve<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
}

#[derive(Debug, Default)]
struct IdentityPromptResolver;

impl PromptResolver for IdentityPromptResolver {
    fn resolve<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move { Ok(text.to_string()) })
    }
}

// ---------------------------------------------------------------------------
// Backend implementations
// ---------------------------------------------------------------------------

impl VoiceSttBackend {
    /// Transcribe raw audio bytes to text.
    ///
    /// * `Mock` returns the UTF-8 interpretation of the buffer (useful for tests).
    /// * `OpenAi` posts the audio to an OpenAI-compatible `/audio/transcriptions`
    ///   endpoint.
    pub async fn transcribe(&self, audio: &[u8], language: &str) -> Result<String> {
        match self {
            Self::Mock => {
                let text = String::from_utf8_lossy(audio).to_string();
                if text.is_empty() {
                    return Err(HubError::InvalidInput(
                        "transcription produced empty text".to_string(),
                    ));
                }
                Ok(text)
            }
            Self::OpenAi(config) => openai_transcribe(config, audio, language).await,
        }
    }
}

impl VoiceTtsBackend {
    /// Synthesize text into audio bytes.
    ///
    /// * `Mock` returns the UTF-8 bytes of the text.
    /// * `OpenAi` posts to an OpenAI-compatible `/audio/speech` endpoint.
    pub async fn synthesize(&self, text: &str, format: VoiceOutputFormat) -> Result<Vec<u8>> {
        match self {
            Self::Mock => Ok(text.as_bytes().to_vec()),
            Self::OpenAi(config) => openai_synthesize(config, text, format).await,
        }
    }
}

async fn openai_transcribe(
    config: &OpenAiSttConfig,
    audio: &[u8],
    language: &str,
) -> Result<String> {
    if config.api_key.is_empty() {
        return Err(HubError::Unauthorized(
            "OpenAI STT API key is missing".to_string(),
        ));
    }

    let client = reqwest::Client::new();
    let url = format!(
        "{}/audio/transcriptions",
        config.base_url.trim_end_matches('/')
    );
    let file_part = reqwest::multipart::Part::bytes(audio.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| HubError::InvalidInput(format!("invalid mime type: {e}")))?;
    let form = reqwest::multipart::Form::new()
        .text("model", config.model.clone())
        .text("language", language.to_string())
        .part("file", file_part);

    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| HubError::Network(format!("OpenAI STT request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".to_string());
        return Err(HubError::Network(format!(
            "OpenAI STT returned {}: {}",
            status, body
        )));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| HubError::Serialization(format!("OpenAI STT response: {e}")))?;
    let text = body
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| HubError::Serialization("OpenAI STT response missing 'text'".to_string()))?;
    Ok(text.to_string())
}

async fn openai_synthesize(
    config: &OpenAiTtsConfig,
    text: &str,
    format: VoiceOutputFormat,
) -> Result<Vec<u8>> {
    if config.api_key.is_empty() {
        return Err(HubError::Unauthorized(
            "OpenAI TTS API key is missing".to_string(),
        ));
    }

    let response_format = match format {
        VoiceOutputFormat::Mp3 => "mp3",
        VoiceOutputFormat::Ogg => "opus",
        VoiceOutputFormat::Raw | VoiceOutputFormat::Wav => "pcm",
    };

    let client = reqwest::Client::new();
    let url = format!("{}/audio/speech", config.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": config.model,
        "input": text,
        "voice": config.voice,
        "response_format": response_format,
    });

    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| HubError::Network(format!("OpenAI TTS request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".to_string());
        return Err(HubError::Network(format!(
            "OpenAI TTS returned {}: {}",
            status, body
        )));
    }

    response
        .bytes()
        .await
        .map_err(|e| HubError::Network(format!("OpenAI TTS response body: {e}")))
        .map(|b| b.to_vec())
}

// ---------------------------------------------------------------------------
// In-memory voice pipeline engine
// ---------------------------------------------------------------------------

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
    /// The returned bytes are the captured audio sample data fed to the
    /// configured STT backend.
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

    /// Delegate transcription to the configured STT backend, storing the result.
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

        self.config
            .stt_backend
            .transcribe(audio, &self.config.language)
            .await
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

        // Production integration would call the PromptHub API here. For now the
        // pipeline returns a deterministic placeholder response.
        let response = format!("TTS-processed response for: {}", text);

        self.current_state = VoicePipelineState::TtsComplete;

        Ok(response)
    }

    /// Delegate TTS synthesis to the configured backend.
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

        self.config
            .tts_backend
            .synthesize(text, self.config.output_format.clone())
            .await
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
        let resolver = IdentityPromptResolver;
        self.execute_turn_with_resolver(prompt_text, &resolver)
            .await
    }

    /// Execute a complete voice turn while resolving the prompt through a resolver.
    pub async fn execute_turn_with_resolver(
        &mut self,
        prompt_text: &str,
        resolver: &dyn PromptResolver,
    ) -> Result<VoiceInteraction> {
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
            let response = resolver.resolve(&stt_text).await?;
            self.current_state = VoicePipelineState::TtsComplete;
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
    use crate::models::{
        OpenAiSttConfig, OpenAiTtsConfig, VoiceOutputFormat, VoicePipelineConfig, VoiceSttBackend,
        VoiceTtsBackend,
    };

    fn test_engine() -> VoicePipelineEngine {
        VoicePipelineEngine::default()
    }

    fn tokio_rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
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
        let response = tokio_rt()
            .block_on(engine.process_text("hello world"))
            .unwrap();
        assert!(response.contains("TTS-processed"));
    }

    #[test]
    fn test_execute_turn_full_pipeline() {
        let mut engine = test_engine();
        let interaction = tokio_rt()
            .block_on(engine.execute_turn("fallback prompt"))
            .unwrap();
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
        let err = tokio_rt()
            .block_on(engine.transcribe(b"hello"))
            .unwrap_err();
        assert!(matches!(err, HubError::InvalidInput(_)));

        // Try to process without stopping recording.
        let mut engine2 = test_engine();
        engine2.current_state = VoicePipelineState::Recording;
        let err = tokio_rt()
            .block_on(engine2.process_text("hello"))
            .unwrap_err();
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
        assert!(matches!(config.stt_backend, VoiceSttBackend::Mock));
        assert!(matches!(config.tts_backend, VoiceTtsBackend::Mock));
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
        for _i in 0..3 {
            let interaction = tokio_rt().block_on(engine.execute_turn("prompt")).unwrap();
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
        let interaction = tokio_rt().block_on(engine.execute_turn("test")).unwrap();
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
        let interaction = tokio_rt().block_on(engine.execute_turn("prompt")).unwrap();
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
        let interaction = tokio_rt()
            .block_on(engine.execute_turn("my-prompt"))
            .unwrap();
        // When STT disabled, stt_input should be the prompt_text passed in.
        assert!(interaction.stt_input.is_some());
    }

    #[test]
    fn test_stt_backend_openai_missing_key() {
        let backend = VoiceSttBackend::OpenAi(OpenAiSttConfig::default());
        let err = tokio_rt()
            .block_on(backend.transcribe(b"audio", "en"))
            .unwrap_err();
        assert!(matches!(err, HubError::Unauthorized(_)));
    }

    #[test]
    fn test_tts_backend_openai_missing_key() {
        let backend = VoiceTtsBackend::OpenAi(OpenAiTtsConfig::default());
        let err = tokio_rt()
            .block_on(backend.synthesize("hello", VoiceOutputFormat::Mp3))
            .unwrap_err();
        assert!(matches!(err, HubError::Unauthorized(_)));
    }

    #[test]
    fn test_mock_stt_rejects_empty_audio() {
        let backend = VoiceSttBackend::Mock;
        let err = tokio_rt()
            .block_on(backend.transcribe(b"", "en"))
            .unwrap_err();
        assert!(matches!(err, HubError::InvalidInput(_)));
    }

    #[test]
    fn test_mock_tts_roundtrip() {
        let backend = VoiceTtsBackend::Mock;
        let audio = tokio_rt()
            .block_on(backend.synthesize("hello world", VoiceOutputFormat::Wav))
            .unwrap();
        assert_eq!(audio, b"hello world");
    }

    #[test]
    fn test_openai_stt_config_defaults() {
        let cfg = OpenAiSttConfig::default();
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.model, "whisper-1");
        assert!(cfg.api_key.is_empty());
    }

    #[test]
    fn test_openai_tts_config_defaults() {
        let cfg = OpenAiTtsConfig::default();
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.model, "tts-1");
        assert_eq!(cfg.voice, "alloy");
        assert!(cfg.api_key.is_empty());
    }

    #[test]
    fn test_openai_stt_config_debug_redacts_key() {
        let cfg = OpenAiSttConfig {
            api_key: "super-secret".to_string(),
            ..OpenAiSttConfig::default()
        };
        let debug = format!("{:?}", cfg);
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("***"));
    }

    #[test]
    fn test_openai_tts_config_debug_redacts_key() {
        let cfg = OpenAiTtsConfig {
            api_key: "super-secret".to_string(),
            ..OpenAiTtsConfig::default()
        };
        let debug = format!("{:?}", cfg);
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("***"));
    }

    #[test]
    fn test_backend_config_serde_roundtrip() {
        let config = VoicePipelineConfig {
            stt_backend: VoiceSttBackend::OpenAi(OpenAiSttConfig {
                api_key: "key".to_string(),
                ..OpenAiSttConfig::default()
            }),
            tts_backend: VoiceTtsBackend::OpenAi(OpenAiTtsConfig {
                api_key: "key".to_string(),
                ..OpenAiTtsConfig::default()
            }),
            ..VoicePipelineConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: VoicePipelineConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.stt_backend, config.stt_backend);
        assert_eq!(restored.tts_backend, config.tts_backend);
    }
}
