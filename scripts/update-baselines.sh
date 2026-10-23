#!/usr/bin/env bash
# Refreshes every corpus ratchet baseline (tests/golden/*.portable.json; see docs/porting/libm.md).
#
#   scripts/update-baselines.sh
#
# Every build computes with the same (portable) math functions, so one set of baselines serves
# every platform. Only refresh after an improvement: the ratchets fail when results drop below
# these files.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p graphviz-pure-diff
gvdiff=target/release/gv-diff
"$gvdiff" --baseline tests/golden/baseline.portable.json --update --worst 0
for e in neato sfdp circo twopi fdp osage patchwork; do
  "$gvdiff" --golden "tests/golden/$e" --baseline "tests/golden/$e-baseline.portable.json" \
    --update --worst 0 "tests/$e"
done
"$gvdiff" render --baseline tests/golden/render/baseline.portable.json --update
"$gvdiff" e2e --update
# The informational record against stock C on macOS (Apple's libm); never a check.
"$gvdiff" e2e --stock --update
