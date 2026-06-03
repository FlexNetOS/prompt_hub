use chrono::Utc;
use prompt_hub::models::*;
use proptest::prelude::*;
use uuid::Uuid;

#[test]
fn test_prompt_creation() {
    let prompt = Prompt {
        id: Uuid::new_v4(),
        name: "test-prompt".to_string(),
        version: semver::Version::new(1, 0, 0),
        status: Status::Active,
        system_prompt: "You are a test assistant.".to_string(),
        user_template: "Hello {{name}}.".to_string(),
        required_vars: vec!["name".to_string()],
        domain: Domain::Coding,
        tags: vec!["test".to_string()],
        target_roles: vec![Role::Implementer],
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
        locale: None,
        multimodal: None,
    };

    assert_eq!(prompt.name, "test-prompt");
    assert_eq!(prompt.status, Status::Active);
    assert_eq!(prompt.domain, Domain::Coding);
}

#[test]
fn test_prompt_default() {
    let prompt = Prompt::default();
    assert_eq!(prompt.name, "untitled");
    assert_eq!(prompt.status, Status::Draft);
    assert_eq!(prompt.version, semver::Version::new(0, 1, 0));
}

#[test]
fn test_semver_parsing() {
    let v = semver::Version::parse("1.2.3").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
}

#[test]
fn test_status_variants() {
    let statuses = [
        Status::Draft,
        Status::Active,
        Status::Deprecated,
        Status::Archived,
        Status::Locked,
    ];
    assert_eq!(statuses.len(), 5);
}

#[test]
fn test_domain_variants() {
    assert_eq!(Domain::Coding, Domain::Coding);
    assert_ne!(Domain::Coding, Domain::DevOps);
}

#[test]
fn test_role_variants() {
    let roles = vec![
        Role::Architect,
        Role::Developer,
        Role::Tester,
        Role::DevOps,
        Role::Analyst,
        Role::Designer,
        Role::Orchestrator,
        Role::Reviewer,
        Role::Implementer,
        Role::Refiner,
    ];
    assert_eq!(roles.len(), 10);
}

#[test]
fn test_evolution_strategy_variants() {
    let strategies = [
        EvolutionStrategy::Mutate,
        EvolutionStrategy::Crossover,
        EvolutionStrategy::AbTest,
        EvolutionStrategy::Semantic,
        EvolutionStrategy::Compress,
        EvolutionStrategy::Expand,
    ];
    assert_eq!(strategies.len(), 6);
}

#[test]
fn test_agent_identity_default() {
    let identity = AgentIdentity::default();
    assert_eq!(identity.name, "anonymous");
    assert!(identity.capabilities.is_empty());
    assert_eq!(identity.specialization_score, 0.0);
}

#[test]
fn test_prompt_meta_default() {
    let meta = PromptMeta::default();
    assert_eq!(meta.usage_count, 0);
    assert_eq!(meta.success_rate, 0.0);
}

#[test]
fn test_prompt_metrics_default() {
    let metrics = PromptMetrics::default();
    assert_eq!(metrics.usage_count, 0);
    assert_eq!(metrics.success_rate, 0.0);
    assert_eq!(metrics.cost_estimate_usd, 0.0);
}

#[test]
fn test_lock_token_creation() {
    let token = LockToken {
        id: Uuid::new_v4(),
        prompt_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        token_hash: "test-token-123".to_string(),
        expires_at: Utc::now(),
        created_at: Utc::now(),
    };
    assert_eq!(token.token_hash, "test-token-123");
}

#[test]
fn test_audit_entry_creation() {
    let entry = AuditEntry {
        id: 0,
        timestamp: Utc::now(),
        agent_id: Uuid::new_v4(),
        action: "Created".to_string(),
        prompt_id: Some(Uuid::new_v4()),
        diff_hash: String::new(),
        before_json: None,
        after_json: Some("abc123".to_string()),
        ip_address: None,
    };
    assert_eq!(entry.action, "Created");
}

#[test]
fn test_audit_action_variants() {
    let actions = vec![
        AuditAction::Created,
        AuditAction::Updated,
        AuditAction::RolledBack,
        AuditAction::Deleted,
        AuditAction::Locked,
        AuditAction::Unlocked,
        AuditAction::Exported,
        AuditAction::Imported,
        AuditAction::Evolved,
        AuditAction::Deployed,
        AuditAction::Reviewed,
    ];
    assert_eq!(actions.len(), 11);
}

#[test]
fn test_generation_params_default() {
    let params = GenerationParams::default();
    assert_eq!(params.temperature, 0.7);
    assert_eq!(params.top_p, 1.0);
    assert!(params.max_tokens.is_none());
}

#[test]
fn test_multimodal_config_default() {
    let config = MultimodalConfig::default();
    assert!(!config.supports_images);
    assert!(!config.supports_audio);
    assert!(!config.supports_video);
}

#[test]
fn test_cost_estimate_default() {
    let ce = CostEstimate::default();
    assert_eq!(ce.cost_usd, 0.0);
    assert_eq!(ce.tokens_input, 0);
    assert_eq!(ce.confidence, 0.0);
}

#[test]
fn test_confidence_score_default() {
    let cs = ConfidenceScore::default();
    assert_eq!(cs.overall, 0.5);
    assert!(cs.requires_confirmation);
}

#[test]
fn test_user_input_default() {
    let ui = UserInput::default();
    assert_eq!(ui.input_type, InputType::Text);
    assert!(ui.extracted_text.is_empty());
    assert!(ui.raw_data.is_empty());
}

#[test]
fn test_project_context_default() {
    let ctx = ProjectContext::default();
    assert_eq!(ctx.language, "unknown");
    assert_eq!(ctx.framework, "unknown");
    assert_eq!(ctx.team_size, 1);
}

#[test]
fn test_execution_result_default() {
    let er = ExecutionResult::default();
    assert!(er.success);
    assert_eq!(er.token_cost, 0.0);
}

#[test]
fn test_intent_default() {
    let intent = Intent::default();
    assert_eq!(intent.raw_text, "");
    assert!(matches!(intent.complexity, Complexity::Simple));
}

#[test]
fn test_complexity_variants() {
    let variants = [
        Complexity::Simple,
        Complexity::Moderate,
        Complexity::Complex,
        Complexity::Research,
    ];
    assert_eq!(variants.len(), 4);
}

#[test]
fn test_urgency_variants() {
    let variants = [
        Urgency::Low,
        Urgency::Medium,
        Urgency::High,
        Urgency::Critical,
    ];
    assert_eq!(variants.len(), 4);
}

#[test]
fn test_skill_level_variants() {
    let variants = [
        SkillLevel::Beginner,
        SkillLevel::Intermediate,
        SkillLevel::Expert,
    ];
    assert_eq!(variants.len(), 3);
}

#[test]
fn test_task_type_variants() {
    let variants = [
        TaskType::Create,
        TaskType::Fix,
        TaskType::Improve,
        TaskType::Explain,
        TaskType::Convert,
        TaskType::Test,
        TaskType::Deploy,
        TaskType::Review,
    ];
    assert_eq!(variants.len(), 8);
}

#[test]
fn test_file_data_creation() {
    let fd = FileData {
        name: "test.rs".to_string(),
        content: b"fn main() {}".to_vec(),
        mime_type: "text/rust".to_string(),
    };
    assert_eq!(fd.name, "test.rs");
    assert_eq!(fd.content.len(), 12);
}

#[test]
fn test_file_entry_creation() {
    let fe = FileEntry {
        path: "src/main.rs".to_string(),
        size: 1024,
        modified: Utc::now(),
    };
    assert_eq!(fe.path, "src/main.rs");
    assert_eq!(fe.size, 1024);
}

#[test]
fn test_artifact_variants() {
    let artifacts = [
        Artifact::Prompt {
            system: "sys".to_string(),
            user: "user".to_string(),
        },
        Artifact::Code {
            path: "p".to_string(),
            content: "c".to_string(),
            language: "rust".to_string(),
        },
        Artifact::Config {
            path: "p".to_string(),
            content: "c".to_string(),
            format: "toml".to_string(),
        },
        Artifact::Test {
            path: "p".to_string(),
            content: "c".to_string(),
            framework: "tokio".to_string(),
        },
        Artifact::Migration {
            path: "p".to_string(),
            content: "c".to_string(),
            database: "sqlite".to_string(),
        },
        Artifact::Documentation {
            title: "t".to_string(),
            content: "c".to_string(),
            format: "md".to_string(),
        },
    ];
    assert_eq!(artifacts.len(), 6);
}

#[test]
fn test_vibe_result_fields() {
    let vr = VibeResult {
        artifacts: vec![],
        summary: "Done".to_string(),
        next_suggestions: vec!["a".to_string()],
        cost_estimate: CostEstimate::default(),
        confidence: 0.95,
        execution_time_ms: 1234,
    };
    assert_eq!(vr.confidence, 0.95);
    assert_eq!(vr.execution_time_ms, 1234);
}

#[test]
fn test_preview_file_creation() {
    let pf = PreviewFile {
        path: "src/lib.rs".to_string(),
        content: "pub fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
        language: "rust".to_string(),
    };
    assert_eq!(pf.path, "src/lib.rs");
}

// ─────────────────────────────────────────────
// Property-based tests
// ─────────────────────────────────────────────

proptest! {
    #[test]
    fn test_uuid_roundtrip(s in "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}") {
        let uuid = Uuid::parse_str(&s);
        prop_assert!(uuid.is_ok());
    }

    #[test]
    fn test_semver_roundtrip(major in 0..100u64, minor in 0..100u64, patch in 0..100u64) {
        let version = semver::Version::new(major, minor, patch);
        prop_assert_eq!(version.major, major);
        prop_assert_eq!(version.minor, minor);
        prop_assert_eq!(version.patch, patch);
    }

    #[test]
    fn test_prompt_name_does_not_panic(name in "[a-zA-Z0-9_-]{1,50}") {
        let prompt = Prompt {
            name: name.clone(),
            ..Prompt::default()
        };
        prop_assert_eq!(prompt.name, name);
    }
}
