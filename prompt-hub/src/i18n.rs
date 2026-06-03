#![forbid(unsafe_code)]

use crate::models::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, instrument};

/// Locale-aware prompt retrieval and internationalization engine.
///
/// Manages locale-specific prompt variants with a fallback chain
/// (e.g., `en-US` → `en` → `en-GB` → `default`).
#[derive(Debug, Clone, Default)]
pub struct I18nEngine {
    translations: HashMap<String, HashMap<String, String>>,
}

/// A locale-aware prompt variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalizedPrompt {
    pub locale: String,
    pub user_template: String,
    pub system_prompt: Option<String>,
}

impl I18nEngine {
    /// Create a new I18n engine.
    pub fn new() -> Self {
        Self {
            translations: HashMap::new(),
        }
    }

    /// Get a localized version of a prompt for the given locale.
    ///
    /// If the prompt has a locale matching the requested one, returns its
    /// user template. Otherwise applies the fallback chain.
    #[instrument]
    pub fn get_localized_prompt(&self, prompt: &Prompt, locale: &str) -> Option<String> {
        // Direct match on the prompt's locale
        if let Some(ref prompt_locale) = prompt.locale {
            if prompt_locale == locale {
                return Some(prompt.user_template.clone());
            }
        }

        // Try the fallback chain
        let chain = Self::fallback_chain(locale);
        for fallback_locale in &chain {
            if let Some(translations) = self.translations.get(fallback_locale) {
                if let Some(template) = translations.get(&prompt.id.to_string()) {
                    info!(
                        prompt_id = %prompt.id,
                        locale = %fallback_locale,
                        "Resolved localized prompt via fallback"
                    );
                    return Some(template.clone());
                }
            }

            // Check if the prompt itself matches this fallback locale
            if let Some(ref prompt_locale) = prompt.locale {
                if prompt_locale == fallback_locale {
                    return Some(prompt.user_template.clone());
                }
            }
        }

        // No localization found; return the base template
        Some(prompt.user_template.clone())
    }

    /// Build a fallback chain for a locale.
    ///
    /// For example:
    /// - `"en-US"` → `["en-US", "en", "default"]`
    /// - `"zh-CN"` → `["zh-CN", "zh", "default"]`
    /// - `"de"` → `["de", "default"]`
    pub fn fallback_chain(locale: &str) -> Vec<String> {
        let mut chain = Vec::new();

        // Add the full locale first
        chain.push(locale.to_string());

        // Add the language code (before the dash)
        if let Some(dash) = locale.find('-') {
            let lang = &locale[..dash];
            chain.push(lang.to_string());

            // Add common script/region variants
            let region = &locale[dash + 1..];
            if lang == "en" {
                // For English, also try other major variants
                if region != "GB" {
                    chain.push("en-GB".to_string());
                }
                if region != "US" {
                    chain.push("en-US".to_string());
                }
            } else if lang == "zh" {
                if region != "TW" && region != "HK" {
                    chain.push("zh-TW".to_string());
                }
                if region != "CN" {
                    chain.push("zh-CN".to_string());
                }
            }
        }

        // Always include default as last resort
        chain.push("default".to_string());

        chain
    }

    /// Register a translation for a prompt in a specific locale.
    pub fn register_translation(&mut self, prompt_id: &str, locale: &str, template: String) {
        self.translations
            .entry(locale.to_string())
            .or_default()
            .insert(prompt_id.to_string(), template);
    }

    /// Get a translation directly by locale and prompt id.
    pub fn get_translation(&self, prompt_id: &str, locale: &str) -> Option<&String> {
        self.translations
            .get(locale)
            .and_then(|map| map.get(prompt_id))
    }

    /// List all registered locales.
    pub fn registered_locales(&self) -> Vec<&String> {
        self.translations.keys().collect()
    }

    /// List all locales that have translations for a given prompt.
    pub fn locales_for_prompt(&self, prompt_id: &str) -> Vec<&String> {
        self.translations
            .iter()
            .filter(|(_, map)| map.contains_key(prompt_id))
            .map(|(locale, _)| locale)
            .collect()
    }

    /// Best-effort locale detection from a preference string.
    ///
    /// Handles quality values like `"en-US,en;q=0.9,fr;q=0.8"`.
    pub fn parse_accept_locale(header: &str) -> Vec<(String, f32)> {
        let mut locales = Vec::new();

        for part in header.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some(semi) = part.find(';') {
                let locale = part[..semi].trim().to_string();
                let q_str = part[semi + 1..].trim();
                let q = if let Some(eq) = q_str.find('=') {
                    q_str[eq + 1..].parse::<f32>().unwrap_or(1.0)
                } else {
                    1.0
                };
                locales.push((locale, q));
            } else {
                locales.push((part.to_string(), 1.0));
            }
        }

        // Sort by quality descending
        locales.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        locales
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_prompt(locale: Option<&str>) -> Prompt {
        Prompt {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "You are a helper.".to_string(),
            user_template: "Help me.".to_string(),
            required_vars: vec![],
            domain: Domain::General,
            tags: vec![],
            target_roles: vec![],
            metadata: PromptMeta::default(),
            metrics: PromptMetrics::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            author: AgentIdentity {
                id: Uuid::new_v4(),
                name: "test".to_string(),
                capabilities: Default::default(),
                token_hash: "".to_string(),
                specialization_score: 0.5,
            },
            deleted_at: None,
            generation_params: None,
            locale: locale.map(|s| s.to_string()),
            multimodal: None,
        }
    }

    #[test]
    fn test_get_localized_prompt_exact_match() {
        let engine = I18nEngine::new();
        let prompt = make_prompt(Some("en-US"));
        let result = engine.get_localized_prompt(&prompt, "en-US");
        assert_eq!(result, Some("Help me.".to_string()));
    }

    #[test]
    fn test_get_localized_prompt_fallback() {
        let mut engine = I18nEngine::new();
        let prompt = make_prompt(None);

        // Register a translation
        let id = prompt.id.to_string();
        engine.register_translation(&id, "en", "Help me (English).".to_string());

        // Request en-US should fall back to en
        let result = engine.get_localized_prompt(&prompt, "en-US");
        assert_eq!(result, Some("Help me (English).".to_string()));
    }

    #[test]
    fn test_fallback_chain_en_us() {
        let chain = I18nEngine::fallback_chain("en-US");
        assert_eq!(chain[0], "en-US");
        assert!(chain.contains(&"en".to_string()));
        assert!(chain.contains(&"default".to_string()));
    }

    #[test]
    fn test_fallback_chain_zh_cn() {
        let chain = I18nEngine::fallback_chain("zh-CN");
        assert_eq!(chain[0], "zh-CN");
        assert!(chain.contains(&"zh".to_string()));
        assert!(chain.contains(&"zh-TW".to_string()));
        assert!(chain.contains(&"default".to_string()));
    }

    #[test]
    fn test_fallback_chain_simple() {
        let chain = I18nEngine::fallback_chain("de");
        assert_eq!(chain[0], "de");
        assert_eq!(chain[chain.len() - 1], "default");
        assert!(chain.len() <= 3);
    }

    #[test]
    fn test_register_and_get_translation() {
        let mut engine = I18nEngine::new();
        engine.register_translation("prompt-1", "fr", "Aidez-moi.".to_string());

        let found = engine.get_translation("prompt-1", "fr");
        assert_eq!(found, Some(&"Aidez-moi.".to_string()));

        let not_found = engine.get_translation("prompt-1", "de");
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_registered_locales() {
        let mut engine = I18nEngine::new();
        engine.register_translation("p1", "en", "Hello".to_string());
        engine.register_translation("p2", "fr", "Bonjour".to_string());
        engine.register_translation("p3", "de", "Hallo".to_string());

        let locales = engine.registered_locales();
        assert_eq!(locales.len(), 3);
    }

    #[test]
    fn test_locales_for_prompt() {
        let mut engine = I18nEngine::new();
        engine.register_translation("p1", "en", "Hello".to_string());
        engine.register_translation("p1", "fr", "Bonjour".to_string());
        engine.register_translation("p2", "de", "Hallo".to_string());

        let locales = engine.locales_for_prompt("p1");
        assert_eq!(locales.len(), 2);
    }

    #[test]
    fn test_parse_accept_locale() {
        let locales = I18nEngine::parse_accept_locale("en-US,en;q=0.9,fr;q=0.8");
        assert_eq!(locales.len(), 3);
        assert_eq!(locales[0], ("en-US".to_string(), 1.0));
        assert_eq!(locales[1], ("en".to_string(), 0.9));
        assert_eq!(locales[2], ("fr".to_string(), 0.8));
    }

    #[test]
    fn test_parse_accept_locale_single() {
        let locales = I18nEngine::parse_accept_locale("de");
        assert_eq!(locales.len(), 1);
        assert_eq!(locales[0], ("de".to_string(), 1.0));
    }

    #[test]
    fn test_parse_accept_locale_empty() {
        let locales = I18nEngine::parse_accept_locale("");
        assert!(locales.is_empty());
    }

    #[test]
    fn test_get_localized_prompt_no_locale_returns_base() {
        let engine = I18nEngine::new();
        let prompt = make_prompt(None);
        let result = engine.get_localized_prompt(&prompt, "ja-JP");
        assert_eq!(result, Some("Help me.".to_string()));
    }
}
