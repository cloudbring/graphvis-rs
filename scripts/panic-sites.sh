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
# --force-warn counts a site even where the source allows its lint (or clippy::restriction).
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
  --force-warn clippy::unwrap_used --force-warn clippy::expect_used \
  --force-warn clippy::indexing_slicing --force-warn clippy::arithmetic_side_effects >"$json"

# One row per file: module, unwrap, expect, index, arith. A site is its whole span, so the two
# indexes of v[i][j] count twice, and a site clippy reports twice (a macro used once) counts once.
# The module is the file's path under crates/, without the package prefix and src/.
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
    | [$kind, .file_name, .line_start, .column_start, .line_end, .column_end] | @tsv' "$json" |
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

# A kind counting nothing means the count broke, not that every site of that kind is gone: a
# renamed crate, a lint clippy no longer knows, or a new JSON format.
broken=$(jq -r 'select(.reason == "compiler-message")
  | select(.message.code.code // "" | test("^(E0602|unknown_lints|renamed_and_removed_lints)$"))
  | .message.rendered' "$json")
empty=$(tail -n +2 "$current" | awk -F'\t' '
  { for (k = 2; k <= 5; k++) n[k] += $k }
  END { split("unwrap expect index arith", kind, " ")
        for (k = 2; k <= 5; k++) if (n[k] == 0) printf " %s", kind[k - 1] }')
if [[ -n "$broken$empty" ]]; then
  echo "panic-sites: the count is broken ($(cargo clippy --version)):" >&2
  [[ -z "$empty" ]] || echo "  no site of kind:$empty" >&2
  [[ -z "$broken" ]] || echo "$broken" >&2
  exit 1
fi

if $update; then
  cp "$current" "$baseline"
  echo "wrote $baseline"
  exit 0
fi

# Every count above its baseline; a module missing from the baseline counts as zero.
if [[ "$(head -n 1 "$baseline")" != "$(head -n 1 "$current")" ]]; then
  echo "panic-sites: the first line of $baseline is not the header 'module<TAB>unwrap<TAB>expect<TAB>index<TAB>arith'" >&2
  exit 1
fi
risen=$(awk -F'\t' '
  FILENAME == ARGV[1] { if (FNR > 1) for (k = 2; k <= 5; k++) base[$1, k] = $k; next }
  FNR == 1 { for (k = 2; k <= 5; k++) kind[k] = $k; next }
  { for (k = 2; k <= 5; k++) if ($k > base[$1, k] + 0)
      printf "  %s: %s %d -> %d\n", $1, kind[k], base[$1, k] + 0, $k }' "$baseline" "$current")
if [[ -n "$risen" ]]; then
  echo
  echo "panic sites went up (baseline: $baseline):"
  echo "$risen"
  echo "($(cargo clippy --version); a new clippy can count differently, see the header of this script)"
  echo "Return an error where C would crash (docs/porting/crashes.md), or follow the C's own check."
  exit 1
fi
if ! cmp -s "$baseline" "$current"; then
  echo
  echo "No count went up. Some went down: run scripts/panic-sites.sh --update to lock that in."
fi
