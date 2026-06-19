# Cycle 77: gather — Smart Context Gathering

## Overview

Replace the dumb `context_gatherer.rs` (flat filesystem walk, no relevance scoring) with a **priority-ranked smart context extractor** that produces `SmartContext` entries with per-file relevance scores and extracted code patterns for prompt engineering workflows.

---

## Data Structures (models.rs — feature-gated or in gather.rs?)

**Decision: New types go in `models.rs` under `#[cfg(feature = "gather")]`** to follow the existing pattern (`Sandbox`, `VoiceInteraction`, etc.) and be available to hub.rs, tests, and any future consumers.

```rust
// In models.rs, gated behind #[cfg(feature = "gather")]

#[derive(Debug, Clone)]
pub struct SmartContext {
    pub project_path: String,
    pub language: String,
    pub framework: String,
    /// Priority-ranked files per language/framework (high relevance first)
    pub relevant_files: Vec<RelevanceEntry>,
    /// Extracted patterns from key files
    pub code_patterns: Vec<CodePattern>,
    /// Summary stats
    pub total_files_scanned: usize,
    pub relevance_threshold: f64,
}

#[derive(Debug, Clone)]
pub struct RelevanceEntry {
    pub path: String,
    pub relevance_score: f64,       // 0.0 .. 1.0
    pub category: FileCategory,     // config / source / test / doc / etc.
    pub language_hint: String,
}

#[derive(Debug, Clone)]
pub enum FileCategory {
    Config,        // Cargo.toml, package.json, tsconfig.json, Makefile...
    SourceEntry,   // main.rs, lib.rs, index.ts, app.py...
    Module,        // src/**/*.rs, lib/**/*.js, app/**/*.py...
    Test,          // tests/, *_test.*, *spec.*...
    Documentation, // README*, CONTRIBUTING*, docs/...
    BuildSystem,   // Makefile, CMakeLists.txt, justfile, Dockerfile...
    Unknown,
}

#[derive(Debug, Clone)]
pub enum PatternType {
    Import(PathPattern),    // import/extern/use/fn from ...
    FunctionSignature(String), // pub fn name(…) -> …
    StructDefinition {
        name: String,
        fields_count: usize,
    },
    TraitDefinition {
        name: String,
        methods: usize,
    },
    CustomPattern {
        label: String,
        snippet: String,
    },
}

#[derive(Debug, Clone)]
pub struct CodePattern {
    pub file_path: String,
    pub pattern_type: PatternType,
    pub line_number: u32,
    /// Confidence that this pattern is architecturally significant (0..1)
    pub significance_score: f64,
}

#[derive(Debug, Clone)]
pub struct PathPattern(pub String);
```

---

## File List & Responsibilities

### 1. `prompt-hub/src/gather.rs` (~250-300 lines) — The new module

Feature-gated with `#[cfg(feature = "gather")]`. Contains the `SmartContextGatherer` struct and all gathering logic.

```rust
#[derive(Debug, Clone, Default)]
pub struct SmartContextGatherer;

impl SmartContextGatherer {
    /// Priority-ranked files per language/framework.
    pub async fn collect_relevant_files(
        &self,
        project_path: &Path,
    ) -> Vec<RelevanceEntry>;

    /// Extract imports, function signatures, struct/def patterns from key files.
    pub async fn extract_patterns(
        &self,
        project_path: &Path,
    ) -> Vec<CodePattern>;

    /// Full smart gather — coherence layer that ties both together.
    pub async fn gather_smart(
        &self,
        project_path: &Path,
    ) -> Result<SmartContext>;
}
```

**Key implementation details:**

- **Relevance scoring algorithm** (no external deps, stdlib only):
  - Start with a **priority score** per filename (known patterns: `Cargo.toml=0.95`, `main.rs=0.9`, `README.md=0.7`, etc.)
  - Apply **directory proximity multiplier**: files at root get 1.2x, first-level subdirs get 1.0x, deeper gets diminishing returns (0.8x per level)
  - Apply **language-specific boosters**: `.rs` in Rust projects → +0.1 to source files; `*.test.*` → +0.15 for tests
  - Apply **cross-reference bonus**: if a file is referenced by another high-score file (e.g., `Cargo.toml` lists a crate that maps to `src/`) → +0.2
  - Final score clamped to `[0.0, 1.0]`, thresholded at `0.3`

- **File scanning**: Use the same `tokio::fs::read_dir` walk as `context_gatherer.rs` but recursively (depth-limited to 5 levels). Skip `.git/`, `.prompthub/`, `target/`, `node_modules/`, `vendor/`, `__pycache__/`.

- **Pattern extraction** (regex-based, no AST parsing — stays dep-free):
  - Import patterns: regex on `use `, `extern crate `, `import `, `from ... import `
  - Function signatures: regex on `pub fn `, `fn pub `, `def `, `func `
  - Struct definitions: regex on `pub struct `, `struct `
  - Trait/Interface definitions: regex on `pub trait `, `impl.*for `, `interface `

- **SmartContext assembly**: combine relevant_files + code_patterns with stats. Threshold default is `0.3` (configurable).

### 2. `prompt-hub/src/models.rs` — New types (~80 lines)

Under existing `#[cfg(feature = "gather")]` block at top of file:

```rust
#[cfg(feature = "gather")]
pub use crate::gather::*; // or re-export individual types
```

Actually, following the pattern in hub.rs for sandbox/voice/multimodal — **types go in models.rs**, gated behind `#[cfg(feature = "gather")]`. The module itself (`smart_context_gatherer`) may or may not be separate. Decision: **one file** `prompt-hub/src/gather.rs` that contains BOTH the types AND the gatherer implementation, exported under one module name for simplicity. This follows the `chaos.rs` pattern (types + impl in one file).

### 3. `prompt-hub/src/lib.rs` — Module declaration (~2 lines)

```rust
#[cfg(feature = "gather")]
pub mod gather;
```

Plus re-export types:
```rust
#[cfg(feature = "gather")]
pub use gather::{RelevanceEntry, CodePattern, SmartContext, FileCategory, PatternType};
```

### 4. `prompt-hub/src/hub.rs` — Hub wiring (~80 lines)

Add in feature-gated imports section:
```rust
#[cfg(feature = "gather")]
use crate::gather::{SmartContextGatherer, SmartContext, RelevanceEntry, CodePattern};
```

Add field to `PromptHub` struct (feature-gated):
```rust
#[cfg(feature = "gather")]
smart_gatherer: Arc<SmartContextGatherer>,
```

Add to the `new()` constructor (feature-gated):
```rust
#[cfg(feature = "gather")]
smart_gatherer: Arc::new(SmartContextGatherer),
```

Add hub methods (feature-gated, ~30 lines total):
```rust
/// Gather smart context for a project path.
#[cfg(feature = "gather")]
pub async fn gather_context_smart(
    &self,
    project_path: &Path,
) -> Result<SmartContext> {
    let gatherer = self.smart_gatherer.clone();
    gatherer.gather_smart(project_path).await
}

/// Collect only the relevance-ranked file list.
#[cfg(feature = "gather")]
pub async fn collect_relevant_files(
    &self,
    project_path: &Path,
) -> Result<Vec<RelevanceEntry>> {
    let gatherer = self.smart_gatherer.clone();
    Ok(gatherer.collect_relevant_files(project_path).await)
}

/// Extract code patterns from a project.
#[cfg(feature = "gather")]
pub async fn extract_patterns(
    &self,
    project_path: &Path,
) -> Result<Vec<CodePattern>> {
    let gatherer = self.smart_gatherer.clone();
    Ok(gatherer.extract_patterns(project_path).await)
}
```

### 5. `prompt-hub/src/gather.rs` — Tests (~100 lines)

In the same file, under `#[cfg(test)]`:

- `test_relevance_scoring_known_files` — verify known filenames get high scores
- `test_relevance_depth_decay` — verify deeper files get lower scores
- `test_relevance_threshold_filtering` — verify threshold filters out low-scoring files
- `test_pattern_extraction_imports` — verify import pattern extraction on a sample file
- `test_pattern_extraction_structs` — verify struct detection
- `test_gather_smart_full_flow` — integration: given a test dir, produce valid SmartContext

Test strategy: create temp dirs with synthetic project structures using `tempfile::tempdir()` (existing crate, already in workspace deps? **Check**). If not available, use `std::env::temp_dir()` + manual cleanup.

**No new dependencies decision**: The gather feature uses only `tokio::fs`, `regex`, `serde`, and standard library. All are already workspace deps or stdlib.

---

## Implementation Approach

### Step 1: Cargo.toml
Add `gather = []` to the "real modules" section (Category C) alongside `chaos`, `quota`, etc.

### Step 2: Models.rs
Add new types under existing feature gate block. ~80 lines.

### Step 3: Gather module
Create `prompt-hub/src/gather.rs`. ~250-300 lines. Contains:
- All data type definitions (as fallback if models.rs is avoided)
- Priority scoring constants map
- File scanning + relevance scoring algorithm
- Pattern extraction via regex
- SmartContext assembly

### Step 4: lib.rs wiring
Module decl + re-export. ~5 lines.

### Step 5: Hub.rs wiring
Import + field + constructor init + 3 pub async methods. ~80 lines.

### Step 6: Tests
In gather.rs under `#[cfg(test)]`. ~100 lines.

### Total estimated lines: ~520-560

---

## Rust-Native Conventions Check

- **No new deps** — uses `tokio::fs`, `regex` (already a workspace dep), stdlib only
- **`#![forbid(unsafe_code)]`** — no unsafe code needed
- **`Result<_, HubError>`** throughout
- **Native `async fn in trait`** — SmartContextGatherer uses async methods directly (Rust 2024)
- **Feature-gated everywhere** — module, types, hub field, hub methods all behind `#[cfg(feature = "gather")]`
- **Follows chaos.rs pattern** — single file for type + impl, no separate models.rs types needed if we keep them in gather.rs itself

**Final decision on types location**: Put ALL types in `gather.rs` to avoid models.rs clutter. The module is self-contained. Only the hub import adds a dependency. This follows the chaos.rs precedent more closely than sandbox/voice which put types in models.rs because they serve multiple consumers. Gather is a single-purpose feature — keep it contained.

---

## Files Changed Summary

| File | Lines | Action |
|------|-------|--------|
| `prompt-hub/Cargo.toml` | +2 | Add `gather = []` feature gate |
| `prompt-hub/src/gather.rs` | ~300 | New file — types, gatherer impl, tests |
| `prompt-hub/src/lib.rs` | +5 | Module decl + re-export |
| `prompt-hub/src/hub.rs` | ~80 | Import, field, constructor, 3 methods |
| **Total** | **~387** | |

---

## Verification Plan

1. `cargo check --workspace --all-features` — must compile with gather feature on AND off
2. `cargo clippy --workspace --all-targets --all-features -D warnings` — must be clean
3. `cargo test --workspace --all-features` — all existing + new tests pass
4. Manual verification: build without `gather` feature → ensure no dead code warnings, no missing type errors
