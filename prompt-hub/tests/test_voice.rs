#![cfg(feature = "voice")]

use prompt_hub::PromptHub;
use prompt_hub::config::HubConfig;
use prompt_hub::models::{VoiceOutputFormat, VoicePipelineConfig};
use std::path::Path;

#[tokio::test]
async fn test_voice_engine_wiring_in_hub() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    // Verify the voice engine exists and starts in Idle.
    let state = hub.get_voice_state().expect("voice engine accessible");
    assert!(matches!(
        state,
        prompt_hub::models::VoicePipelineState::Idle
    ));

    // Configure voice pipeline via hub.
    hub.configure_voice(VoicePipelineConfig {
        max_duration_secs: 120,
        sample_rate: 48000,
        language: "fr".to_string(),
        tts_enabled: true,
        stt_enabled: true,
        output_format: VoiceOutputFormat::Mp3,
    })
    .expect("configure voice");

    // Verify config change took effect.
    let state = hub
        .get_voice_state()
        .expect("voice engine accessible after configure");
    assert!(matches!(
        state,
        prompt_hub::models::VoicePipelineState::Idle
    ));

    // Execute a full turn through the hub.
    let interaction = hub
        .execute_voice_turn("test prompt")
        .await
        .expect("execute voice turn");

    assert!(interaction.stt_input.is_some());
    assert!(matches!(
        interaction.playback_status,
        prompt_hub::models::VoicePlaybackStatus::Playing
    ));

    // Verify history.
    let history = hub.get_voice_history();
    assert_eq!(history.len(), 1);

    // Reset pipeline.
    hub.reset_voice_pipeline();
    let state = hub
        .get_voice_state()
        .expect("voice engine accessible after reset");
    assert!(matches!(
        state,
        prompt_hub::models::VoicePipelineState::Idle
    ));

    // Verify output format access.
    let fmt = hub
        .get_voice_output_format()
        .expect("output format accessible");
    assert!(matches!(fmt, VoiceOutputFormat::Mp3));
}
