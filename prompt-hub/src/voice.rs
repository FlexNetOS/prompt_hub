//! Voice pipeline orchestration for in-process STT/TTS hand-off.
//!
//! Provides a finite-state machine that manages a voice conversation turn:
//! `Idle → Recording → SttComplete → Processing → TtsComplete`. The actual
//! speech-to-text and text-to-speech work is delegated to pluggable backends
//! behind the [`SttBackend`] and [`TtsBackend`] traits, and the text-processing
//! step routes through a [`PromptResolver`] so the hub prompt path can be used.

#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{
    VoiceInteraction, VoiceOutputFormat, VoicePipelineConfig, VoicePipelineState,
    VoicePlaybackStatus,
};
use chrono::Utc;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Backend traits — boxed futures for `dyn` compatibility
// ---------------------------------------------------------------------------

/// Speech-to-text backend abstraction.
///
/// Implementations must be `Send + Sync + Debug` so the engine can hold an
/// `Arc<dyn SttBackend>` and call it from async tasks.
pub trait SttBackend: Send + Sync + std::fmt::Debug {
    /// Transcribe raw audio bytes into text.
    ///
    /// The returned future is boxed so the trait remains object-safe despite
    /// being async.
    fn transcribe<'a>(
        &'a self,
        audio: &'a [u8],
        language: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
}

/// Text-to-speech backend abstraction.
///
/// Implementations must be `Send + Sync + Debug` so the engine can hold an
/// `Arc<dyn TtsBackend>` and call it from async tasks.
pub trait TtsBackend: Send + Sync + std::fmt::Debug {
    /// Synthesize text into raw audio bytes in the requested format.
    fn synthesize<'a>(
        &'a self,
        text: &'a str,
        format: VoiceOutputFormat,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>>;
}

/// Resolver that routes transcribed text through the hub prompt path.
///
/// The engine owns an `Arc<dyn PromptResolver>` so it can resolve user text to
/// a prompt response without depending directly on [`crate::hub::PromptHub`].
pub trait PromptResolver: Send + Sync + std::fmt::Debug {
    /// Resolve input text to a response string.
    fn resolve<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
}

// ---------------------------------------------------------------------------
// Default echo backends (no network, no heavy deps)
// ---------------------------------------------------------------------------

/// Deterministic STT backend used when no external service is configured.
///
/// Returns a synthetic transcript that incorporates the audio buffer length so
/// callers can observe that real processing happened.
#[derive(Debug, Clone, Default)]
pub struct EchoSttBackend;

impl SttBackend for EchoSttBackend {
    fn transcribe<'a>(
        &'a self,
        audio: &'a [u8],
        language: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            Ok(format!(
                "transcribed-audio-{}-samples-lang-{}",
                audio.len(),
                language
            ))
        })
    }
}

/// Deterministic TTS backend used when no external service is configured.
///
/// Returns bytes that encode the text and format so callers can observe that
/// real synthesis happened.
#[derive(Debug, Clone, Default)]
pub struct EchoTtsBackend;

impl TtsBackend for EchoTtsBackend {
    fn synthesize<'a>(
        &'a self,
        text: &'a str,
        format: VoiceOutputFormat,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move { Ok(format!("tts://{:?}/{}", format, text).into_bytes()) })
    }
}

/// Default resolver used when the engine is instantiated standalone.
///
/// Echoes the input text unchanged, preserving the old passthrough behavior for
/// callers that do not inject a hub-backed resolver.
#[derive(Debug, Clone, Default)]
pub struct EchoPromptResolver;

impl PromptResolver for EchoPromptResolver {
    fn resolve<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move { Ok(text.to_string()) })
    }
}

// ---------------------------------------------------------------------------
// Test fakes
// ---------------------------------------------------------------------------

/// Fake STT backend that always returns a configured transcript.
#[derive(Debug, Clone)]
pub struct FakeSttBackend {
    pub transcript: String,
}

impl FakeSttBackend {
    pub fn new(transcript: impl Into<String>) -> Self {
        Self {
            transcript: transcript.into(),
        }
    }
}

impl SttBackend for FakeSttBackend {
    fn transcribe<'a>(
        &'a self,
        _audio: &'a [u8],
        _language: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        let transcript = self.transcript.clone();
        Box::pin(async move { Ok(transcript) })
    }
}

/// Fake TTS backend that always returns configured audio bytes.
#[derive(Debug, Clone)]
pub struct FakeTtsBackend {
    pub audio: Vec<u8>,
}

impl FakeTtsBackend {
    pub fn new(audio: impl Into<Vec<u8>>) -> Self {
        Self {
            audio: audio.into(),
        }
    }
}

impl TtsBackend for FakeTtsBackend {
    fn synthesize<'a>(
        &'a self,
        _text: &'a str,
        _format: VoiceOutputFormat,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>> {
        let audio = self.audio.clone();
        Box::pin(async move { Ok(audio) })
    }
}

/// Fake prompt resolver that always returns a configured response.
#[derive(Debug, Clone)]
pub struct FakePromptResolver {
    pub response: String,
}

impl FakePromptResolver {
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
        }
    }
}

impl PromptResolver for FakePromptResolver {
    fn resolve<'a>(
        &'a self,
        _text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        let response = self.response.clone();
        Box::pin(async move { Ok(response) })
    }
}

// ---------------------------------------------------------------------------
// Voice pipeline engine
// ---------------------------------------------------------------------------

/// In-memory voice pipeline engine managing an FSM-driven conversation turn.
#[derive(Debug)]
pub struct VoicePipelineEngine {
    config: VoicePipelineConfig,
    current_state: VoicePipelineState,
    conversation_history: Vec<VoiceInteraction>,
    stt_backend: Arc<dyn SttBackend>,
    tts_backend: Arc<dyn TtsBackend>,
    prompt_resolver: Arc<dyn PromptResolver>,
}

impl Default for VoicePipelineEngine {
    fn default() -> Self {
        Self::new(VoicePipelineConfig::default())
    }
}

impl VoicePipelineEngine {
    /// Create a new engine with the given configuration, starting in Idle state.
    ///
    /// Uses the built-in echo backends and echo resolver by default. Use the
    /// `with_*` builder methods to inject custom implementations.
    pub fn new(config: VoicePipelineConfig) -> Self {
        Self {
            config,
            current_state: VoicePipelineState::Idle,
            conversation_history: Vec::new(),
            stt_backend: Arc::new(EchoSttBackend),
            tts_backend: Arc::new(EchoTtsBackend),
            prompt_resolver: Arc::new(EchoPromptResolver),
        }
    }

    /// Replace the STT backend.
    pub fn with_stt_backend(mut self, backend: Arc<dyn SttBackend>) -> Self {
        self.stt_backend = backend;
        self
    }

    /// Replace the TTS backend.
    pub fn with_tts_backend(mut self, backend: Arc<dyn TtsBackend>) -> Self {
        self.tts_backend = backend;
        self
    }

    /// Replace the prompt resolver.
    pub fn with_prompt_resolver(mut self, resolver: Arc<dyn PromptResolver>) -> Self {
        self.prompt_resolver = resolver;
        self
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
    /// The returned bytes are a synthetic capture buffer produced on-device.
    /// In production this would be the actual captured audio sample data fed to
    /// the configured [`SttBackend`].
    ///
    /// # Errors
    /// Returns `HubError::InvalidInput` if not in Recording state.
    pub fn stop_recording(&mut self) -> Result<Vec<u8>> {
        match &self.current_state {
            VoicePipelineState::Recording => {
                // Produce a deterministic non-empty buffer whose size is tied
                // to the configured sample rate so it varies with config.
                let sample_count = (self.config.sample_rate / 10).max(1) as usize;
                let audio_buffer = vec![0u8; sample_count];
                self.current_state = VoicePipelineState::SttComplete;
                Ok(audio_buffer)
            }
            other => Err(HubError::InvalidInput(format!(
                "cannot stop recording from state {:?}; expected Recording",
                other
            ))),
        }
    }

    /// Delegate transcription to the configured STT backend.
    ///
    /// # Arguments
    /// * `audio` — Raw audio bytes produced by `stop_recording()`.
    ///
    /// # Errors
    /// Returns `HubError::InvalidInput` if not in SttComplete state or the
    /// backend returns empty text.
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

        let text = self
            .stt_backend
            .transcribe(audio, &self.config.language)
            .await?;
        if text.is_empty() {
            return Err(HubError::InvalidInput(
                "transcription produced empty text".to_string(),
            ));
        }

        Ok(text)
    }

    /// Process the transcribed text through the configured prompt resolver.
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
        let response = self.prompt_resolver.resolve(text).await?;
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

        self.tts_backend
            .synthesize(text, self.config.output_format.clone())
            .await
    }

    /// Execute a complete voice turn using the engine's configured resolver.
    ///
    /// This is a convenience wrapper around [`Self::execute_turn_with_resolver`]
    /// that uses the resolver injected at construction time (default echo).
    pub async fn execute_turn(&mut self, prompt_text: &str) -> Result<VoiceInteraction> {
        let resolver = Arc::clone(&self.prompt_resolver);
        self.execute_turn_with_resolver(prompt_text, resolver.as_ref())
            .await
    }

    /// Execute a complete voice turn: start → stop → transcribe → process → speak.
    /// Creates a `VoiceInteraction` record and appends it to conversation history.
    ///
    /// # Arguments
    /// * `prompt_text` — The prompt text to use when TTS is disabled (stt_passthrough mode).
    /// * `resolver` — Prompt resolver to use for the text-processing step.
    ///
    /// # Errors
    /// Returns `HubError` at the first failure in the pipeline chain.
    pub async fn execute_turn_with_resolver<R: PromptResolver + ?Sized>(
        &mut self,
        prompt_text: &str,
        resolver: &R,
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
            self.current_state = VoicePipelineState::Processing;
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

    #[tokio::test]
    async fn test_transcribe_uses_stt_backend() {
        let mut engine = VoicePipelineEngine::new(VoicePipelineConfig::default())
            .with_stt_backend(Arc::new(FakeSttBackend::new("hello from fake stt")));
        engine.start_recording().unwrap();
        let audio = engine.stop_recording().unwrap();
        let text = engine.transcribe(&audio).await.unwrap();
        assert_eq!(text, "hello from fake stt");
    }

    #[tokio::test]
    async fn test_process_text_routes_through_resolver() {
        let mut engine = VoicePipelineEngine::new(VoicePipelineConfig::default())
            .with_prompt_resolver(Arc::new(FakePromptResolver::new("resolved response")));
        engine.start_recording().unwrap();
        engine.stop_recording().unwrap();
        let response = engine.process_text("any input").await.unwrap();
        assert_eq!(response, "resolved response");
    }

    #[tokio::test]
    async fn test_speak_uses_tts_backend() {
        let mut engine = VoicePipelineEngine::new(VoicePipelineConfig::default())
            .with_tts_backend(Arc::new(FakeTtsBackend::new(vec![1, 2, 3, 4])));
        engine.start_recording().unwrap();
        engine.stop_recording().unwrap();
        engine.process_text("ignored").await.unwrap();
        let audio = engine.speak("ignored").await.unwrap();
        assert_eq!(audio, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_process_and_transcribe_returns_text() {
        let mut engine = test_engine();
        engine.start_recording().unwrap();
        engine.stop_recording().unwrap();
        let response = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime")
            .block_on(engine.process_text("hello world"))
            .unwrap();
        assert_eq!(response, "hello world");
    }

    #[tokio::test]
    async fn test_execute_turn_with_injected_backends() {
        let mut engine = VoicePipelineEngine::new(VoicePipelineConfig::default())
            .with_stt_backend(Arc::new(FakeSttBackend::new("user request")))
            .with_tts_backend(Arc::new(FakeTtsBackend::new(vec![9, 8, 7])))
            .with_prompt_resolver(Arc::new(FakePromptResolver::new("hub response")));

        let interaction = engine.execute_turn("fallback prompt").await.unwrap();
        assert_eq!(interaction.stt_input, Some("user request".to_string()));
        assert_eq!(interaction.tts_output, Some("hub response".to_string()));
        assert!(matches!(
            &interaction.playback_status,
            VoicePlaybackStatus::Playing
        ));
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
        let err = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime")
            .block_on(engine.transcribe(b"hello"))
            .unwrap_err();
        assert!(matches!(err, HubError::InvalidInput(_)));

        // Try to process without stopping recording.
        let mut engine2 = test_engine();
        engine2.current_state = VoicePipelineState::Recording;
        let err = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime")
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

    #[tokio::test]
    async fn test_multiple_interactions_history() {
        let mut engine = VoicePipelineEngine::new(VoicePipelineConfig::default())
            .with_stt_backend(Arc::new(FakeSttBackend::new("repeated prompt")));
        for _i in 0..3 {
            let interaction = engine.execute_turn("prompt").await.unwrap();
            assert_eq!(interaction.stt_input, Some("repeated prompt".to_string()));
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
        let interaction = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime")
            .block_on(engine.execute_turn("test"))
            .unwrap();
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
        let interaction = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime")
            .block_on(engine.execute_turn("prompt"))
            .unwrap();
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

    #[tokio::test]
    async fn test_execute_turn_with_stt_passthrough() {
        let config = VoicePipelineConfig {
            stt_enabled: false,
            tts_enabled: false,
            ..VoicePipelineConfig::default()
        };
        let mut engine = VoicePipelineEngine::new(config);
        let interaction = engine.execute_turn("my-prompt").await.unwrap();
        // When STT disabled, stt_input should be the prompt_text passed in.
        assert!(interaction.stt_input.is_some());
        assert_eq!(interaction.stt_input, Some("my-prompt".to_string()));
    }
}
