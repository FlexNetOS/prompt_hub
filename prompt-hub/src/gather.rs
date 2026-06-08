#![forbid(unsafe_code)]

//! Smart context gathering — priority-ranked file discovery and code pattern extraction.
//!
//! Provides a `SmartContextGatherer` that walks a project directory, scores files by
//! relevance using a configurable priority map with depth decay, and extracts structural
//! patterns (imports, function signatures, struct/trait definitions) via lightweight
//! regex over the first 200 lines of each file.
//!
//! This module is feature-gated behind `gather`.

use crate::error::{HubError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, instrument, warn};

// ─────────────────────────────────────────────
// Data types
// ─────────────────────────────────────────────

/// Category of a file for relevance purposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileCategory {
    /// Configuration files (Cargo.toml, package.json, etc.)
    Config,
    /// Entry-point source files (main.rs, lib.rs, index.ts, etc.)
    SourceEntry,
    /// Module source files (src/**/*.rs, lib/**/*.js, etc.)
    Module,
    /// Test files
    Test,
    /// Documentation files (README, CONTRIBUTING, docs/, etc.)
    Documentation,
    /// Build-system files (Makefile, Dockerfile, justfile, etc.)
    BuildSystem,
    /// No specific category detected.
    Unknown,
}

/// A file entry with its computed relevance score and category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceEntry {
    /// File path relative to the project root (or absolute if the scan started there).
    pub path: String,
    /// Relevance score in the range `[0.0, 1.0]`, higher = more relevant.
    pub relevance_score: f64,
    /// The category assigned by the scanner.
    pub category: FileCategory,
}

/// A structural pattern extracted from a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePattern {
    /// Absolute path of the source file.
    pub file_path: String,
    /// Type of pattern detected.
    pub pattern_type: PatternType,
    /// 1-based line number where the pattern was found.
    pub line_number: u32,
    /// Estimated architectural significance in `[0.0, 1.0]`.
    pub significance_score: f64,
}

/// A structural pattern category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternType {
    /// An import / include directive (with the matched path pattern).
    Import(PathPattern),
    /// A function signature (the raw text of `fn name(...)`).
    FunctionSignature(String),
    /// A struct definition with its name and field count.
    StructDefinition { name: String, fields_count: usize },
    /// A trait definition with its name and inferred method count.
    TraitDefinition { name: String, methods: usize },
    /// A user-defined custom pattern (e.g. enum, impl block).
    CustomPattern { label: String, snippet: String },
}

/// A file-path glob/regex used in an import directive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathPattern(pub String);

/// Enhanced project context combining base metadata with smart relevance + patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartContext {
    /// Base project metadata (language, framework, etc.).
    pub project_path: String,
    /// Detected language of the project.
    pub language: String,
    /// Detected framework / runtime.
    pub framework: String,
    /// Priority-ranked files, sorted descending by relevance score.
    pub relevant_files: Vec<RelevanceEntry>,
    /// Extracted structural patterns from key source files.
    pub code_patterns: Vec<CodePattern>,
    /// Total number of files scanned (including low-score ones).
    pub total_files_scanned: usize,
    /// Minimum relevance threshold applied during filtering.
    pub relevance_threshold: f64,
}

// ─────────────────────────────────────────────
// Priority maps (filename → base score + category)
// ─────────────────────────────────────────────

/// Files that are always treated as top-priority config.
fn priority_config_files() -> HashMap<&'static str, (f64, FileCategory)> {
    [
        ("cargo.toml", (0.95, FileCategory::Config)),
        ("package.json", (0.95, FileCategory::Config)),
        ("pyproject.toml", (0.90, FileCategory::Config)),
        ("setup.cfg", (0.70, FileCategory::Config)),
        ("go.mod", (0.90, FileCategory::Config)),
        ("go.sum", (0.60, FileCategory::Config)),
        ("docker-compose.yml", (0.85, FileCategory::BuildSystem)),
        ("docker-compose.yaml", (0.85, FileCategory::BuildSystem)),
        ("dockerfile", (0.80, FileCategory::BuildSystem)),
    ]
    .into_iter()
    .collect()
}

/// Entry-point source files.
fn priority_entry_files() -> HashMap<&'static str, (f64, FileCategory)> {
    [
        ("main.rs", (0.95, FileCategory::SourceEntry)),
        ("lib.rs", (0.90, FileCategory::SourceEntry)),
        ("index.ts", (0.85, FileCategory::SourceEntry)),
        ("index.js", (0.80, FileCategory::SourceEntry)),
        ("app.py", (0.85, FileCategory::SourceEntry)),
        ("main.go", (0.90, FileCategory::SourceEntry)),
        ("mod.rs", (0.75, FileCategory::SourceEntry)),
    ]
    .into_iter()
    .collect()
}

/// Documentation files.
fn priority_doc_files() -> HashMap<&'static str, (f64, FileCategory)> {
    [
        ("readme.md", (0.85, FileCategory::Documentation)),
        ("readme.txt", (0.70, FileCategory::Documentation)),
        ("contributing.md", (0.70, FileCategory::Documentation)),
        ("license", (0.60, FileCategory::Documentation)),
        ("changelog.md", (0.55, FileCategory::Documentation)),
        ("readme", (0.65, FileCategory::Documentation)),
    ]
    .into_iter()
    .collect()
}

/// Build-system files that don't match the config map above.
fn priority_build_files() -> HashMap<&'static str, (f64, FileCategory)> {
    [
        ("makefile", (0.75, FileCategory::BuildSystem)),
        ("justfile", (0.70, FileCategory::BuildSystem)),
        ("cmakelists.txt", (0.75, FileCategory::BuildSystem)),
        ("build.gradle", (0.65, FileCategory::BuildSystem)),
        ("build.gradle.kts", (0.65, FileCategory::BuildSystem)),
        ("pom.xml", (0.65, FileCategory::BuildSystem)),
        ("yarn.lock", (0.50, FileCategory::Config)),
        ("pnpm-lock.yaml", (0.50, FileCategory::Config)),
        ("package-lock.json", (0.50, FileCategory::Config)),
    ]
    .into_iter()
    .collect()
}

/// Rust-specific entry-point hints for `Cargo.toml` binaries / libs.
fn priority_rust_entry() -> HashMap<&'static str, (f64, FileCategory)> {
    [
        ("main.rs", (0.95, FileCategory::SourceEntry)),
        ("lib.rs", (0.90, FileCategory::SourceEntry)),
        ("mod.rs", (0.75, FileCategory::Module)),
    ]
    .into_iter()
    .collect()
}

// ─────────────────────────────────────────────
// SmartContextGatherer
// ─────────────────────────────────────────────

/// Default relevance threshold applied when filtering low-score files.
const DEFAULT_THRESHOLD: f64 = 0.3;

/// Depth-limit for recursive scanning (levels).
const MAX_DEPTH: u32 = 5;

/// Number of lines to read from each file during pattern extraction.
const PATTERN_READ_LINES: usize = 200;

/// Depth-decay multiplier per level.
const DEAY_PER_LEVEL: f64 = 0.8;

#[derive(Debug, Clone, Default)]
pub struct SmartContextGatherer;

impl SmartContextGatherer {
    // ── public API ────────────────────────────────────────────────────────

    /// Gather enhanced project context with relevance-ranked files and extracted code patterns.
    #[instrument(skip(self))]
    pub async fn gather_smart(&self, project_path: &Path) -> Result<SmartContext> {
        info!("Starting smart context gather for {:?}", project_path);

        // Base context via the existing gatherer.
        let base = crate::context_gatherer::ContextGatherer::gather(project_path).await?;

        // Scan all files (depth-limited walk) and score them.
        let all_files = self.scan_and_score(project_path).await?;

        // Filter by threshold and sort descending.
        let mut relevant: Vec<RelevanceEntry> = all_files
            .into_iter()
            .filter(|e| e.relevance_score >= DEFAULT_THRESHOLD)
            .collect();
        relevant.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Extract patterns from the top files (up to 10 with score >= 0.5).
        let pattern_files: Vec<&RelevanceEntry> = relevant
            .iter()
            .filter(|e| e.relevance_score >= 0.5)
            .take(10)
            .collect();
        let code_patterns = self
            .extract_patterns_from_entries(project_path, &pattern_files)
            .await;

        info!(
            "Smart gather complete: {} files scanned, {} relevant, {} patterns extracted",
            base.existing_files.len(),
            relevant.len(),
            code_patterns.len()
        );

        Ok(SmartContext {
            project_path: base.project_path,
            language: base.language,
            framework: base.framework,
            relevant_files: relevant,
            code_patterns,
            total_files_scanned: base.existing_files.len(),
            relevance_threshold: DEFAULT_THRESHOLD,
        })
    }

    /// Collect relevance-ranked files for a project (threshold applied).
    pub async fn collect_relevant_files(&self, project_path: &Path) -> Vec<RelevanceEntry> {
        let all = match self.scan_and_score(project_path).await {
            Ok(files) => files,
            Err(e) => {
                warn!(
                    "collect_relevant_files failed for {:?}: {}",
                    project_path, e
                );
                return Vec::new();
            }
        };
        let mut relevant: Vec<RelevanceEntry> = all
            .into_iter()
            .filter(|e| e.relevance_score >= DEFAULT_THRESHOLD)
            .collect();
        relevant.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        relevant
    }

    /// Extract code patterns from key source files.
    pub async fn extract_patterns(&self, project_path: &Path) -> Vec<CodePattern> {
        let entries = self.collect_relevant_files(project_path).await;
        let pattern_files: Vec<&RelevanceEntry> = entries
            .iter()
            .filter(|e| e.relevance_score >= 0.5)
            .take(10)
            .collect();
        self.extract_patterns_from_entries(project_path, &pattern_files)
            .await
    }

    // ── internal helpers ───────────────────────────────────────────────────

    /// Walk the directory tree (depth-limited), score each file, return scored entries.
    async fn scan_and_score(&self, root: &Path) -> Result<Vec<RelevanceEntry>> {
        let mut entries = Vec::new();

        // Iterative depth-first walk using a stack of (path, depth).
        let mut stack: Vec<(std::path::PathBuf, u32)> = vec![(root.to_path_buf(), 0)];

        while let Some((dir, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }

            let mut stream = tokio::fs::read_dir(&dir)
                .await
                .map_err(|e| HubError::Io(format!("scan directory {:?}: {}", dir, e)))?;

            while let Some(entry) = stream
                .next_entry()
                .await
                .map_err(|e| HubError::Io(format!("read dir entry from {:?}: {}", dir, e)))?
            {
                let path = entry.path();
                let fname_lower = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_lowercase())
                    .unwrap_or_default();

                // Skip hidden / noisy directories.
                if path.is_dir()
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| {
                            name.starts_with('.')
                                || name == "target"
                                || name == "node_modules"
                                || name == "vendor"
                                || name == "__pycache__"
                                || name == ".git"
                        })
                {
                    continue;
                }

                if path.is_dir() {
                    stack.push((path, depth + 1));
                } else {
                    // Score the file.
                    let score = self.score_file(&path, root, depth);
                    let category = self.category_file(&fname_lower, &path);

                    entries.push(RelevanceEntry {
                        path: path
                            .strip_prefix(root)
                            .ok()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.to_string_lossy().to_string()),
                        relevance_score: score,
                        category,
                    });
                }
            }
        }

        Ok(entries)
    }

    /// Score a single file based on priority map + depth decay.
    fn score_file(&self, path: &Path, root: &Path, depth: u32) -> f64 {
        let fname_lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        // Check all priority maps in order.
        let mut base_score = 0.0;
        for map_fn in [
            priority_config_files,
            priority_entry_files,
            priority_doc_files,
            priority_build_files,
        ] {
            if let Some(score) = map_fn().get(fname_lower.as_str()).map(|(s, _)| *s) {
                base_score = score;
                break;
            }
        }

        // Default base score for source files with known extensions.
        if base_score == 0.0 {
            let is_source = fname_lower.ends_with(".rs")
                || fname_lower.ends_with(".ts")
                || fname_lower.ends_with(".js")
                || fname_lower.ends_with(".py")
                || fname_lower.ends_with(".go")
                || fname_lower.ends_with(".java");
            if is_source {
                base_score = 0.5; // baseline for source files (survives depth decay)
            }
        }

        // Depth decay.
        let depth_decay = DEAY_PER_LEVEL.powi(depth as i32);
        let mut final_score = base_score * depth_decay;

        // Language-specific boosters (heuristic).
        if fname_lower.ends_with(".rs") && path.parent().is_some_and(|p| p == root.join("src")) {
            final_score = (final_score + 0.1).min(1.0);
        }
        if fname_lower.contains("test")
            || fname_lower.contains("_test")
            || fname_lower.ends_with("_spec")
        {
            final_score = (final_score + 0.15).min(1.0);
        }

        // Clamp.
        final_score.clamp(0.0, 1.0)
    }

    /// Determine the category of a file.
    fn category_file(&self, fname_lower: &str, _path: &Path) -> FileCategory {
        if fname_lower.contains("test")
            || fname_lower.contains("_test")
            || fname_lower.ends_with("_spec")
        {
            return FileCategory::Test;
        }

        for map_fn in [
            priority_config_files,
            priority_entry_files,
            priority_doc_files,
            priority_build_files,
        ] {
            if let Some((_, cat)) = map_fn().get(fname_lower) {
                return cat.clone();
            }
        }

        // Fall back to extension-based detection.
        if fname_lower.ends_with(".rs")
            || fname_lower.ends_with(".ts")
            || fname_lower.ends_with(".js")
        {
            FileCategory::Module
        } else {
            FileCategory::Unknown
        }
    }

    /// Extract patterns from a list of high-score files.
    async fn extract_patterns_from_entries(
        &self,
        root: &Path,
        entries: &[&RelevanceEntry],
    ) -> Vec<CodePattern> {
        let mut patterns = Vec::new();

        for entry in entries {
            let full_path = root.join(&entry.path).to_string_lossy().to_string();
            if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                let lines: Vec<&str> = content.lines().take(PATTERN_READ_LINES).collect();
                for (idx, line) in lines.iter().enumerate() {
                    let line_num = (idx + 1) as u32;
                    let trimmed = line.trim();

                    // Import patterns.
                    if let Some(m) = regex_import(trimmed) {
                        patterns.push(CodePattern {
                            file_path: full_path.clone(),
                            pattern_type: PatternType::Import(PathPattern(m.to_string())),
                            line_number: line_num,
                            significance_score: 0.6,
                        });
                        continue;
                    }

                    // Function signatures.
                    if let Some(m) = regex_fn(trimmed) {
                        patterns.push(CodePattern {
                            file_path: full_path.clone(),
                            pattern_type: PatternType::FunctionSignature(m.to_string()),
                            line_number: line_num,
                            significance_score: 0.7,
                        });
                        continue;
                    }

                    // Struct definitions.
                    if let Some(m) = regex_struct(trimmed) {
                        patterns.push(CodePattern {
                            file_path: full_path.clone(),
                            pattern_type: PatternType::StructDefinition {
                                name: m.to_string(),
                                fields_count: count_fields(trimmed),
                            },
                            line_number: line_num,
                            significance_score: 0.8,
                        });
                        continue;
                    }

                    // Trait definitions.
                    if let Some(m) = regex_trait(trimmed) {
                        patterns.push(CodePattern {
                            file_path: full_path.clone(),
                            pattern_type: PatternType::TraitDefinition {
                                name: m.to_string(),
                                methods: count_trait_methods(&content),
                            },
                            line_number: line_num,
                            significance_score: 0.85,
                        });
                    }
                }
            } else {
                debug!("Could not read file for pattern extraction: {}", entry.path);
            }
        }

        patterns
    }
}

// ─────────────────────────────────────────────
// Regex helpers (inline, no precompiled regex needed)
// ─────────────────────────────────────────────

/// Match import-like lines: `use path::to::item`, `import ... from '...'`, etc.
fn regex_import(line: &str) -> Option<&str> {
    if line.starts_with("use ") || line.starts_with("extern crate ") {
        return Some(line);
    }
    if line.starts_with("import ") && line.contains("from") {
        return Some(line);
    }
    if line.starts_with("#[macro_use]")
        || line.starts_with("#[path = \"")
        || line.starts_with("include!(\"")
    {
        return Some(line);
    }
    None
}

/// Match function signatures: `pub fn name(...)` or `fn name(...)`.
fn regex_fn(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if (trimmed.starts_with("pub fn ") || trimmed.starts_with("fn "))
        && !trimmed.starts_with("pub fn #[")
    // skip attribute on fn
    {
        return Some(trimmed);
    }
    None
}

/// Match struct definitions: `pub struct Name` or `struct Name`.
fn regex_struct(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
        return Some(trimmed);
    }
    None
}

/// Match trait definitions: `pub trait Name` or `impl ... for Type`.
fn regex_trait(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
        return Some(trimmed);
    }
    None
}

/// Count struct fields from a single-line representation.
fn count_fields(line: &str) -> usize {
    // Heuristic: count `{ ... }` contents or `where` blocks; simple comma counting for flat structs.
    line.split_once('{')
        .and_then(|(_, body)| body.find('}').map(|end| (body, end)))
        .map(|(body, end)| body[..end].matches(',').count() + 1)
        .unwrap_or_else(|| line.matches(',').count() + 1)
}

/// Approximate trait method count from content (for significance scoring).
fn count_trait_methods(content: &str) -> usize {
    content
        .lines()
        .filter(|l| l.trim().starts_with("fn "))
        .count()
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a temporary project directory with the given files.
    fn setup_project(dir: &TempDir, files: &[(&str, &str)]) {
        for (path, content) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full, content.as_bytes()).unwrap();
        }
    }

    #[tokio::test]
    async fn test_relevant_files_rust_project() {
        let tmp = TempDir::new().unwrap();
        setup_project(
            &tmp,
            &[
                (
                    "Cargo.toml",
                    r#"[package]\nname = \"my-app\"\n\n[dependencies]\naxum = \"0.8\""#,
                ),
                ("README.md", "# My App\n\nA sample project."),
                (
                    "src/main.rs",
                    r#"use axum::Router;

pub fn main() {
    println!("hello");
}

pub struct AppState {
    db: String,
}
"#,
                ),
            ],
        );

        let gatherer = SmartContextGatherer;
        let entries = gatherer.collect_relevant_files(tmp.path()).await;

        // Cargo.toml should have the highest score.
        let cargo_entry = entries.iter().find(|e| e.path == "Cargo.toml");
        assert!(cargo_entry.is_some());
        assert!(cargo_entry.unwrap().relevance_score >= 0.9);

        // README should be high.
        let readme_entry = entries.iter().find(|e| e.path == "README.md");
        assert!(readme_entry.is_some());
        assert!(readme_entry.unwrap().relevance_score >= 0.7);

        // src/main.rs should be present with category SourceEntry or Module.
        let main_entry = entries.iter().find(|e| e.path == "src/main.rs");
        assert!(main_entry.is_some());
    }

    #[tokio::test]
    async fn test_relevant_files_js_project() {
        let tmp = TempDir::new().unwrap();
        setup_project(
            &tmp,
            &[
                (
                    "package.json",
                    r#"{"name": "my-app", "dependencies": {"react": "^18.0.0"}}"#,
                ),
                ("README.md", "# My App"),
                (
                    "src/index.ts",
                    r#"import React from 'react';

export function App() {
    return null;
}
"#,
                ),
            ],
        );

        let gatherer = SmartContextGatherer;
        let entries = gatherer.collect_relevant_files(tmp.path()).await;

        assert!(entries.iter().any(|e| e.path == "package.json"));
        assert!(entries.iter().any(|e| e.path == "README.md"));
    }

    #[tokio::test]
    async fn test_depth_decay() {
        let tmp = TempDir::new().unwrap();
        setup_project(
            &tmp,
            &[
                ("src/main.rs", "fn main() {}"),
                ("src/lib/deep.rs", "fn deep() {}"),
            ],
        );

        let gatherer = SmartContextGatherer;
        let entries = gatherer.collect_relevant_files(tmp.path()).await;

        let src_main = entries.iter().find(|e| e.path == "src/main.rs");
        let src_deep = entries.iter().find(|e| e.path == "src/lib/deep.rs");

        // root-level file should have higher score than deeper one.
        assert!(src_main.is_some());
        assert!(src_deep.is_some());
        assert!(src_main.unwrap().relevance_score > src_deep.unwrap().relevance_score);
    }

    #[tokio::test]
    async fn test_extract_patterns_functions() {
        let tmp = TempDir::new().unwrap();
        setup_project(
            &tmp,
            &[(
                "src/lib.rs",
                r#"pub struct Config {
    name: String,
}

pub trait AppService {
    fn init(&self);
    fn run(&self);
}

pub fn main() {}

fn helper() {}
"#,
            )],
        );

        let gatherer = SmartContextGatherer;
        let patterns = gatherer.extract_patterns(tmp.path()).await;

        // We should see at least one function signature.
        let fn_patterns: Vec<&CodePattern> = patterns
            .iter()
            .filter(|p| matches!(p.pattern_type, PatternType::FunctionSignature(_)))
            .collect();
        assert!(!fn_patterns.is_empty(), "Expected function patterns");

        // Collect the signatures.
        let sigs: Vec<&str> = fn_patterns
            .iter()
            .filter_map(|p| {
                if let PatternType::FunctionSignature(s) = &p.pattern_type {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();

        assert!(sigs.iter().any(|s| s.contains("pub fn main")));
    }

    #[tokio::test]
    async fn test_extract_patterns_imports() {
        let tmp = TempDir::new().unwrap();
        setup_project(
            &tmp,
            &[(
                "src/main.rs",
                r#"use std::collections::HashMap;
extern crate serde_json;
import React from 'react';

pub fn main() {}
"#,
            )],
        );

        let gatherer = SmartContextGatherer;
        let patterns = gatherer.extract_patterns(tmp.path()).await;

        let import_patterns: Vec<&CodePattern> = patterns
            .iter()
            .filter(|p| matches!(p.pattern_type, PatternType::Import(_)))
            .collect();

        assert!(!import_patterns.is_empty(), "Expected import patterns");

        let import_strings: Vec<&str> = import_patterns
            .iter()
            .filter_map(|p| {
                if let PatternType::Import(PathPattern(p)) = &p.pattern_type {
                    Some(p.as_str())
                } else {
                    None
                }
            })
            .collect();

        assert!(
            import_strings
                .iter()
                .any(|s| s.contains("std::collections"))
        );
    }

    #[tokio::test]
    async fn test_gather_smart_full_flow() {
        let tmp = TempDir::new().unwrap();
        setup_project(
            &tmp,
            &[
                ("Cargo.toml", r#"[package]\nname = \"app\""#),
                ("README.md", "# App"),
                (
                    "src/main.rs",
                    r#"use std::io;

pub fn main() {
    println!("hello");
}
"#,
                ),
            ],
        );

        let gatherer = SmartContextGatherer;
        let ctx = gatherer.gather_smart(tmp.path()).await.unwrap();

        // Base metadata should be populated.
        assert!(!ctx.language.is_empty());
        assert!(ctx.language == "rust" || ctx.language == "unknown");

        // Should have relevant files (threshold = 0.3).
        assert!(!ctx.relevant_files.is_empty(), "Expected relevant files");

        // Total scanned should reflect base context file count.
        assert!(ctx.total_files_scanned > 0);

        // Should have at least one pattern (the fn main).
        let has_fn = ctx
            .code_patterns
            .iter()
            .any(|p| matches!(p.pattern_type, PatternType::FunctionSignature(_)));
        assert!(has_fn, "Expected function patterns in SmartContext");
    }

    #[tokio::test]
    async fn test_empty_dir_relevance() {
        let tmp = TempDir::new().unwrap();

        let gatherer = SmartContextGatherer;
        let entries = gatherer.collect_relevant_files(tmp.path()).await;

        // Empty directory should produce no relevant files above threshold.
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_pattern_extraction_no_match() {
        let tmp = TempDir::new().unwrap();
        setup_project(
            &tmp,
            &[(
                "notes.txt",
                "# Just some notes\nNo patterns here.\nPlain text only.",
            )],
        );

        let gatherer = SmartContextGatherer;
        let patterns = gatherer.extract_patterns(tmp.path()).await;

        // Plain text file should produce no import/struct/trait patterns.
        assert!(
            patterns.is_empty(),
            "Expected no patterns from plain text, got {:?}",
            patterns
        );
    }

    #[test]
    fn test_category_file_detects_rust_source() {
        let gatherer = SmartContextGatherer;
        // Use a non-priority .rs file to test the extension-based fallback.
        assert_eq!(
            gatherer.category_file("foo.rs", Path::new("/tmp/foo.rs")),
            FileCategory::Module
        );
        assert_eq!(
            gatherer.category_file("readme.md", Path::new("/tmp/readme.md")),
            priority_doc_files()["readme.md"].1.clone()
        );
    }

    #[test]
    fn test_score_known_priority_file() {
        let gatherer = SmartContextGatherer;
        let path = Path::new("/some/Cargo.toml");

        // Root-level file gets full score.
        let score = gatherer.score_file(path, Path::new("/some"), 0);
        assert!((score - 0.95).abs() < f64::EPSILON);

        // One level deep gets decayed score.
        let path2 = Path::new("/some/sub/Cargo.toml");
        let score2 = gatherer.score_file(path2, Path::new("/some"), 1);
        assert!(score2 < score);
    }
}
