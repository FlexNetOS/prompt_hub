#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::*;
use tracing::{debug, info, instrument};

/// Types of previews that can be generated before execution.
///
/// Previews let the user see what will be built before committing to it,
/// improving trust and reducing surprises.
#[derive(Debug, Clone)]
pub enum PreviewType {
    /// Interactive HTML webpage preview
    Webpage { html: String, interactive: bool },
    /// OpenAPI specification preview
    ApiSpec { openapi_json: String },
    /// Database ER diagram
    DatabaseSchema { er_diagram: String },
    /// Code file preview with diff
    CodePreview {
        files: Vec<PreviewFile>,
        diff: String,
    },
    /// Architecture diagram in Mermaid syntax
    ArchitectureDiagram { mermaid: String },
}

/// A single file in a code preview.
#[derive(Debug, Clone)]
pub struct PreviewFile {
    pub path: String,
    pub content: String,
    pub language: String,
}

/// Preview engine for generating previews of execution plans.
///
/// Produces human- and machine-readable previews that can be shown
/// to the user for confirmation before actual execution.
#[derive(Debug, Clone, Default)]
pub struct PreviewEngine;

impl PreviewEngine {
    /// Generate a preview for the given execution plan.
    #[instrument]
    pub async fn generate(&self, plan: &ExecutionPlan) -> Result<PreviewType> {
        info!("Generating preview for plan: {}", plan.title);

        let mermaid = Self::generate_mermaid_diagram(plan);
        debug!("Generated Mermaid diagram with {} chars", mermaid.len());

        Ok(PreviewType::ArchitectureDiagram { mermaid })
    }

    /// Generate a code preview for a list of artifacts.
    #[instrument]
    pub async fn preview_artifacts(&self, artifacts: &[Artifact]) -> Result<PreviewType> {
        let mut files = Vec::new();
        let mut diff_lines = Vec::new();

        for (i, artifact) in artifacts.iter().enumerate() {
            match artifact {
                Artifact::Code {
                    path,
                    content,
                    language,
                } => {
                    files.push(PreviewFile {
                        path: path.clone(),
                        content: content.clone(),
                        language: language.clone(),
                    });
                    diff_lines.push(format!("+ [{i}] {path} ({language})"));
                }
                Artifact::Config {
                    path,
                    content,
                    format,
                } => {
                    files.push(PreviewFile {
                        path: path.clone(),
                        content: content.clone(),
                        language: format.clone(),
                    });
                    diff_lines.push(format!("+ [{i}] {path} ({format})"));
                }
                Artifact::Prompt { system, user } => {
                    files.push(PreviewFile {
                        path: format!("prompt_{i}.md"),
                        content: format!(
                            "## System Prompt\n\n```\n{}\n```\n\n## User Prompt\n\n```\n{}\n```\n",
                            system, user
                        ),
                        language: "markdown".to_string(),
                    });
                    diff_lines.push(format!("+ [{i}] prompt_{i}.md (prompt)"));
                }
                Artifact::Test {
                    path,
                    content,
                    framework,
                } => {
                    files.push(PreviewFile {
                        path: path.clone(),
                        content: content.clone(),
                        language: framework.clone(),
                    });
                    diff_lines.push(format!("+ [{i}] {path} (test/{framework})"));
                }
                Artifact::Migration {
                    path,
                    content,
                    database,
                } => {
                    files.push(PreviewFile {
                        path: path.clone(),
                        content: content.clone(),
                        language: "sql".to_string(),
                    });
                    diff_lines.push(format!("+ [{i}] {path} (migration/{database})"));
                }
                Artifact::Documentation {
                    title,
                    content,
                    format,
                } => {
                    files.push(PreviewFile {
                        path: format!("docs/{title}.{format}"),
                        content: content.clone(),
                        language: format.clone(),
                    });
                    diff_lines.push(format!("+ [{i}] docs/{title}.{format} (doc)"));
                }
            }
        }

        Ok(PreviewType::CodePreview {
            files,
            diff: diff_lines.join("\n"),
        })
    }

    /// Generate a database schema preview.
    #[instrument]
    pub async fn preview_database(
        &self,
        tables: &[(&str, &[(&str, &str)])],
    ) -> Result<PreviewType> {
        let mut er_diagram = String::from("erDiagram\n");

        for (table_name, columns) in tables {
            er_diagram.push_str(&format!("    {} {{\n", table_name));
            for (col_name, col_type) in *columns {
                er_diagram.push_str(&format!("        {} {}\n", col_type, col_name));
            }
            er_diagram.push_str("    }\n");
        }

        Ok(PreviewType::DatabaseSchema { er_diagram })
    }

    /// Generate an API spec preview from endpoint definitions.
    #[instrument]
    pub async fn preview_api(&self, endpoints: &[(&str, &str, &str)]) -> Result<PreviewType> {
        let mut spec = serde_json::Map::new();
        spec.insert(
            "openapi".to_string(),
            serde_json::Value::String("3.0.0".to_string()),
        );

        let mut paths = serde_json::Map::new();
        for (method, path, description) in endpoints {
            let mut path_item = serde_json::Map::new();
            let mut operation = serde_json::Map::new();
            operation.insert(
                "summary".to_string(),
                serde_json::Value::String(description.to_string()),
            );
            path_item.insert(method.to_lowercase(), serde_json::Value::Object(operation));
            paths.insert(path.to_string(), serde_json::Value::Object(path_item));
        }

        spec.insert("paths".to_string(), serde_json::Value::Object(paths));

        let openapi_json = serde_json::to_string_pretty(&serde_json::Value::Object(spec))
            .map_err(|e| HubError::Serialization(format!("JSON: {e}")))?;

        Ok(PreviewType::ApiSpec { openapi_json })
    }

    /// Generate a Mermaid diagram from an execution plan.
    fn generate_mermaid_diagram(plan: &ExecutionPlan) -> String {
        let mut mermaid = String::from("graph TD\n");
        mermaid.push_str(&format!(
            "    title[\"{}\"]\n",
            plan.title.replace('"', "\\\"")
        ));
        mermaid.push_str("    style title fill:#f9f,stroke:#333\n\n");

        // Emit all steps as nodes
        for (i, step) in plan.steps.iter().enumerate() {
            let node_id = format!("step{i}");
            let escaped_desc = step.description.replace('"', "\\\"");
            mermaid.push_str(&format!("    {node_id}[\"{escaped_desc}\"]\n"));

            // Style based on estimated duration
            let duration_style = if step.estimated_duration_secs < 60 {
                "fill:#9f9"
            } else if step.estimated_duration_secs < 300 {
                "fill:#ff9"
            } else {
                "fill:#f99"
            };
            mermaid.push_str(&format!("    style {node_id} {duration_style}\n"));
        }

        mermaid.push('\n');

        // Emit dependency edges
        for (i, step) in plan.steps.iter().enumerate() {
            for &dep in &step.dependencies {
                mermaid.push_str(&format!("    step{dep} --> step{i}\n"));
            }
        }

        // If no dependencies, show sequential flow
        if plan.steps.len() > 1 {
            let has_any_deps = plan.steps.iter().any(|s| !s.dependencies.is_empty());
            if !has_any_deps {
                for i in 0..plan.steps.len() - 1 {
                    mermaid.push_str(&format!("    step{i} --> step{}\n", i + 1));
                }
            }
        }

        mermaid
    }

    /// Generate a simple interactive HTML preview.
    #[instrument]
    pub async fn preview_webpage(
        &self,
        title: &str,
        description: &str,
        features: &[&str],
    ) -> Result<PreviewType> {
        let features_html: String = features
            .iter()
            .map(|f| format!("    <li>{}</li>\n", f))
            .collect();

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} - Preview</title>
    <style>
        body {{ font-family: system-ui, sans-serif; max-width: 800px; margin: 0 auto; padding: 2rem; }}
        h1 {{ color: #333; }}
        ul {{ line-height: 1.8; }}
        .preview-badge {{ background: #4CAF50; color: white; padding: 0.5rem 1rem; border-radius: 4px; display: inline-block; }}
    </style>
</head>
<body>
    <span class="preview-badge">Preview</span>
    <h1>{title}</h1>
    <p>{description}</p>
    <h2>Features</h2>
    <ul>
{features_html}
    </ul>
</body>
</html>"#
        );

        Ok(PreviewType::Webpage {
            html,
            interactive: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_preview_generate_mermaid() {
        let plan = ExecutionPlan {
            title: "Build Login".to_string(),
            description: "Create a login system".to_string(),
            steps: vec![
                ExecutionStep {
                    id: 0,
                    description: "Set up project".to_string(),
                    action: "init".to_string(),
                    dependencies: vec![],
                    estimated_duration_secs: 30,
                },
                ExecutionStep {
                    id: 1,
                    description: "Create auth module".to_string(),
                    action: "generate".to_string(),
                    dependencies: vec![0],
                    estimated_duration_secs: 120,
                },
                ExecutionStep {
                    id: 2,
                    description: "Create UI".to_string(),
                    action: "generate".to_string(),
                    dependencies: vec![0],
                    estimated_duration_secs: 90,
                },
            ],
            total_estimated_duration_secs: 240,
        };

        let engine = PreviewEngine::default();
        let preview = engine.generate(&plan).await.unwrap();

        match preview {
            PreviewType::ArchitectureDiagram { mermaid } => {
                assert!(mermaid.contains("graph TD"));
                assert!(mermaid.contains("step0"));
                assert!(mermaid.contains("step1"));
                assert!(mermaid.contains("step2"));
                assert!(mermaid.contains("step0 --> step1"));
                assert!(mermaid.contains("step0 --> step2"));
            }
            other => panic!("Expected ArchitectureDiagram, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_preview_artifacts() {
        let artifacts = vec![
            Artifact::Prompt {
                system: "You are a developer".to_string(),
                user: "Build a login page".to_string(),
            },
            Artifact::Code {
                path: "src/auth.rs".to_string(),
                content: "pub fn login() {{}}".to_string(),
                language: "rust".to_string(),
            },
        ];

        let engine = PreviewEngine::default();
        let preview = engine.preview_artifacts(&artifacts).await.unwrap();

        match preview {
            PreviewType::CodePreview { files, diff } => {
                assert_eq!(files.len(), 2);
                assert!(diff.contains("src/auth.rs"));
                assert!(files.iter().any(|f| f.path == "src/auth.rs"));
            }
            other => panic!("Expected CodePreview, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_preview_database() {
        let engine = PreviewEngine::default();
        let tables = vec![
            (
                "users",
                &[
                    ("id", "INT"),
                    ("email", "VARCHAR"),
                    ("password_hash", "VARCHAR"),
                ] as &[(&str, &str)],
            ),
            (
                "posts",
                &[("id", "INT"), ("title", "VARCHAR"), ("user_id", "INT")] as &[(&str, &str)],
            ),
        ];

        let preview = engine.preview_database(&tables).await.unwrap();

        match preview {
            PreviewType::DatabaseSchema { er_diagram } => {
                assert!(er_diagram.contains("erDiagram"));
                assert!(er_diagram.contains("users"));
                assert!(er_diagram.contains("posts"));
                assert!(er_diagram.contains("INT id"));
            }
            other => panic!("Expected DatabaseSchema, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_preview_api() {
        let engine = PreviewEngine::default();
        let endpoints = vec![
            ("POST", "/api/auth/login", "Authenticate user"),
            ("GET", "/api/auth/me", "Get current user"),
        ];

        let preview = engine.preview_api(&endpoints).await.unwrap();

        match preview {
            PreviewType::ApiSpec { openapi_json } => {
                assert!(openapi_json.contains("3.0.0"));
                assert!(openapi_json.contains("/api/auth/login"));
                assert!(openapi_json.contains("Authenticate user"));
            }
            other => panic!("Expected ApiSpec, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_preview_webpage() {
        let engine = PreviewEngine::default();
        let preview = engine
            .preview_webpage(
                "My App",
                "A cool application",
                &["Auth", "Dashboard", "Settings"],
            )
            .await
            .unwrap();

        match preview {
            PreviewType::Webpage { html, interactive } => {
                assert!(html.contains("My App"));
                assert!(html.contains("A cool application"));
                assert!(html.contains("Auth"));
                assert!(!interactive);
            }
            other => panic!("Expected Webpage, got {:?}", other),
        }
    }

    #[test]
    fn test_generate_mermaid_empty_plan() {
        let plan = ExecutionPlan {
            title: "Empty".to_string(),
            steps: vec![],
            ..Default::default()
        };

        let mermaid = PreviewEngine::generate_mermaid_diagram(&plan);
        assert!(mermaid.contains("graph TD"));
    }

    #[tokio::test]
    async fn test_preview_sequential_flow() {
        // Plan with no explicit dependencies should show sequential flow
        let plan = ExecutionPlan {
            title: "Sequential".to_string(),
            description: "Sequential plan".to_string(),
            steps: vec![
                ExecutionStep {
                    id: 0,
                    description: "A".to_string(),
                    action: "a".to_string(),
                    dependencies: vec![],
                    estimated_duration_secs: 10,
                },
                ExecutionStep {
                    id: 1,
                    description: "B".to_string(),
                    action: "b".to_string(),
                    dependencies: vec![],
                    estimated_duration_secs: 10,
                },
            ],
            total_estimated_duration_secs: 20,
        };

        let mermaid = PreviewEngine::generate_mermaid_diagram(&plan);
        assert!(mermaid.contains("step0 --> step1"));
    }
}
