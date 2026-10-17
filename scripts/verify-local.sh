#!/usr/bin/env zsh
# Local verification & content-addressed CI receipt generator.
# Runs fmt, clippy, unit tests, and wasm32 target checks once using warm local sccache,
# then writes .ci-verified.json keyed on the git tree SHA of the source code so GitHub Actions
# does not wastefully re-run the same compilation and test suite when the tree matches.

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
./scripts/cargo-locked.sh check --target wasm32-unknown-unknown -p gv-wasm

# Compute deterministic hash of all tracked source files (excluding .ci-verified.json)
SOURCE_HASH=$(git ls-files -s | grep -v '\.ci-verified\.json$' | shasum -a 256 | awk '{print $1}')
RUSTC_VER=$(rustc --version)
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

cat > .ci-verified.json <<EOF
{
  "source_hash": "${SOURCE_HASH}",
  "rustc_version": "${RUSTC_VER}",
  "targets_verified": ["aarch64-apple-darwin", "wasm32-unknown-unknown"],
  "verified_at": "${TIMESTAMP}",
  "sccache_enabled": true
}
EOF

echo "==> Verification complete! Wrote .ci-verified.json (source_hash=${SOURCE_HASH})"
sccache --show-stats | grep -E "Compile requests|Cache hits|Cache misses|Hit rate" || true
