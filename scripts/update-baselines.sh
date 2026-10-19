#!/usr/bin/env bash
# Refreshes every corpus ratchet baseline for one math profile (see docs/porting/libm.md).
#
#   scripts/update-baselines.sh            # this build's profile (Apple libm on macOS)
#   scripts/update-baselines.sh --portable # *.portable.json, via --features gv-core/portable-math
#
# gv-diff picks the profile's file itself, so the paths below are the Apple names in both cases.
# Only refresh after an improvement: the ratchets fail when results drop below these files.
set -euo pipefail
cd "$(dirname "$0")/.."
features=()
if [[ "${1:-}" == "--portable" ]]; then
  features=(--features gv-core/portable-math)
fi
cargo build --release -p gv-diff "${features[@]+"${features[@]}"}"
gvdiff=target/release/gv-diff
"$gvdiff" --baseline tests/golden/baseline.json --update --worst 0
for e in neato sfdp circo twopi fdp osage patchwork; do
  "$gvdiff" --golden "tests/golden/$e" --baseline "tests/golden/$e-baseline.json" \
    --update --worst 0 "tests/$e"
done
"$gvdiff" render --baseline tests/golden/render/baseline.json --update
"$gvdiff" e2e --update
