#![forbid(unsafe_code)]

//! Accessibility output formatting — WCAG-compliant text transformations.
//!
//! Transforms raw prompt output into accessible formats: plain-text normalization,
//! structured JSON with content-type detection, dyslexia-friendly rendering with
//! unicode word separators, and high-contrast Unicode braille (U+2800–U+28FF).
//!
//! # Feature gate
//!
//! This module is compiled only when the `accessibility` feature flag is enabled:
//! ```toml
//! [features]
//! accessibility = []
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors specific to the accessibility module.
#[derive(Error, Debug, Clone)]
pub enum AccessibilityError {
    #[error("content is empty")]
    EmptyContent,

    #[error("content exceeds 10 MB limit ({} bytes)", 0)]
    ContentTooLarge(usize),

    #[error("input is not valid text: {0}")]
    InvalidText(String),

    #[error("unsupported format for multisensory mode")]
    UnsupportedFormat,

    #[error("JSON serialization failed: {0}")]
    JsonError(String),
}

// ---------------------------------------------------------------------------
// Format types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for accessible output transformation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityConfig {
    /// Which format to produce. Defaults to [`AccessibilityFormat::PlainText`].
    pub format: AccessibilityFormat,
    /// Max sentence length for dyslexia-friendly mode (0 = no splitting).
    /// Default is 40 characters for dyslexia-friendly mode.
    #[serde(default = "default_max_sentence_length")]
    pub max_sentence_length: usize,
    /// Whether to include all formats simultaneously (multi-sensory mode).
    /// Only meaningful when format is [`AccessibilityFormat::StructuredJson`].
    #[serde(default)]
    pub multisensory: bool,
}

fn default_max_sentence_length() -> usize {
    40
}

impl Default for AccessibilityConfig {
    /// Default config produces plain-text output.
    fn default() -> Self {
        Self {
            format: AccessibilityFormat::PlainText,
            max_sentence_length: default_max_sentence_length(),
            multisensory: false,
        }
    }
}

impl AccessibilityConfig {
    /// Create a config for plain-text output (stripped whitespace + paragraphs).
    pub fn plain() -> Self {
        Self {
            format: AccessibilityFormat::PlainText,
            ..Default::default()
        }
    }

    /// Create a config for structured JSON output with content-type detection.
    pub fn structured() -> Self {
        Self {
            format: AccessibilityFormat::StructuredJson,
            ..Default::default()
        }
    }

    /// Create a dyslexia-friendly config with middot word separators and sentence splitting.
    pub fn dyslexia_friendly() -> Self {
        Self {
            format: AccessibilityFormat::DyslexiaFriendly,
            max_sentence_length: 40,
            multisensory: false,
        }
    }

    /// Create a high-contrast braille config (Unicode U+2800 range).
    pub fn braille() -> Self {
        Self {
            format: AccessibilityFormat::HighContrastBraille,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

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

/// Multi-sensory output combining all accessible formats simultaneously.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibleMultiOutput {
    /// Plain text representation.
    pub plain: AccessibleOutput,
    /// Structured JSON with content-type metadata.
    pub structured: AccessibleOutput,
    /// Unicode braille representation.
    pub braille: AccessibleOutput,
}

// ---------------------------------------------------------------------------
// Public transform entry point
// ---------------------------------------------------------------------------

/// Transform raw content into an accessible output using the given config.
///
/// # Errors
/// Returns [`AccessibilityError`] if the input is empty or contains
/// unencodable sequences.
pub fn transform(
    content: &str,
    config: &AccessibilityConfig,
) -> Result<AccessibleOutput, AccessibilityError> {
    // Validate input
    if content.is_empty() {
        return Err(AccessibilityError::EmptyContent);
    }

    if content.len() > 10 * 1024 * 1024 {
        // 10 MB limit
        return Err(AccessibilityError::ContentTooLarge(content.len()));
    }

    match config.format {
        AccessibilityFormat::PlainText => plain_text(content),
        AccessibilityFormat::StructuredJson => structured_json(content, config.multisensory),
        AccessibilityFormat::DyslexiaFriendly => {
            dyslexia_friendly(content, config.max_sentence_length)
        }
        AccessibilityFormat::HighContrastBraille => high_contrast_braille(content),
    }
}

/// Transform raw content into all accessible formats simultaneously.
///
/// Useful when the display layer needs to provide multiple accessibility
/// options at once (e.g., screen reader + braille display).
pub fn transform_all(content: &str) -> Result<AccessibleMultiOutput, AccessibilityError> {
    let plain_cfg = AccessibilityConfig::plain();
    let json_cfg = AccessibilityConfig::structured();
    let braille_cfg = AccessibilityConfig::braille();

    let plain = transform(content, &plain_cfg)?;
    let structured = transform(content, &json_cfg)?;
    let braille = transform(content, &braille_cfg)?;

    Ok(AccessibleMultiOutput {
        plain,
        structured,
        braille,
    })
}

// ---------------------------------------------------------------------------
// Plain text transformer
// ---------------------------------------------------------------------------

/// Normalize raw text into clean plain text with proper paragraph structure.
fn plain_text(content: &str) -> Result<AccessibleOutput, AccessibilityError> {
    let mut result = String::with_capacity(content.len());
    let lines: Vec<&str> = content.lines().collect();
    let mut in_code_block = false;
    let mut code_lines: Vec<&str> = Vec::new();

    for line in &lines {
        // Detect fenced code blocks (lines starting with spaces/tabs — heuristic)
        if line.starts_with(' ') || line.starts_with('\t') {
            if !in_code_block {
                // Flush any accumulated text before code block
                if !code_lines.is_empty() {
                    result.push_str(&normalize_paragraph(&code_lines.join("\n")));
                    result.push('\n');
                    code_lines.clear();
                }
                in_code_block = true;
            }
            if in_code_block {
                code_lines.push(line);
            }
        } else {
            if in_code_block && !code_lines.is_empty() {
                for cl in &code_lines {
                    result.push_str(cl);
                    result.push('\n');
                }
                code_lines.clear();
                in_code_block = false;
            }

            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                // Blank line: paragraph separator — always push one \n.
                // The previous text line ends with \n, so two consecutive \n gives one blank line.
                result.push('\n');
            } else {
                // Normalize excess whitespace within the line, then strip trailing whitespace
                let normalized = normalize_paragraph(trimmed);
                if !normalized.is_empty() {
                    result.push_str(&trim_line(&normalized));
                    result.push('\n');
                }
            }
        }
    }

    // Flush any remaining code block lines
    if in_code_block && !code_lines.is_empty() {
        for cl in &code_lines {
            result.push_str(cl);
            result.push('\n');
        }
    }

    // Trim trailing whitespace from last line but keep paragraph separation
    let result = result.trim_end_matches('\n').to_string();

    Ok(AccessibleOutput::Plain(if result.is_empty() {
        "\n".to_string()
    } else {
        format!("{}\n", result)
    }))
}

/// Normalize a single paragraph: strip excess whitespace.
fn normalize_paragraph(paragraph: &str) -> String {
    let mut result = String::with_capacity(paragraph.len());
    let mut prev_space = false;
    for c in paragraph.chars() {
        if c.is_whitespace() {
            if !prev_space && !result.ends_with(char::is_whitespace) {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

/// Trim trailing whitespace from a single line.
fn trim_line(line: &str) -> String {
    line.trim_end().to_string()
}

// ---------------------------------------------------------------------------
// Structured JSON transformer
// ---------------------------------------------------------------------------

/// Content-type detection heuristics for markdown-like text.
#[derive(Debug, Clone, Default)]
struct ContentTypes {
    has_lists: bool,
    has_tables: bool,
    has_code_blocks: bool,
    list_items: Vec<String>,
    table_rows: Vec<Vec<String>>,
    code_blocks: Vec<(Option<String>, String)>, // (lang, body)
}

/// Convert text to structured JSON with semantic section detection.
fn structured_json(
    content: &str,
    multisensory: bool,
) -> Result<AccessibleOutput, AccessibilityError> {
    let _types = detect_content_types(content);

    let mut sections = Vec::new();

    // Process the content line by line for structured output
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Detect headings (lines starting with # followed by space)
        if let Some(level) = detect_heading(line) {
            let text = strip_heading_marker(line);
            sections.push(serde_json::json!({
                "type": "heading",
                "level": level,
                "text": text
            }));
            i += 1;
            continue;
        }

        // Detect fenced code blocks (``` markers)
        if line.trim().starts_with("```") {
            let lang = if line.trim().len() > 3 {
                Some(line.trim()[3..].to_string())
            } else {
                None
            };
            let mut code_body: Vec<&str> = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code_body.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                // Skip closing ```
                i += 1;
            }
            let body = code_body.join("\n");
            sections.push(serde_json::json!({
                "type": "code_block",
                "language": lang.unwrap_or_else(|| "text".to_string()),
                "content": body
            }));
            continue;
        }

        // Detect tables (lines containing | delimiters)
        if detect_table_line(line) {
            let mut table_rows = Vec::new();
            while i < lines.len() && detect_table_line(lines[i]) {
                let row: Vec<String> = lines[i]
                    .split('|')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_string())
                    .collect();
                if !row.is_empty() {
                    table_rows.push(row);
                }
                i += 1;
            }
            if table_rows.len() >= 2 {
                let (headers, data_rows) = table_rows.split_at(1);
                sections.push(serde_json::json!({
                    "type": "table",
                    "headers": headers[0],
                    "rows": data_rows
                        .iter()
                        .map(|r| serde_json::Value::Array(r.iter().cloned().map(serde_json::Value::String).collect()))
                        .collect::<Vec<_>>()
                }));
            } else if !table_rows.is_empty() {
                // Single row treated as list
                for row in &table_rows {
                    sections.push(serde_json::json!({
                        "type": "list",
                        "items": row.clone()
                    }));
                }
            }
            continue;
        }

        // Detect bullet lists (lines starting with -, *, or bullet char)
        if detect_bullet(line) {
            let mut items = Vec::new();
            while i < lines.len() && detect_bullet(lines[i]) {
                let item = strip_bullet_marker(lines[i]);
                items.push(item);
                i += 1;
            }
            sections.push(serde_json::json!({
                "type": "list",
                "items": items
            }));
            continue;
        }

        // Detect numbered lists
        if detect_numbered_list(lines[i]) {
            let mut items = Vec::new();
            while i < lines.len() && detect_numbered_list(lines[i]) {
                let item = strip_numbered_marker(lines[i]);
                items.push(item);
                i += 1;
            }
            sections.push(serde_json::json!({
                "type": "ordered_list",
                "items": items
            }));
            continue;
        }

        // Plain paragraph — accumulate consecutive lines
        let mut para_lines: Vec<&str> = Vec::new();
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && detect_heading(lines[i]).is_none()
            && !lines[i].trim().starts_with("```")
            && !detect_table_line(lines[i])
            && !detect_bullet(lines[i])
            && !detect_numbered_list(lines[i])
        {
            para_lines.push(lines[i].trim());
            i += 1;
        }

        if !para_lines.is_empty() {
            sections.push(serde_json::json!({
                "type": "paragraph",
                "text": para_lines.join(" ")
            }));
        }
    }

    // Build the final JSON structure
    let mut output = serde_json::Map::new();

    let section_count = sections.len();
    output.insert("sections".to_string(), serde_json::Value::Array(sections));

    // Add metadata
    let mut metadata = serde_json::Map::new();
    metadata.insert("section_count".to_string(), section_count.into());
    metadata.insert("has_lists".to_string(), _types.has_lists.into());
    metadata.insert("has_tables".to_string(), _types.has_tables.into());
    metadata.insert("has_code".to_string(), _types.has_code_blocks.into());
    output.insert("metadata".to_string(), serde_json::Value::Object(metadata));

    if multisensory {
        // Also add the plain text and braille versions
        let plain_text_out =
            plain_text(content).unwrap_or_else(|_| AccessibleOutput::Plain(String::new()));
        let braille_text_out = high_contrast_braille(content)
            .unwrap_or_else(|_| AccessibleOutput::Braille(String::new()));

        let pt_str = match plain_text_out {
            AccessibleOutput::Plain(s) => s,
            _ => String::new(),
        };
        let br_str = match braille_text_out {
            AccessibleOutput::Braille(s) => s,
            _ => String::new(),
        };

        output.insert("plain_text".to_string(), serde_json::Value::String(pt_str));
        output.insert("braille".to_string(), serde_json::Value::String(br_str));
    }

    Ok(AccessibleOutput::Structured(serde_json::Value::Object(
        output,
    )))
}

// ---------------------------------------------------------------------------
// Content-type detection helpers
// ---------------------------------------------------------------------------

/// Detect if a line is a heading (starts with 1-6 # followed by space).
fn detect_heading(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let mut level = 0;
    for c in trimmed.chars() {
        if c == '#' {
            level += 1;
            if level > 6 {
                return None;
            }
        } else {
            break;
        }
    }
    if level > 0 && trimmed.len() > level && trimmed.as_bytes()[level] == b' ' {
        Some(level)
    } else {
        None
    }
}

/// Strip the heading marker from a heading line.
fn strip_heading_marker(line: &str) -> String {
    let trimmed = line.trim_start();
    let mut level = 0;
    for c in trimmed.chars() {
        if c == '#' {
            level += 1;
        } else {
            break;
        }
    }
    // Skip the # chars and the following space
    if level > 0 && trimmed.len() > level + 1 {
        trimmed[level + 1..].to_string()
    } else {
        line.to_string()
    }
}

/// Detect if a line looks like a table row (contains | delimiters).
fn detect_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    // Must contain multiple | delimiters
    trimmed.contains('|') && trimmed.matches('|').count() >= 2
}

/// Detect if a line is a bullet list item.
fn detect_bullet(line: &str) -> bool {
    let trimmed = line.trim_start();
    let first = trimmed.as_bytes().first();
    matches!(first, Some(&b'-') | Some(&b'*'))
        && trimmed.len() > 1
        && trimmed.as_bytes()[1].is_ascii_whitespace()
}

/// Strip the bullet marker from a list item.
fn strip_bullet_marker(line: &str) -> String {
    let trimmed = line.trim_start();
    if (trimmed.starts_with('-') || trimmed.starts_with('*'))
        && trimmed.len() > 1
        && trimmed.as_bytes()[1].is_ascii_whitespace()
    {
        return trim_leading(trimmed, 2);
    }
    // Check for bullet character (• = U+2022)
    if trimmed.starts_with('\u{2022}') {
        return trim_leading(trimmed, 3);
    }
    line.to_string()
}

/// Detect if a line is a numbered list item.
fn detect_numbered_list(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.chars().take_while(|c| c.is_ascii_digit()).count() >= 1 && trimmed.contains('.')
}

/// Strip the numbered marker from a list item.
fn strip_numbered_marker(line: &str) -> String {
    let trimmed = line.trim_start();
    if let Some(dot_pos) = trimmed.find('.')
        && dot_pos > 0
        && dot_pos < trimmed.len()
        && (dot_pos + 1 >= trimmed.len() || trimmed.as_bytes()[dot_pos + 1].is_ascii_whitespace())
    {
        return trim_leading(trimmed, dot_pos + 2);
    }
    line.to_string()
}

/// Trim N characters from the start of a string.
fn trim_leading(s: &str, chars_to_skip: usize) -> String {
    s.char_indices()
        .nth(chars_to_skip)
        .map(|(idx, _)| s[idx..].trim_start().to_string())
        .unwrap_or_else(|| s.trim_start().to_string())
}

/// Detect content types in raw text and return a structured summary.
fn detect_content_types(content: &str) -> ContentTypes {
    let mut result = ContentTypes::default();
    let lines: Vec<&str> = content.lines().collect();

    for line in &lines {
        if detect_heading(line).is_some() {
            continue; // handled separately
        }

        if line.trim().starts_with("```") {
            result.has_code_blocks = true;
            continue;
        }

        if detect_table_line(line) {
            result.has_tables = true;
            let row: Vec<String> = line
                .split('|')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect();
            if !row.is_empty() {
                result.table_rows.push(row);
            }
            continue;
        }

        if detect_bullet(line) || detect_numbered_list(line) {
            result.has_lists = true;
            let item = if detect_bullet(line) {
                strip_bullet_marker(line)
            } else {
                strip_numbered_marker(line)
            };
            result.list_items.push(item);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Dyslexia-friendly transformer
// ---------------------------------------------------------------------------

/// Convert text to dyslexia-friendly format with unicode word separators.
fn dyslexia_friendly(
    content: &str,
    max_sentence_length: usize,
) -> Result<AccessibleOutput, AccessibilityError> {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = String::new();

    for (li, line) in lines.iter().enumerate() {
        if li > 0 {
            result.push('\n');
        }

        let words: Vec<&str> = split_words(line);
        if words.is_empty() {
            continue;
        }

        // Add middot separators between words
        for (wi, word) in words.iter().enumerate() {
            if wi > 0 {
                result.push('\u{00B7}'); // MIDDOT
            }
            result.push_str(word);
        }

        // Split long sentences if requested
        if max_sentence_length > 0 {
            let sentence_result = split_long_sentences(&result, max_sentence_length);
            result = sentence_result;
        }

        // Add em-space after periods to simulate line spacing
        result = replace_period_spacing(&result);

        // Replace straight quotes with smart quotes
        result = smart_quotes(&result);
    }

    Ok(AccessibleOutput::Plain(result))
}

/// Split a string into words by whitespace groups.
fn split_words(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

/// Split sentences longer than max_length at natural breaks (commas, "and", etc.).
fn split_long_sentences(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let mut current_chunk = String::new();

    for word in text.split('\u{00B7}') {
        let candidate = if current_chunk.is_empty() {
            word.to_string()
        } else {
            format!("{}\u{00AD}{}", current_chunk, word) // SOFT HYPHEN at break point
        };

        if candidate.len() > max_len && !current_chunk.is_empty() {
            // Flush current chunk and start new one
            let para_sep = '\u{2029}';
            if result.ends_with(para_sep) {
                // Replace last separator with the chunk
                if let Some(pos) = result.rfind(para_sep) {
                    result.truncate(pos);
                    current_chunk = String::new();
                }
            } else {
                result.push(para_sep);
                current_chunk = String::new();
            }
        }

        if !current_chunk.is_empty() {
            current_chunk.push('\u{00B7}');
        }
        current_chunk.push_str(word);
    }

    if !current_chunk.is_empty() {
        let para_sep = '\u{2029}';
        if result.ends_with(para_sep) {
            if let Some(pos) = result.rfind(para_sep) {
                result.truncate(pos);
            }
        } else {
            result.push(para_sep);
        }
        result.push_str(&current_chunk);
    }

    result
}

/// Replace period spacing with em-space for better readability.
fn replace_period_spacing(text: &str) -> String {
    text.replace(". ", ".\u{2005}\u{2005}")
}

/// Replace straight quotes with smart quotes.
fn smart_quotes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_quote = false;
    for c in text.chars() {
        if c == '"' {
            if in_quote {
                result.push('\u{201D}'); // RIGHT DOUBLE QUOTATION MARK
            } else {
                result.push('\u{201C}'); // LEFT DOUBLE QUOTATION MARK
            }
            in_quote = !in_quote;
        } else {
            result.push(c);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// High-contrast braille transformer
// ---------------------------------------------------------------------------

/// Map ASCII text to Unicode braille patterns (U+2800–U+28FF).
fn high_contrast_braille(content: &str) -> Result<AccessibleOutput, AccessibilityError> {
    let mut result = String::new();

    for c in content.chars() {
        match c {
            // Space: U+2800 (blank cell, all dots off)
            ' ' => result.push('\u{2800}'),

            // Lowercase a-z: position in alphabet | 0x2800
            'a'..='z' => {
                result.push(char::from_u32(((c as u32) - (b'a' as u32) + 1) | 0x2800).unwrap())
            }

            // Uppercase A-Z: same base with dot-7 (U+2840 = dots 7 added)
            'A'..='Z' => result
                .push(char::from_u32(((c as u32) - (b'A' as u32) + 1) | 0x2800 | 0x40).unwrap()),

            // Numbers 0-9: braille digit indicator (dots 3-4-5-6 = 0x30)
            '0' => result.push('\u{2831}'),
            '1' => result.push('\u{2832}'),
            '2' => result.push('\u{2833}'),
            '3' => result.push('\u{2834}'),
            '4' => result.push('\u{2835}'),
            '5' => result.push('\u{2836}'),
            '6' => result.push('\u{2837}'),
            '7' => result.push('\u{2838}'),
            '8' => result.push('\u{2839}'),
            '9' => result.push((('0' as u32 - b'0' as u32 + 1) | 0x2830) as u8 as char),

            // Common punctuation mapped to standard braille equivalents
            '.' => result.push('\u{282E}'), // braille dots-346 = comma equivalent
            ',' => result.push('\u{2820}'), // braille dots-3
            '!' => result.push('\u{2819}'), // braille dots-125
            '?' => result.push('\u{2819}'), // simplified
            ';' => result.push('\u{2824}'), // braille dots-134
            ':' => result.push('\u{2823}'), // braille dots-13
            '\'' => result.push('\u{281B}'), // apostrophe: dots-135
            '-' => result.push('\u{2807}'), // hyphen: dots-123
            '"' => result.push('\u{280D}'), // quotation mark: dots-145
            '(' => result.push('\u{281B}'), // open paren: dots-135
            ')' => result.push('\u{281F}'), // close paren: dots-1356
            '[' => result.push('\u{280A}'), // open bracket: dots-14
            ']' => result.push('\u{281C}'), // close bracket: dots-124
            '{' => result.push('\u{280E}'), // open brace: dots-134
            '}' => result.push('\u{283E}'), // close brace: dots-2356 (approximation)
            '/' => result.push('\u{2827}'), // slash approximation
            '\\' => result.push('\u{280F}'), // backslash approximation

            // Newlines: blank cell + actual newline for readability
            '\n' => {
                result.push('\u{2800}');
                result.push('\n');
            }
            '\r' => {
                result.push('\u{2800}');
                result.push('\r');
            }

            // Unrecognized ASCII: use unknown pattern (dots-123568)
            c if c.is_ascii() => result.push('\u{283F}'),

            // Non-ASCII: pass through as-is
            _ => result.push(c),
        }
    }

    Ok(AccessibleOutput::Braille(result))
}

// ---------------------------------------------------------------------------
// Helper methods on AccessibleOutput for ergonomic access
// ---------------------------------------------------------------------------

impl AccessibleOutput {
    /// Extract the plain text content if this is a Plain variant.
    pub fn as_plain(&self) -> Option<&str> {
        match self {
            AccessibleOutput::Plain(s) => Some(s),
            _ => None,
        }
    }

    /// Extract the braille content if this is a Braille variant.
    pub fn as_braille(&self) -> Option<&str> {
        match self {
            AccessibleOutput::Braille(s) => Some(s),
            _ => None,
        }
    }

    /// Check if this output is in structured JSON format.
    pub fn is_structured(&self) -> bool {
        matches!(self, AccessibleOutput::Structured(_))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain text: strips excess whitespace and normalizes paragraphs.
    #[test]
    fn test_plain_text_normalizes_whitespace() {
        let input = "Hello   world\n\nThis   is   a  paragraph.\n\nMultiple   spaces.";
        let config = AccessibilityConfig::plain();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Plain(text) => {
                assert!(text.contains("Hello world"));
                assert!(text.contains("This is a paragraph."));
                assert!(!text.contains("   ")); // no triple spaces
            }
            other => panic!("Expected Plain output, got {:?}", other),
        }
    }

    /// Structured JSON: identifies bullet points.
    #[test]
    fn test_structured_json_detects_bullets() {
        let input = "\n\n- First item\n- Second item\n- Third item";
        let config = AccessibilityConfig::structured();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Structured(json) => {
                let sections = json["sections"]
                    .as_array()
                    .expect("sections must be an array");
                assert!(sections.iter().any(|s| s["type"] == "list"));
            }
            other => panic!("Expected Structured output, got {:?}", other),
        }
    }

    /// Structured JSON: identifies fenced code blocks.
    #[test]
    fn test_structured_json_detects_code_blocks() {
        let input = "Some code:\n\n```\nfn main() {}\n```";
        let config = AccessibilityConfig::structured();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Structured(json) => {
                let sections = json["sections"]
                    .as_array()
                    .expect("sections must be an array");
                assert!(sections.iter().any(|s| s["type"] == "code_block"));
                // Verify language is set
                if let Some(code_section) = sections.iter().find(|s| s["type"] == "code_block") {
                    assert_eq!(code_section["content"], "fn main() {}\n");
                }
            }
            other => panic!("Expected Structured output, got {:?}", other),
        }
    }

    /// Dyslexia-friendly: inserts middot separators between words.
    #[test]
    fn test_dyslexia_friendly_adds_word_separators() {
        let input = "hello world foo bar";
        let config = AccessibilityConfig::dyslexia_friendly();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Plain(text) => {
                assert!(text.contains('\u{00B7}')); // MIDDOT must appear
                assert!(text.contains("hello"));
                assert!(text.contains("world"));
            }
            other => panic!("Expected Plain output, got {:?}", other),
        }
    }

    /// High-contrast braille: maps lowercase letters correctly.
    #[test]
    fn test_high_contrast_braille_maps_lowercase() {
        let input = "az";
        let config = AccessibilityConfig::braille();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Braille(text) => {
                assert_eq!(text.len(), 2);
                // a -> U+2801 (dots-1)
                let a_braille = text.chars().next().expect("should have first char");
                assert_eq!(a_braille, '\u{2801}');
                // z: 'z' - 'a' + 1 = 26, 26 | 0x2800 = 0x281A
                let z_braille = text.chars().nth(1).expect("should have second char");
                assert_eq!(z_braille, '\u{281A}');
            }
            other => panic!("Expected Braille output, got {:?}", other),
        }
    }

    /// High-contrast braille: uppercase letters get dot-7 marker.
    #[test]
    fn test_high_contrast_braille_maps_uppercase() {
        let input = "AZ";
        let config = AccessibilityConfig::braille();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Braille(text) => {
                // A -> U+2841 (base 0x2801 | 0x40 = 0x2841)
                let a_braille = text.chars().next().expect("should have first char");
                assert_eq!(a_braille, '\u{2841}');
            }
            other => panic!("Expected Braille output, got {:?}", other),
        }
    }

    /// High-contrast braille: space maps to U+2800 (zero-width blank cell).
    #[test]
    fn test_braille_space_is_zero_width() {
        let input = "a b";
        let config = AccessibilityConfig::braille();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Braille(text) => {
                assert_eq!(text.len(), 5); // a + space + b (space becomes U+2800)
                assert_eq!(text.chars().nth(1), Some('\u{2800}'));
            }
            other => panic!("Expected Braille output, got {:?}", other),
        }
    }

    /// Multi-sensory: config with multisensory=true returns Structured + Braille fields.
    #[test]
    fn test_multisensory_output_includes_both() {
        let input = "# Hello\n- Item 1\n- Item 2";
        let mut config = AccessibilityConfig::structured();
        config.multisensory = true;
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Structured(json) => {
                // Should have the base sections
                assert!(
                    !json["sections"]
                        .as_array()
                        .expect("sections must exist")
                        .is_empty()
                );
                // Multi-sensory adds plain_text and braille fields
                assert!(json.get("plain_text").is_some());
                assert!(json.get("braille").is_some());

                let pt = json["plain_text"]
                    .as_str()
                    .expect("plain_text should be string");
                assert!(!pt.is_empty());

                let br = json["braille"].as_str().expect("braille should be string");
                assert!(!br.is_empty());
            }
            other => panic!("Expected Structured output, got {:?}", other),
        }
    }
}
