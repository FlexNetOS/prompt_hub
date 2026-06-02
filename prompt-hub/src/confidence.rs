#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::{ConfidenceScore, Intent, ProjectContext};
use tracing::instrument;

/// Scores confidence of an action for auto-confirmation decisions.
///
/// Uses a weighted combination of four signals to decide whether
/// the system is confident enough to execute without asking the user.
#[derive(Debug, Clone)]
pub struct ConfidenceScorer {
    /// How clearly was the user's intent understood (0.0-1.0)
    pub intent_clarity: f64,
    /// How complete is the gathered context (0.0-1.0)
    pub context_completeness: f64,
    /// How well does the selected skill match (0.0-1.0)
    pub skill_match: f64,
    /// Historical success rate for similar tasks (0.0-1.0)
    pub historical_success: f64,
}

impl Default for ConfidenceScorer {
    fn default() -> Self {
        Self {
            intent_clarity: 0.8,
            context_completeness: 0.7,
            skill_match: 0.8,
            historical_success: 0.9,
        }
    }
}

impl ConfidenceScorer {
    /// Calculate the overall confidence score (0.0-1.0).
    ///
    /// Weights:
    /// - Intent clarity: 30% (most important — did we understand what they want?)
    /// - Context completeness: 25% (do we have enough project context?)
    /// - Skill match: 25% (is the selected skill a good fit?)
    /// - Historical success: 20% (have similar tasks succeeded before?)
    pub fn overall(&self) -> f64 {
        (self.intent_clarity * 0.3
            + self.context_completeness * 0.25
            + self.skill_match * 0.25
            + self.historical_success * 0.2)
            .clamp(0.0, 1.0)
    }

    /// Whether user confirmation is required (< 80% confidence).
    pub fn requires_confirmation(&self) -> bool {
        self.overall() < 0.80
    }

    /// Build a full `ConfidenceScore` with all computed fields.
    pub fn score(&self) -> ConfidenceScore {
        let overall = self.overall();
        ConfidenceScore {
            intent_clarity: self.intent_clarity,
            context_completeness: self.context_completeness,
            skill_match: self.skill_match,
            historical_success: self.historical_success,
            overall,
            requires_confirmation: overall < 0.80,
        }
    }

    /// Score from intent and project context — heuristic-based.
    #[instrument]
    pub fn from_intent(intent: &Intent, _context: &ProjectContext) -> Self {
        // Intent clarity: more entities = clearer intent
        let clarity = match intent.extracted_entities.len() {
            n if n >= 3 => 0.92,
            2 => 0.82,
            1 => 0.68,
            _ => 0.48,
        };

        // Adjust clarity based on complexity alignment
        let clarity = if intent.task_type == crate::models::TaskType::Create {
            // Create tasks are usually clearer
            (clarity * 1.05).min(0.99)
        } else {
            clarity
        };

        // Skill match based on domain specificity
        let skill = match intent.domain {
            crate::models::Domain::Coding
            | crate::models::Domain::DevOps
            | crate::models::Domain::Security => 0.85,
            _ => 0.72,
        };

        Self {
            intent_clarity: clarity,
            context_completeness: 0.70,
            skill_match: skill,
            historical_success: 0.88,
        }
    }

    /// Create a scorer with explicitly set values (builder-style).
    pub fn with_values(
        intent_clarity: f64,
        context_completeness: f64,
        skill_match: f64,
        historical_success: f64,
    ) -> Self {
        Self {
            intent_clarity: intent_clarity.clamp(0.0, 1.0),
            context_completeness: context_completeness.clamp(0.0, 1.0),
            skill_match: skill_match.clamp(0.0, 1.0),
            historical_success: historical_success.clamp(0.0, 1.0),
        }
    }

    /// Format a human-readable confidence explanation.
    pub fn explain(&self) -> String {
        let overall = self.overall();
        let level = if overall >= 0.90 {
            "Very High"
        } else if overall >= 0.80 {
            "High"
        } else if overall >= 0.60 {
            "Medium"
        } else if overall >= 0.40 {
            "Low"
        } else {
            "Very Low"
        };

        format!(
            "Confidence: {} ({:.0}%)\n\
             - Intent clarity:     {:.0}%\n\
             - Context complete:   {:.0}%\n\
             - Skill match:        {:.0}%\n\
             - Historical success: {:.0}%\n\
             - Auto-confirm: {}",
            level,
            overall * 100.0,
            self.intent_clarity * 100.0,
            self.context_completeness * 100.0,
            self.skill_match * 100.0,
            self.historical_success * 100.0,
            if self.requires_confirmation() {
                "No (needs confirmation)"
            } else {
                "Yes"
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_intent_with_entities(count: usize) -> Intent {
        let mut entities = std::collections::HashMap::new();
        for i in 0..count {
            entities.insert(format!("key{i}"), format!("value{i}"));
        }
        Intent {
            extracted_entities: entities,
            ..Default::default()
        }
    }

    fn test_context() -> ProjectContext {
        ProjectContext::default()
    }

    #[test]
    fn test_overall_calculation() {
        let scorer = ConfidenceScorer::default();
        let expected = 0.8 * 0.3 + 0.7 * 0.25 + 0.8 * 0.25 + 0.9 * 0.2;
        assert!((scorer.overall() - expected).abs() < f64::EPSILON * 10.0);
    }

    #[test]
    fn test_overall_clamped() {
        let scorer = ConfidenceScorer::with_values(2.0, 2.0, 2.0, 2.0);
        assert_eq!(scorer.overall(), 1.0);

        let scorer = ConfidenceScorer::with_values(-1.0, -1.0, -1.0, -1.0);
        assert_eq!(scorer.overall(), 0.0);
    }

    #[test]
    fn test_requires_confirmation() {
        let high = ConfidenceScorer::with_values(0.95, 0.95, 0.95, 0.95);
        assert!(!high.requires_confirmation());

        let low = ConfidenceScorer::with_values(0.5, 0.5, 0.5, 0.5);
        assert!(low.requires_confirmation());

        let borderline = ConfidenceScorer::with_values(0.8, 0.8, 0.8, 0.8);
        // overall = 0.8, requires_confirmation checks < 0.80
        // 0.8 * 0.3 + 0.8 * 0.25 + 0.8 * 0.25 + 0.8 * 0.2 = 0.8
        // 0.8 is NOT < 0.80, so no confirmation required
        assert!(!borderline.requires_confirmation());
    }

    #[test]
    fn test_score_struct() {
        let scorer = ConfidenceScorer::default();
        let score = scorer.score();

        assert_eq!(score.intent_clarity, scorer.intent_clarity);
        assert_eq!(score.context_completeness, scorer.context_completeness);
        assert_eq!(score.skill_match, scorer.skill_match);
        assert_eq!(score.historical_success, scorer.historical_success);
        assert!((score.overall - scorer.overall()).abs() < f64::EPSILON);
        assert_eq!(score.requires_confirmation, scorer.requires_confirmation());
    }

    #[test]
    fn test_from_intent_many_entities() {
        let intent = test_intent_with_entities(4);
        let context = test_context();
        let scorer = ConfidenceScorer::from_intent(&intent, &context);

        assert!(scorer.intent_clarity >= 0.9);
        assert!(scorer.skill_match > 0.0);
        assert!(scorer.overall() > 0.5);
    }

    #[test]
    fn test_from_intent_no_entities() {
        let intent = test_intent_with_entities(0);
        let context = test_context();
        let scorer = ConfidenceScorer::from_intent(&intent, &context);

        assert!(scorer.intent_clarity < 0.6);
        assert!(scorer.overall() < 0.8);
        assert!(scorer.requires_confirmation());
    }

    #[test]
    fn test_from_intent_one_entity() {
        let intent = test_intent_with_entities(1);
        let context = test_context();
        let scorer = ConfidenceScorer::from_intent(&intent, &context);

        assert!(scorer.intent_clarity >= 0.6);
        assert!(scorer.intent_clarity < 0.8);
    }

    #[test]
    fn test_explain_output() {
        let scorer = ConfidenceScorer::default();
        let explanation = scorer.explain();

        assert!(explanation.contains("Confidence:"));
        assert!(explanation.contains("Intent clarity:"));
        assert!(explanation.contains("Auto-confirm:"));
    }

    #[test]
    fn test_explain_very_high() {
        let scorer = ConfidenceScorer::with_values(0.98, 0.97, 0.99, 0.96);
        let explanation = scorer.explain();

        assert!(explanation.contains("Very High"));
        assert!(explanation.contains("Auto-confirm: Yes"));
    }

    #[test]
    fn test_explain_low() {
        let scorer = ConfidenceScorer::with_values(0.3, 0.3, 0.3, 0.3);
        let explanation = scorer.explain();

        assert!(explanation.contains("Low"));
        assert!(explanation.contains("needs confirmation"));
    }

    #[test]
    fn test_default_values() {
        let scorer = ConfidenceScorer::default();
        assert_eq!(scorer.intent_clarity, 0.8);
        assert_eq!(scorer.context_completeness, 0.7);
        assert_eq!(scorer.skill_match, 0.8);
        assert_eq!(scorer.historical_success, 0.9);
    }

    #[test]
    fn test_with_values_clamping() {
        let scorer = ConfidenceScorer::with_values(1.5, -0.5, 0.5, 0.5);
        assert_eq!(scorer.intent_clarity, 1.0);
        assert_eq!(scorer.context_completeness, 0.0);
        assert_eq!(scorer.skill_match, 0.5);
    }

    #[test]
    fn test_from_intent_create_task() {
        let intent = Intent {
            task_type: crate::models::TaskType::Create,
            extracted_entities: {
                let mut m = std::collections::HashMap::new();
                m.insert("framework".to_string(), "react".to_string());
                m.insert("auth".to_string(), "google".to_string());
                m
            },
            ..Default::default()
        };
        let context = test_context();
        let scorer = ConfidenceScorer::from_intent(&intent, &context);

        // Create tasks get a clarity boost
        assert!(scorer.intent_clarity >= 0.82);
    }
}
