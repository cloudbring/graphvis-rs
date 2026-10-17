#!/usr/bin/env bash
# Regenerates tests/golden/*.dot from C Graphviz 16.1.0.
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
PLUGIN_SRC="${1:-/opt/homebrew/lib/graphviz}"
OUT="$REPO_ROOT/tests/golden"

version="$(dot -V 2>&1)"
case "$version" in
  *"version 16.1.0"*) ;;
  *) echo "need Graphviz 16.1.0, found: $version" >&2; exit 1 ;;
esac

PLUGINS="$(mktemp -d)"
trap 'rm -rf "$PLUGINS"' EXIT
for p in core dot_layout neato_layout; do
  cp "$PLUGIN_SRC"/libgvplugin_"$p".*dylib "$PLUGINS"/ 2>/dev/null ||
    cp "$PLUGIN_SRC"/libgvplugin_"$p".so* "$PLUGINS"/
done
GVBINDIR="$PLUGINS" dot -c
export GVBINDIR="$PLUGINS"

mkdir -p "$OUT"
count=0
for f in "$REPO_ROOT"/tests/wild/*.gv "$REPO_ROOT"/tests/fixtures/*.gv; do
  stem="$(basename "$f" .gv)"
  # C can exit non-zero yet still emit a layout (e.g. it drops an edge after a
  # Pshortestpath failure); that output is still the reference behaviour.
  dot -Tdot "$f" > "$OUT/$stem.dot" 2>/dev/null || echo "C dot reported errors on $f" >&2
  if [ -s "$OUT/$stem.dot" ]; then
    count=$((count + 1))
  else
    rm -f "$OUT/$stem.dot"
  fi
done
echo "wrote $count golden layouts to $OUT"
