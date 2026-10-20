#!/usr/bin/env zsh
# Resource-governed, serialized Cargo wrapper with sccache and shared target directory.
# Prevents concurrent subagents from spawning competing Rust builds and exhausting RAM/CPU.

set -euo pipefail

export PATH="/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:$PATH"
export RUSTC_WRAPPER="/opt/homebrew/bin/sccache"
export SCCACHE_DIR="${HOME}/.cache/sccache-graphviz-pure"
export SCCACHE_CACHE_SIZE="20G"
export CARGO_TARGET_DIR="${HOME}/.cache/cargo-target-graphviz-pure"
export CARGO_BUILD_JOBS=6

mkdir -p "$SCCACHE_DIR" "$CARGO_TARGET_DIR"

LOCK_DIR="/tmp/graphviz-pure-cargo.lock"
WAIT_SEC=0
while ! mkdir "$LOCK_DIR" 2>/dev/null; do
  sleep 1
  WAIT_SEC=$((WAIT_SEC + 1))
  if (( WAIT_SEC > 300 )); then
    echo "Warning: Stale cargo lock detected after 300s, breaking lock." >&2
    rmdir "$LOCK_DIR" 2>/dev/null || true
  fi
done
trap 'rmdir "$LOCK_DIR" 2>/dev/null || true' EXIT INT TERM

# Check system memory pressure before running heavy compilation
FREE_PAGES=$(vm_stat | awk '/Pages free|Pages speculative|Pages inactive/ {gsub(/\./,"",$NF); sum += $NF} END {print sum}')
FREE_MB=$(( FREE_PAGES * 16384 / 1048576 ))
if (( FREE_MB < 1024 )); then
  echo "[cargo-locked] Low available memory (${FREE_MB} MB), throttling CARGO_BUILD_JOBS=2" >&2
  export CARGO_BUILD_JOBS=2
fi

cargo "$@"
