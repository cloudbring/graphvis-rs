#!/usr/bin/env bash
# Packages the crates that the release publishes (publish unset, as in scripts/crates-to-publish.py)
# and checks their licence files with scripts/check-package-notices.py. --no-verify: nothing is
# compiled. CI runs it on every change and the release workflow before anything is published.
set -euo pipefail
cd "$(dirname "$0")/.."
# One line per publishable crate: -p NAME PATH-OF-ITS-.crate
list=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, os, sys
m = json.load(sys.stdin)
for p in m["packages"]:
    if p["publish"] is None:
        crate = "{}-{}.crate".format(p["name"], p["version"])
        print(p["name"], os.path.join(m["target_directory"], "package", crate))
')
test -n "$list" || { echo "error: no publishable crates" >&2; exit 1; }
args=() crates=()
while read -r name crate; do
  args+=(-p "$name")
  crates+=("$crate")
done <<< "$list"
cargo package --locked --no-verify "${args[@]}"
python3 scripts/check-package-notices.py "${crates[@]}"
