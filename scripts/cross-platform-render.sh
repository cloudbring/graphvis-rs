#!/usr/bin/env bash
# Renders the cross-platform check's outputs for one platform into OUT:
#   platform.json   which platform, toolchain and math profile produced them
#   hashes.json     gv-diff e2e --hashes: the -Tsvg hash of every corpus input
#   samples/NAME.svg  dot-rs -Tsvg of each site gallery sample, byte for byte as written
#   samples/NAME.exit its exit status (C's: some inputs exit 1 with a drawing, like structs)
# scripts/cross-platform-compare.py then requires every platform's files to be identical.
#
# Usage: scripts/cross-platform-render.sh OUT ID LABEL [cargo args...]
#   e.g. scripts/cross-platform-render.sh out/macos macos-arm64 "macOS arm64" --features gv-core/portable-math
set -euo pipefail
OUT="$1"; ID="$2"; LABEL="$3"; shift 3
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p "$OUT/samples"
cargo build --release -q -p gv-cli -p gv-diff "$@"
EXE=""; case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) EXE=.exe ;; esac
# The run must show the portable profile, or it is not the build this check is about.
./target/release/gv-diff$EXE e2e --hashes "$OUT/hashes.json" | tee "$OUT/e2e.txt"
grep -q "math profile: portable" "$OUT/e2e.txt"
"${PYTHON:-python3}" - "$OUT" "$EXE" <<'PY'
import json, subprocess, sys
out, exe = sys.argv[1], sys.argv[2]
for s in json.load(open("site/src/data/samples.json")):
    src = f"site/public/samples/{s['name']}/{s['wasm_input']}"
    r = subprocess.run([f"target/release/dot-rs{exe}", f"-K{s['engine']}", "-Tsvg", src], capture_output=True)
    # An exit status of 1 can come with a valid drawing, as in C, so only empty output fails.
    if not r.stdout:
        sys.exit(f"{s['name']}: dot-rs wrote no output (exit {r.returncode}): {r.stderr.decode(errors='replace')}")
    open(f"{out}/samples/{s['name']}.svg", "wb").write(r.stdout)
    open(f"{out}/samples/{s['name']}.exit", "w").write(f"{r.returncode}\n")
PY
"${PYTHON:-python3}" - "$OUT" "$ID" "$LABEL" <<'PY'
import json, platform, subprocess, sys
out, pid, label = sys.argv[1:4]
rustc = subprocess.run(["rustc", "-V"], capture_output=True, text=True).stdout.strip()
json.dump({"id": pid, "label": label, "os": platform.system(), "arch": platform.machine(),
           "runtime": rustc, "build": "native"}, open(f"{out}/platform.json", "w"), indent=1)
PY
echo "wrote $OUT"
