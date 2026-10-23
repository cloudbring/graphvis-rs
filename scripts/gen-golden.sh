#!/usr/bin/env bash
# Regenerates tests/golden/*.dot from C Graphviz 16.1.0.
#
# C computes with the port's own math functions: every layout runs through
# scripts/dot-portable-libm.sh, which interposes graphviz-pure-portable-libm into the stock
# `dot`, so the goldens differ from the port only where the port differs from C, never through
# the platform's libm (docs/porting/libm.md). Build it first:
#   cargo build --release -p graphviz-pure-portable-libm
# C_DOT overrides the C command (C_DOT=dot gives stock C, for comparison only).
#
# C must measure text the same way the port does, with the built-in font metric
# tables (textspan_lut.c) rather than pango/cairo. This script builds a plugin
# directory holding only the core, dot and neato layout plugins and points
# GVBINDIR at it, so no text-layout plugin is available and C falls back to
# estimate_textspan_size().
#
# Usage: scripts/gen-golden.sh [GRAPHVIZ_LIB_DIR]   (default: /opt/homebrew/lib/graphviz)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
C_DOT="${C_DOT:-$REPO_ROOT/scripts/dot-portable-libm.sh}"
PLUGIN_SRC="${1:-/opt/homebrew/lib/graphviz}"
OUT="$REPO_ROOT/tests/golden"

version="$(dot -V 2>&1)"
case "$version" in
  *"version 16.1.0"*) ;;
  *) echo "need Graphviz 16.1.0, found: $version" >&2; exit 1 ;;
esac

PLUGINS="$(mktemp -d)"
for p in core dot_layout neato_layout; do
  cp "$PLUGIN_SRC"/libgvplugin_"$p".*dylib "$PLUGINS"/ 2>/dev/null ||
    cp "$PLUGIN_SRC"/libgvplugin_"$p".so* "$PLUGINS"/
done
GVBINDIR="$PLUGINS" dot -c
export GVBINDIR="$PLUGINS"

RUNS="$(mktemp -d)"
trap 'rm -rf "$PLUGINS" "$RUNS"' EXIT

# gen_dir OUT_DIR INPUT...: lays out each input with C and writes OUT_DIR/<stem>.dot.
# Some layouts depend on heap addresses in C (fdp, twopi; see docs/porting/libm.md), so C runs
# three times and the output at least two runs agree on is kept (else the first run's), and the
# inputs whose runs disagree are listed.
gen_dir() {
  local out="$1"; shift
  mkdir -p "$out"
  local count=0 f stem r
  for f in "$@"; do
    stem="$(basename "$f" .gv)"
    # C can exit non-zero yet still emit a layout (e.g. it drops an edge after a
    # Pshortestpath failure); that output is still the reference behaviour.
    for r in 1 2 3; do
      "$C_DOT" -Tdot "$f" > "$RUNS/$r" 2>/dev/null || { [ "$r" = 1 ] && echo "C dot reported errors on $f" >&2; }
    done
    if cmp -s "$RUNS/1" "$RUNS/2" && cmp -s "$RUNS/1" "$RUNS/3"; then
      cp "$RUNS/1" "$out/$stem.dot"
    else
      echo "C unstable over 3 runs: ${f#"$REPO_ROOT"/}" >&2
      if cmp -s "$RUNS/2" "$RUNS/3"; then cp "$RUNS/2" "$out/$stem.dot"; else cp "$RUNS/1" "$out/$stem.dot"; fi
    fi
    if [ -s "$out/$stem.dot" ]; then
      count=$((count + 1))
    else
      rm -f "$out/$stem.dot"
    fi
  done
  echo "wrote $count golden layouts to $out"
}

# Main corpus: the engine is chosen by each graph's `layout` attribute (dot by default).
gen_dir "$OUT" "$REPO_ROOT"/tests/wild/*.gv "$REPO_ROOT"/tests/fixtures/*.gv
# Neato corpus: every input sets layout=neato (see scripts/gen-neato-corpus.py).
gen_dir "$OUT/neato" "$REPO_ROOT"/tests/neato/*.gv
# Sfdp corpus: every input sets layout=sfdp (see scripts/gen-sfdp-corpus.py).
gen_dir "$OUT/sfdp" "$REPO_ROOT"/tests/sfdp/*.gv
# Circo and twopi corpora: every input sets layout=circo|twopi (see scripts/gen-circo-corpus.py, gen-twopi-corpus.py).
gen_dir "$OUT/circo" "$REPO_ROOT"/tests/circo/*.gv
gen_dir "$OUT/twopi" "$REPO_ROOT"/tests/twopi/*.gv
# fdp, osage and patchwork corpora (scripts/gen-<engine>-corpus.py).
gen_dir "$OUT/fdp" "$REPO_ROOT"/tests/fdp/*.gv
gen_dir "$OUT/osage" "$REPO_ROOT"/tests/osage/*.gv
gen_dir "$OUT/patchwork" "$REPO_ROOT"/tests/patchwork/*.gv
