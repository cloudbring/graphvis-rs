#!/usr/bin/env bash
# Broad dot validation beyond the corpus: lays out every graph of the Graphviz
# 16.1.0 sources (graphs/**, tests/graphs/**, tests/regression_tests/**) with C
# and with the port, in several variants, and writes both -Tdot outputs side by
# side so `gv-diff pairs` can compare them.
#
# Usage: scripts/dot-broad-validate.sh c|port [OUT_DIR] [TIMEOUT_SECONDS]
#   c     runs C dot (GVBINDIR=~/src/gv-ref/lutplugins: built-in text metrics)
#   port  runs target/release/dot-rs
# Outputs: OUT_DIR/<side>/<variant>/<stem>.dot, plus OUT_DIR/<side>/times.tsv
# (variant, stem, exit status, seconds). Then:
#   target/release/gv-diff pairs OUT_DIR/c/<variant> OUT_DIR/port/<variant>
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SIDE="${1:?c|port}"
OUT="${2:-/tmp/gv-broad}"
TMO="${3:-60}"
SRC="${GV_SRC:-$HOME/src/gv-ref/graphviz-16.1.0}"
JOBS="${JOBS:-6}"

case "$SIDE" in
  c) BIN="dot"; export GVBINDIR="${GVBINDIR:-$HOME/src/gv-ref/lutplugins}" ;;
  port) BIN="$REPO_ROOT/target/release/dot-rs" ;;
  *) echo "side must be c or port" >&2; exit 1 ;;
esac

mkdir -p "$OUT/$SIDE"
LIST="$OUT/inputs.txt"
if [ ! -s "$LIST" ]; then
  find "$SRC/graphs" "$SRC/tests/graphs" "$SRC/tests/regression_tests" \
    \( -name '*.gv' -o -name '*.dot' \) -type f | sort > "$LIST"
fi

# variant name and the -G flags it adds
VARIANTS=(
  "base:-q"
  "lr:-Grankdir=LR"
  "ortho:-Gsplines=ortho"
  "conc:-Gconcentrate=true"
  "newrank:-Gnewrank=true"
)

run_one() {
  # $1 variant, $2 flags, $3 input
  local v="$1" flags="$2" f="$3" rel stem out t0 t1 st
  rel="${f#"$SRC"/}"
  stem="$(echo "$rel" | sed 's/\.[^.]*$//; s#[/ ]#_#g')"
  out="$OUT/$SIDE/$v/$stem.dot"
  t0=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
  st=0
  # shellcheck disable=SC2086
  perl -e 'alarm shift; exec @ARGV or die' "$TMO" "$BIN" -Kdot -Tdot $flags "$f" \
    > "$out" 2>/dev/null || st=$?
  t1=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
  [ -s "$out" ] || rm -f "$out"
  printf '%s\t%s\t%s\t%s\n' "$v" "$stem" "$st" "$(echo "$t1 - $t0" | bc)"
}
export -f run_one
export OUT SIDE BIN TMO SRC

: > "$OUT/$SIDE/times.tsv"
for spec in "${VARIANTS[@]}"; do
  v="${spec%%:*}"; flags="${spec#*:}"
  mkdir -p "$OUT/$SIDE/$v"
  while IFS= read -r f; do printf '%s\0%s\0%s\0' "$v" "$flags" "$f"; done < "$LIST" |
    xargs -0 -n 3 -P "$JOBS" bash -c 'run_one "$@"' _ >> "$OUT/$SIDE/times.tsv"
done
echo "done: $(wc -l < "$OUT/$SIDE/times.tsv") runs in $OUT/$SIDE"
