#![cfg(feature = "voice")]

use prompt_hub::PromptHub;
use prompt_hub::config::HubConfig;
use prompt_hub::models::{Prompt, VoiceOutputFormat, VoicePipelineConfig};
use std::path::Path;

#[tokio::test]
async fn test_voice_engine_wiring_in_hub() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    // Verify the voice engine exists and starts in Idle.
    let state = hub
        .get_voice_state()
        .await
        .expect("voice engine accessible");
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
        ..VoicePipelineConfig::default()
    })
    .await
    .expect("configure voice");

    // Verify config change took effect.
    let state = hub
        .get_voice_state()
        .await
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
    let history = hub.get_voice_history().await;
    assert_eq!(history.len(), 1);

    // Reset pipeline.
    hub.reset_voice_pipeline().await;
    let state = hub
        .get_voice_state()
        .await
        .expect("voice engine accessible after reset");
    assert!(matches!(
        state,
        prompt_hub::models::VoicePipelineState::Idle
    ));

    // Verify output format access.
    let fmt = hub
        .get_voice_output_format()
        .await
        .expect("output format accessible");
    assert!(matches!(fmt, VoiceOutputFormat::Mp3));
}

#[tokio::test]
async fn test_voice_turn_routes_through_hub_prompt_path() {
    let hub = PromptHub::new(Path::new(":memory:"), HubConfig::default())
        .await
        .expect("hub creation");

    // Seed a prompt that matches the orchestrator role. The transcript will be
    // processed by the hub, which should find this prompt and return its
    // user_template as the voice response.
    let prompt = Prompt::new("hello-voice", "Hello from the prompt path!");
    hub.register(prompt, &Default::default())
        .await
        .expect("register prompt");

    // Run a voice turn. The default echo STT backend will produce a transcript
    // from the synthetic audio; the hub resolver should route it through
    // process_input + get(Role::Orchestrator, ...).
    let interaction = hub
        .execute_voice_turn("hello voice")
        .await
        .expect("execute voice turn");

    assert!(interaction.stt_input.is_some());
    // The response should come from the seeded prompt's user_template.
    assert_eq!(
        interaction.tts_output,
        Some("Hello from the prompt path!".to_string())
    );
    assert!(matches!(
        interaction.playback_status,
        prompt_hub::models::VoicePlaybackStatus::Playing
    ));
}

#[tokio::test]
async fn test_voice_turn_falls_back_to_transcript_when_no_prompt_matches() {
    let hub = PromptHub::new(Path::new(":memory:"), HubConfig::default())
        .await
        .expect("hub creation");

    // No prompts registered, so the resolver should echo the transcript back.
    let interaction = hub
        .execute_voice_turn("totally unique query with no match")
        .await
        .expect("execute voice turn");

    let transcript = interaction.stt_input.expect("stt input present");
    assert_eq!(interaction.tts_output, Some(transcript));
}
