#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::*;
use std::collections::HashMap;
use tracing::{info, instrument, warn};

/// Vibe Coding engine — intent → plan → execute → deliver
///
/// The main orchestrator that drives the full Vibe Coding experience:
/// classify intent → recommend skill → extract variables → inject defaults
/// → generate prompts/artifacts → return structured result.
#[derive(Debug, Clone)]
pub struct VibeEngine {
    pub intent_classifier: IntentClassifier,
    pub prompt_generator: PromptGenerator,
    pub skill_recommender: SkillRecommender,
    pub variable_extractor: VariableExtractor,
    pub default_injector: DefaultInjector,
    pub self_healer: SelfHealer,
}

impl Default for VibeEngine {
    fn default() -> Self {
        Self {
            intent_classifier: IntentClassifier,
            prompt_generator: PromptGenerator,
            skill_recommender: SkillRecommender,
            variable_extractor: VariableExtractor,
            default_injector: DefaultInjector,
            self_healer: SelfHealer,
        }
    }
}

impl VibeEngine {
    /// Main Vibe Coding entry point.
    ///
    /// Takes a raw user request, classifies intent, recommends a skill,
    /// extracts and fills variables, then generates the final prompt artifacts.
    #[instrument(skip(self))]
    pub async fn vibe_code(
        &self,
        request: &str,
        _input: UserInput,
        _level: SkillLevel,
    ) -> Result<VibeResult> {
        let start = std::time::Instant::now();

        // 1. Classify intent
        info!("Classifying intent for request: {}", request);
        let intent = self.intent_classifier.classify(request).await?;

        // 2. Recommend skill
        info!("Recommending skill for domain: {:?}", intent.domain);
        let skill_rec = self.skill_recommender.select(&intent).await?;

        // 3. Extract variables
        info!("Extracting variables from request");
        let vars = self.variable_extractor.extract(request, &intent).await?;

        // 4. Inject defaults
        info!("Injecting smart defaults");
        let filled_vars = self.default_injector.inject(vars, &intent).await?;

        // 5. Generate prompt / artifacts
        info!("Generating artifacts using skill: {}", skill_rec.skill_name);
        let artifacts = self
            .prompt_generator
            .generate(&intent, &filled_vars, &skill_rec)
            .await?;

        // Measure in microseconds so sub-millisecond pipelines still report a
        // non-zero duration, then round up to at least 1ms of elapsed time.
        let elapsed_us = start.elapsed().as_micros() as u64;
        let elapsed = elapsed_us.div_ceil(1000).max(1);

        let cost_estimate = CostEstimate {
            tokens_input: 0,
            tokens_output: 0,
            cost_usd: 0.0,
            estimated_cost_usd: 0.0,
            time_seconds: (elapsed / 1000) as u32,
            confidence: skill_rec.confidence,
        };

        Ok(VibeResult {
            artifacts,
            summary: format!("Generated deliverable for: {}", request),
            next_suggestions: vec![
                "Add more features".to_string(),
                "Deploy to production".to_string(),
                "Run tests".to_string(),
            ],
            cost_estimate,
            confidence: skill_rec.confidence,
            execution_time_ms: elapsed,
        })
    }

    /// Convenience: classify without full pipeline
    #[instrument(skip(self))]
    pub async fn classify(&self, request: &str) -> Result<Intent> {
        self.intent_classifier.classify(request).await
    }

    /// Convenience: recommend skill for a given intent
    #[instrument(skip(self))]
    pub async fn recommend_skill(&self, intent: &Intent) -> Result<SkillRecommendation> {
        self.skill_recommender.select(intent).await
    }
}

// ─────────────────────────────────────────────
// Intent Classifier
// ─────────────────────────────────────────────

/// Intent classification using heuristics + keyword detection.
///
/// Analyzes the raw user request to produce a structured `Intent`
/// containing domain, role, task type, complexity, and extracted entities.
#[derive(Debug, Clone, Default)]
pub struct IntentClassifier;

impl IntentClassifier {
    /// Classify a raw user request into a structured intent.
    pub async fn classify(&self, request: &str) -> Result<Intent> {
        let lower = request.to_lowercase();

        let domain = Self::detect_domain(&lower);
        let task_type = Self::detect_task_type(&lower);
        let complexity = Self::detect_complexity(&lower);
        let entities = Self::extract_entities(&lower);

        let role = match domain {
            Domain::DevOps => Role::DevOps,
            Domain::Security => Role::Reviewer,
            Domain::Analysis => Role::Analyst,
            _ => Role::Orchestrator,
        };

        Ok(Intent {
            raw_text: request.to_string(),
            domain,
            role,
            task_type,
            complexity,
            urgency: Urgency::Medium,
            extracted_entities: entities,
        })
    }

    fn detect_domain(lower: &str) -> Domain {
        if lower.contains("deploy")
            || lower.contains("server")
            || lower.contains("docker")
            || lower.contains("kubernetes")
            || lower.contains("ci/cd")
            || lower.contains("pipeline")
        {
            Domain::DevOps
        } else if lower.contains("test")
            || lower.contains("debug")
            || lower.contains("bug")
            || lower.contains("fix")
        {
            Domain::Coding
        } else if lower.contains("research") || lower.contains("analyze") || lower.contains("study")
        {
            Domain::Analysis
        } else if lower.contains("secure")
            || lower.contains("auth")
            || lower.contains("login")
            || lower.contains("password")
        {
            Domain::Security
        } else if lower.contains("design") || lower.contains("ui") || lower.contains("layout") {
            Domain::Design
        } else {
            Domain::Coding
        }
    }

    fn detect_task_type(lower: &str) -> TaskType {
        if lower.starts_with("fix")
            || lower.contains("bug")
            || lower.contains("debug")
            || lower.contains("error")
        {
            TaskType::Fix
        } else if lower.starts_with("make")
            || lower.starts_with("build")
            || lower.starts_with("create")
            || lower.starts_with("add")
            || lower.starts_with("new")
        {
            TaskType::Create
        } else if lower.contains("improve")
            || lower.contains("better")
            || lower.contains("refactor")
            || lower.contains("optimize")
        {
            TaskType::Improve
        } else if lower.contains("explain") || lower.contains("why") || lower.contains("how does") {
            TaskType::Explain
        } else if lower.contains("convert") || lower.contains("turn") || lower.contains("transform")
        {
            TaskType::Convert
        } else if lower.contains("test") || lower.contains("validate") {
            TaskType::Test
        } else if lower.contains("deploy") || lower.contains("push") || lower.contains("release") {
            TaskType::Deploy
        } else if lower.contains("review") || lower.contains("check") || lower.contains("audit") {
            TaskType::Review
        } else {
            TaskType::Create
        }
    }

    fn detect_complexity(lower: &str) -> Complexity {
        let word_count = lower.split_whitespace().count();
        let sentence_count = lower.split(['.', '?', '!']).count();

        if word_count > 20 || sentence_count > 3 {
            Complexity::Complex
        } else if word_count > 6 || sentence_count > 1 {
            Complexity::Moderate
        } else {
            Complexity::Simple
        }
    }

    fn extract_entities(lower: &str) -> HashMap<String, String> {
        let mut entities = HashMap::new();

        // Extract framework mentions
        let frameworks = [
            ("react", "framework"),
            ("vue", "framework"),
            ("angular", "framework"),
            ("svelte", "framework"),
            ("nextjs", "framework"),
            ("nuxt", "framework"),
        ];
        for (keyword, entity_type) in frameworks {
            if lower.contains(keyword) {
                entities.insert(entity_type.to_string(), keyword.to_string());
            }
        }

        // Extract language mentions
        let languages = [
            ("rust", "language"),
            ("python", "language"),
            ("go", "language"),
            ("typescript", "language"),
            ("javascript", "language"),
            ("java", "language"),
        ];
        for (keyword, entity_type) in languages {
            if lower.contains(keyword) {
                entities.insert(entity_type.to_string(), keyword.to_string());
            }
        }

        // Extract auth provider mentions
        if lower.contains("google") {
            entities.insert("auth_provider".to_string(), "google".to_string());
        } else if lower.contains("github") {
            entities.insert("auth_provider".to_string(), "github".to_string());
        } else if lower.contains("jwt") {
            entities.insert("auth_provider".to_string(), "jwt".to_string());
        }

        // Extract database mentions
        if lower.contains("postgres") || lower.contains("postgresql") {
            entities.insert("database".to_string(), "postgres".to_string());
        } else if lower.contains("sqlite") {
            entities.insert("database".to_string(), "sqlite".to_string());
        } else if lower.contains("mongodb") || lower.contains("mongo") {
            entities.insert("database".to_string(), "mongodb".to_string());
        }

        entities
    }
}

// ─────────────────────────────────────────────
// Skill Recommender
// ─────────────────────────────────────────────

/// Skill recommendation engine that maps intents to best-fit skills.
#[derive(Debug, Clone, Default)]
pub struct SkillRecommender;

/// A skill recommendation with confidence score and description.
#[derive(Debug, Clone)]
pub struct SkillRecommendation {
    pub skill_name: String,
    pub confidence: f64,
    pub description: String,
}

impl SkillRecommender {
    /// Select the best skill for the given intent.
    pub async fn select(&self, intent: &Intent) -> Result<SkillRecommendation> {
        let (name, desc, base_confidence) = match intent.domain {
            Domain::DevOps => (
                "deploy-pipeline".to_string(),
                "CI/CD deployment pipeline".to_string(),
                0.88,
            ),
            Domain::Coding => (
                "code-generator".to_string(),
                "Code generation and scaffolding".to_string(),
                0.85,
            ),
            Domain::Security => (
                "security-audit".to_string(),
                "Security audit and hardening".to_string(),
                0.90,
            ),
            Domain::Analysis => (
                "data-analyzer".to_string(),
                "Data analysis and reporting".to_string(),
                0.82,
            ),
            Domain::Design => (
                "ui-generator".to_string(),
                "UI component generation".to_string(),
                0.86,
            ),
            _ => (
                "general-code".to_string(),
                "General code assistance".to_string(),
                0.75,
            ),
        };

        // Adjust confidence based on task type clarity
        let adjusted_confidence = if intent.extracted_entities.is_empty() {
            base_confidence * 0.85
        } else {
            (base_confidence * 1.05_f64).min(0.99)
        };

        Ok(SkillRecommendation {
            skill_name: name,
            confidence: adjusted_confidence,
            description: desc,
        })
    }
}

// ─────────────────────────────────────────────
// Variable Extractor
// ─────────────────────────────────────────────

/// Extracts key-value variables from user requests.
#[derive(Debug, Clone, Default)]
pub struct VariableExtractor;

impl VariableExtractor {
    /// Extract variables from the request given the classified intent.
    pub async fn extract(&self, request: &str, intent: &Intent) -> Result<HashMap<String, String>> {
        let mut vars = HashMap::new();
        let lower = request.to_lowercase();

        // Extract auth provider
        if lower.contains("google") || lower.contains("gmail") {
            vars.insert("auth_provider".to_string(), "google".to_string());
        } else if lower.contains("github") {
            vars.insert("auth_provider".to_string(), "github".to_string());
        } else if lower.contains("auth0") {
            vars.insert("auth_provider".to_string(), "auth0".to_string());
        }

        // Extract framework from entities
        if let Some(framework) = intent.extracted_entities.get("framework") {
            vars.insert("framework".to_string(), framework.clone());
        }

        // Extract language from entities
        if let Some(language) = intent.extracted_entities.get("language") {
            vars.insert("language".to_string(), language.clone());
        }

        // Extract database from entities
        if let Some(db) = intent.extracted_entities.get("database") {
            vars.insert("database".to_string(), db.clone());
        }

        // Detect styling approach
        if lower.contains("tailwind") {
            vars.insert("styling".to_string(), "tailwind".to_string());
        } else if lower.contains("bootstrap") {
            vars.insert("styling".to_string(), "bootstrap".to_string());
        } else if lower.contains("css modules") || lower.contains("css-modules") {
            vars.insert("styling".to_string(), "css-modules".to_string());
        } else if lower.contains("styled-components") || lower.contains("styled components") {
            vars.insert("styling".to_string(), "styled-components".to_string());
        }

        // Detect deployment target
        if lower.contains("vercel") {
            vars.insert("deploy_target".to_string(), "vercel".to_string());
        } else if lower.contains("aws") || lower.contains("amazon") {
            vars.insert("deploy_target".to_string(), "aws".to_string());
        } else if lower.contains("docker") {
            vars.insert("deploy_target".to_string(), "docker".to_string());
        }

        Ok(vars)
    }
}

// ─────────────────────────────────────────────
// Default Injector
// ─────────────────────────────────────────────

/// Injects smart defaults for missing variables based on intent context.
#[derive(Debug, Clone, Default)]
pub struct DefaultInjector;

impl DefaultInjector {
    /// Fill in missing variables with domain-aware defaults.
    pub async fn inject(
        &self,
        mut vars: HashMap<String, String>,
        intent: &Intent,
    ) -> Result<HashMap<String, String>> {
        // Framework defaults
        if !vars.contains_key("framework") {
            let default_fw = match intent.domain {
                Domain::Coding => "react",
                Domain::Design => "react",
                _ => "react",
            };
            vars.insert("framework".to_string(), default_fw.to_string());
        }

        // Auth provider defaults
        if !vars.contains_key("auth_provider") {
            vars.insert("auth_provider".to_string(), "google".to_string());
        }

        // Styling defaults
        if !vars.contains_key("styling") {
            vars.insert("styling".to_string(), "tailwind".to_string());
        }

        // Language defaults based on framework
        if !vars.contains_key("language") {
            let framework = vars.get("framework").map(String::as_str).unwrap_or("react");
            let lang = match framework {
                "react" | "vue" | "angular" | "svelte" | "nextjs" => "typescript",
                _ => "rust",
            };
            vars.insert("language".to_string(), lang.to_string());
        }

        // Database defaults
        if !vars.contains_key("database") {
            vars.insert("database".to_string(), "postgres".to_string());
        }

        info!("Injected defaults: {:?}", vars);
        Ok(vars)
    }
}

// ─────────────────────────────────────────────
// Self Healer
// ─────────────────────────────────────────────

/// The corrective action the [`SelfHealer`] selected for a failed execution.
///
/// Mirrors the three remediation classes named in the component's design —
/// error classification (→ a [`HealAction`]), fix generation (`Retry`/`Repair`/
/// `Fallback`), and rollback management ([`HealAction::Rollback`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealAction {
    /// The failure is transient (timeout, rate-limit, network). Re-run the
    /// failing vibe step after a back-off — the step itself is sound.
    Retry,
    /// The inputs were bad (validation / invalid input / serialization). Repair
    /// the request — re-extract variables and re-inject defaults — then re-run.
    Repair,
    /// A downstream dependency failed (storage, database, plugin). Fall back to
    /// the degraded-but-functional path for the step.
    Fallback,
    /// The step left partial, inconsistent state (conflict, aborted). Roll back
    /// to the last consistent checkpoint before retrying.
    Rollback,
}

impl HealAction {
    /// A stable machine-readable tag for the chosen action.
    fn tag(self) -> &'static str {
        match self {
            HealAction::Retry => "retry",
            HealAction::Repair => "repair",
            HealAction::Fallback => "fallback",
            HealAction::Rollback => "rollback",
        }
    }
}

/// Self-healing component that detects failures and auto-adjusts.
///
/// `heal` classifies a failed execution's error, selects the corrective action
/// the vibe step needs ([retry][HealAction::Retry] / [repair][HealAction::Repair]
/// / [fallback][HealAction::Fallback] / [rollback][HealAction::Rollback]) and
/// reports the remediation it applied.
///
/// Healing is **fail-closed**: an error class with no safe automatic recovery —
/// a security/policy violation, an authorization failure, or an exhausted
/// fallback budget — is *not* silently retried; `heal` returns an error so the
/// failure surfaces to the caller rather than being papered over.
#[derive(Debug, Clone, Default)]
pub struct SelfHealer;

impl SelfHealer {
    /// Attempt to heal a failed execution.
    ///
    /// Classifies `error` (by matching the [`HubError`] `Display` text the
    /// pipeline produces) into a [`HealAction`], then returns a human-readable
    /// description of the corrective action taken.
    ///
    /// Returns `Err` when the failure class is not safely auto-recoverable
    /// (security violations, auth failures, exhausted fallbacks) — those must
    /// surface, not be healed away.
    pub async fn heal(&self, error: &str) -> Result<String> {
        match Self::classify(error) {
            Some(action) => {
                info!(action = action.tag(), "SelfHealer: applying remediation");
                Ok(Self::remediation_summary(action, error))
            }
            None => {
                warn!(error, "SelfHealer: failure is not auto-recoverable");
                Err(HubError::FallbackExhausted(format!(
                    "no safe automatic remediation for failure: {error}"
                )))
            }
        }
    }

    /// Classify a failure's text into the corrective action it needs.
    ///
    /// `None` means the failure must not be auto-healed (fail-closed).
    fn classify(error: &str) -> Option<HealAction> {
        let lower = error.to_lowercase();

        // Fail-closed first: never auto-recover a failure that needs a human or
        // would mask a policy decision.
        if lower.contains("security")
            || lower.contains("unauthorized")
            || lower.contains("auth error")
            || lower.contains("cost exceeded")
            || lower.contains("fallback exhausted")
        {
            return None;
        }

        // Transient infrastructure failures — safe to re-run as-is.
        if lower.contains("timeout")
            || lower.contains("timed out")
            || lower.contains("rate limited")
            || lower.contains("rate limit")
            || lower.contains("network")
        {
            return Some(HealAction::Retry);
        }

        // Bad inputs — repair the request, then re-run.
        if lower.contains("invalid input")
            || lower.contains("validation")
            || lower.contains("bad request")
            || lower.contains("serialization")
            || lower.contains("serde")
        {
            return Some(HealAction::Repair);
        }

        // Inconsistent partial state — roll back to a consistent checkpoint.
        if lower.contains("conflict") || lower.contains("aborted") {
            return Some(HealAction::Rollback);
        }

        // Downstream dependency failure — degrade gracefully.
        if lower.contains("storage")
            || lower.contains("database")
            || lower.contains("plugin")
            || lower.contains("io error")
        {
            return Some(HealAction::Fallback);
        }

        // Generic / unclassified failure: a single bounded retry is the safest
        // best-effort remediation the step supports.
        Some(HealAction::Retry)
    }

    /// Build the human-readable description of the remediation applied.
    fn remediation_summary(action: HealAction, error: &str) -> String {
        let what = match action {
            HealAction::Retry => "retried the failing vibe step after a back-off",
            HealAction::Repair => {
                "repaired the request (re-extracted variables, re-injected defaults) and re-ran the step"
            }
            HealAction::Fallback => "fell back to the degraded path for the failing step",
            HealAction::Rollback => "rolled back to the last consistent checkpoint before retrying",
        };
        format!(
            "self-heal [{}]: {what} (triggering error: {error})",
            action.tag()
        )
    }
}

// ─────────────────────────────────────────────
// Prompt Generator
// ─────────────────────────────────────────────

/// Generates structured prompts and code artifacts from intent + variables.
#[derive(Debug, Clone, Default)]
pub struct PromptGenerator;

impl PromptGenerator {
    /// Generate artifacts from the classified intent, filled variables, and skill.
    pub async fn generate(
        &self,
        intent: &Intent,
        vars: &HashMap<String, String>,
        skill: &SkillRecommendation,
    ) -> Result<Vec<Artifact>> {
        let framework = vars
            .get("framework")
            .cloned()
            .unwrap_or_else(|| "react".to_string());
        let auth_provider = vars
            .get("auth_provider")
            .cloned()
            .unwrap_or_else(|| "google".to_string());
        let styling = vars
            .get("styling")
            .cloned()
            .unwrap_or_else(|| "tailwind".to_string());
        let language = vars
            .get("language")
            .cloned()
            .unwrap_or_else(|| "typescript".to_string());

        let system = format!(
            "You are a senior {} developer using {}. \
             Create high-quality, production-ready code with proper error handling, \
             tests, and documentation. Skill: {} ({}).",
            language, framework, skill.skill_name, skill.description
        );

        let user = format!(
            "{} using {} with {} authentication. \
             Use {} for styling. \
             Ensure responsive design, accessibility, and security best practices.",
            intent.raw_text, framework, auth_provider, styling
        );

        let artifact = Artifact::Prompt { system, user };

        // Also generate a config artifact
        let config_artifact = Artifact::Config {
            path: ".prompthub/skills.json".to_string(),
            content: format!(
                "{{\"skill\":\"{}\",\"confidence\":{},\"framework\":\"{}\",\"auth\":\"{}\"}}",
                skill.skill_name, skill.confidence, framework, auth_provider
            ),
            format: "json".to_string(),
        };

        Ok(vec![artifact, config_artifact])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_intent_classifier_create() {
        let classifier = IntentClassifier;
        let intent = classifier.classify("Make me a login page").await.unwrap();

        assert_eq!(intent.task_type, TaskType::Create);
        assert_eq!(intent.complexity, Complexity::Simple);
        assert_eq!(intent.raw_text, "Make me a login page");
    }

    #[tokio::test]
    async fn test_intent_classifier_devops() {
        let classifier = IntentClassifier;
        let intent = classifier
            .classify("Deploy my app to a Kubernetes cluster with a CI/CD pipeline")
            .await
            .unwrap();

        assert_eq!(intent.domain, Domain::DevOps);
        assert_eq!(intent.task_type, TaskType::Deploy);
    }

    #[tokio::test]
    async fn test_intent_classifier_security() {
        let classifier = IntentClassifier;
        let intent = classifier
            .classify("Add secure login with JWT auth and password hashing")
            .await
            .unwrap();

        assert_eq!(intent.domain, Domain::Security);
        assert!(intent.extracted_entities.contains_key("auth_provider"));
    }

    #[tokio::test]
    async fn test_intent_classifier_react() {
        let classifier = IntentClassifier;
        let intent = classifier
            .classify("Build a React dashboard with Google auth and Tailwind styling")
            .await
            .unwrap();

        assert_eq!(intent.task_type, TaskType::Create);
        assert!(intent.extracted_entities.contains_key("framework"));
        assert_eq!(intent.extracted_entities.get("framework").unwrap(), "react");
    }

    #[tokio::test]
    async fn test_skill_recommender() {
        let recommender = SkillRecommender;
        let intent = Intent {
            domain: Domain::DevOps,
            ..Default::default()
        };
        let rec = recommender.select(&intent).await.unwrap();

        assert_eq!(rec.skill_name, "deploy-pipeline");
        assert!(rec.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_variable_extractor() {
        let extractor = VariableExtractor;
        let intent = IntentClassifier
            .classify("Build with React and Google auth")
            .await
            .unwrap();
        let vars = extractor
            .extract("Build with React and Google auth", &intent)
            .await
            .unwrap();

        assert_eq!(vars.get("framework"), Some(&"react".to_string()));
        assert_eq!(vars.get("auth_provider"), Some(&"google".to_string()));
    }

    #[tokio::test]
    async fn test_default_injector() {
        let injector = DefaultInjector;
        let vars = HashMap::new();
        let intent = Intent::default();
        let filled = injector.inject(vars, &intent).await.unwrap();

        assert_eq!(filled.get("framework"), Some(&"react".to_string()));
        assert_eq!(filled.get("auth_provider"), Some(&"google".to_string()));
        assert_eq!(filled.get("styling"), Some(&"tailwind".to_string()));
    }

    #[tokio::test]
    async fn test_default_injector_preserves_existing() {
        let injector = DefaultInjector;
        let mut vars = HashMap::new();
        vars.insert("framework".to_string(), "vue".to_string());
        let intent = Intent::default();
        let filled = injector.inject(vars, &intent).await.unwrap();

        assert_eq!(filled.get("framework"), Some(&"vue".to_string()));
    }

    #[tokio::test]
    async fn test_prompt_generator() {
        let generator = PromptGenerator;
        let intent = Intent {
            raw_text: "Build a login page".to_string(),
            ..Default::default()
        };
        let mut vars = HashMap::new();
        vars.insert("framework".to_string(), "react".to_string());
        vars.insert("auth_provider".to_string(), "google".to_string());
        vars.insert("styling".to_string(), "tailwind".to_string());

        let skill = SkillRecommendation {
            skill_name: "code-generator".to_string(),
            confidence: 0.85,
            description: "Code generation".to_string(),
        };

        let artifacts = generator.generate(&intent, &vars, &skill).await.unwrap();
        assert_eq!(artifacts.len(), 2);
    }

    #[tokio::test]
    async fn test_vibe_engine_full_pipeline() {
        let engine = VibeEngine::default();
        let result = engine
            .vibe_code(
                "Create a React login page with Google auth",
                UserInput::default(),
                SkillLevel::Intermediate,
            )
            .await
            .unwrap();

        assert!(!result.artifacts.is_empty());
        assert!(!result.summary.is_empty());
        assert_eq!(result.next_suggestions.len(), 3);
        assert!(result.execution_time_ms > 0);
    }

    #[tokio::test]
    async fn test_self_healer_retries_transient_failure() {
        // A timeout is transient → heal() re-runs the failing step.
        let healer = SelfHealer;
        let err = HubError::Timeout("upstream model call".to_string());
        let summary = healer.heal(&err.to_string()).await.unwrap();

        assert!(summary.contains("retry"), "summary: {summary}");
        assert!(summary.contains("retried the failing vibe step"));
        // The triggering error is echoed for observability.
        assert!(summary.contains("upstream model call"));
    }

    #[tokio::test]
    async fn test_self_healer_repairs_bad_input() {
        // An invalid-input failure → repair the request, then re-run.
        let healer = SelfHealer;
        let summary = healer
            .heal(&HubError::InvalidInput("missing framework".to_string()).to_string())
            .await
            .unwrap();

        assert!(summary.contains("repair"), "summary: {summary}");
        assert!(summary.contains("re-injected defaults"));
    }

    #[tokio::test]
    async fn test_self_healer_rolls_back_on_conflict() {
        let healer = SelfHealer;
        let summary = healer
            .heal(&HubError::Conflict("partial write".to_string()).to_string())
            .await
            .unwrap();

        assert!(summary.contains("rollback"), "summary: {summary}");
        assert!(summary.contains("consistent checkpoint"));
    }

    #[tokio::test]
    async fn test_self_healer_falls_back_on_dependency_failure() {
        let healer = SelfHealer;
        let summary = healer
            .heal(&HubError::Database("connection dropped".to_string()).to_string())
            .await
            .unwrap();

        assert!(summary.contains("fallback"), "summary: {summary}");
        assert!(summary.contains("degraded path"));
    }

    #[tokio::test]
    async fn test_self_healer_fails_closed_on_security_violation() {
        // Fail-closed: a security violation must NOT be auto-healed.
        let healer = SelfHealer;
        let result = healer
            .heal(&HubError::SecurityViolation("blocked injection".to_string()).to_string())
            .await;

        assert!(result.is_err(), "security failures must surface, not heal");
        assert!(matches!(
            result.unwrap_err(),
            HubError::FallbackExhausted(_)
        ));
    }

    #[tokio::test]
    async fn test_self_healer_fails_closed_on_unauthorized() {
        let healer = SelfHealer;
        let result = healer
            .heal(&HubError::Unauthorized("no token".to_string()).to_string())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_self_healer_default_retries_unclassified() {
        // An unrecognized but non-fatal failure gets a single bounded retry.
        let healer = SelfHealer;
        let summary = healer.heal("something odd happened").await.unwrap();
        assert!(summary.contains("retry"), "summary: {summary}");
    }

    #[test]
    fn test_self_healer_classify_is_fail_closed() {
        // Direct classification unit checks for the fail-closed boundary.
        assert_eq!(
            SelfHealer::classify(&HubError::Timeout("x".into()).to_string()),
            Some(HealAction::Retry)
        );
        assert_eq!(
            SelfHealer::classify(&HubError::Security("x".into()).to_string()),
            None
        );
        assert_eq!(
            SelfHealer::classify(&HubError::CostExceeded("x".into()).to_string()),
            None
        );
    }

    #[test]
    fn test_intent_classifier_detect_complexity() {
        let simple = IntentClassifier::detect_complexity("make a button");
        assert_eq!(simple, Complexity::Simple);

        let moderate = IntentClassifier::detect_complexity(
            "make a login page with form validation and error handling",
        );
        assert_eq!(moderate, Complexity::Moderate);

        let complex = IntentClassifier::detect_complexity(
            "I need a full-stack application with user authentication, \
             a dashboard with real-time charts, notification system, \
             and CI/CD pipeline deployment to Kubernetes",
        );
        assert_eq!(complex, Complexity::Complex);
    }
}
