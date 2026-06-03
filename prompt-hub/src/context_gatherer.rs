#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{FileEntry, ProjectContext};
use chrono::Utc;
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, instrument, warn};

/// Auto-context-gathering from the filesystem.
///
/// Scans a project directory to detect language, framework, database,
/// styling approach, auth patterns, and file structure — enabling
/// zero-question Vibe Coding.
#[derive(Debug, Clone, Default)]
pub struct ContextGatherer;

impl ContextGatherer {
    /// Gather full project context from the given directory path.
    #[instrument]
    pub async fn gather(project_path: &Path) -> Result<ProjectContext> {
        info!("Gathering context from {:?}", project_path);

        let mut files = Vec::new();
        let mut deps: HashMap<String, String> = HashMap::new();

        // Scan directory
        let mut entries = tokio::fs::read_dir(project_path)
            .await
            .map_err(|e| HubError::Internal(format!("Read dir: {e}")))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| HubError::Internal(format!("Dir entry: {e}")))?
        {
            let path = entry.path();

            // Skip hidden directories and common non-project paths
            if let Some(fname) = path.file_name().and_then(|n| n.to_str())
                && fname.starts_with('.')
                && fname != ".github"
                && fname != ".prompthub"
                && fname != ".env"
                && fname != ".env.local"
                && path.is_dir()
            {
                continue;
            }

            if let Ok(meta) = entry.metadata().await {
                files.push(FileEntry {
                    path: path.to_string_lossy().to_string(),
                    size: meta.len(),
                    modified: Utc::now(),
                });
            }

            // Detect language / framework from key files at the root level
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                match name {
                    "Cargo.toml" => {
                        deps.insert("language".to_string(), "rust".to_string());
                        deps.insert(
                            "framework".to_string(),
                            detect_rust_framework(project_path).await,
                        );
                    }
                    "package.json" => {
                        deps.insert("language".to_string(), "javascript".to_string());
                        if let Ok(pkg) = read_package_json(project_path).await {
                            deps.insert("framework".to_string(), pkg);
                        }
                    }
                    "requirements.txt" | "pyproject.toml" => {
                        deps.insert("language".to_string(), "python".to_string());
                        deps.insert(
                            "framework".to_string(),
                            detect_python_framework(project_path).await,
                        );
                    }
                    "go.mod" => {
                        deps.insert("language".to_string(), "go".to_string());
                    }
                    "pom.xml" | "build.gradle" | "build.gradle.kts" => {
                        deps.insert("language".to_string(), "java".to_string());
                    }
                    _ => {}
                }
            }
        }

        let language = deps
            .get("language")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let framework = deps
            .get("framework")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let database = detect_database(project_path).await;
        let styling = detect_styling(project_path).await;
        let auth = detect_auth(project_path).await;

        // Read environment variables from .env if present
        let env_vars = read_dotenv(project_path).await.unwrap_or_default();

        info!(
            "Detected: lang={}, fw={}, db={:?}, styling={:?}, auth={:?}",
            language, framework, database, styling, auth
        );

        Ok(ProjectContext {
            project_path: project_path.to_string_lossy().to_string(),
            language,
            framework,
            database,
            styling,
            auth,
            existing_files: files,
            environment_variables: env_vars,
            team_size: 1,
        })
    }

    /// Quick-check: does the path look like a supported project?
    pub async fn is_project(project_path: &Path) -> bool {
        if !project_path.is_dir() {
            return false;
        }

        let markers = [
            "Cargo.toml",
            "package.json",
            "go.mod",
            "requirements.txt",
            "pyproject.toml",
            "pom.xml",
            "build.gradle",
        ];

        for marker in &markers {
            if project_path.join(marker).exists() {
                return true;
            }
        }

        false
    }
}

// ─────────────────────────────────────────────
// Detection helpers
// ─────────────────────────────────────────────

async fn detect_database(path: &Path) -> Option<String> {
    // Check for Prisma
    if path.join("prisma/schema.prisma").exists() {
        return Some("postgres".to_string());
    }

    // Check for docker-compose with db services
    if path.join("docker-compose.yml").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("docker-compose.yml")).await
    {
        if content.contains("postgres") || content.contains("postgresql") {
            return Some("postgres".to_string());
        }
        if content.contains("mysql") {
            return Some("mysql".to_string());
        }
        if content.contains("mongodb") {
            return Some("mongodb".to_string());
        }
        if content.contains("redis") {
            return Some("redis".to_string());
        }
    }

    // Check for SQLx
    if path.join("sqlx-data.json").exists() || path.join("migrations").is_dir() {
        return Some("postgres".to_string());
    }

    // Check for Diesel
    if path.join("diesel.toml").exists() {
        return Some("postgres".to_string());
    }

    // Check for SeaORM
    if path.join("Cargo.toml").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("Cargo.toml")).await
    {
        if content.contains("sea-orm") || content.contains("sqlx") {
            return Some("postgres".to_string());
        }
        if content.contains("diesel") {
            return Some("diesel-supported".to_string());
        }
        if content.contains("rusqlite") {
            return Some("sqlite".to_string());
        }
    }

    // Check Python projects
    if path.join("requirements.txt").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("requirements.txt")).await
    {
        if content.contains("sqlalchemy") {
            return Some("postgres".to_string());
        }
        if content.contains("django") {
            return Some("django-orm".to_string());
        }
        if content.contains("psycopg") || content.contains("psycopg2") {
            return Some("postgres".to_string());
        }
    }

    // Check package.json for ORM hints
    if path.join("package.json").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("package.json")).await
    {
        if content.contains("prisma") || content.contains("@prisma") {
            return Some("postgres".to_string());
        }
        if content.contains("mongoose") {
            return Some("mongodb".to_string());
        }
        if content.contains("typeorm") {
            return Some("postgres".to_string());
        }
    }

    None
}

async fn detect_styling(path: &Path) -> Option<String> {
    if path.join("tailwind.config.js").exists()
        || path.join("tailwind.config.ts").exists()
        || path.join("tailwind.config.mjs").exists()
    {
        return Some("tailwind".to_string());
    }

    if path.join("postcss.config.js").exists() || path.join("postcss.config.ts").exists() {
        return Some("postcss".to_string());
    }

    // Check package.json for styled-components
    if path.join("package.json").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("package.json")).await
    {
        if content.contains("styled-components") {
            return Some("styled-components".to_string());
        }
        if content.contains("@emotion") {
            return Some("emotion".to_string());
        }
        if content.contains("sass") || content.contains("node-sass") {
            return Some("sass".to_string());
        }
        if content.contains("bootstrap") {
            return Some("bootstrap".to_string());
        }
    }

    // Check Cargo.toml for Rust styling
    if path.join("Cargo.toml").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("Cargo.toml")).await
        && (content.contains("tailwind-rs") || content.contains("tailwind_css"))
    {
        return Some("tailwind".to_string());
    }

    None
}

async fn detect_auth(path: &Path) -> Option<String> {
    // Check for auth directory
    if path.join("src/auth").is_dir()
        || path.join("app/auth").is_dir()
        || path.join("pages/auth").is_dir()
    {
        return Some("custom".to_string());
    }

    // Check for middleware (Next.js auth pattern)
    if path.join("middleware.ts").exists() || path.join("middleware.js").exists() {
        return Some("middleware".to_string());
    }

    // Check package.json for auth libraries
    if path.join("package.json").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("package.json")).await
    {
        if content.contains("next-auth") || content.contains("@auth") {
            return Some("next-auth".to_string());
        }
        if content.contains("@clerk") {
            return Some("clerk".to_string());
        }
        if content.contains("passport") {
            return Some("passport".to_string());
        }
        if content.contains("@supabase") {
            return Some("supabase".to_string());
        }
    }

    // Check Cargo.toml for Rust auth
    if path.join("Cargo.toml").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("Cargo.toml")).await
    {
        if content.contains("jsonwebtoken") || content.contains("jwt") {
            return Some("jwt".to_string());
        }
        if content.contains("oauth2") {
            return Some("oauth2".to_string());
        }
    }

    // Check for .env with auth vars
    if path.join(".env").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join(".env")).await
        && (content.contains("JWT") || content.contains("AUTH"))
    {
        return Some("jwt".to_string());
    }

    None
}

async fn detect_rust_framework(path: &Path) -> String {
    if let Ok(content) = tokio::fs::read_to_string(path.join("Cargo.toml")).await {
        if content.contains("axum") {
            return "axum".to_string();
        }
        if content.contains("actix-web") || content.contains("actix_web") {
            return "actix-web".to_string();
        }
        if content.contains("rocket") {
            return "rocket".to_string();
        }
        if content.contains("salvo") {
            return "salvo".to_string();
        }
        if content.contains("leptos") {
            return "leptos".to_string();
        }
        if content.contains("dioxus") {
            return "dioxus".to_string();
        }
        if content.contains("tauri") {
            return "tauri".to_string();
        }
    }
    "unknown".to_string()
}

async fn read_package_json(path: &Path) -> Result<String> {
    let content = tokio::fs::read_to_string(path.join("package.json"))
        .await
        .map_err(|e| HubError::Io(format!("package.json: {e}")))?;

    // Quick heuristic: check for framework in dependencies
    if content.contains("next") {
        Ok("nextjs".to_string())
    } else if content.contains("react") {
        Ok("react".to_string())
    } else if content.contains("vue") {
        Ok("vue".to_string())
    } else if content.contains("@angular") || content.contains("angular") {
        Ok("angular".to_string())
    } else if content.contains("svelte") {
        Ok("svelte".to_string())
    } else if content.contains("express") {
        Ok("express".to_string())
    } else if content.contains("fastify") {
        Ok("fastify".to_string())
    } else {
        Ok("node".to_string())
    }
}

async fn detect_python_framework(path: &Path) -> String {
    if let Ok(content) = tokio::fs::read_to_string(path.join("requirements.txt")).await {
        if content.contains("django") {
            return "django".to_string();
        }
        if content.contains("flask") {
            return "flask".to_string();
        }
        if content.contains("fastapi") {
            return "fastapi".to_string();
        }
    }
    if path.join("pyproject.toml").exists()
        && let Ok(content) = tokio::fs::read_to_string(path.join("pyproject.toml")).await
    {
        if content.contains("django") {
            return "django".to_string();
        }
        if content.contains("flask") {
            return "flask".to_string();
        }
        if content.contains("fastapi") {
            return "fastapi".to_string();
        }
    }
    "unknown".to_string()
}

async fn read_dotenv(path: &Path) -> Result<HashMap<String, String>> {
    let mut vars = HashMap::new();

    for filename in [".env", ".env.local", ".env.development"] {
        let dotenv_path = path.join(filename);
        if dotenv_path.exists() {
            match tokio::fs::read_to_string(&dotenv_path).await {
                Ok(content) => {
                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        if let Some((key, value)) = line.split_once('=') {
                            // Only capture non-secret keys
                            let key = key.trim().to_string();
                            if !is_sensitive_key(&key) {
                                vars.insert(key, value.trim().to_string());
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Could not read {}: {}", filename, e);
                }
            }
        }
    }

    Ok(vars)
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("secret")
        || lower.contains("password")
        || lower.contains("token")
        || lower.contains("key")
        || lower.contains("credential")
        || lower.contains("api_key")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_context_gatherer_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let ctx = ContextGatherer::gather(tmp.path()).await.unwrap();

        assert_eq!(ctx.language, "unknown");
        assert_eq!(ctx.framework, "unknown");
    }

    #[tokio::test]
    async fn test_detect_rust_project() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            r#"
[package]
name = "test-app"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
"#,
        )
        .unwrap();

        let ctx = ContextGatherer::gather(tmp.path()).await.unwrap();
        assert_eq!(ctx.language, "rust");
        assert_eq!(ctx.framework, "axum");
    }

    #[tokio::test]
    async fn test_detect_node_react_project() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("package.json"),
            r#"
{
  "name": "my-app",
  "dependencies": {
    "react": "^18.0.0",
    "react-dom": "^18.0.0"
  }
}
"#,
        )
        .unwrap();

        let ctx = ContextGatherer::gather(tmp.path()).await.unwrap();
        assert_eq!(ctx.language, "javascript");
        assert_eq!(ctx.framework, "react");
    }

    #[tokio::test]
    async fn test_detect_tailwind() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("package.json"),
            "{}
",
        )
        .unwrap();
        fs::write(tmp.path().join("tailwind.config.js"), "module.exports = {}").unwrap();

        let styling = detect_styling(tmp.path()).await;
        assert_eq!(styling, Some("tailwind".to_string()));
    }

    #[tokio::test]
    async fn test_detect_database_from_docker_compose() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("docker-compose.yml"),
            r#"
services:
  db:
    image: postgres:15
"#,
        )
        .unwrap();

        let db = detect_database(tmp.path()).await;
        assert_eq!(db, Some("postgres".to_string()));
    }

    #[tokio::test]
    async fn test_is_project_true() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]\n").unwrap();

        assert!(ContextGatherer::is_project(tmp.path()).await);
    }

    #[tokio::test]
    async fn test_is_project_false() {
        let tmp = TempDir::new().unwrap();
        assert!(!ContextGatherer::is_project(tmp.path()).await);
    }

    #[tokio::test]
    async fn test_read_dotenv_filters_secrets() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join(".env"),
            r#"
DATABASE_URL=postgres://localhost/mydb
API_KEY=super-secret
PUBLIC_VAR=hello
SECRET_TOKEN=shhh
"#,
        )
        .unwrap();

        let vars = read_dotenv(tmp.path()).await.unwrap();
        assert!(vars.contains_key("PUBLIC_VAR"));
        assert_eq!(vars.get("PUBLIC_VAR").unwrap(), "hello");
        assert!(!vars.contains_key("API_KEY"));
        assert!(!vars.contains_key("SECRET_TOKEN"));
    }

    #[test]
    fn test_is_sensitive_key() {
        assert!(is_sensitive_key("API_KEY"));
        assert!(is_sensitive_key("DATABASE_PASSWORD"));
        assert!(is_sensitive_key("secret_token"));
        assert!(!is_sensitive_key("PUBLIC_VAR"));
        assert!(!is_sensitive_key("PORT"));
    }

    #[tokio::test]
    async fn test_detect_python_project() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("requirements.txt"), "fastapi\nuvicorn\n").unwrap();

        let ctx = ContextGatherer::gather(tmp.path()).await.unwrap();
        assert_eq!(ctx.language, "python");
        assert_eq!(ctx.framework, "fastapi");
    }

    #[tokio::test]
    async fn test_detect_nextjs() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("package.json"),
            r#"{"dependencies":{"next":"^14.0.0","react":"^18.0.0"}}"#,
        )
        .unwrap();

        let ctx = ContextGatherer::gather(tmp.path()).await.unwrap();
        assert_eq!(ctx.framework, "nextjs");
    }

    #[tokio::test]
    async fn test_detect_go_project() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("go.mod"),
            "module example.com/test\n\ngo 1.21\n",
        )
        .unwrap();

        let ctx = ContextGatherer::gather(tmp.path()).await.unwrap();
        assert_eq!(ctx.language, "go");
    }
}
