//! Integration tests for the touch feature — hub wiring + dispatch pipeline.

#![cfg(feature = "touch")]

use prompt_hub::PromptHub;
use prompt_hub::config::HubConfig;
use prompt_hub::touch::{
    HapticFeedback, PinchDir, SwipeDir, TouchAction, TouchConfig, TouchEvent, gesture_to_action,
};
use std::path::Path;

/// Basic hub construction with touch feature ensures the field is wired.
#[tokio::test]
async fn test_touch_config_accessible_on_hub() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let cfg = hub.touch_config();
    let inner = cfg
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(inner.swipe_threshold, 50);
    assert_eq!(inner.tap_debounce_ms, 300);
    assert!(inner.haptic_feedback);
}

/// Dispatch a Tap event and verify the resolved action + haptic.
#[tokio::test]
async fn test_dispatch_tap() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let result = hub
        .dispatch_touch(TouchEvent::Tap)
        .await
        .expect("tap dispatch");

    assert!(matches!(result.action, TouchAction::SelectPrompt(0)));
    assert_eq!(result.count, 0);
    assert_eq!(result.haptic, Some(HapticFeedback::Tick));
}

/// Dispatch SwipeDown with haptic feedback enabled.
#[tokio::test]
async fn test_dispatch_swipe_down() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let result = hub
        .dispatch_touch(TouchEvent::Swipe(SwipeDir::Down))
        .await
        .expect("swipe down dispatch");

    assert!(matches!(result.action, TouchAction::ScrollDown));
    assert_eq!(result.haptic, Some(HapticFeedback::Vibrate));
}

/// Dispatch SwipeUp with haptic feedback enabled.
#[tokio::test]
async fn test_dispatch_swipe_up() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let result = hub
        .dispatch_touch(TouchEvent::Swipe(SwipeDir::Up))
        .await
        .expect("swipe up dispatch");

    assert!(matches!(result.action, TouchAction::ScrollUp));
    assert_eq!(result.haptic, Some(HapticFeedback::Vibrate));
}

/// Dispatch an unsupported gesture (Left swipe) and verify error.
#[tokio::test]
async fn test_dispatch_unsupported_gesture() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let err = hub
        .dispatch_touch(TouchEvent::Swipe(SwipeDir::Left))
        .await
        .unwrap_err();

    // Should be InvalidInput because Left swipe maps to None in gesture_to_action.
    assert!(
        matches!(err, prompt_hub::HubError::InvalidInput(_)),
        "expected InvalidInput, got {:?}",
        err
    );
}

/// LongPress → ExpandDetail with haptic.
#[tokio::test]
async fn test_dispatch_long_press() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let result = hub
        .dispatch_touch(TouchEvent::LongPress)
        .await
        .expect("long press dispatch");

    assert!(matches!(result.action, TouchAction::ExpandDetail));
    assert_eq!(result.haptic, Some(HapticFeedback::Tick));
}

/// PinchIn → CollapseDetail with haptic.
#[tokio::test]
async fn test_dispatch_pinch_in() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let result = hub
        .dispatch_touch(TouchEvent::Pinch(PinchDir::In))
        .await
        .expect("pinch in dispatch");

    assert!(matches!(result.action, TouchAction::CollapseDetail));
    assert_eq!(result.haptic, Some(HapticFeedback::Tick));
}

/// PinchOut → CreatePrompt with haptic.
#[tokio::test]
async fn test_dispatch_pinch_out() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let result = hub
        .dispatch_touch(TouchEvent::Pinch(PinchDir::Out))
        .await
        .expect("pinch out dispatch");

    assert!(matches!(result.action, TouchAction::CreatePrompt));
    assert_eq!(result.haptic, Some(HapticFeedback::Vibrate));
}

/// MultiTap(2) → SearchFocus with haptic.
#[tokio::test]
async fn test_dispatch_multi_tap_2() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let result = hub
        .dispatch_touch(TouchEvent::MultiTap(2))
        .await
        .expect("multi tap dispatch");

    assert!(matches!(result.action, TouchAction::SearchFocus));
    assert_eq!(result.haptic, Some(HapticFeedback::Tick));
}

/// Disabling haptic_feedback suppresses the haptic signal in results.
#[tokio::test]
async fn test_haptic_suppression() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    // Disable haptic feedback.
    hub.set_touch_config(TouchConfig {
        haptic_feedback: false,
        ..Default::default()
    });

    let result = hub
        .dispatch_touch(TouchEvent::Tap)
        .await
        .expect("tap dispatch");

    assert!(result.haptic.is_none(), "haptic should be suppressed");
}

/// Verify gesture_to_action directly covers all 7 default mappings.
#[test]
fn test_gesture_to_action_all_mappings() {
    let cfg = TouchConfig::default();

    let cases: [(TouchEvent, TouchAction); 7] = [
        (TouchEvent::Tap, TouchAction::SelectPrompt(0)),
        (TouchEvent::Swipe(SwipeDir::Down), TouchAction::ScrollDown),
        (TouchEvent::Swipe(SwipeDir::Up), TouchAction::ScrollUp),
        (TouchEvent::LongPress, TouchAction::ExpandDetail),
        (TouchEvent::Pinch(PinchDir::In), TouchAction::CollapseDetail),
        (TouchEvent::Pinch(PinchDir::Out), TouchAction::CreatePrompt),
        (TouchEvent::MultiTap(2), TouchAction::SearchFocus),
    ];

    for (gesture, expected) in &cases {
        let actual = gesture_to_action(gesture, &cfg).expect("should resolve");
        assert_eq!(&actual, expected);
    }
}

/// MultiTap(3) does NOT map to any action.
#[tokio::test]
async fn test_multi_tap_3_unsupported() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config)
        .await
        .expect("hub creation");

    let err = hub
        .dispatch_touch(TouchEvent::MultiTap(3))
        .await
        .unwrap_err();

    assert!(matches!(err, prompt_hub::HubError::InvalidInput(_)));
}
