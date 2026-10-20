#!/usr/bin/env zsh
# Local check script: runs the same fmt, clippy, test and wasm32 checks as CI.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

export PATH="/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:$PATH"

echo "==> [1/4] Checking Rust formatting..."
cargo fmt --all -- --check

echo "==> [2/4] Running Clippy lints (native)..."
./scripts/cargo-locked.sh clippy --workspace --all-targets -- -D warnings

echo "==> [3/4] Running workspace tests..."
./scripts/cargo-locked.sh test --workspace

echo "==> [4/4] Verifying wasm32-unknown-unknown build..."
./scripts/cargo-locked.sh check --target wasm32-unknown-unknown -p graphviz-pure-wasm

echo "==> Local verification complete."
