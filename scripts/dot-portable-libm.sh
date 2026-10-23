#!/usr/bin/env bash
# Runs C Graphviz `dot` with graphviz-pure's portable math functions in place of the system libm.
#
# Every build of the port computes with the same portable math functions
# (`graphviz_pure_core::math::portable`), never a platform libm. This script gives C the same
# functions, by loading crates/graphviz-pure-portable-libm into dot, so that the two can be
# compared with the math library taken out of the equation. It is the C reference of the corpus
# checks (`gv-diff e2e`, `scripts/gen-golden.sh`):
#
#   cargo build --release -p graphviz-pure-portable-libm
#   GVBINDIR=~/src/gv-ref/lutplugins cargo run --release -p graphviz-pure-diff -- e2e --generate --runs 5
#   cargo run --release -p graphviz-pure-diff -- e2e
#
# On macOS the library is loaded with DYLD_INSERT_LIBRARIES and interposes libm; on Linux,
# with LD_PRELOAD (scripts/c-oracle-linux/). The variable is set here rather than inherited
# because macOS strips DYLD_* variables when it starts a protected program such as /bin/bash.
# GV_PORTABLE_LIBM overrides the library's path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
case "$(uname -s)" in
  Darwin) LIB="${GV_PORTABLE_LIBM:-$TARGET/release/libgraphviz_pure_portable_libm.dylib}"; VAR=DYLD_INSERT_LIBRARIES ;;
  *)      LIB="${GV_PORTABLE_LIBM:-$TARGET/release/libgraphviz_pure_portable_libm.so}"; VAR=LD_PRELOAD ;;
esac
[ -f "$LIB" ] || { echo "missing $LIB; run cargo build --release -p graphviz-pure-portable-libm" >&2; exit 2; }
env "$VAR=$LIB" dot "$@"
