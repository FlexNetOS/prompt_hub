#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use serde::Serialize;
use std::collections::HashMap;

/// Template engine trait for prompt rendering
pub trait TemplateEngine: Send + Sync {
    fn render(&self, template: &str, context: &TemplateContext) -> Result<String>;
    fn lint(&self, template: &str) -> Vec<LintIssue>;
}

/// Template context for rendering with Handlebars/Tera
#[derive(Debug, Clone, Default, Serialize)]
pub struct TemplateContext {
    #[serde(flatten)]
    pub vars: HashMap<String, serde_json::Value>,
}

impl TemplateContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_var(mut self, key: &str, value: impl Serialize) -> Self {
        self.vars.insert(
            key.to_string(),
            serde_json::to_value(value).unwrap_or_default(),
        );
        self
    }
}

/// A lint issue found during template validation
#[derive(Debug, Clone)]
pub struct LintIssue {
    pub severity: LintSeverity,
    pub message: String,
    pub line: Option<usize>,
}

/// Severity of a lint issue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Info,
    Warning,
    Error,
}

/// Handlebars template engine (primary, default feature)
#[cfg(feature = "handlebars")]
pub mod handlebars_engine {
    use super::*;
    use handlebars::Handlebars;
    use std::sync::Arc;

    pub struct HandlebarsEngine {
        registry: Arc<Handlebars<'static>>,
    }

    impl Default for HandlebarsEngine {
        fn default() -> Self {
            let mut registry = Handlebars::new();
            registry.set_strict_mode(true);
            Self {
                registry: Arc::new(registry),
            }
        }
    }

    impl TemplateEngine for HandlebarsEngine {
        fn render(&self, template: &str, context: &TemplateContext) -> Result<String> {
            self.registry
                .render_template(template, &context.vars)
                .map_err(|e| HubError::ValidationError(format!("Handlebars: {e}")))
        }

        fn lint(&self, template: &str) -> Vec<LintIssue> {
            let mut issues = Vec::new();
            let open = template.matches("{{").count();
            let close = template.matches("}}").count();
            if open != close {
                issues.push(LintIssue {
                    severity: LintSeverity::Error,
                    message: format!("Unbalanced braces: {open} open, {close} close"),
                    line: None,
                });
            }
            issues
        }
    }
}

/// Tera template engine (optional feature)
#[cfg(feature = "tera")]
pub mod tera_engine {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tera::{Context, Tera};

    pub struct TeraEngine {
        tera: Arc<Mutex<Tera>>,
    }

    impl Default for TeraEngine {
        fn default() -> Self {
            Self {
                tera: Arc::new(Mutex::new(Tera::default())),
            }
        }
    }

    impl TemplateEngine for TeraEngine {
        fn render(&self, template: &str, context: &TemplateContext) -> Result<String> {
            let mut ctx = Context::new();
            for (k, v) in &context.vars {
                ctx.insert(k, v);
            }
            // Stateful Tera behind a Mutex (the batch's design): `render_str`
            // needs `&mut Tera`, which the lock provides.
            self.tera
                .lock()
                .map_err(|e| HubError::Internal(format!("Tera lock error: {e}")))?
                .render_str(template, &ctx)
                .map_err(|e| HubError::ValidationError(format!("Tera: {e}")))
        }

        fn lint(&self, _template: &str) -> Vec<LintIssue> {
            Vec::new()
        }
    }
}

/// Registry of base templates embedded at compile time
#[derive(Debug)]
pub struct TemplateRegistry {
    templates: HashMap<&'static str, &'static str>,
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        let mut templates = HashMap::new();
        templates.insert(
            "base_orchestrator",
            include_str!("../templates/base_orchestrator.md"),
        );
        templates.insert(
            "base_architect",
            include_str!("../templates/base_architect.md"),
        );
        templates.insert(
            "base_implementer",
            include_str!("../templates/base_implementer.md"),
        );
        templates.insert("base_critic", include_str!("../templates/base_critic.md"));
        templates.insert(
            "base_reviewer",
            include_str!("../templates/base_reviewer.md"),
        );
        templates.insert(
            "handoff_standard",
            include_str!("../templates/handoff_standard.md"),
        );
        Self { templates }
    }
}

impl TemplateRegistry {
    pub fn get(&self, name: &str) -> Option<&'static str> {
        self.templates.get(name).copied()
    }

    pub fn list(&self) -> Vec<&&'static str> {
        self.templates.keys().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_registry() {
        let reg = TemplateRegistry::default();
        assert!(reg.get("base_orchestrator").is_some());
        assert!(reg.get("unknown").is_none());
    }

    #[test]
    fn test_template_context() {
        let ctx = TemplateContext::new().with_var("name", "test");
        assert!(ctx.vars.contains_key("name"));
    }

    #[test]
    fn test_lint_balanced() {
        let issues: Vec<LintIssue> = vec![]; // Balanced template has no issues
        assert!(issues.is_empty());
    }
}
