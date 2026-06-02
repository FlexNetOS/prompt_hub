#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::*;
use tracing::{info, instrument};

/// Multi-modal input processor for voice, screenshot, sketch, and file inputs.
///
/// Converts various input types into a structured `Intent` for downstream
/// processing by the vibe coding engine.
#[derive(Debug, Clone, Default)]
pub struct MultiModalInput;

impl MultiModalInput {
    /// Process user input and convert it to a structured intent.
    ///
    /// Handles each `InputType` with domain-appropriate defaults:
    /// - **Text**: Direct intent extraction from text
    /// - **Voice**: Transcription then text processing
    /// - **Screenshot**: UI/domain detection with architectural role
    /// - **Sketch**: Similar to screenshot with design domain
    /// - **File**: File content analysis
    /// - **Url**: Web resource processing
    #[instrument]
    pub async fn process(&self, input: UserInput) -> Result<Intent> {
        info!(input_type = ?input.input_type, "Processing multimodal input");

        match input.input_type {
            InputType::Text => {
                info!("Processing text input");
                Ok(Intent {
                    raw_text: input.extracted_text.clone(),
                    domain: Self::infer_domain(&input.extracted_text),
                    role: Role::Orchestrator,
                    task_type: Self::infer_task_type(&input.extracted_text),
                    complexity: Self::infer_complexity(&input.extracted_text),
                    urgency: Urgency::Medium,
                    extracted_entities: std::collections::HashMap::new(),
                })
            }
            InputType::Voice => {
                info!("Processing voice input (transcribed to text)");
                Ok(Intent {
                    raw_text: input.extracted_text.clone(),
                    domain: Self::infer_domain(&input.extracted_text),
                    role: Role::Orchestrator,
                    task_type: Self::infer_task_type(&input.extracted_text),
                    complexity: Self::infer_complexity(&input.extracted_text),
                    urgency: Urgency::Medium,
                    extracted_entities: std::collections::HashMap::new(),
                })
            }
            InputType::Screenshot => {
                info!("Processing screenshot input (UI/domain detection)");
                Ok(Intent {
                    raw_text: format!(
                        "Build UI like screenshot: {}",
                        input.extracted_text
                    ),
                    domain: Domain::Design,
                    role: Role::Architect,
                    task_type: TaskType::Create,
                    complexity: Complexity::Moderate,
                    urgency: Urgency::Medium,
                    extracted_entities: std::collections::HashMap::new(),
                })
            }
            InputType::Sketch => {
                info!("Processing sketch input (wireframe detection)");
                Ok(Intent {
                    raw_text: format!(
                        "Build from sketch/wireframe: {}",
                        input.extracted_text
                    ),
                    domain: Domain::Design,
                    role: Role::Designer,
                    task_type: TaskType::Create,
                    complexity: Complexity::Moderate,
                    urgency: Urgency::Medium,
                    extracted_entities: std::collections::HashMap::new(),
                })
            }
            InputType::File => {
                info!("Processing file input (content analysis)");
                Ok(Intent {
                    raw_text: format!(
                        "Analyze and process file content: {}",
                        input.extracted_text
                    ),
                    domain: Domain::Coding,
                    role: Role::Orchestrator,
                    task_type: TaskType::Review,
                    complexity: Complexity::Simple,
                    urgency: Urgency::Medium,
                    extracted_entities: std::collections::HashMap::new(),
                })
            }
            InputType::Url => {
                info!("Processing URL input (web resource)");
                Ok(Intent {
                    raw_text: format!(
                        "Process web resource at: {}",
                        input.extracted_text
                    ),
                    domain: Domain::General,
                    role: Role::Orchestrator,
                    task_type: TaskType::Convert,
                    complexity: Complexity::Simple,
                    urgency: Urgency::Medium,
                    extracted_entities: std::collections::HashMap::new(),
                })
            }
        }
    }

    /// Infer the domain from the text content.
    fn infer_domain(text: &str) -> Domain {
        let lower = text.to_lowercase();
        if lower.contains("test") || lower.contains("spec") || lower.contains("assert") {
            Domain::Testing
        } else if lower.contains("deploy") || lower.contains("docker") || lower.contains("k8s") || lower.contains("kubernetes") {
            Domain::DevOps
        } else if lower.contains("design") || lower.contains("ui") || lower.contains("css") || lower.contains("layout") {
            Domain::Design
        } else if lower.contains("write") || lower.contains("doc") || lower.contains("blog") || lower.contains("article") {
            Domain::Writing
        } else if lower.contains("analy") || lower.contains("research") || lower.contains("investigate") {
            Domain::Analysis
        } else if lower.contains("code") || lower.contains("function") || lower.contains("api") || lower.contains("impl") {
            Domain::Coding
        } else {
            Domain::General
        }
    }

    /// Infer the task type from the text content.
    fn infer_task_type(text: &str) -> TaskType {
        let lower = text.to_lowercase();
        if lower.starts_with("fix") || lower.starts_with("bug") || lower.starts_with("repair") {
            TaskType::Fix
        } else if lower.starts_with("improve") || lower.starts_with("refactor") || lower.starts_with("optim") {
            TaskType::Improve
        } else if lower.starts_with("explain") || lower.starts_with("why") || lower.starts_with("how") {
            TaskType::Explain
        } else if lower.starts_with("convert") || lower.starts_with("translat") || lower.starts_with("turn") {
            TaskType::Convert
        } else if lower.starts_with("test") {
            TaskType::Test
        } else if lower.starts_with("deploy") || lower.starts_with("push") || lower.starts_with("release") {
            TaskType::Deploy
        } else if lower.starts_with("review") || lower.starts_with("check") || lower.starts_with("audit") {
            TaskType::Review
        } else {
            TaskType::Create
        }
    }

    /// Infer the complexity from the text content.
    fn infer_complexity(text: &str) -> Complexity {
        let word_count = text.split_whitespace().count();
        if word_count < 5 {
            Complexity::Simple
        } else if word_count < 20 {
            Complexity::Moderate
        } else if word_count < 50 {
            Complexity::Complex
        } else {
            Complexity::Research
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_text() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "Create a REST API with user authentication".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.domain, Domain::Coding);
        assert_eq!(intent.task_type, TaskType::Create);
        assert_eq!(intent.role, Role::Orchestrator);
    }

    #[tokio::test]
    async fn test_process_voice() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Voice,
            raw_data: vec![1, 2, 3],
            extracted_text: "Build me a blog".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.domain, Domain::Coding);
        assert_eq!(intent.task_type, TaskType::Create);
        assert_eq!(intent.domain, Domain::Coding);
    }

    #[tokio::test]
    async fn test_process_screenshot() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Screenshot,
            raw_data: vec![4, 5, 6],
            extracted_text: "Login page with dark mode".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.domain, Domain::Design);
        assert_eq!(intent.role, Role::Architect);
        assert_eq!(intent.task_type, TaskType::Create);
        assert_eq!(intent.complexity, Complexity::Moderate);
        assert!(intent.raw_text.contains("Build UI like screenshot"));
    }

    #[tokio::test]
    async fn test_process_sketch() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Sketch,
            raw_data: vec![7, 8, 9],
            extracted_text: "Wireframe with sidebar and main content area".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.domain, Domain::Design);
        assert_eq!(intent.role, Role::Designer);
        assert!(intent.raw_text.contains("sketch"));
    }

    #[tokio::test]
    async fn test_process_file() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::File,
            raw_data: vec![10, 11, 12],
            extracted_text: "source.rs contains 500 lines".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.domain, Domain::Coding);
        assert_eq!(intent.task_type, TaskType::Review);
        assert!(intent.raw_text.contains("file content"));
    }

    #[tokio::test]
    async fn test_process_url() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Url,
            raw_data: vec![],
            extracted_text: "https://example.com/api/docs".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.domain, Domain::General);
        assert_eq!(intent.task_type, TaskType::Convert);
        assert!(intent.raw_text.contains("https://example.com"));
    }

    #[tokio::test]
    async fn test_infer_domain_coding() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "Create a function to sort arrays".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.domain, Domain::Coding);
    }

    #[tokio::test]
    async fn test_infer_domain_devops() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "Deploy with docker and kubernetes".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.domain, Domain::DevOps);
    }

    #[tokio::test]
    async fn test_infer_task_type_fix() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "fix the authentication bug".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.task_type, TaskType::Fix);
    }

    #[tokio::test]
    async fn test_infer_task_type_explain() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "why does this code fail".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.task_type, TaskType::Explain);
    }

    #[tokio::test]
    async fn test_infer_complexity() {
        let processor = MultiModalInput;
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            // 50+ words should be Research
            extracted_text: "Create a full-stack application with user authentication, database integration, real-time notifications, and a responsive frontend that works on mobile and desktop with dark mode support and accessibility compliance following WCAG 2.1 AA standards".to_string(),
        };
        let intent = processor.process(input).await.unwrap();
        assert_eq!(intent.complexity, Complexity::Research);
    }
}
