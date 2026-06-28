#!/usr/bin/env bash
#
# drift_guard.sh — enforce the "Rust-native invariant" from CLAUDE.md.
#
# Splits findings the same way CLAUDE.md does:
#   * Rust source drift  -> BLOCKING  (the workspace is the contract)
#   * Prose/harness drift -> ADVISORY (prose may be stale or foreign)
#
# Usage:
#   scripts/drift_guard.sh [file ...]
# With no args it inspects files changed vs. the BASE_REF (default: origin/main),
# falling back to the whole tree when no base is available.
#
# Emits GitHub Actions annotations (::error / ::warning) when run in CI, and
# writes a markdown summary to $GITHUB_STEP_SUMMARY / drift-report.md.
# Exit code is non-zero only when a BLOCKING (Rust-source) drift is found.

set -uo pipefail

BASE_REF="${BASE_REF:-origin/main}"
REPORT="${DRIFT_REPORT:-drift-report.md}"
blocking=0
advisory=0

# ---- collect target files -------------------------------------------------
if [ "$#" -gt 0 ]; then
  mapfile -t FILES < <(printf '%s\n' "$@")
else
  if git rev-parse --verify "$BASE_REF" >/dev/null 2>&1; then
    mapfile -t FILES < <(git diff --name-only --diff-filter=d "$BASE_REF"...HEAD)
  else
    mapfile -t FILES < <(git ls-files)
  fi
fi

# Third-party vendored crate sources (vendor/) are NOT subject to the
# Rust-native contract — they are external dependencies copied verbatim by
# `cargo vendor`, not this workspace's code (and legitimately use async_trait,
# C, unsafe, etc.). Drop them so vendoring does not trip the guard. target/ is
# build output and is excluded for the same reason.
FILTERED=()
for _f in "${FILES[@]}"; do
  case "$_f" in
    vendor/*|target/*) continue ;;
    *) FILTERED+=("$_f") ;;
  esac
done
FILES=("${FILTERED[@]+"${FILTERED[@]}"}")

: >"$REPORT"
{
  echo "# Rust-Native Guard report"
  echo
} >>"$REPORT"

is_ci() { [ -n "${GITHUB_ACTIONS:-}" ]; }

# annotate <level> <file> <line> <message>
annotate() {
  local level="$1" file="$2" line="$3" msg="$4"
  if is_ci; then
    echo "::${level} file=${file},line=${line}::${msg}"
  fi
  echo "- **${level^^}** \`${file}:${line}\` — ${msg}" >>"$REPORT"
}

# ---- patterns -------------------------------------------------------------
# Rust-source drift = foreign idioms that contradict the workspace contract.
# (grep -nE; matches reported with line numbers.)
# NOTE: `unsafe` is intentionally NOT here — it is already a hard error via
# `#![forbid(unsafe_code)]` (the compiler rejects it) and scripts/check_safety.sh.
# We only flag the `async_trait` crate idiom, which nothing else catches: this
# workspace uses native `async fn in trait` + boxed-future variants for `dyn`.
rust_block_patterns=(
  'async_trait'
  '#\[async_trait'
)

# Prose/harness drift = non-Rust-native commands or foreign-language examples.
prose_cmd_patterns=(
  '\bnpm\b' '\byarn\b' '\bpnpm\b' '\bnpx\b'
  '\bpip[0-9]?\b' '\bpytest\b' '\bpython[0-9.]*\b'
  '\bgo (test|build|run)\b'
  '\bmake\b' '\bgradle\b' '\bmvn\b'
  '\bbundle exec\b' '\brspec\b'
)
prose_fence_patterns=(
  '```(python|py|js|javascript|ts|typescript|go|golang|ruby|rb|java|c\+\+|cpp)'
)

is_rust_src() { [[ "$1" == */src/*.rs || "$1" == */benches/*.rs ]]; }
is_prose() {
  case "$1" in
    *.md|.agent.md|.agents.md|.instructions.md|.prompt.md|GEMINI.md|AGENTS.md) return 0 ;;
    .junie/*|skills/*|.github/copilot-instructions.md) return 0 ;;
    *) return 1 ;;
  esac
}

scan() {
  local file="$1"; shift
  local level="$1"; shift          # error | warning
  local kind="$1"; shift           # human label
  local code_only="$1"; shift      # 1 = ignore matches that live only in // comments
  local -n pats="$1"
  local p hit lineno text code
  for p in "${pats[@]}"; do
    while IFS=: read -r lineno text; do
      [ -z "$lineno" ] && continue
      # A match that survives only inside a `//` comment is not real drift
      # (e.g. a comment documenting "no async_trait"); skip it.
      if [ "$code_only" = "1" ]; then
        code="${text%%//*}"
        grep -qE "$p" <<<"$code" || continue
      fi
      annotate "$level" "$file" "$lineno" "${kind}: '${text#"${text%%[![:space:]]*}"}'"
      if [ "$level" = "error" ]; then blocking=$((blocking+1)); else advisory=$((advisory+1)); fi
    done < <(grep -nE "$p" -- "$file" 2>/dev/null)
  done
}

for f in "${FILES[@]}"; do
  [ -f "$f" ] || continue
  if is_rust_src "$f"; then
    scan "$f" error "Rust drift (use native async fn in trait, not async_trait)" 1 rust_block_patterns
  elif is_prose "$f"; then
    scan "$f" warning "non-Cargo command in docs (verify & transform to cargo/just)" 0 prose_cmd_patterns
    scan "$f" warning "foreign-language example (verify intent; convert to Rust-native if it describes this codebase)" 0 prose_fence_patterns
  fi
done

{
  echo
  echo "## Summary"
  echo "- Blocking (Rust-source) drift: **${blocking}**"
  echo "- Advisory (prose/harness) drift: **${advisory}**"
  echo
  if [ "$blocking" -gt 0 ]; then
    echo "> Blocking findings violate the Rust-native contract. Transform to native idioms"
    echo "> (native \`async fn in trait\` + boxed futures for dyn, \`Result<_, HubError>\`, no \`unsafe\`) and re-push."
  fi
  if [ "$advisory" -gt 0 ]; then
    echo "> Advisory findings are prose that drifted from Rust-native conventions. Per CLAUDE.md,"
    echo "> verify against the code, transform to Rust-native, and reconcile the doc — these do not block."
  fi
} >>"$REPORT"

if is_ci && [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat "$REPORT" >>"$GITHUB_STEP_SUMMARY"
fi

echo "drift_guard: ${blocking} blocking, ${advisory} advisory (report: ${REPORT})"
[ "$blocking" -eq 0 ]
