# syntax=docker/dockerfile:1

# Build stage
FROM rust:1.91-slim AS builder
WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy workspace manifests
COPY Cargo.toml Cargo.toml
COPY rust-toolchain.toml rust-toolchain.toml
COPY prompt-hub/Cargo.toml prompt-hub/
COPY prompthub/Cargo.toml prompthub/
COPY prompthub-server/Cargo.toml prompthub-server/

# Build dependencies cache layer
RUN mkdir -p prompt-hub/src prompthub/src prompthub-server/src && \
    echo "pub fn dummy() {}" > prompt-hub/src/lib.rs && \
    echo "fn main() {}" > prompthub/src/main.rs && \
    echo "fn main() {}" > prompthub-server/src/main.rs && \
    cargo build --release --workspace && \
    rm -rf prompt-hub/src prompthub/src prompthub-server/src

# Copy full source
COPY prompt-hub/src prompt-hub/src/
COPY prompthub/src prompthub/src/
COPY prompthub-server/src prompthub-server/src/
# Crate-embedded assets (include_str!) now live under the prompt-hub crate;
# README is needed for the lib's `#![doc = include_str!("../README.md")]`.
COPY prompt-hub/migrations prompt-hub/migrations/
COPY prompt-hub/templates prompt-hub/templates/
COPY prompt-hub/README.md prompt-hub/README.md
COPY docs docs/

# Build release binaries
RUN cargo build --release --bin prompthub-server --bin prompthub

# Runtime stage - distroless
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /app/target/release/prompthub-server /bin/
COPY --from=builder /app/target/release/prompthub /bin/
COPY --from=builder /app/prompt-hub/migrations /migrations
COPY --from=builder /app/prompt-hub/templates /templates
COPY --from=builder /app/docs /docs

# Security: run as non-root
USER nonroot:nonroot

ENV PROMPTHUB_DB_PATH=/data/prompthub.db
ENV RUST_LOG=info

EXPOSE 8080

# Health check (distroless has no shell, use the binary itself)
ENTRYPOINT ["/bin/prompthub-server"]
