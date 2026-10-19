#!/usr/bin/env bash
# Runs C Graphviz `dot` with graphvis-rs's portable math functions in place of the system libm.
#
# Off macOS (Linux, wasm32) the port cannot use Apple's closed-source sin, cos, pow and
# atan2, so it uses portable ones (`gv_core::math::portable`). This script gives C the same
# functions, by loading crates/gv-portable-libm into dot with DYLD_INSERT_LIBRARIES, so that
# the two can be compared with the math library taken out of the equation:
#
#   cargo build --release -p gv-portable-libm
#   GVBINDIR=~/src/gv-ref/lutplugins cargo run --release -p gv-diff -- e2e --generate \
#     --c-dot scripts/dot-portable-libm.sh --golden tests/golden/e2e.libm-matched.json
#   cargo run --release -p gv-diff --features gv-core/portable-math -- e2e \
#     --golden tests/golden/e2e.libm-matched.json --baseline tests/golden/e2e-baseline.libm-matched.json
#
# On macOS the library is loaded with DYLD_INSERT_LIBRARIES and interposes libm; on Linux,
# with LD_PRELOAD (scripts/c-oracle-linux/). The variable is set here rather than inherited
# because macOS strips DYLD_* variables when it starts a protected program such as /bin/bash.
# GV_PORTABLE_LIBM overrides the library's path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
case "$(uname -s)" in
  Darwin) LIB="${GV_PORTABLE_LIBM:-$TARGET/release/libgv_portable_libm.dylib}"; VAR=DYLD_INSERT_LIBRARIES ;;
  *)      LIB="${GV_PORTABLE_LIBM:-$TARGET/release/libgv_portable_libm.so}"; VAR=LD_PRELOAD ;;
esac
[ -f "$LIB" ] || { echo "missing $LIB; run cargo build --release -p gv-portable-libm" >&2; exit 2; }
env "$VAR=$LIB" dot "$@"
