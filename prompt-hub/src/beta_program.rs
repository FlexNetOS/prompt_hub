//! Beta testing program for phased prompt rollouts.
//!
//! Manages beta cohorts, rollout stages with percentage-based deployment,
//! and participant feedback collection.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A rollout stage in a phased deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RolloutStage {
    /// Testing with internal team only (no external users).
    Internal,
    /// Small beta group — up to 10% of total.
    Alpha(u8), // percentage cap
    /// Medium rollout — up to 50%.
    Beta50(u8),
    /// Large rollout — up to 90%.
    Beta90(u8),
    /// Full production rollout.
    Production,
}

impl RolloutStage {
    /// Get the current percentage cap for this stage.
    pub fn percentage(&self) -> u8 {
        match self {
            RolloutStage::Internal => 0,
            RolloutStage::Alpha(pct) | RolloutStage::Beta50(pct) | RolloutStage::Beta90(pct) => {
                *pct
            }
            RolloutStage::Production => 100,
        }
    }

    /// Check if the stage allows a given percentage of users.
    pub fn allows_percentage(&self, pct: u8) -> bool {
        self.percentage() >= pct
    }
}

/// A beta cohort (group of participants).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCohort {
    pub id: String,
    pub name: String,
    /// Participants in this cohort.
    pub members: Vec<String>,
    /// Current rollout stage for this cohort.
    pub stage: RolloutStage,
    /// Overall satisfaction score (1-5 average).
    pub satisfaction_avg: f64,
    /// Total feedback items collected.
    pub feedback_count: usize,
}

impl BetaCohort {
    /// Create a new beta cohort.
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            members: vec![],
            stage: RolloutStage::Internal,
            satisfaction_avg: 0.0,
            feedback_count: 0,
        }
    }

    /// Enroll a participant in this cohort. Returns false if already enrolled.
    pub fn enroll(&mut self, participant_id: &str) -> bool {
        if !self.members.contains(&participant_id.to_string()) {
            self.members.push(participant_id.to_string());
            true
        } else {
            false
        }
    }

    /// Unenroll a participant from this cohort.
    pub fn unenroll(&mut self, participant_id: &str) {
        self.members.retain(|m| m != participant_id);
    }

    /// Record feedback from a participant and update satisfaction metrics.
    pub fn record_feedback(&mut self, score: u8) {
        let total = self.satisfaction_avg * (self.feedback_count as f64);
        self.feedback_count += 1;
        self.satisfaction_avg = (total + score as f64) / (self.feedback_count as f64);
    }

    /// Advance to the next rollout stage.
    pub fn advance_stage(&mut self, target: RolloutStage) {
        if target.percentage() > self.stage.percentage() || target == RolloutStage::Production {
            self.stage = target;
        }
    }

    /// Check if a given percentage of the cohort is enrolled.
    pub fn enrollment_ratio(&self, total_population: u32) -> f64 {
        if total_population == 0 {
            return 1.0; // no population means we can rollout fully
        }
        self.members.len() as f64 / total_population as f64 * 100.0
    }
}

/// Feedback item from a beta participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaFeedback {
    pub cohort_id: String,
    pub participant_id: String,
    pub score: u8, // 1-5 satisfaction
    pub comment: String,
}

/// A beta testing program managing multiple cohorts.
#[derive(Debug)]
pub struct BetaProgram {
    config: Arc<RwLock<BetaProgramConfig>>,
}

impl BetaProgram {
    /// Create a new beta program with the given config.
    pub fn new(config: BetaProgramConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Get the current config (cloned).
    pub fn config(&self) -> BetaProgramConfig {
        self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Create a new beta cohort.
    pub fn create_cohort(&self, id: &str, name: &str) -> BetaCohort {
        let mut config = self.config.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let cohort = BetaCohort::new(id, name);
        config.cohorts.insert(id.to_string(), cohort.clone());
        cohort
    }

    /// Get a reference to a specific cohort.
    pub fn cohort(&self, id: &str) -> Option<BetaCohort> {
        self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner).cohorts.get(id).cloned()
    }

    /// Enroll a participant in a cohort.
    pub fn enroll(&self, cohort_id: &str, participant_id: &str) -> bool {
        let mut config = self.config.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cohort) = config.cohorts.get_mut(cohort_id) {
            cohort.enroll(participant_id);
            true
        } else {
            false
        }
    }

    /// Unenroll a participant from a cohort.
    pub fn unenroll(&self, cohort_id: &str, participant_id: &str) -> bool {
        let mut config = self.config.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cohort) = config.cohorts.get_mut(cohort_id) {
            cohort.unenroll(participant_id);
            true
        } else {
            false
        }
    }

    /// Record feedback from a participant and update metrics.
    pub fn record_feedback(
        &self,
        cohort_id: &str,
        participant_id: &str,
        score: u8,
        comment: String,
    ) -> bool {
        let mut config = self.config.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cohort) = config.cohorts.get_mut(cohort_id) {
            cohort.record_feedback(score);
            config.feedbacks.push(BetaFeedback {
                cohort_id: cohort_id.to_string(),
                participant_id: participant_id.to_string(),
                score,
                comment,
            });
            true
        } else {
            false
        }
    }

    /// Advance a cohort's rollout stage.
    pub fn advance_stage(&self, cohort_id: &str, target: RolloutStage) -> bool {
        let mut config = self.config.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cohort) = config.cohorts.get_mut(cohort_id) {
            cohort.advance_stage(target);
            true
        } else {
            false
        }
    }

    /// Get overall program stats.
    pub fn stats(&self) -> ProgramStats {
        let config = self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        let total_cohorts = config.cohorts.len();
        let total_participants: usize = config.cohorts.values().map(|c| c.members.len()).sum();
        let avg_satisfaction = if total_cohorts == 0 {
            0.0
        } else {
            config
                .cohorts
                .values()
                .map(|c| c.satisfaction_avg)
                .sum::<f64>()
                / total_cohorts as f64
        };

        ProgramStats {
            total_cohorts,
            total_participants,
            avg_satisfaction: (avg_satisfaction * 10.0).round() / 10.0, // round to 1 decimal
            total_feedbacks: config.feedbacks.len(),
        }
    }

    /// Get the overall average stage across all cohorts.
    pub fn average_stage(&self) -> RolloutStage {
        let config = self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        if config.cohorts.is_empty() {
            return RolloutStage::Internal;
        }
        let avg_pct: f64 = config
            .cohorts
            .values()
            .map(|c| c.stage.percentage())
            .sum::<u8>() as f64
            / config.cohorts.len() as f64;
        if avg_pct >= 100.0 {
            RolloutStage::Production
        } else if avg_pct >= 75.0 {
            RolloutStage::Beta90(avg_pct as u8)
        } else if avg_pct >= 25.0 {
            RolloutStage::Beta50(avg_pct as u8)
        } else if avg_pct > 0.0 {
            RolloutStage::Alpha(avg_pct as u8)
        } else {
            RolloutStage::Internal
        }
    }
}

impl Default for BetaProgram {
    fn default() -> Self {
        Self::new(BetaProgramConfig::default())
    }
}

/// Configuration for a beta program.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaProgramConfig {
    pub cohorts: HashMap<String, BetaCohort>,
    pub feedbacks: Vec<BetaFeedback>,
}

/// Overall program statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramStats {
    pub total_cohorts: usize,
    pub total_participants: usize,
    pub avg_satisfaction: f64,
    pub total_feedbacks: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cohort_creation() {
        let cohort = BetaCohort::new("beta-1", "Test Beta");
        assert_eq!(cohort.id, "beta-1");
        assert_eq!(cohort.name, "Test Beta");
        assert_eq!(cohort.stage, RolloutStage::Internal);
    }

    #[test]
    fn test_cohort_enroll_unenroll() {
        let mut cohort = BetaCohort::new("beta-1", "Test");
        assert!(cohort.enroll("user-1"));
        assert_eq!(cohort.members.len(), 1);

        // Cannot enroll same user twice.
        cohort.enroll("user-1");
        assert_eq!(cohort.members.len(), 1);

        cohort.unenroll("user-1");
        assert_eq!(cohort.members.len(), 0);
    }

    #[test]
    fn test_cohort_feedback() {
        let mut cohort = BetaCohort::new("beta-1", "Test");
        cohort.record_feedback(4);
        cohort.record_feedback(5);
        assert_eq!(cohort.feedback_count, 2);
        // (4 + 5) / 2 = 4.5
        assert!((cohort.satisfaction_avg - 4.5).abs() < 0.01);
    }

    #[test]
    fn test_rollout_stage_percentages() {
        assert_eq!(RolloutStage::Internal.percentage(), 0);
        assert_eq!(RolloutStage::Alpha(10).percentage(), 10);
        assert_eq!(RolloutStage::Beta50(50).percentage(), 50);
        assert_eq!(RolloutStage::Beta90(90).percentage(), 90);
        assert_eq!(RolloutStage::Production.percentage(), 100);
    }

    #[test]
    fn test_advance_stage() {
        let mut cohort = BetaCohort::new("beta-1", "Test");
        cohort.advance_stage(RolloutStage::Alpha(10));
        assert_eq!(cohort.stage, RolloutStage::Alpha(10));

        cohort.advance_stage(RolloutStage::Beta50(50));
        assert_eq!(cohort.stage, RolloutStage::Beta50(50));

        // Can't go backwards.
        cohort.advance_stage(RolloutStage::Alpha(10));
        assert_eq!(cohort.stage, RolloutStage::Beta50(50));
    }

    #[test]
    fn test_beta_program_create_cohort() {
        let program = BetaProgram::default();
        let cohort = program.create_cohort("beta-1", "Test");
        assert_eq!(cohort.id, "beta-1");

        let stats = program.stats();
        assert_eq!(stats.total_cohorts, 1);
    }

    #[test]
    fn test_beta_program_stats() {
        let program = BetaProgram::default();
        program.create_cohort("beta-1", "Test");
        program.record_feedback("beta-1", "user-1", 4, "Great!".to_string());

        let stats = program.stats();
        assert_eq!(stats.total_cohorts, 1);
        assert_eq!(stats.total_participants, 0); // no enrollments yet
        assert_eq!(stats.total_feedbacks, 1);
    }

    #[test]
    fn test_program_average_stage_internal() {
        let program = BetaProgram::default();
        assert_eq!(program.average_stage(), RolloutStage::Internal);
    }
}
