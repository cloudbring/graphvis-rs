#!/usr/bin/env bash
# Renders the cross-platform check's outputs for one platform into OUT:
#   platform.json   which platform and toolchain produced them
#   hashes.json     gv-diff e2e --hashes: the -Tsvg hash of every corpus input
#   samples/NAME.svg  graphviz-pure -Tsvg of each site gallery sample, byte for byte as written
#   samples/NAME.png  its -Tpng (native platforms; must match too)
#   samples/NAME.exit its exit status (C's: some inputs exit 1 with a drawing, like structs)
# scripts/cross-platform-compare.py then requires every platform's files to be identical.
#
# Usage: scripts/cross-platform-render.sh OUT ID LABEL [cargo args...]
#   e.g. scripts/cross-platform-render.sh out/macos macos-arm64 "macOS arm64"
set -euo pipefail
OUT="$1"; ID="$2"; LABEL="$3"; shift 3
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p "$OUT/samples"
cargo build --release -q -p graphviz-pure-cli -p graphviz-pure-diff "$@"
EXE=""; case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) EXE=.exe ;; esac
./target/release/gv-diff$EXE e2e --hashes "$OUT/hashes.json" | tee "$OUT/e2e.txt"
"${PYTHON:-python3}" - "$OUT" "$EXE" <<'PY'
import json, subprocess, sys
out, exe = sys.argv[1], sys.argv[2]
for s in json.load(open("site/src/data/samples.json")):
    src = f"site/public/samples/{s['name']}/{s['wasm_input']}"
    r = subprocess.run([f"target/release/graphviz-pure{exe}", f"-K{s['engine']}", "-Tsvg", src], capture_output=True)
    # An exit status of 1 can come with a valid drawing, as in C, so only empty output fails.
    if not r.stdout:
        sys.exit(f"{s['name']}: graphviz-pure wrote no output (exit {r.returncode}): {r.stderr.decode(errors='replace')}")
    open(f"{out}/samples/{s['name']}.svg", "wb").write(r.stdout)
    open(f"{out}/samples/{s['name']}.exit", "w").write(f"{r.returncode}\n")
    # PNG too (graphviz-pure-raster): the bundled fonts make it the same bytes everywhere.
    png = subprocess.run([f"target/release/graphviz-pure{exe}", f"-K{s['engine']}", "-Tpng", src], capture_output=True).stdout
    if png:
        open(f"{out}/samples/{s['name']}.png", "wb").write(png)
PY
"${PYTHON:-python3}" - "$OUT" "$ID" "$LABEL" <<'PY'
import json, platform, subprocess, sys
out, pid, label = sys.argv[1:4]
rustc = subprocess.run(["rustc", "-V"], capture_output=True, text=True).stdout.strip()
json.dump({"id": pid, "label": label, "os": platform.system(), "arch": platform.machine(),
           "runtime": rustc, "build": "native"}, open(f"{out}/platform.json", "w"), indent=1)
PY
echo "wrote $OUT"
