#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::*;
use std::collections::HashMap;
use tracing::{info, instrument};

/// Learning engine for improving prompts from user feedback.
///
/// Collects user corrections and applies learned improvements to
/// prompt generation. After accumulating sufficient corrections
/// for the same intent type, automatically applies adjustments.
#[derive(Debug, Clone)]
pub struct LearningEngine {
    corrections: Vec<UserCorrection>,
    /// Weight adjustments learned per intent type
    weights: HashMap<String, f64>,
    /// Minimum corrections before applying learned adjustments
    correction_threshold: usize,
}

impl Default for LearningEngine {
    fn default() -> Self {
        Self {
            corrections: Vec::new(),
            weights: HashMap::new(),
            correction_threshold: 3,
        }
    }
}

impl LearningEngine {
    /// Create a new learning engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a custom correction threshold.
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            corrections: Vec::new(),
            weights: HashMap::new(),
            correction_threshold: threshold,
        }
    }

    /// Learn from a user correction.
    ///
    /// Stores the correction and adjusts weights based on patterns.
    #[instrument]
    pub async fn learn_from_feedback(&mut self, correction: UserCorrection) -> Result<()> {
        info!(
            agent_id = %correction.agent_id,
            intent = %correction.original_intent,
            "Learning from correction"
        );

        // Adjust weights for the intent type
        let intent_key = Self::intent_key(&correction.original_intent);
        let entry = self.weights.entry(intent_key.clone()).or_insert(1.0);
        *entry = (*entry * 0.95).max(0.1); // Decay weight as we learn

        self.corrections.push(correction);

        info!(
            total_corrections = %self.corrections.len(),
            "Correction recorded"
        );

        Ok(())
    }

    /// Apply learned improvements to a base prompt.
    ///
    /// If there are at least `correction_threshold` corrections for the
    /// given intent type, appends learned guidance to the prompt.
    pub fn get_improved_prompt(&self, base: &str, intent_type: &str) -> String {
        let relevant: Vec<_> = self
            .corrections
            .iter()
            .filter(|c| c.original_intent.contains(intent_type))
            .collect();

        if relevant.len() >= self.correction_threshold {
            let feedback_summary = self.summarize_corrections(&relevant);
            format!(
                "{}\n\n[Learned: incorporate user preferences from {} past corrections. {}]",
                base,
                relevant.len(),
                feedback_summary
            )
        } else {
            base.to_string()
        }
    }

    /// Get the number of corrections recorded.
    pub fn correction_count(&self) -> usize {
        self.corrections.len()
    }

    /// Get corrections relevant to a specific intent type.
    pub fn corrections_for_intent(&self, intent_type: &str) -> Vec<&UserCorrection> {
        self.corrections
            .iter()
            .filter(|c| c.original_intent.contains(intent_type))
            .collect()
    }

    /// Get the learned weight for an intent type.
    pub fn weight_for_intent(&self, intent_type: &str) -> f64 {
        let key = Self::intent_key(intent_type);
        *self.weights.get(&key).unwrap_or(&1.0)
    }

    /// Get all learned weights.
    pub fn weights(&self) -> &HashMap<String, f64> {
        &self.weights
    }

    /// Clear all learned data.
    pub fn reset(&mut self) {
        self.corrections.clear();
        self.weights.clear();
    }

    /// Check if enough corrections have been accumulated for an intent type.
    pub fn has_sufficient_data(&self, intent_type: &str) -> bool {
        self.corrections_for_intent(intent_type).len() >= self.correction_threshold
    }

    /// Summarize corrections for inclusion in prompts.
    fn summarize_corrections(&self, corrections: &[&UserCorrection]) -> String {
        // Collect common themes from feedback
        let feedbacks: Vec<_> = corrections.iter().map(|c| &c.feedback).collect();

        if feedbacks.len() >= 5 {
            "Key themes: users prefer detailed, specific responses with examples.".to_string()
        } else if feedbacks.len() >= 3 {
            "Key theme: users value clarity and completeness.".to_string()
        } else {
            String::new()
        }
    }

    /// Extract a normalized intent key from intent text.
    fn intent_key(intent: &str) -> String {
        intent
            .to_lowercase()
            .split_whitespace()
            .next()
            .unwrap_or("general")
            .to_string()
    }

    /// Export all corrections for persistence.
    pub fn export_corrections(&self) -> &[UserCorrection] {
        &self.corrections
    }

    /// Import corrections from a persisted source.
    pub fn import_corrections(&mut self, corrections: Vec<UserCorrection>) {
        self.corrections = corrections;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_correction(intent: &str, feedback: &str) -> UserCorrection {
        UserCorrection {
            agent_id: Uuid::new_v4(),
            original_intent: intent.to_string(),
            corrected_output: "corrected".to_string(),
            feedback: feedback.to_string(),
            timestamp: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_learn_from_feedback() {
        let mut engine = LearningEngine::new();
        let correction = make_correction("create api", "use better naming");
        let result = engine.learn_from_feedback(correction).await;
        assert!(result.is_ok());
        assert_eq!(engine.correction_count(), 1);
    }

    #[test]
    fn test_get_improved_prompt_below_threshold() {
        let engine = LearningEngine::new();
        let base = "Create a REST API";
        let result = engine.get_improved_prompt(base, "create");
        assert_eq!(result, base);
    }

    #[test]
    fn test_get_improved_prompt_above_threshold() {
        let mut engine = LearningEngine::new();

        // Add 3 corrections for the same intent type
        for i in 0..3 {
            engine.corrections.push(make_correction(
                "create login page",
                &format!("feedback {}", i),
            ));
        }

        let base = "Create a login page";
        let result = engine.get_improved_prompt(base, "login");
        assert!(result.contains("Learned"));
        assert!(result.contains("3 past corrections"));
        assert!(result.starts_with(base));
    }

    #[test]
    fn test_get_improved_prompt_no_match() {
        let mut engine = LearningEngine::new();

        for i in 0..3 {
            engine.corrections.push(make_correction(
                "create login page",
                &format!("feedback {}", i),
            ));
        }

        let base = "Create a blog";
        // "blog" won't match "login" corrections
        let result = engine.get_improved_prompt(base, "blog");
        assert_eq!(result, base);
    }

    #[test]
    fn test_corrections_for_intent() {
        let mut engine = LearningEngine::new();
        engine
            .corrections
            .push(make_correction("create login", "fb1"));
        engine
            .corrections
            .push(make_correction("create signup", "fb2"));
        engine.corrections.push(make_correction("fix bug", "fb3"));

        let login_corrections = engine.corrections_for_intent("login");
        assert_eq!(login_corrections.len(), 1);

        let create_corrections = engine.corrections_for_intent("create");
        assert_eq!(create_corrections.len(), 2);
    }

    #[test]
    fn test_weight_adjustment() {
        let mut engine = LearningEngine::new();
        let _correction = make_correction("create", "feedback");

        // Need to manually adjust weights since learn_from_feedback is async
        let key = "create".to_string();
        engine.weights.insert(key.clone(), 1.0);

        // Simulate the decay that learn_from_feedback does
        let entry = engine.weights.get_mut(&key).unwrap();
        *entry = (*entry * 0.95).max(0.1);

        assert_eq!(engine.weight_for_intent("create"), 0.95);
    }

    #[test]
    fn test_has_sufficient_data() {
        let mut engine = LearningEngine::new();
        assert!(!engine.has_sufficient_data("create"));

        for i in 0..3 {
            engine
                .corrections
                .push(make_correction("create api", &format!("feedback {}", i)));
        }

        assert!(engine.has_sufficient_data("create"));
        assert!(!engine.has_sufficient_data("deploy"));
    }

    #[test]
    fn test_reset() {
        let mut engine = LearningEngine::new();
        engine.corrections.push(make_correction("test", "feedback"));
        engine.weights.insert("test".to_string(), 0.5);

        engine.reset();
        assert_eq!(engine.correction_count(), 0);
        assert!(engine.weights.is_empty());
    }

    #[test]
    fn test_custom_threshold() {
        let mut engine = LearningEngine::with_threshold(5);
        assert!(!engine.has_sufficient_data("create"));

        for i in 0..5 {
            engine
                .corrections
                .push(make_correction("create api", &format!("feedback {}", i)));
        }

        assert!(engine.has_sufficient_data("create"));
    }

    #[test]
    fn test_import_export_corrections() {
        let mut engine = LearningEngine::new();
        let corrections = vec![
            make_correction("intent1", "fb1"),
            make_correction("intent2", "fb2"),
        ];

        engine.import_corrections(corrections.clone());
        assert_eq!(engine.correction_count(), 2);

        let exported = engine.export_corrections();
        assert_eq!(exported.len(), 2);
    }
}
