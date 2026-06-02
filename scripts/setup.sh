#!/usr/bin/env bash
# Environment setup script for prompt_hub workspace
# Usage: bash scripts/setup.sh

set -euo pipefail

RUST_VERSION="1.96.0"
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

red() { echo -e "\033[31m$*\033[0m"; }
green() { echo -e "\033[32m$*\033[0m"; }
yellow() { echo -e "\033[33m$*\033[0m"; }

info() { green "[INFO] $*"; }
warn() { yellow "[WARN] $*"; }
error() { red "[ERROR] $*"; }

# ---- Detect OS and architecture ----
detect_platform() {
    local _arch="$(uname -m)"
    local _os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    
    case "$_arch" in
        x86_64) RUSTUP_ARCH="x86_64-unknown-linux-gnu" ;;
        aarch64|arm64) RUSTUP_ARCH="aarch64-unknown-linux-gnu" ;;
        *) error "Unsupported architecture: $_arch"; exit 1 ;;
    esac
    
    info "Platform: $_os / $_arch -> $RUSTUP_ARCH"
}

# ---- Install rustup + Rust toolchain ----
install_rust() {
    if command -v rustc &>/dev/null && rustc --version | grep -q "$RUST_VERSION"; then
        info "Rust $RUST_VERSION already installed"
        return 0
    fi

    info "Installing Rust $RUST_VERSION via rustup..."
    
    # Download rustup-init
    local _url="https://static.rust-lang.org/rustup/dist/${RUSTUP_ARCH}/rustup-init"
    local _init="/tmp/rustup-init"
    
    info "Downloading rustup-init from $_url"
    curl --proto '=https' --tlsv1.2 -sSfL "$" -o "$_init" || {
        error "Failed to download rustup-init"
        exit 1
    }
    chmod +x "$_init"
    
    # Install with explicit paths (non-interactive)
    export CARGO_HOME="$CARGO_HOME"
    export RUSTUP_HOME="$RUSTUP_HOME"
    
    "$_init" -y --default-toolchain "$RUST_VERSION" \
        --component rustfmt,clippy,rust-src \
        --profile default \
        --no-modify-path
    
    # Source cargo env
    if [ -f "$CARGO_HOME/env" ]; then
        source "$CARGO_HOME/env"
    fi
    
    # Verify
    if command -v rustc &>/dev/null; then
        info "Rust installed: $(rustc --version)"
        info "Cargo installed: $(cargo --version)"
    else
        error "Rust installation failed"
        exit 1
    fi
}

# ---- Install additional cargo tools ----
install_cargo_tools() {
    info "Installing cargo tools..."
    
    local _tools=(
        "cargo-nextest"
        "cargo-tarpaulin"
        "cargo-audit"
        "cargo-deny"
        "cargo-mutants"
        "git-cliff"
        "cargo-outdated"
        "cargo-tree"
        "cargo-bloat"
    )
    
    for _tool in "${_tools[@]}"; do
        if cargo install --list 2>/dev/null | grep -q "^$_tool "; then
            info "  $_tool already installed"
        else
            info "  Installing $_tool..."
            cargo install "$_tool" 2>/dev/null || warn "  Failed to install $_tool (optional)"
        fi
    done
}

# ---- Verify workspace dependencies ----
verify_workspace() {
    info "Verifying workspace dependencies..."
    
    cd "$(dirname "$0")/.."
    
    # Check key deps are available
    cargo fetch 2>/dev/null || warn "cargo fetch failed (may need network)"
    
    info "Workspace ready at $(pwd)"
}

# ---- Print environment summary ----
print_env() {
    echo ""
    green "========================================"
    green "  prompt_hub Environment Ready"
    green "========================================"
    echo ""
    echo "  Rust:     $(rustc --version 2>/dev/null || echo 'NOT INSTALLED')"
    echo "  Cargo:    $(cargo --version 2>/dev/null || echo 'NOT INSTALLED')"
    echo "  Clippy:   $(cargo clippy --version 2>/dev/null || echo 'NOT INSTALLED')"
    echo "  Rustfmt:  $(rustfmt --version 2>/dev/null || echo 'NOT INSTALLED')"
    echo "  CARGO_HOME: $CARGO_HOME"
    echo "  RUSTUP_HOME: $RUSTUP_HOME"
    echo ""
    echo "  Add to your shell profile:"
    echo "    source $CARGO_HOME/env"
    echo ""
    green "  Quick start:"
    echo "    just check    # cargo check --workspace"
    echo "    just test     # cargo test --workspace"
    echo "    just build    # cargo build --release"
    echo ""
}

# ---- Main ----
main() {
    info "Setting up prompt_hub development environment..."
    
    detect_platform
    install_rust
    install_cargo_tools
    verify_workspace
    print_env
}

main "$@"
