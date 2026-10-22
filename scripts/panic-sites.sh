#!/usr/bin/env bash
# The panic-site ratchet (see docs/porting/crashes.md): counts, per source file of the layout and
# geom crates, the sites that panic on data C trusts, and fails when any count goes up.
#
#   scripts/panic-sites.sh            # print the counts and compare them with the baseline
#   scripts/panic-sites.sh --update   # rewrite the baseline from this tree
#
# A site is one clippy warning, from the library code only (no tests):
#   unwrap  clippy::unwrap_used             .unwrap() on an Option or Result
#   expect  clippy::expect_used             .expect(..)
#   index   clippy::indexing_slicing        v[i] or v[a..b] that can go out of bounds
#   arith   clippy::arithmetic_side_effects integer + - * / % << >> that can overflow or divide by 0
# The lints are passed on the command line for this run only; they are not on in the workspace.
#
# The check fails when one module's count of one kind is higher than in tests/panic-sites.tsv, or
# when a module that is not listed has any site. A count going down passes; run --update to lock
# it in. A new clippy release can count some construct differently: refresh with --update then,
# and say so in the pull request.
set -euo pipefail
cd "$(dirname "$0")/.."

baseline=tests/panic-sites.tsv
crates=(graphviz-pure-layout graphviz-pure-geom)
update=false
case "${1:-}" in
  --update) update=true ;;
  "") ;;
  *) echo "usage: scripts/panic-sites.sh [--update]" >&2; exit 2 ;;
esac

json=$(mktemp)
current=$(mktemp)
trap 'rm -f "$json" "$current"' EXIT

pkgs=()
for c in "${crates[@]}"; do pkgs+=(-p "$c"); done
cargo clippy --quiet "${pkgs[@]}" --lib --message-format=json -- \
  -W clippy::unwrap_used -W clippy::expect_used -W clippy::indexing_slicing \
  -W clippy::arithmetic_side_effects >"$json"

# One row per file: module, unwrap, expect, index, arith. A site is counted once even if clippy
# reports it twice. The module is the file's path under crates/, without the package prefix and src/.
{
  printf 'module\tunwrap\texpect\tindex\tarith\n'
  jq -r '
    select(.reason == "compiler-message")
    | .message as $m
    | ({"clippy::unwrap_used": "unwrap", "clippy::expect_used": "expect",
        "clippy::indexing_slicing": "index", "clippy::arithmetic_side_effects": "arith"}
       [$m.code.code // ""]) as $kind
    | select($kind != null)
    | $m.spans[] | select(.is_primary)
    | select(.file_name | test("^crates/graphviz-pure-(layout|geom)/src/"))
    | [$kind, .file_name, .line_start, .column_start] | @tsv' "$json" |
    sort -u |
    awk -F'\t' '
      { f = $2; sub(/^crates\/graphviz-pure-/, "", f); sub(/\/src\//, "/", f)
        n[f, $1]++; files[f] = 1 }
      END { for (f in files)
              printf "%s\t%d\t%d\t%d\t%d\n", f, n[f, "unwrap"], n[f, "expect"], n[f, "index"], n[f, "arith"] }' |
    LC_ALL=C sort
} >"$current"

# The table, most sites first.
{
  printf 'module\tunwrap\texpect\tindex\tarith\ttotal\n'
  tail -n +2 "$current" | awk -F'\t' -v OFS='\t' '{ print $0, $2 + $3 + $4 + $5 }' |
    LC_ALL=C sort -t$'\t' -k6,6nr -k1,1
  tail -n +2 "$current" | awk -F'\t' -v OFS='\t' '
    { u += $2; e += $3; i += $4; a += $5 } END { print "TOTAL", u, e, i, a, u + e + i + a }'
} | column -t -s$'\t'

if $update; then
  cp "$current" "$baseline"
  echo "wrote $baseline"
  exit 0
fi

# Every count above its baseline; a module missing from the baseline counts as zero.
risen=$(awk -F'\t' '
  NR == FNR { if (FNR > 1) for (k = 2; k <= 5; k++) base[$1, k] = $k; next }
  FNR == 1 { for (k = 2; k <= 5; k++) kind[k] = $k; next }
  { for (k = 2; k <= 5; k++) if ($k > base[$1, k] + 0)
      printf "  %s: %s %d -> %d\n", $1, kind[k], base[$1, k] + 0, $k }' "$baseline" "$current")
if [[ -n "$risen" ]]; then
  echo
  echo "panic sites went up (baseline: $baseline):"
  echo "$risen"
  echo "Return an error where C would crash (docs/porting/crashes.md), or follow the C's own check."
  exit 1
fi
if ! cmp -s "$baseline" "$current"; then
  echo
  echo "No count went up. Some went down: run scripts/panic-sites.sh --update to lock that in."
fi
