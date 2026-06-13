# Design: Accessibility Output Formatting (P1)

**Backlog Item**: `[tasks/FlexNetOS-NN]` — TBD  
**Priority**: P1  
**Feature Flag**: `accessibility = []`  
**No new deps required** — uses only stdlib + existing workspace deps (`serde_json`, `thiserror`)

---

## 1. Product Scope

Transform raw prompt output into WCAG-accessible formats. Not quality auditing (that already exists as `AccessibilityChecker` in `quality_gate.rs`). This is about **output rendering**: making the text that prompt_hub returns to users screen-reader friendly, dyslexia-friendly, and high-contrast accessible.

### What it covers
1. Content-type detection: identify lists, tables, code blocks in raw text
2. Plain-text normalization: strip excess whitespace, proper paragraph breaks
3. Structured JSON: convert detected content into explicit structure fields (`items`, `headings`, `code_blocks`)
4. Dyslexia-friendly format: word separators (unicode middot), increased line spacing via spacing chars, sentence splitting
5. High-contrast braille: map text to Unicode braille patterns (U+2800–U+28FF)
6. Multi-sensory output: combine structured JSON + plain text + braille in one response

### What it does NOT cover
- Quality auditing of prompts (already in `quality_gate.rs`)
- Runtime color/contrast theming (that's a TUI concern)
- New deps — pure stdlib + serde_json

---

## 2. Blast Radius

### Direct impacts
| File | Change Type | Risk |
|------|-------------|------|
| `prompt-hub/Cargo.toml` | Add `accessibility = []` to `[features]` | None (stub feature) |
| `prompthub/Cargo.toml` | Add `accessibility = ["prompt-hub/accessibility"]` pass-through | None (passive gate) |
| `prompt-hub/src/lib.rs` | Add `#[cfg(feature = "accessibility")] pub mod accessibility;` | None — cfg-gated |
| `prompt-hub/src/accessibility.rs` | **New file** ~600-800 lines | New module, zero callers yet |
| `prompt-hub/src/hub.rs` | Add `accessible_output()` impl method + field | Low — follows existing pattern |
| `prompthub/src/cli.rs` | Add `Accessibility { format: ... }` CLI subcommand (cfg-gated) | Low — optional command |
| `prompthub/src/commands/accessibility.rs` | **New file** ~150 lines | New module, cfg-gated |
| `prompthub/src/commands/mod.rs` | Add cfg-gated mod decl | None |
| `prompthub-server/src/routes.rs` | Add `/accessible` route (cfg-gated) | Low — thin wrapper over core |

### Caller analysis
- `PromptHub` struct: 107 fields already. One more `#[allow(dead_code)]` field inside `accessibility.rs` is fine (the types live there; no field on PromptHub needed since transform is pure function).
- No existing `accessible_output` method — zero caller risk.
- Quality gate's `AccessibilityChecker` (quality_gate.rs:54) shares the word "accessibility" but is a different concern — no name collision.

### Risk classification: LOW
- Zero callers on new API surface.
- New types confined to one module.
- Feature-gated at every layer (feature flag, cfg, CLI command, server route).
- No new dependencies.

---

## 3. Type Signatures

```rust
// ── prompt-hub/src/accessibility.rs ────────────────────────────

/// Output format selected by the caller for accessible rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessibilityFormat {
    /// Stripped plain text with proper paragraph structure.
    PlainText,
    /// Content restructured into explicit JSON (items, headings, code_blocks).
    StructuredJson,
    /// Dyslexia-friendly: word separators, line spacing, sentence splitting.
    DyslexiaFriendly,
    /// Unicode braille (U+2800–U+28FF) conversion of input text.
    HighContrastBraille,
}

/// Configuration for accessible output transformation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityConfig {
    /// Which format to produce. Defaults to `PlainText`.
    pub format: AccessibilityFormat,
    /// Max sentence length for dyslexia-friendly mode (0 = auto). Defaults to 0.
    pub max_sentence_length: usize,
    /// Whether to include all formats simultaneously (multi-sensory mode).
    /// Only meaningful when format is `StructuredJson`.
    pub multisensory: bool,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            format: AccessibilityFormat::PlainText,
            max_sentence_length: 0,
            multisensory: false,
        }
    }
}

/// Transforms raw content into an accessible output using the given config.
///
/// # Errors
/// Returns [`HubError::InvalidInput`] if the input is empty or contains
/// unencodable unicode sequences.
pub fn transform(
    content: &str,
    config: &AccessibilityConfig,
) -> Result<AccessibleOutput> {
    ...
}

/// The result of a transformation — typed by format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessibleOutput {
    /// Plain text output (PlainText or DyslexiaFriendly).
    Plain(String),
    /// Structured JSON with explicit semantic fields.
    Structured(serde_json::Value),
    /// Braille character string (U+2800–U+28FF range).
    Braille(String),
}

// ── Internal helpers (pub(crate) or private) ────────────────────

/// Detect content types in raw text and return a structured summary.
fn detect_content_types(content: &str) -> ContentTypes { ... }

#[derive(Debug, Clone, Default)]
struct ContentTypes {
    has_lists: bool,
    has_tables: bool,
    has_code_blocks: bool,
    list_items: Vec<String>,
    table_rows: Vec<Vec<String>>,
    code_blocks: Vec<(Option<String>, String)>,  // (lang, body)
}

/// Convert text to Unicode braille patterns.
fn to_braille(text: &str) -> Result<String> { ... }

/// Convert text to dyslexia-friendly format.
fn to_dyslexia_friendly(content: &str, max_sentence_len: usize) -> Result<String> { ... }

/// Restructure content into an explicit JSON structure.
fn to_structured_json(content: &str) -> serde_json::Value { ... }
```

---

## 4. Implementation Details per Format

### 4.1 PlainText
- Strip multiple consecutive whitespace runs to single space (`regex r"\s+" => " "`).
- Split on blank lines into paragraphs; trim each paragraph.
- Preserve leading indentation for code-like blocks (heuristic: if >60% of lines start with spaces/tabs).
- Remove trailing whitespace from every line.

### 4.2 StructuredJson
Content-type detection heuristics:
- **Lists**: Lines matching `^\s*[-*]\s+` or `^\s*\d+\.\s+` are list items. Preserve order in `{"type":"list","items":[...]}`.
- **Tables**: Consecutive lines with `|` delimiters (Markdown table syntax) → `{"type":"table","headers":[...],"rows":[[...]]}`.
- **Code blocks**: Text between fenced ``` markers or indented >3 spaces for 3+ consecutive lines → `{"type":"code","language":"...", "content":"..."}`.
- **Headings**: Lines matching `^#{1,6}\s+` → `{"type":"heading","level":N,"text":"..."}`.
- Everything else goes into a `{"type":"paragraph","text":"..."}` section.

Output structure (top-level JSON):
```json
{
  "sections": [
    {"type": "paragraph", "text": "..."},
    {"type": "list", "items": ["...", "..."]},
    {"type": "code", "language": "rust", "content": "..."},
    ...
  ],
  "metadata": {
    "section_count": 3,
    "has_lists": true,
    "has_tables": false,
    "has_code": true
  }
}
```

### 4.3 DyslexiaFriendly
- Insert unicode middots (`\u{00B7}`) between words (detected by whitespace).
- Replace single spaces with `\u{2005}\u{2005}\u{2005}` (three-em-space) for increased line height between paragraphs.
- Detect sentence boundaries: `\. ` or `\n\n` → insert soft paragraph breaks (`\u{2029}`).
- Split sentences longer than `max_sentence_length` (default 40 chars) at conjunctions/commas if possible, otherwise at nearest word boundary. Append soft break (`\u{00AD}`).
- Replace common ambiguity patterns: `"` → `\u{201C}` and `\u{201D}` (smart quotes), `'` used as apostrophe preserved.

### 4.4 HighContrastBraille
Standard Unicode braille mapping (8-dot):

```
braille_char = U+2800 | char_code   for lowercase ASCII letters
braille_char = U+2800 | (char_code - 64)  for uppercase ASCII letters
```

Specifically:
- Space → `\u{2800}` (blank cell, all dots off)
- `a-z` (U+0061–U+007A) → `\u{2861}..`\u{287A}` (dots 1,2,3,4,5 + lower index offset)
  - In code: `(c as u32 - b'a' + 1) | 0x2800` — maps `a→\u{2801}`, `b→\u{2802}`... (simple position mapping)
- `A-Z` (U+0041–U+005A) → same but via upper: `(c as u32 - b'A' + 1) | 0x2800` — actually this produces the SAME braille char since lowercase and uppercase map to the same braille cell in standard braille (case is implicit). For accessibility distinction, preserve case by adding dot-7 (`\u{2840}` = U+2800 | 0x40) for uppercase.
  - `A` → `\u{2841}`, `B` → `\u{2842}` ... (with dot-7 marker)
  - `a` → `\u{2801}`, `b` → `\u{2802}` ... (without dot-7)
- Numbers `0-9` → U+2831–U+2839 (standard braille numbers, dots 3-4-5-6-8 set + letter position)
  - In code: `(c as u32 - b'0' + 1) | 0x2830`
- Punctuation mapped to standard braille equivalents where available. Unrecognized ASCII → replace with `\u{283F}` (braille pattern dots-123568 = "unknown") or render as raw char in a comment section.
- Newlines → `\u{2800}\n` (blank cell + actual newline for readability).

This approach uses the simplest correct mapping: character position in ASCII alphabet → braille dot pattern, with case distinction via dot-7 marker. No external library needed.

---

## 5. Hub Integration — `accessible_output()` method

```rust
// In prompt-hub/src/hub.rs (inside impl PromptHub)

/// Transform prompt output into an accessible format.
///
/// This is a pure transformation with no storage or auth side effects. It
/// reads the raw content and produces formatted output suitable for screen
/// readers, dyslexia-friendly rendering, or braille display.
#[cfg(feature = "accessibility")]
pub async fn accessible_output(
    &self,
    prompt_id: Uuid,
    config: accessibility::AccessibilityConfig,
) -> Result<accessibility::AccessibleOutput> {
    use crate::accessibility;

    // Fetch the prompt content first (reuses existing storage path).
    let prompt = self.storage.get_prompt_by_id(prompt_id).await?
        .ok_or_else(|| HubError::NotFound(format!("prompt {}", prompt_id)))?;

    accessibility::transform(&prompt.content, &config)
}

// Multi-sensory variant: returns all formats simultaneously.
#[cfg(feature = "accessibility")]
pub async fn accessible_output_all(
    &self,
    prompt_id: Uuid,
) -> Result<AccessibleMultiOutput> {
    use crate::accessibility;

    let prompt = self.storage.get_prompt_by_id(prompt_id).await?
        .ok_or_else(|| HubError::NotFound(format!("prompt {}", prompt_id)))?;

    let plain = accessibility::transform(&prompt.content, &AccessibilityConfig { format: AccessibilityFormat::PlainText, ..Default::default() })?;
    let structured = accessibility::transform(&prompt.content, &AccessibilityConfig { format: AccessibilityFormat::StructuredJson, multisensory: true, ..Default::default() })?;
    let braille = accessibility::transform(&prompt.content, &AccessibilityConfig { format: AccessibilityFormat::HighContrastBraille, ..Default::default() })?;

    Ok(AccessibleMultiOutput { plain, structured, braille })
}
```

**Note**: `AccessibleMultiOutput` is a new enum in the same module:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibleMultiOutput {
    pub plain: accessibility::AccessibleOutput,
    pub structured: accessibility::AccessibleOutput,
    pub braille: accessibility::AccessibleOutput,
}
```

---

## 6. CLI Integration

```rust
// In prompthub/src/cli.rs — cfg-gated addition:

/// Format for accessible output
#[derive(Subcommand, Debug, Clone)]
pub enum AccessibilitySubCommand {
    /// Transform prompt output to plain accessible text
    Plain { id: Uuid },
    /// Transform to structured JSON with semantic sections
    Json { id: Uuid },
    /// Transform to dyslexia-friendly format
    Dyslexic {
        id: Uuid,
        #[arg(long, default_value_t = 40)]
        max_sentence_length: usize,
    },
    /// Convert to high-contrast braille
    Braille { id: Uuid },
    /// Get all formats simultaneously (multi-sensory)
    All { id: Uuid },
}

// Add to Commands enum:
/// Accessible output formatting (WCAG-compliant)
#[cfg(feature = "accessibility")]
Accessible {
    #[command(subcommand)]
    subcommand: AccessibilitySubCommand,
},
```

CLI handler in `main.rs` match arm:
```rust
#[cfg(feature = "accessibility")]
Commands::Accessible { subcommand } => {
    use prompt_hub::accessibility::{AccessibilityConfig, AccessibilityFormat};
    let config = match subcommand { ... };
    let hub = PromptHub::new(...).await?;
    let output = hub.accessible_output(prompt_id, config).await?;
    println!("{}", format_output(&output));
}
```

---

## 7. Server Route Integration

Add to `prompthub-server/src/routes.rs` (cfg-gated):

```rust
// POST /api/v1/prompt/{id}/accessible
// Query param: format = plain|json|dyslexic|braille|all
// Body (optional): max_sentence_length for dyslexic mode
#[cfg(feature = "accessibility")]
pub async fn accessible_output(
    State(state): State<AppState>,
    Extension(identity): Extension<AgentIdentity>,
    Path(prompt_id): Path<Uuid>,
    Query(params): Query<AccessibleQueryParams>,
) -> impl IntoResponse { ... }
```

Response format matches the core types (JSON-serializable `AccessibleOutput` or `AccessibleMultiOutput`).

---

## 8. Test Strategy — 6-8 tests + hub integration

### Unit tests in `accessibility.rs` (`#[cfg(test)] mod tests`)

| # | Test name | Format | What it verifies |
|---|-----------|--------|------------------|
| 1 | `test_plain_text_normalizes_whitespace` | PlainText | Multiple spaces/newlines → single space, paragraphs preserved |
| 2 | `test_plain_text_preserves_code_blocks` | PlainText | Indented code blocks retain structure |
| 3 | `test_structured_json_detects_lists` | StructuredJson | Bullet and numbered lists detected as `items` array |
| 4 | `test_structured_json_detects_code` | StructuredJson | Fenced code blocks captured with language hint |
| 5 | `test_dyslexia_friendly_adds_separators` | DyslexiaFriendly | Word separators (middot) inserted, max sentence length respected |
| 6 | `test_braille_maps_letters_correctly` | HighContrastBraille | `a→\u{2801}`, `b→\u{2802}`, `A→\u{2841}` (case-distinct) |
| 7 | `test_braille_handles_numbers_punctuation` | HighContrastBraille | Numbers map to braille number pattern, punctuation handled |
| 8 | `test_multisensory_output_includes_all_formats` | StructuredJson (multisensory=true) | Output JSON includes plain_text, structured sections, and braille fields |

### Integration test in `prompt-hub/tests/test_accessibility.rs` (`#[cfg(feature = "accessibility")]`)

| # | Test name | What it verifies |
|---|-----------|------------------|
| 9 | `test_hub_accessible_output()` | Creates PromptHub, registers a prompt with mixed content (list + code), calls `hub.accessible_output()`, verifies output structure |
| 10 | `test_hub_accessible_output_all()` | Same setup, calls `hub.accessible_output_all()`, verifies all three outputs present and non-empty |

Total: **10 tests** (8 unit + 2 integration).

---

## 9. File Edit Order (leaf-first)

1. `prompt-hub/src/accessibility.rs` — **new file**, all types + impl
2. `prompt-hub/Cargo.toml` — add `accessibility = []` to features (around line 68)
3. `prompthub/Cargo.toml` — add `accessibility = ["prompt-hub/accessibility"]` pass-through (around line 50)
4. `prompt-hub/src/lib.rs` — add cfg-gated module decl + re-export (after existing module list)
5. `prompt-hub/src/hub.rs` — add `accessible_output()` and `accessible_output_all()` methods
6. `prompthub/src/cli.rs` — add `AccessibilitySubCommand` enum + Commands variant (cfg-gated)
7. `prompthub/src/commands/mod.rs` — add cfg-gated mod decl for `accessibility`
8. `prompthub/src/commands/accessibility.rs` — **new file**, CLI handler impl
9. `prompthub/src/main.rs` — add match arm for new Commands variant
10. `prompthub-server/src/routes.rs` — add cfg-gated route (thin wrapper)
11. `prompthub-server/src/main.rs` or routes module import if needed

---

## 10. Verification Gates

Both configurations must pass:
```bash
# Default build (accessibility gated OUT)
cargo check --workspace
cargo clippy --workspace -- -D warnings

# Full matrix (accessibility gated IN)
just test    # cargo test --workspace --all-features
just lint    # cargo clippy --workspace --all-features -- -D warnings
just fmt     # then: git diff --quiet
```

**Key concern**: The default build compiles `hub.rs` which has `#[allow(dead_code)]`. Adding `accessible_output()` behind `#[cfg(feature = "accessibility")]` means it is only compiled during `--all-features`. This is the existing pattern for features like `chaos`, `quota`, etc. — verify that `PromptHub::new()` doesn't have any field wiring that requires `accessibility`. Since this feature has no new fields on PromptHub (pure static function in a new module), both configs should compile clean.

---

## 11. Edge Cases & Error Handling

| Scenario | Handling |
|----------|----------|
| Empty input content | Return `HubError::InvalidInput("content is empty")` |
| Content > 10MB | Return `HubError::InvalidInput("content exceeds 10MB limit")` |
| Non-ASCII unicode (CJK, Arabic) | Braille mode: map via standard braille contractions where possible; fall back to raw chars with a `[UNMAPPED]` annotation in a sidecar note. Plain/Dyslexic/Json pass through unchanged. |
| Binary content (non-text) | Return `HubError::InvalidInput("content is not valid text")` — check via UTF-8 validity |
| JSON mode with invalid serde path | Always safe since we construct Value directly, never deserialize untrusted input |

---

## 12. Done Criteria

- [ ] All types defined in single file `prompt-hub/src/accessibility.rs`
- [ ] `accessibility = []` feature in both Cargo.toml files
- [ ] No new dependencies (stdlib + serde_json only)
- [ ] `transform()` returns `Result<AccessibleOutput>`
- [ ] 4 format variants implemented (PlainText, StructuredJson, DyslexiaFriendly, HighContrastBraille)
- [ ] Braille uses Unicode U+2800–U+28FF range with standard dot patterns
- [ ] Hub method `accessible_output()` wired on PromptHub
- [ ] CLI command `prompthub accessible <format>` added (cfg-gated)
- [ ] Server route `POST /api/v1/prompt/{id}/accessible` added (cfg-gated)
- [ ] 8 unit tests + 2 integration tests = 10 total
- [ ] `cargo check --workspace` green (default build, feature OFF)
- [ ] `just test` green (feature ON)
- [ ] `just lint` zero warnings
- [ ] `just fmt` clean
