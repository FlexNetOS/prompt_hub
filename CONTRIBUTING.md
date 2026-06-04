# Contributing to PromptHub

Thank you for your interest in contributing to PromptHub! This document
provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Development Setup](#development-setup)
- [Code Style](#code-style)
- [Testing](#testing)
- [Commit Conventions](#commit-conventions)
- [Pull Request Process](#pull-request-process)
- [Release Process](#release-process)

## Prerequisites

- **Rust** — latest stable toolchain (see `rust-toolchain.toml`)
- **just** — command runner (`cargo install just`)
- **Docker** — for containerized builds and integration tests
- **clang + lld** — for faster linking (Linux)

### Recommended Dev Tools

Install all recommended tools with:

```bash
just tools
```

This installs:

| Tool | Purpose |
|------|---------|
| `cargo-nextest` | Faster test runner |
| `cargo-tarpaulin` | Code coverage |
| `cargo-audit` | Security audit |
| `cargo-deny` | License & dependency checking |
| `cargo-mutants` | Mutation testing |
| `git-cliff` | Changelog generation |

## Development Setup

1. **Clone the repository:**

   ```bash
   git clone https://github.com/prompthub/prompthub.git
   cd prompthub
   ```

2. **Verify your environment:**

   ```bash
   just check
   just test
   ```

   `bash scripts/setup.sh` provisions the toolchain and activates the
   version-controlled git hooks (`git config core.hooksPath .githooks`). To
   enable just the hooks without the full setup, run that one command. The
   `pre-commit` hook enforces worktree-only commits and runs the lint/test gate
   (`scripts/code_review.sh`); set `SKIP_REVIEW_TESTS=1` to lint without tests.

3. **Run the full validation suite:**

   ```bash
   just lint
   just nextest
   just audit
   ```

## Code Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- Run `cargo fmt` before committing.
- Run `cargo clippy --workspace --all-features -- -D warnings` and resolve all warnings.
- Use `#[derive(Debug)]` on all public types.
- Document all public items with doc comments (`///`).
- Prefer `?` over `.unwrap()` / `.expect()` in library code.
- Use `thiserror` for library errors and `anyhow` for application errors.
- Keep functions focused and under ~50 lines where practical.
- Use meaningful variable names; avoid single-letter names except in closures.

### Unsafe Code

PromptHub uses `#![forbid(unsafe_code)]` — **no unsafe code is permitted**.

## Testing

- Every module should have a corresponding `tests` submodule (using `#[cfg(test)]`).
- Aim for >80% code coverage on library crates.
- Use `cargo nextest` for the fastest test feedback loop.
- Integration tests live in the `tests/` directory at the workspace root.
- Use `insta` for snapshot testing where applicable.

### Running Tests

```bash
# Run all tests
just test

# Run with nextest (recommended)
just nextest

# Run a specific test
cargo test --package prompt-hub -- module_name::test_name

# Generate coverage report
just coverage
```

## Commit Conventions

We follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | A new feature |
| `fix` | A bug fix |
| `docs` | Documentation changes only |
| `style` | Code style changes (formatting, semicolons, etc.) |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or correcting tests |
| `chore` | Changes to build process, dependencies, or auxiliary tools |
| `ci` | CI/CD configuration changes |
| `security` | Security-related changes |

### Examples

```
feat(search): add hybrid search combining FTS5 and vector similarity

fix(auth): resolve timing attack in argon2id comparison
docs(api): document rate-limiting headers
refactor(db): extract migration runner into standalone module
test(rbac): add tests for AgentIdentity permission matrix
```

### Breaking Changes

Breaking changes must include a `BREAKING CHANGE:` footer or append `!` after
the type/scope:

```
feat(api)!: remove deprecated v1 endpoints

BREAKING CHANGE: The /api/v1/* endpoints have been removed.
Clients must migrate to /api/v2/*.
```

## Pull Request Process

1. **Fork and branch:** Create a feature branch from `main`:

   ```bash
   git checkout -b feat/my-feature-name
   ```

2. **Make your changes:** Follow the code style and testing guidelines above.

3. **Pre-flight checks:** Run the full validation suite before pushing:

   ```bash
   just check && just lint && just nextest && just audit
   ```

4. **Commit:** Use Conventional Commits format.

5. **Push and open a PR:** Include:
   - A clear description of the change
   - Motivation and context
   - Link to any related issues
   - Checklist of completed items

6. **Review:** PRs require at least one approval before merging.

### PR Checklist

- [ ] `just check` passes
- [ ] `just lint` passes
- [ ] `just test` passes
- [ ] New tests added for new functionality
- [ ] Documentation updated (doc comments + user-facing docs)
- [ ] CHANGELOG.md updated under `[Unreleased]`
- [ ] Commit messages follow Conventional Commits

## Release Process

1. Update `CHANGELOG.md` following [Keep a Changelog](https://keepachangelog.com/) format.
2. Bump version numbers in relevant `Cargo.toml` files.
3. Create a release PR.
4. After merge, tag the release:

   ```bash
   git tag -a v0.1.0 -m "Release v0.1.0"
   git push origin v0.1.0
   ```

5. The CI pipeline will build and publish release artifacts automatically.

## Questions?

Feel free to open a [Discussion](https://github.com/prompthub/prompthub/discussions)
or reach out to the maintainers.

Happy coding!
