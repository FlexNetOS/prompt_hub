# PromptHub development tasks

default:
    @just --list

# Run all checks
check:
    cargo check --workspace --all-features

# Run all tests
test:
    cargo test --workspace --all-features

# Run with nextest
nextest:
    cargo nextest run --workspace --all-features

# Run clippy
lint:
    cargo clippy --workspace --all-features -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Generate documentation
doc:
    cargo doc --workspace --all-features --no-deps --open

# Verify the docs build is warning-clean (mirrors the CI `doc` job)
doc-check:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps

# Generate CHANGELOG.md from Conventional-Commit history (uses .cliff.toml)
changelog:
    git-cliff --output CHANGELOG.md

# Run benchmarks
bench:
    cargo bench --workspace

# Build release
build:
    cargo build --release --workspace

# Build Docker image
docker:
    docker build -f docker/Dockerfile -t prompthub:latest .

# Run the server
serve:
    cargo run --bin prompthub-server -- --port 8080

# Run the CLI
cli *args:
    cargo run --bin prompthub -- {{args}}

# Run security audit
audit:
    cargo audit

# Check dependencies
deps:
    cargo tree -e normal --prefix none | sort -u

# Run mutation testing
mutants:
    cargo mutants --workspace

# Coverage report
coverage:
    cargo tarpaulin --workspace --all-features --out html
    @echo "Open tarpaulin-report.html"

# Clean build artifacts
clean:
    cargo clean
    rm -rf target/

# Install dev tools
tools:
    cargo install cargo-nextest cargo-tarpaulin cargo-audit cargo-deny cargo-mutants git-cliff
