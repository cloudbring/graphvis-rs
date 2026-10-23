#!/usr/bin/env bash
# Regenerates THIRD-PARTY-LICENSES.md: the licence texts and copyright notices of every crate
# compiled into the graphviz-pure binary and the graphviz-pure-wasm module.
#
#   scripts/third-party-licenses.sh           rewrite THIRD-PARTY-LICENSES.md
#   scripts/third-party-licenses.sh --check   fail if the committed file is stale (CI)
#
# Needs cargo-about at exactly the version below:
#   cargo install --locked --features cli cargo-about@0.9.2
# Configuration: about.toml (accepted licences, targets, clarifications). Template:
# scripts/third-party-licenses.hbs. The scan reads only Cargo.lock and the crates.io sources of
# the locked versions (--frozen: no network), so the output is the same on every machine.
set -euo pipefail

CARGO_ABOUT_VERSION=0.9.2
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/THIRD-PARTY-LICENSES.md"

check=0
case "${1:-}" in
  --check) check=1 ;;
  "") ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

have="$(cargo about --version 2>/dev/null || true)"
if [[ "$have" != "cargo-about $CARGO_ABOUT_VERSION" ]]; then
  echo "error: need cargo-about $CARGO_ABOUT_VERSION, found '${have:-none}'." >&2
  echo "  cargo install --locked --features cli cargo-about@$CARGO_ABOUT_VERSION" >&2
  exit 1
fi

# --frozen needs the locked crates in the local registry; fetching is the only network step.
cargo fetch --locked --manifest-path "$root/Cargo.toml" >/dev/null

raw="$(mktemp)"
tmp="$(mktemp)"
trap 'rm -f "$raw" "$tmp"' EXIT

# --workspace: the CLI and the wasm crate are both workspace members; the other members add no
# third-party crates of their own today, and workspace crates are left out by the template.
# --all-features matches deny.toml (the optional features, such as graphviz-pure-core/math-trace,
# add no crates today, but would be covered if they did). --fail: an unresolved licence is an error, not a silent canonical text.
cargo about generate \
  --manifest-path "$root/Cargo.toml" \
  --config "$root/about.toml" \
  --workspace --all-features --frozen --fail \
  --threshold 0.9 \
  --output-file "$raw" \
  "$root/scripts/third-party-licenses.hbs"

# Strip carriage returns and trailing whitespace that some crates' licence files carry.
LC_ALL=C sed -e 's/[[:space:]]*$//' "$raw" > "$tmp"

if (( check )); then
  if ! diff -u "$out" "$tmp"; then
    echo "error: THIRD-PARTY-LICENSES.md is stale; run scripts/third-party-licenses.sh and commit the result." >&2
    exit 1
  fi
  echo "THIRD-PARTY-LICENSES.md is up to date."
else
  cp "$tmp" "$out"
  echo "wrote $out"
fi
