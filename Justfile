# GitCortex — one-line developer entry points.
# Install: `brew install just`  (or `cargo install just`).
# List all targets: `just`.

set shell := ["bash", "-cu"]
set dotenv-load := false

# Default: show help
default:
    @just --list --unsorted

# ─── Setup ──────────────────────────────────────────────────────────────────

# Install all toolchains + workspace deps. Run once per fresh clone.
bootstrap:
    @command -v mise >/dev/null || (echo "Install mise first: https://mise.jdx.dev" && exit 1)
    mise install
    cargo fetch
    cd viz && npm ci

# Just refresh deps (no toolchain install)
deps:
    cargo fetch
    cd viz && npm ci

# ─── Dev loop ───────────────────────────────────────────────────────────────

# Run backend + viz frontend in parallel (Ctrl-C kills both)
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    cleanup() { kill 0 2>/dev/null || true; }
    trap cleanup EXIT
    cargo run -p gitcortex -- viz --port 5678 &
    cd viz && npm run dev
    wait

# Backend only (no viz HMR — use when iterating on Rust)
dev-backend:
    cargo run -p gitcortex -- viz --port 5678

# Frontend only (assumes gcx viz is already running)
dev-viz:
    cd viz && npm run dev

# ─── Build ──────────────────────────────────────────────────────────────────

# Build the frontend bundle, then the gcx binary in release mode
build:
    cd viz && npm run build
    cargo build --release --workspace

# Build everything in debug
build-debug:
    cd viz && npm run build
    cargo build --workspace

# Install the gcx binary into ~/.local/bin
install: build
    @mkdir -p ~/.local/bin
    @cp target/release/gcx ~/.local/bin/gcx
    @echo "Installed gcx → ~/.local/bin/gcx"

# ─── Lint / format / test ───────────────────────────────────────────────────

# Run absolutely everything CI runs
ci: fmt-check clippy test viz-lint viz-test viz-build

# Auto-fix formatting (Rust + frontend)
fmt:
    cargo fmt --all
    cd viz && npm run format

# Check formatting without writing
fmt-check:
    cargo fmt --all -- --check
    cd viz && npm run format:check

# Clippy with -D warnings
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run all Rust tests
test:
    cargo test --workspace

# Frontend lint (eslint + tsc)
viz-lint:
    cd viz && npm run lint

# Frontend unit tests (vitest)
viz-test:
    cd viz && npm run test --if-present

# Frontend production build
viz-build:
    cd viz && npm run build

# ─── Quality / security audits ──────────────────────────────────────────────

# Run cargo-deny (advisories + licenses + duplicates)
deny:
    cargo deny --workspace check

# RustSec advisory scan
audit:
    cargo audit

# Detect unused dependencies
machete:
    cargo machete

# Run all quality audits
audit-all: deny audit machete

# ─── Release ────────────────────────────────────────────────────────────────

# Print what `cargo dist build --target ...` would produce, no actual build
dist-plan:
    cargo dist plan

# Local dist build (for testing before tagging a release)
dist-build:
    cargo dist build --target $(rustc -Vv | grep host | cut -d' ' -f2)

# ─── Cleanup ────────────────────────────────────────────────────────────────

# Remove build artefacts (keeps node_modules)
clean:
    cargo clean
    rm -rf crates/gitcortex-viz/dist-viz

# Nuclear: also remove node_modules and the kuzu DB
clean-all: clean
    rm -rf viz/node_modules
    rm -rf ~/.local/share/gitcortex
