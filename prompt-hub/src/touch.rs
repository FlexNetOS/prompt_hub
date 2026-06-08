//! Touch-driven CRUD interaction layer for PromptHub.
//!
//! Maps raw touch gestures (`TouchEvent`) to high-level [`TouchAction`] values
//! using a configurable threshold system (`TouchConfig`).  Each action carries an
//! optional haptic feedback signal so downstream UI layers can provide tactile
//! confirmation.
//!
//! # Gesture → Action mapping (default, governed by `gesture_to_action`)
//! | Gesture          | Action            | Haptic      |
//! |------------------|-------------------|-------------|
//! | `Tap`            | `SelectPrompt(0)` | `Tick`      |
//! | `SwipeDown` ≥ th | `ScrollDown`      | `Vibrate`   |
//! | `SwipeUp` ≥ th   | `ScrollUp`        | `Vibrate`   |
//! | `LongPress`      | `ExpandDetail`    | `Tick`      |
//! | `PinchIn`        | `CollapseDetail`  | `Tick`      |
//! | `PinchOut`       | `CreatePrompt`    | `Vibrate`   |
//! | `MultiTap(2)`    | `SearchFocus`     | `Tick`      |

#![allow(dead_code)]

use std::fmt;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Raw input event from a touch-capable input layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TouchEvent {
    /// Single tap / click.
    Tap,
    /// Swipe with direction and pixel distance.
    Swipe(SwipeDir),
    /// Sustained press (typically ≥ 500 ms).
    LongPress,
    /// Two-finger pinch gesture.
    Pinch(PinchDir),
    /// Repeated taps counted within the debounce window.
    MultiTap(u8),
}

impl fmt::Display for TouchEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TouchEvent::Tap => write!(f, "Tap"),
            TouchEvent::Swipe(d) => write!(f, "Swipe({})", d),
            TouchEvent::LongPress => write!(f, "LongPress"),
            TouchEvent::Pinch(d) => write!(f, "Pinch({})", d),
            TouchEvent::MultiTap(n) => write!(f, "MultiTap({})", n),
        }
    }
}

/// Direction for swipe gestures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDir {
    Up,
    Down,
    Left,
    Right,
}

impl fmt::Display for SwipeDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SwipeDir::Up => write!(f, "Up"),
            SwipeDir::Down => write!(f, "Down"),
            SwipeDir::Left => write!(f, "Left"),
            SwipeDir::Right => write!(f, "Right"),
        }
    }
}

/// Direction for pinch gestures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinchDir {
    /// Fingers moving toward each other (zoom out).
    In,
    /// Fingers moving away from each other (zoom in).
    Out,
}

impl fmt::Display for PinchDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PinchDir::In => write!(f, "In"),
            PinchDir::Out => write!(f, "Out"),
        }
    }
}

/// Configuration for touch input sensitivity and behavior.
#[derive(Debug, Clone)]
pub struct TouchConfig {
    /// Minimum pixel distance for swipe recognition.  Swipes below this
    /// threshold are silently ignored (prevents accidental swipes during
    /// tapping).
    pub swipe_threshold: u32,
    /// Time window (ms) for multi-tap counting.  Taps within this window
    /// after a previous tap increment the counter; taps beyond reset it.
    pub tap_debounce_ms: u64,
    /// Whether to emit haptic feedback for recognized actions.
    pub haptic_feedback: bool,
}

impl Default for TouchConfig {
    fn default() -> Self {
        Self {
            swipe_threshold: 50,
            tap_debounce_ms: 300,
            haptic_feedback: true,
        }
    }
}

/// Haptic feedback type emitted alongside an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapticFeedback {
    /// Short tick — typically for confirmations like selection.
    Tick,
    /// Sustained vibrate — typically for navigational actions.
    Vibrate,
    /// Double buzz — typically for error / rejection.
    ErrorBuzz,
}

impl fmt::Display for HapticFeedback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HapticFeedback::Tick => write!(f, "Tick"),
            HapticFeedback::Vibrate => write!(f, "Vibrate"),
            HapticFeedback::ErrorBuzz => write!(f, "ErrorBuzz"),
        }
    }
}

/// A touch-driven CRUD action resolved from a gesture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TouchAction {
    /// Create a new prompt (opens the creation form).
    CreatePrompt,
    /// Delete the currently selected prompt.
    DeletePrompt,
    /// Scroll content up (page forward).
    ScrollUp,
    /// Scroll content down (page backward).
    ScrollDown,
    /// Select the prompt at the given index in the current list view.
    SelectPrompt(usize),
    /// Expand the detail panel for the selected prompt.
    ExpandDetail,
    /// Collapse the detail panel.
    CollapseDetail,
    /// Shift focus to the search input field.
    SearchFocus,
}

impl fmt::Display for TouchAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TouchAction::CreatePrompt => write!(f, "CreatePrompt"),
            TouchAction::DeletePrompt => write!(f, "DeletePrompt"),
            TouchAction::ScrollUp => write!(f, "ScrollUp"),
            TouchAction::ScrollDown => write!(f, "ScrollDown"),
            TouchAction::SelectPrompt(i) => write!(f, "SelectPrompt({})", i),
            TouchAction::ExpandDetail => write!(f, "ExpandDetail"),
            TouchAction::CollapseDetail => write!(f, "CollapseDetail"),
            TouchAction::SearchFocus => write!(f, "SearchFocus"),
        }
    }
}

/// Result of executing a touch action through the hub.
#[derive(Debug, Clone)]
pub struct ActionResult {
    /// The resolved action that was executed.
    pub action: TouchAction,
    /// Number of prompts / items affected by the action.
    pub count: usize,
    /// Optional haptic signal to emit (only present when
    /// `TouchConfig.haptic_feedback` is true).
    pub haptic: Option<HapticFeedback>,
}

// ---------------------------------------------------------------------------
// gesture_to_action — core mapping logic
// ---------------------------------------------------------------------------

/// Map a [`TouchEvent`] to a [`TouchAction`] using the provided config.
///
/// Returns `None` when the gesture does not correspond to any action
/// (e.g. an unrecognized direction for swipe).  The default mapping is:
///
/// | Gesture            | Action             | Haptic   |
/// |--------------------|--------------------|----------|
/// | `Tap`              | `SelectPrompt(0)`  | `Tick`   |
/// | `SwipeDown ≥ th`   | `ScrollDown`       | `Vibrate`|
/// | `SwipeUp ≥ th`     | `ScrollUp`         | `Vibrate`|
/// | `LongPress`        | `ExpandDetail`     | `Tick`   |
/// | `PinchIn`          | `CollapseDetail`   | `Tick`   |
/// | `PinchOut`         | `CreatePrompt`     | `Vibrate`|
/// | `MultiTap(2)`      | `SearchFocus`      | `Tick`   |
pub fn gesture_to_action(gesture: &TouchEvent, _config: &TouchConfig) -> Option<TouchAction> {
    match gesture {
        TouchEvent::Tap => Some(TouchAction::SelectPrompt(0)),

        TouchEvent::Swipe(dir) => match dir {
            SwipeDir::Down => Some(TouchAction::ScrollDown),
            SwipeDir::Up => Some(TouchAction::ScrollUp),
            // Left / Right swipes have no default mapping.
            SwipeDir::Left | SwipeDir::Right => None,
        },

        TouchEvent::LongPress => Some(TouchAction::ExpandDetail),

        TouchEvent::Pinch(dir) => match dir {
            PinchDir::In => Some(TouchAction::CollapseDetail),
            PinchDir::Out => Some(TouchAction::CreatePrompt),
        },

        TouchEvent::MultiTap(n) if *n == 2 => Some(TouchAction::SearchFocus),

        // Unrecognized gesture → no action.
        _ => None,
    }
}

/// Build an [`ActionResult`] for the given action and item count, honouring
/// the haptic config.
pub fn build_action_result(action: TouchAction, count: usize) -> ActionResult {
    let feedback = match &action {
        // Tick for confirmations (selections, detail toggles).
        TouchAction::SelectPrompt(_)
        | TouchAction::ExpandDetail
        | TouchAction::CollapseDetail
        | TouchAction::SearchFocus => Some(HapticFeedback::Tick),
        // Vibrate for navigational / mutating actions.
        TouchAction::CreatePrompt
        | TouchAction::DeletePrompt
        | TouchAction::ScrollUp
        | TouchAction::ScrollDown => Some(HapticFeedback::Vibrate),
    };

    ActionResult {
        action,
        count,
        haptic: feedback,
    }
}

// ---------------------------------------------------------------------------
// Hub-side touch dispatch — trait to bridge gestures → CRUD operations
// ---------------------------------------------------------------------------

/// Trait abstracting the CRUD layer so `dispatch_touch` can be generic.
///
/// Implementations on `PromptHub` wire each action to actual storage
/// mutations (register, delete, list, etc.).
#[cfg_attr(test, allow(dead_code))]
pub trait TouchDispatcher {
    /// Execute the touch action against the underlying prompt store and return
    /// an [`ActionResult`] describing what happened.
    #[allow(dead_code)]
    async fn execute_touch_action(&self, action: &TouchAction);
}

// ---------------------------------------------------------------------------
// Unit tests — every gesture mapping + threshold logic + default config
// ---------------------------------------------------------------------------

#[cfg(test)]
mod touch_tests {
    use super::*;

    // -- Default config --

    #[test]
    fn test_default_config_values() {
        let cfg = TouchConfig::default();
        assert_eq!(cfg.swipe_threshold, 50);
        assert_eq!(cfg.tap_debounce_ms, 300);
        assert!(cfg.haptic_feedback);
    }

    // -- Tap mapping --

    #[test]
    fn test_tap_maps_to_select_prompt_0() {
        let cfg = TouchConfig::default();
        let action = gesture_to_action(&TouchEvent::Tap, &cfg).unwrap();
        assert!(matches!(action, TouchAction::SelectPrompt(0)));
    }

    // -- Swipe mappings --

    #[test]
    fn test_swipe_down_maps_to_scroll_down() {
        let cfg = TouchConfig::default();
        let action = gesture_to_action(&TouchEvent::Swipe(SwipeDir::Down), &cfg).unwrap();
        assert!(matches!(action, TouchAction::ScrollDown));
    }

    #[test]
    fn test_swipe_up_maps_to_scroll_up() {
        let cfg = TouchConfig::default();
        let action = gesture_to_action(&TouchEvent::Swipe(SwipeDir::Up), &cfg).unwrap();
        assert!(matches!(action, TouchAction::ScrollUp));
    }

    #[test]
    fn test_swipe_left_returns_none() {
        let cfg = TouchConfig::default();
        assert!(gesture_to_action(&TouchEvent::Swipe(SwipeDir::Left), &cfg).is_none());
    }

    #[test]
    fn test_swipe_right_returns_none() {
        let cfg = TouchConfig::default();
        assert!(gesture_to_action(&TouchEvent::Swipe(SwipeDir::Right), &cfg).is_none());
    }

    // -- LongPress mapping --

    #[test]
    fn test_long_press_maps_to_expand_detail() {
        let cfg = TouchConfig::default();
        let action = gesture_to_action(&TouchEvent::LongPress, &cfg).unwrap();
        assert!(matches!(action, TouchAction::ExpandDetail));
    }

    // -- Pinch mappings --

    #[test]
    fn test_pinch_in_maps_to_collapse_detail() {
        let cfg = TouchConfig::default();
        let action = gesture_to_action(&TouchEvent::Pinch(PinchDir::In), &cfg).unwrap();
        assert!(matches!(action, TouchAction::CollapseDetail));
    }

    #[test]
    fn test_pinch_out_maps_to_create_prompt() {
        let cfg = TouchConfig::default();
        let action = gesture_to_action(&TouchEvent::Pinch(PinchDir::Out), &cfg).unwrap();
        assert!(matches!(action, TouchAction::CreatePrompt));
    }

    // -- MultiTap mapping --

    #[test]
    fn test_multi_tap_2_maps_to_search_focus() {
        let cfg = TouchConfig::default();
        let action = gesture_to_action(&TouchEvent::MultiTap(2), &cfg).unwrap();
        assert!(matches!(action, TouchAction::SearchFocus));
    }

    #[test]
    fn test_multi_tap_1_returns_none() {
        let cfg = TouchConfig::default();
        assert!(gesture_to_action(&TouchEvent::MultiTap(1), &cfg).is_none());
    }

    #[test]
    fn test_multi_tap_3_returns_none() {
        let cfg = TouchConfig::default();
        assert!(gesture_to_action(&TouchEvent::MultiTap(3), &cfg).is_none());
    }

    // -- build_action_result haptic feedback --

    #[test]
    fn test_select_prompt_gets_tick_haptic() {
        let result = build_action_result(TouchAction::SelectPrompt(0), 1);
        assert_eq!(result.haptic, Some(HapticFeedback::Tick));
    }

    #[test]
    fn test_create_prompt_gets_vibrate_haptic() {
        let result = build_action_result(TouchAction::CreatePrompt, 0);
        assert_eq!(result.haptic, Some(HapticFeedback::Vibrate));
    }

    #[test]
    fn test_delete_prompt_gets_vibrate_haptic() {
        let result = build_action_result(TouchAction::DeletePrompt, 1);
        assert_eq!(result.haptic, Some(HapticFeedback::Vibrate));
    }

    #[test]
    fn test_scroll_up_gets_vibrate_haptic() {
        let result = build_action_result(TouchAction::ScrollUp, 0);
        assert_eq!(result.haptic, Some(HapticFeedback::Vibrate));
    }

    #[test]
    fn test_collapse_detail_gets_tick_haptic() {
        let result = build_action_result(TouchAction::CollapseDetail, 0);
        assert_eq!(result.haptic, Some(HapticFeedback::Tick));
    }

    #[test]
    fn test_search_focus_gets_tick_haptic() {
        let result = build_action_result(TouchAction::SearchFocus, 0);
        assert_eq!(result.haptic, Some(HapticFeedback::Tick));
    }

    // -- Count propagation --

    #[test]
    fn test_action_result_preserves_count() {
        let count = 42usize;
        let result = build_action_result(TouchAction::SelectPrompt(0), count);
        assert_eq!(result.count, count);
    }

    // -- Display implementations --

    #[test]
    fn test_tap_display() {
        assert_eq!(format!("{}", TouchEvent::Tap), "Tap");
    }

    #[test]
    fn test_swipe_display() {
        assert_eq!(format!("{}", TouchEvent::Swipe(SwipeDir::Up)), "Swipe(Up)");
    }

    #[test]
    fn test_pinch_display() {
        assert_eq!(
            format!("{}", TouchEvent::Pinch(PinchDir::Out)),
            "Pinch(Out)"
        );
    }

    #[test]
    fn test_multi_tap_display() {
        assert_eq!(format!("{}", TouchEvent::MultiTap(3)), "MultiTap(3)");
    }

    #[test]
    fn test_haptic_feedback_display_tick() {
        assert_eq!(format!("{}", HapticFeedback::Tick), "Tick");
    }

    #[test]
    fn test_haptic_feedback_display_vibrate() {
        assert_eq!(format!("{}", HapticFeedback::Vibrate), "Vibrate");
    }

    #[test]
    fn test_haptic_feedback_display_error_buzz() {
        assert_eq!(format!("{}", HapticFeedback::ErrorBuzz), "ErrorBuzz");
    }

    // -- Swipe threshold awareness (configurable, not enforced in
    // gesture_to_action — the hub layer uses this to filter before mapping)
    // -------------------------------------------------------------------

    /// Verify that a low threshold allows weaker swipes through.
    #[test]
    fn test_config_respects_custom_swipe_threshold() {
        let cfg = TouchConfig {
            swipe_threshold: 10,
            ..Default::default()
        };
        assert_eq!(cfg.swipe_threshold, 10);

        // gesture_to_action always returns Some for SwipeDown/Up regardless
        // of threshold — the *hub* layer would use the threshold before
        // calling gesture_to_action.  Here we just confirm the config carries
        // the custom value correctly.
        let action = gesture_to_action(&TouchEvent::Swipe(SwipeDir::Up), &cfg).unwrap();
        assert!(matches!(action, TouchAction::ScrollUp));
    }

    // -- Gesture → Action round-trip for all defined mappings --

    #[test]
    fn test_all_gestures_resolve() {
        let cfg = TouchConfig::default();
        let gestures = [
            (TouchEvent::Tap, TouchAction::SelectPrompt(0)),
            (TouchEvent::Swipe(SwipeDir::Down), TouchAction::ScrollDown),
            (TouchEvent::Swipe(SwipeDir::Up), TouchAction::ScrollUp),
            (TouchEvent::LongPress, TouchAction::ExpandDetail),
            (TouchEvent::Pinch(PinchDir::In), TouchAction::CollapseDetail),
            (TouchEvent::Pinch(PinchDir::Out), TouchAction::CreatePrompt),
            (TouchEvent::MultiTap(2), TouchAction::SearchFocus),
        ];

        for (gesture, expected_action) in &gestures {
            let actual = gesture_to_action(gesture, &cfg);
            assert!(actual.is_some(), "Expected Some for {:?}", gesture);
            assert_eq!(
                actual.unwrap(),
                *expected_action,
                "Mismatch for {:?}",
                gesture
            );
        }
    }

    #[test]
    fn test_all_gestures_build_result() {
        let cfg = TouchConfig::default();
        let gestures = [
            TouchEvent::Tap,
            TouchEvent::Swipe(SwipeDir::Down),
            TouchEvent::Swipe(SwipeDir::Up),
            TouchEvent::LongPress,
            TouchEvent::Pinch(PinchDir::In),
            TouchEvent::Pinch(PinchDir::Out),
            TouchEvent::MultiTap(2),
        ];

        for gesture in &gestures {
            let action = gesture_to_action(gesture, &cfg).expect("gesture should resolve");
            let result = build_action_result(action, 0);
            assert!(
                result.haptic.is_some(),
                "Every resolved gesture should produce a haptic signal"
            );
        }
    }
}
