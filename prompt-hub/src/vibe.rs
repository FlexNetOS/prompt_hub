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

        // 5. Generate prompt / artifacts — the most failure-prone step (it is
        //    the seam to artifact generation). On a transient/recoverable
        //    failure the SelfHealer genuinely re-runs it; fail-closed classes
        //    (security/auth) surface instead of being retried.
        info!("Generating artifacts using skill: {}", skill_rec.skill_name);
        let artifacts = match self
            .prompt_generator
            .generate(&intent, &filled_vars, &skill_rec)
            .await
        {
            Ok(artifacts) => artifacts,
            Err(err) => {
                warn!(error = %err, "artifact generation failed; invoking SelfHealer");
                self.self_healer
                    .heal_with(&err.to_string(), || {
                        self.prompt_generator
                            .generate(&intent, &filled_vars, &skill_rec)
                    })
                    .await?
            }
        };

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

    /// Whether this action recovers by **re-running** the failing operation.
    ///
    /// [`Retry`][HealAction::Retry], [`Repair`][HealAction::Repair] and
    /// [`Rollback`][HealAction::Rollback] all recover by re-executing the step
    /// (optionally after repairing inputs / rolling back partial state).
    /// [`Fallback`][HealAction::Fallback] does **not** re-run the same
    /// operation — it switches to a different, degraded path — so it is not a
    /// retry and [`SelfHealer::heal_with`] does not re-invoke the operation for
    /// it.
    fn is_reexecuting(self) -> bool {
        matches!(
            self,
            HealAction::Retry | HealAction::Repair | HealAction::Rollback
        )
    }
}

/// The decision [`SelfHealer::heal`] reached for a failed execution.
///
/// This is a *recommendation*, not an executed remediation: `heal` has no
/// handle to the failing operation, so it can only classify the failure and
/// name the action a caller should take. The caller drives the actual recovery
/// — e.g. by calling [`SelfHealer::heal_with`] with the operation to re-run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemediationAction {
    /// The corrective action class the failure maps to.
    pub action: HealAction,
    /// Honest, human-readable description of what was *decided* (not claimed to
    /// have been mechanically performed by `heal` itself).
    pub summary: String,
}

/// Maximum number of re-execution attempts [`SelfHealer::heal_with`] will make
/// after the initial failure before giving up and surfacing the last error.
const MAX_HEAL_RETRIES: u32 = 3;

/// Self-healing component that detects failures and drives real recovery.
///
/// Two entry points, with a clean honesty boundary between them:
///
/// * [`heal`][SelfHealer::heal] is **decision-only**: it classifies a failure
///   into the corrective action the vibe step needs ([retry][HealAction::Retry]
///   / [repair][HealAction::Repair] / [fallback][HealAction::Fallback] /
///   [rollback][HealAction::Rollback]) and returns a [`RemediationAction`]
///   describing what it *decided*. It does not — and cannot — re-execute
///   anything on its own.
/// * [`heal_with`][SelfHealer::heal_with] takes a handle to the failing
///   operation (an async closure) and **actually re-runs it** when the failure
///   class is re-executable, returning the recovered value on success.
///
/// Healing is **fail-closed**: an error class with no safe automatic recovery —
/// a security/policy violation, an authorization failure, or an exhausted
/// fallback budget — is *not* retried; both entry points surface an error so the
/// failure reaches the caller rather than being papered over.
#[derive(Debug, Clone, Default)]
pub struct SelfHealer;

impl SelfHealer {
    /// Decide how a failed execution *should* be remediated, without executing
    /// the remediation.
    ///
    /// Classifies `error` (by matching the [`HubError`] `Display` text the
    /// pipeline produces) into a [`HealAction`] and returns a
    /// [`RemediationAction`] naming the action and describing the decision.
    /// Because `SelfHealer` holds no handle to the failing operation, this
    /// method only *recommends* — use [`heal_with`][SelfHealer::heal_with] to
    /// actually re-run the operation.
    ///
    /// Returns `Err` when the failure class is not safely auto-recoverable
    /// (security violations, auth failures, exhausted fallbacks) — those must
    /// surface, not be healed away.
    pub async fn heal(&self, error: &str) -> Result<RemediationAction> {
        match Self::classify(error) {
            Some(action) => {
                info!(action = action.tag(), "SelfHealer: remediation decided");
                Ok(RemediationAction {
                    action,
                    summary: Self::decision_summary(action, error),
                })
            }
            None => {
                warn!(error, "SelfHealer: failure is not auto-recoverable");
                Err(HubError::FallbackExhausted(format!(
                    "no safe automatic remediation for failure: {error}"
                )))
            }
        }
    }

    /// Heal a failed execution by **actually re-running** the operation.
    ///
    /// `error` is the failure that just occurred; `operation` is an async
    /// closure that re-executes the failing step. The failure is classified:
    ///
    /// * **Re-executable** classes ([retry][HealAction::Retry],
    ///   [repair][HealAction::Repair], [rollback][HealAction::Rollback]) cause
    ///   `operation` to be genuinely re-invoked — up to `MAX_HEAL_RETRIES`
    ///   times — returning the first recovered `Ok(T)`. If every attempt fails,
    ///   the last error is surfaced.
    /// * **Fallback** does not re-run the same operation (the caller is expected
    ///   to provide a degraded path elsewhere); `heal_with` surfaces the failure
    ///   so the caller can take the fallback branch deliberately.
    /// * **Fail-closed** classes (security / auth / exhausted) are never
    ///   retried — the original failure is returned immediately.
    ///
    /// On a recovered success the returned value is the genuine output of the
    /// re-run operation — there is no fabricated "retried" claim.
    pub async fn heal_with<T, F, Fut>(&self, error: &str, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let Some(action) = Self::classify(error) else {
            warn!(error, "SelfHealer: failure is not auto-recoverable");
            return Err(HubError::FallbackExhausted(format!(
                "no safe automatic remediation for failure: {error}"
            )));
        };

        if !action.is_reexecuting() {
            // Fallback: re-running the *same* operation would just fail again.
            // Surface so the caller can take its degraded path deliberately.
            info!(
                action = action.tag(),
                "SelfHealer: failure is not re-executable; surfacing for fallback"
            );
            return Err(HubError::FallbackExhausted(format!(
                "self-heal [{}]: re-execution not applicable for failure: {error}",
                action.tag()
            )));
        }

        let mut attempts = 0u32;
        let mut last_err = error.to_string();
        while attempts < MAX_HEAL_RETRIES {
            attempts += 1;
            info!(
                action = action.tag(),
                attempt = attempts,
                "SelfHealer: re-executing failing operation"
            );
            match operation().await {
                Ok(value) => {
                    info!(
                        action = action.tag(),
                        attempt = attempts,
                        "SelfHealer: operation recovered"
                    );
                    return Ok(value);
                }
                Err(e) => {
                    // A re-run that surfaces a fail-closed class must not be
                    // retried further — promote it immediately.
                    if Self::classify(&e.to_string()).is_none() {
                        warn!(
                            error = %e,
                            "SelfHealer: re-run produced a fail-closed error; surfacing"
                        );
                        return Err(e);
                    }
                    last_err = e.to_string();
                }
            }
        }

        warn!(
            attempts,
            last_err, "SelfHealer: retries exhausted; surfacing failure"
        );
        Err(HubError::FallbackExhausted(format!(
            "self-heal [{}]: operation still failing after {attempts} attempt(s): {last_err}",
            action.tag()
        )))
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

    /// Build the honest, human-readable description of the remediation the
    /// healer **decided on** (not a claim that it was already executed —
    /// `heal` only decides; `heal_with` executes).
    fn decision_summary(action: HealAction, error: &str) -> String {
        let what = match action {
            HealAction::Retry => "re-run the failing vibe step after a back-off",
            HealAction::Repair => {
                "repair the request (re-extract variables, re-inject defaults), then re-run the step"
            }
            HealAction::Fallback => "switch to the degraded path for the failing step",
            HealAction::Rollback => "roll back to the last consistent checkpoint, then re-run",
        };
        format!(
            "self-heal [{}]: recommended action — {what} (triggering error: {error})",
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
    async fn test_self_healer_decides_retry_for_transient_failure() {
        // A timeout is transient → heal() *decides* a retry (it does not claim
        // to have re-run anything; that is heal_with's job).
        let healer = SelfHealer;
        let err = HubError::Timeout("upstream model call".to_string());
        let decision = healer.heal(&err.to_string()).await.unwrap();

        assert_eq!(decision.action, HealAction::Retry);
        // Honest wording: "recommended action — re-run …", not "retried …".
        assert!(
            decision.summary.contains("recommended action"),
            "summary: {}",
            decision.summary
        );
        // The triggering error is echoed for observability.
        assert!(decision.summary.contains("upstream model call"));
    }

    #[tokio::test]
    async fn test_self_healer_decides_repair_for_bad_input() {
        let healer = SelfHealer;
        let decision = healer
            .heal(&HubError::InvalidInput("missing framework".to_string()).to_string())
            .await
            .unwrap();
        assert_eq!(decision.action, HealAction::Repair);
    }

    #[tokio::test]
    async fn test_self_healer_decides_rollback_on_conflict() {
        let healer = SelfHealer;
        let decision = healer
            .heal(&HubError::Conflict("partial write".to_string()).to_string())
            .await
            .unwrap();
        assert_eq!(decision.action, HealAction::Rollback);
    }

    #[tokio::test]
    async fn test_self_healer_decides_fallback_on_dependency_failure() {
        let healer = SelfHealer;
        let decision = healer
            .heal(&HubError::Database("connection dropped".to_string()).to_string())
            .await
            .unwrap();
        assert_eq!(decision.action, HealAction::Fallback);
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
    async fn test_self_healer_default_decides_retry_for_unclassified() {
        // An unrecognized but non-fatal failure gets a single bounded retry.
        let healer = SelfHealer;
        let decision = healer.heal("something odd happened").await.unwrap();
        assert_eq!(decision.action, HealAction::Retry);
    }

    // ── heal_with: PROVES real re-execution, not prose ────────────────────

    #[tokio::test]
    async fn test_heal_with_genuinely_retries_and_recovers() {
        use std::cell::Cell;

        // The operation fails once (transient timeout) then succeeds. heal_with
        // must ACTUALLY re-invoke it and return the genuine recovered value.
        let calls = Cell::new(0u32);
        let healer = SelfHealer;
        let transient = HubError::Timeout("first call timed out".to_string());

        let recovered: i32 = healer
            .heal_with(&transient.to_string(), || {
                let n = calls.get() + 1;
                calls.set(n);
                async move {
                    if n == 1 {
                        // The re-run sees the same transient class on attempt 1.
                        Err(HubError::Timeout("still timing out".to_string()))
                    } else {
                        Ok(42)
                    }
                }
            })
            .await
            .unwrap();

        // Proof of real remediation: the operation was re-invoked, and the value
        // returned is the operation's genuine output (not a fabricated string).
        assert_eq!(recovered, 42);
        assert_eq!(
            calls.get(),
            2,
            "operation must be genuinely re-executed until it recovers"
        );
    }

    #[tokio::test]
    async fn test_heal_with_surfaces_after_exhausting_retries() {
        use std::cell::Cell;

        // The operation never recovers → heal_with re-runs up to the bound,
        // then surfaces the failure (no false success).
        let calls = Cell::new(0u32);
        let healer = SelfHealer;
        let result: Result<i32> = healer
            .heal_with(&HubError::Network("down".to_string()).to_string(), || {
                calls.set(calls.get() + 1);
                async { Err(HubError::Network("still down".to_string())) }
            })
            .await;

        assert!(result.is_err(), "exhausted retries must surface");
        assert_eq!(
            calls.get(),
            MAX_HEAL_RETRIES,
            "must attempt exactly MAX_HEAL_RETRIES times"
        );
    }

    #[tokio::test]
    async fn test_heal_with_fails_closed_does_not_invoke_operation() {
        use std::cell::Cell;

        // Fail-closed: a security failure must NOT re-run the operation at all.
        let calls = Cell::new(0u32);
        let healer = SelfHealer;
        let result: Result<i32> = healer
            .heal_with(
                &HubError::SecurityViolation("blocked".to_string()).to_string(),
                || {
                    calls.set(calls.get() + 1);
                    async { Ok(0) }
                },
            )
            .await;

        assert!(result.is_err(), "security failures must surface, not heal");
        assert_eq!(
            calls.get(),
            0,
            "fail-closed must never re-invoke the operation"
        );
    }

    #[tokio::test]
    async fn test_heal_with_stops_when_rerun_hits_fail_closed() {
        use std::cell::Cell;

        // A transient failure triggers a re-run; that re-run surfaces a
        // security error → heal_with must stop immediately, not keep retrying.
        let calls = Cell::new(0u32);
        let healer = SelfHealer;
        let result: Result<i32> = healer
            .heal_with(&HubError::Timeout("t".to_string()).to_string(), || {
                calls.set(calls.get() + 1);
                async { Err(HubError::SecurityViolation("escalated".to_string())) }
            })
            .await;

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), HubError::SecurityViolation(_)),
            "a fail-closed re-run error must be surfaced verbatim"
        );
        assert_eq!(calls.get(), 1, "must not retry past a fail-closed re-run");
    }

    #[tokio::test]
    async fn test_heal_with_does_not_rerun_fallback_class() {
        use std::cell::Cell;

        // A pure-fallback class (storage/database/plugin) is not a re-run of the
        // same op → heal_with surfaces so the caller takes its degraded path.
        let calls = Cell::new(0u32);
        let healer = SelfHealer;
        let result: Result<i32> = healer
            .heal_with(
                &HubError::Database("dropped".to_string()).to_string(),
                || {
                    calls.set(calls.get() + 1);
                    async { Ok(7) }
                },
            )
            .await;

        assert!(result.is_err(), "fallback class is not re-executed");
        assert_eq!(
            calls.get(),
            0,
            "fallback must not re-run the same operation"
        );
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
