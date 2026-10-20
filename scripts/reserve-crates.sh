#!/usr/bin/env bash
# One-time: publishes 0.0.0 placeholders of the seven graphviz-pure crate names, so that trusted
# publishing can be configured for them (crates.io only allows it for existing crates). Run
# after `cargo login` with a short-lived "publish-new" token; see docs/releasing.md.
# crates.io rate-limits new crate names; the script waits and retries when told to.
set -euo pipefail
crates=(graphviz-pure-core graphviz-pure-parser graphviz-pure-geom graphviz-pure-layout
        graphviz-pure-render graphviz-pure graphviz-pure-cli)
work=$(mktemp -d)
for c in "${crates[@]}"; do
  if curl -fsS -A "graphviz-pure reserve script" "https://crates.io/api/v1/crates/$c" >/dev/null 2>&1; then
    echo "$c already exists"; continue
  fi
  d="$work/$c"; mkdir -p "$d/src"
  cat > "$d/Cargo.toml" <<TOML
[package]
name = "$c"
version = "0.0.0"
edition = "2021"
license = "EPL-2.0"
description = "Placeholder for graphviz-pure, a Rust port of Graphviz; the first release is 0.1.0."
repository = "https://github.com/graphviz-pure/graphviz-pure"
readme = "README.md"
TOML
  printf '//! Placeholder: see <https://github.com/graphviz-pure/graphviz-pure>.\n' > "$d/src/lib.rs"
  printf '# %s\n\nPlaceholder for [graphviz-pure](https://github.com/graphviz-pure/graphviz-pure); the first release is 0.1.0.\n' "$c" > "$d/README.md"
  until (cd "$d" && cargo publish --allow-dirty 2>&1 | tee "$d/log"); do
    grep -q "429\|rate limit" "$d/log" || exit 1
    echo "rate limited; retrying in 10 minutes"; sleep 600
  done
done
echo "Now configure trusted publishing for each crate (docs/releasing.md) and delete the token."
