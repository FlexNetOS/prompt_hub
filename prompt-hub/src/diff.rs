#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

/// Prompt diff engine for comparing prompt versions.
///
/// Generates line-based diffs between prompt versions and tracks
/// lineage relationships through version history.
#[derive(Debug, Clone, Default)]
pub struct PromptDiff;

/// A single line difference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLine {
    /// Line present in both versions
    Context {
        line_no_a: usize,
        line_no_b: usize,
        content: String,
    },
    /// Line removed (present in old, absent in new)
    Removed { line_no: usize, content: String },
    /// Line added (absent in old, present in new)
    Added { line_no: usize, content: String },
}

/// Complete diff between two prompt versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    pub old_version: String,
    pub new_version: String,
    pub lines: Vec<DiffLine>,
    pub additions: usize,
    pub deletions: usize,
    pub unchanged: usize,
}

/// Change summary for a version transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub total_changes: usize,
    pub is_significant: bool,
    pub change_ratio: f64,
    pub largest_change_block: usize,
}

impl PromptDiff {
    /// Create a new diff engine.
    pub fn new() -> Self {
        Self
    }

    /// Compute a line-based diff between two prompt texts.
    #[instrument(skip(self), fields(old_len = old_text.len(), new_len = new_text.len()))]
    pub fn compute(
        &self,
        old_text: &str,
        new_text: &str,
        old_ver: &str,
        new_ver: &str,
    ) -> DiffResult {
        let old_lines: Vec<&str> = old_text.lines().collect();
        let new_lines: Vec<&str> = new_text.lines().collect();

        let (lcs_indices, _lcs_len) = Self::longest_common_subsequence(&old_lines, &new_lines);

        let mut lines = Vec::new();
        let mut additions = 0usize;
        let mut deletions = 0usize;
        let mut unchanged = 0usize;

        let mut old_idx = 0usize;
        let mut new_idx = 0usize;
        let mut lcs_pos = 0usize;

        while old_idx < old_lines.len() || new_idx < new_lines.len() {
            if lcs_pos < lcs_indices.len()
                && old_idx < old_lines.len()
                && new_idx < new_lines.len()
                && lcs_indices[lcs_pos] == (old_idx, new_idx)
            {
                // Matching line
                lines.push(DiffLine::Context {
                    line_no_a: old_idx + 1,
                    line_no_b: new_idx + 1,
                    content: old_lines[old_idx].to_string(),
                });
                unchanged += 1;
                old_idx += 1;
                new_idx += 1;
                lcs_pos += 1;
            } else if old_idx < old_lines.len()
                && (lcs_pos >= lcs_indices.len()
                    || new_idx >= new_lines.len()
                    || old_idx < lcs_indices[lcs_pos].0)
            {
                // Line removed
                lines.push(DiffLine::Removed {
                    line_no: old_idx + 1,
                    content: old_lines[old_idx].to_string(),
                });
                deletions += 1;
                old_idx += 1;
            } else if new_idx < new_lines.len() {
                // Line added
                lines.push(DiffLine::Added {
                    line_no: new_idx + 1,
                    content: new_lines[new_idx].to_string(),
                });
                additions += 1;
                new_idx += 1;
            } else {
                break;
            }
        }

        info!(
            "Diff computed: +{} -{} unchanged={}",
            additions, deletions, unchanged
        );

        DiffResult {
            old_version: old_ver.to_string(),
            new_version: new_ver.to_string(),
            lines,
            additions,
            deletions,
            unchanged,
        }
    }

    /// Summarize the significance of changes.
    pub fn summarize(&self, diff: &DiffResult) -> ChangeSummary {
        let total_changes = diff.additions + diff.deletions;
        let total_lines = diff.additions + diff.deletions + diff.unchanged;
        let change_ratio = if total_lines > 0 {
            total_changes as f64 / total_lines as f64
        } else {
            0.0
        };

        // Calculate largest contiguous change block
        let mut largest_block = 0usize;
        let mut current_block = 0usize;
        for line in &diff.lines {
            match line {
                DiffLine::Added { .. } | DiffLine::Removed { .. } => {
                    current_block += 1;
                    largest_block = largest_block.max(current_block);
                }
                DiffLine::Context { .. } => current_block = 0,
            }
        }

        ChangeSummary {
            total_changes,
            is_significant: change_ratio > 0.3 || largest_block > 5,
            change_ratio,
            largest_change_block: largest_block,
        }
    }

    /// Check if two versions are identical.
    pub fn is_identical(&self, diff: &DiffResult) -> bool {
        diff.additions == 0 && diff.deletions == 0
    }

    /// Format diff as unified patch text.
    pub fn format_unified(&self, diff: &DiffResult) -> String {
        let mut output = format!(
            "--- version {}\n+++ version {}\n",
            diff.old_version, diff.new_version
        );
        for line in &diff.lines {
            match line {
                DiffLine::Context { content, .. } => {
                    output.push_str(&format!(" {}\n", content));
                }
                DiffLine::Removed { content, .. } => {
                    output.push_str(&format!("-{content}\n"));
                }
                DiffLine::Added { content, .. } => {
                    output.push_str(&format!("+{content}\n"));
                }
            }
        }
        output
    }

    /// Compute longest common subsequence (Myers algorithm simplified).
    fn longest_common_subsequence<T: Eq>(a: &[T], b: &[T]) -> (Vec<(usize, usize)>, usize) {
        let m = a.len();
        let n = b.len();

        // Use dynamic programming to find LCS
        let mut dp = vec![vec![0; n + 1]; m + 1];

        for i in (0..m).rev() {
            for j in (0..n).rev() {
                if a[i] == b[j] {
                    dp[i][j] = dp[i + 1][j + 1] + 1;
                } else {
                    dp[i][j] = dp[i + 1][j].max(dp[i][j + 1]);
                }
            }
        }

        // Backtrack to find the LCS path
        let mut lcs = Vec::new();
        let mut i = 0usize;
        let mut j = 0usize;
        while i < m && j < n {
            if a[i] == b[j] {
                lcs.push((i, j));
                i += 1;
                j += 1;
            } else if i < m
                && dp.get(i + 1).and_then(|row| row.get(j)).unwrap_or(&0)
                    >= dp.get(i).and_then(|row| row.get(j + 1)).unwrap_or(&0)
            {
                i += 1;
            } else {
                j += 1;
            }
        }

        let len = lcs.len();
        (lcs, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_texts() {
        let diff = PromptDiff::new();
        let result = diff.compute("line1\nline2", "line1\nline2", "v1", "v2");
        assert!(diff.is_identical(&result));
        assert_eq!(result.additions, 0);
        assert_eq!(result.deletions, 0);
        assert_eq!(result.unchanged, 2);
    }

    #[test]
    fn test_addition() {
        let diff = PromptDiff::new();
        let result = diff.compute("line1", "line1\nline2", "v1", "v2");
        assert_eq!(result.additions, 1);
        assert_eq!(result.deletions, 0);
        assert_eq!(result.unchanged, 1);
    }

    #[test]
    fn test_deletion() {
        let diff = PromptDiff::new();
        let result = diff.compute("line1\nline2", "line1", "v1", "v2");
        assert_eq!(result.additions, 0);
        assert_eq!(result.deletions, 1);
        assert_eq!(result.unchanged, 1);
    }

    #[test]
    fn test_mixed_changes() {
        let diff = PromptDiff::new();
        let result = diff.compute("A\nB\nC", "A\nX\nC\nD", "v1", "v2");
        assert!(result.additions > 0 || result.deletions > 0);
        assert_eq!(result.unchanged, 2); // "A" and "C" match but B is replaced
    }

    #[test]
    fn test_summary() {
        let diff = PromptDiff::new();
        let result = diff.compute("old content here", "new content here", "v1", "v2");
        let summary = diff.summarize(&result);
        assert!(summary.total_changes > 0);
        assert!(summary.change_ratio > 0.0);
    }

    #[test]
    fn test_format_unified() {
        let diff = PromptDiff::new();
        let result = diff.compute("A\nB", "A\nC", "v1", "v2");
        let formatted = diff.format_unified(&result);
        assert!(formatted.contains("--- version v1"));
        assert!(formatted.contains("+++ version v2"));
    }

    #[test]
    fn test_empty_inputs() {
        let diff = PromptDiff::new();
        let result = diff.compute("", "", "v1", "v2");
        assert!(diff.is_identical(&result));
    }

    #[test]
    fn test_completely_different() {
        let diff = PromptDiff::new();
        let result = diff.compute("A\nB\nC", "X\nY\nZ", "v1", "v2");
        assert_eq!(result.unchanged, 0);
    }

    #[test]
    fn test_diff_line_variants() {
        let added = DiffLine::Added {
            line_no: 1,
            content: "test".to_string(),
        };
        let removed = DiffLine::Removed {
            line_no: 1,
            content: "test".to_string(),
        };
        let ctx = DiffLine::Context {
            line_no_a: 1,
            line_no_b: 1,
            content: "test".to_string(),
        };
        assert_ne!(added, removed);
        assert_ne!(added, ctx);
    }

    #[test]
    fn test_change_summary_significant() {
        let diff = PromptDiff::new();
        // A very different text should be significant
        let result = diff.compute("A\nB\nC\nD\nE\nF\nG\nH\nI\nJ", "X\nY\nZ", "v1", "v2");
        let summary = diff.summarize(&result);
        assert!(summary.is_significant);
    }

    #[test]
    fn test_default() {
        let diff: PromptDiff = Default::default();
        let result = diff.compute("a", "b", "v1", "v2");
        assert_eq!(result.unchanged, 0);
    }
}
