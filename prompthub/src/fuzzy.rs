#![forbid(unsafe_code)]

use prompt_hub::models::*;

/// Fuzzy prompt finder using substring matching
///
/// Performs case-insensitive matching across prompt name, tags, and system prompt content.
#[derive(Debug)]
pub struct FuzzyPromptFinder;

impl FuzzyPromptFinder {
    /// Create a new finder instance
    pub fn new() -> Self {
        Self
    }

    /// Find prompts matching the given query
    ///
    /// # Arguments
    /// * `prompts` - Slice of prompts to search
    /// * `query` - Search query string
    ///
    /// # Returns
    /// Vector of references to matching prompts
    pub fn find<'a>(prompts: &'a [Prompt], query: &str) -> Vec<&'a Prompt> {
        if query.is_empty() {
            return prompts.iter().collect();
        }
        let query_lower = query.to_lowercase();
        prompts
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
                    || p.system_prompt.to_lowercase().contains(&query_lower)
                    || p.user_template.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Find prompts with a minimum score threshold
    pub fn find_with_threshold<'a>(
        prompts: &'a [Prompt],
        query: &str,
        threshold: f64,
    ) -> Vec<(&'a Prompt, f64)> {
        if query.is_empty() || threshold <= 0.0 {
            return prompts.iter().map(|p| (p, 1.0)).collect();
        }
        let query_lower = query.to_lowercase();
        prompts
            .iter()
            .filter_map(|p| {
                let mut score = 0.0f64;
                if p.name.to_lowercase().contains(&query_lower) {
                    score += 0.5;
                }
                if p.tags
                    .iter()
                    .any(|t| t.to_lowercase().contains(&query_lower))
                {
                    score += 0.3;
                }
                if p.system_prompt.to_lowercase().contains(&query_lower) {
                    score += 0.1;
                }
                if p.user_template.to_lowercase().contains(&query_lower) {
                    score += 0.1;
                }
                if score >= threshold {
                    Some((p, score))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Default for FuzzyPromptFinder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_prompt(name: &str, tags: Vec<&str>, system: &str) -> Prompt {
        Prompt {
            id: Uuid::new_v4(),
            name: name.to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: system.to_string(),
            user_template: "Hello.".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: tags.into_iter().map(|t| t.to_string()).collect(),
            target_roles: vec![Role::Developer],
            metadata: PromptMeta::default(),
            metrics: PromptMetrics::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            author: AgentIdentity::default(),
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        }
    }

    #[test]
    fn test_fuzzy_find_by_name() {
        let prompts = vec![
            create_test_prompt(
                "error-handler",
                vec!["rust", "error"],
                "Handle errors gracefully.",
            ),
            create_test_prompt("logger-config", vec!["config"], "Configure logging."),
        ];
        let results = FuzzyPromptFinder::find(&prompts, "error");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "error-handler");
    }

    #[test]
    fn test_fuzzy_find_by_tag() {
        let prompts = vec![
            create_test_prompt("a", vec!["rust"], "system"),
            create_test_prompt("b", vec!["python"], "system"),
        ];
        let results = FuzzyPromptFinder::find(&prompts, "rust");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "a");
    }

    #[test]
    fn test_fuzzy_find_empty_query() {
        let prompts = vec![
            create_test_prompt("a", vec![], "sys"),
            create_test_prompt("b", vec![], "sys"),
        ];
        let results = FuzzyPromptFinder::find(&prompts, "");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_fuzzy_find_no_match() {
        let prompts = vec![create_test_prompt("a", vec![], "system")];
        let results = FuzzyPromptFinder::find(&prompts, "xyz-nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_fuzzy_find_case_insensitive() {
        let prompts = vec![create_test_prompt("ErrorHandler", vec![], "system")];
        let results = FuzzyPromptFinder::find(&prompts, "error");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_find_with_threshold() {
        let prompts = vec![
            create_test_prompt("error-handler", vec!["rust", "error"], "Handle errors."),
            create_test_prompt("other", vec!["misc"], "Unrelated prompt."),
        ];
        let results = FuzzyPromptFinder::find_with_threshold(&prompts, "error", 0.4);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.name, "error-handler");
    }
}
