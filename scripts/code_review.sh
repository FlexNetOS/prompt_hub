#!/usr/bin/env bash
#
# scripts/code_review.sh
# Enforce git-worktree usage and run the project's review gate before a commit.
#
# Safe to run from any directory: it resolves the repo root itself. Set
# SKIP_REVIEW_TESTS=1 to skip the (slow) test run and lint only.

set -euo pipefail

# Always operate from the repo root, regardless of the caller's cwd.
REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# 1. Enforce worktree usage.
#    Canonical detection: in the main working tree the per-worktree git dir and
#    the shared common dir are identical; in a linked worktree they differ.
GIT_DIR="$(cd "$(git rev-parse --git-dir)" && pwd -P)"
COMMON_DIR="$(cd "$(git rev-parse --git-common-dir)" && pwd -P)"

if [ "$GIT_DIR" = "$COMMON_DIR" ]; then
    echo "ERROR: Git worktree MUST be used for changes. Commits in the main repository are prohibited." >&2
    echo "Create one with: git worktree add ../<name> -b <branch>" >&2
    exit 1
fi

echo "Worktree detected. Starting code review..."

# 2. Review gate — match the project's real gate (justfile / ci.yml),
#    not a bare default-member build.
echo "Running cargo clippy (workspace, all features, -D warnings)..."
cargo clippy --workspace --all-features -- -D warnings

if [ "${SKIP_REVIEW_TESTS:-0}" = "1" ]; then
    echo "SKIP_REVIEW_TESTS=1 set — skipping tests."
else
    echo "Running tests (workspace, all features)..."
    cargo test --workspace --all-features
fi

echo "Code review passed!"
