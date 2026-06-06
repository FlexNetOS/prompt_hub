# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com) and commits are grouped
by [Conventional Commits](https://www.conventionalcommits.org).
## [unreleased]

### Bug Fixes

- Make --all-features workspace compile, lint, test, and safety-check green
- *(publish)* Vendor crate assets so `cargo publish` can package prompt-hub
- *(security)* Resolve cargo-audit vulns and migrate cargo-deny config
- *(ci)* Green the post-merge Format, Publish, and Docker jobs
- *(ci)* Correct invalid GitHub Actions expression in ai-safety-deployment
- *(ci)* Green Docker Build and Cargo Deny
- *(security)* Remediate Dependabot alerts + add reasoning-anchored harness
- *(deps,build)* Remove 32 unused deps + fix default-feature build (Qodana triage)
- *(audit)* Hex-encode sha2 0.11 digest to restore green build (#30)
- *(loop)* Make HANDOFF resume location-agnostic (don't assume the worktree) (#35)
- *(cli)* Route tracing logs to stderr so stdout stays machine-readable (#36)

### CI/CD

- Add Rust-native drift guard + CLAUDE.md
- *(ai)* Gate AI workflows behind ENABLE_AI_WORKFLOWS so they skip, not fail
- *(doc)* Enforce RUSTDOCFLAGS=-D warnings so the doc build stays warning-clean (#37)

### Documentation

- Link harness autonomous-operation upgrade kit
- *(todo,backlog)* Record /verify findings + reconcile completed work (#34)

### Features

- Adopt junie orchestration + audit/qodana tooling (re-anchored on main)
- *(otel)* Wire Prometheus text exposition; drop vulnerable protobuf path
- *(cli)* Add `prompthub metrics` subcommand (Prometheus exposition) (#31)

### Miscellaneous

- Version-control git hooks via core.hooksPath
- *(harness)* Add autonomous construction-crew harness for prompt_hub
- *(harness)* Default /prompt-loop to apply mode (push→PR→auto-merge on green)
- *(quality)* Drop unnecessary path qualifications (qodana triage) (#32)
- *(loop)* Handoff cycle 3 (budget reached, 3/3 merged) (#33)

### Testing

- *(p3)* Add sanitization edge-case + LockManager concurrency tests

### Bench

- Fix benches to compile + run under -D warnings

### Clippy

- Make --all-targets green under -D warnings (test modules + src)


