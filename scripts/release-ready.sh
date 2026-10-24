#!/usr/bin/env bash
# Is VERSION ready to tag? Prints a Markdown checklist and exits 0 only if every item passes.
# The commit checked is the release pull request's head while it is open, its merge commit once it
# is merged, and master's head without a PR. CI on the pull request's head also counts for its
# merge commit when the two have the same tree. Reads only; see docs/releasing.md.
#
#   - no open priority:p1 or priority:p2 issue, and no open "Cross-platform fails on master";
#   - ci.yml, cross-platform.yml and fuzz.yml succeeded on the commit;
#   - a workflow_dispatch run of release.yml (the dry run, which publishes nothing) succeeded on it;
#   - every pull request since the last release has a changelog entry, the breaking labels ask for
#     VERSION, and CHANGELOG.md has VERSION's section with its intro written;
#   - Cargo.toml, web/package.json and web/jsr.json carry VERSION, and vVERSION is not tagged yet;
#   - the e2e fidelity count (tests/golden/e2e-baseline.libm-matched.portable.json) is at least
#     the last release's.
#
# Usage: scripts/release-ready.sh VERSION [PR]
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/release-lib.sh

version=${1:?usage: scripts/release-ready.sh VERSION [PR]}
pr=${2:-}
check_version "$version"
fetch_master
if [ -n "$pr" ]; then
  git fetch --quiet "$REMOTE_URL" "+refs/pull/$pr/head:refs/remotes/origin/pr/$pr"
  read -r state head merge < <(gh pr view "$pr" --repo "$REPO" --json state,headRefOid,mergeCommit \
    -q '"\(.state) \(.headRefOid) \(.mergeCommit.oid // "-")"')
  case "$state" in
    MERGED) target=$merge what="the merge commit of #$pr" ;;
    OPEN) target=$head what="the head of #$pr" ;;
    *) die "#$pr is $state" ;;
  esac
else
  target=$(git rev-parse origin/master) what="master's head"
fi
shas=()
while read -r sha; do shas+=("$sha"); done < <(candidates "$target" "$pr")
prev=$(prev_tag "$target" "$version")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fails=0
item() { # item PASS TEXT [DETAIL]
  if [ "$1" = 1 ]; then echo "- [x] $2"; else echo "- [ ] $2${3:+ — $3}"; fails=$((fails + 1)); fi
}

echo "## Release $version: readiness of ${target:0:12} ($what)"
echo

# Open issues that block a release.
for p in p1 p2; do
  open=$(gh issue list --repo "$REPO" --state open --label "priority:$p" --limit 200 --json number -q 'map("#\(.number)") | join(", ")')
  item "$([ -z "$open" ] && echo 1)" "No open \`priority:$p\` issue" "$open"
done
xp=$(gh issue list --repo "$REPO" --state open --search 'in:title "Cross-platform fails on master"' --json number,title \
  -q 'map(select(.title == "Cross-platform fails on master") | "#\(.number)") | join(", ")')
item "$([ -z "$xp" ] && echo 1)" "No open \"Cross-platform fails on master\" issue" "$xp"

# Workflows on the commit (or its twin): the newest run must have succeeded.
for wf in ci.yml cross-platform.yml fuzz.yml release.yml; do
  event="" label="\`$wf\` succeeded"
  [ "$wf" = release.yml ] && event=workflow_dispatch label="The release dry run (\`release.yml\`, workflow_dispatch) succeeded"
  best="" detail="no run on ${target:0:12}"
  for sha in "${shas[@]}"; do
    run=$(latest_run "$wf" "$sha" "$event")
    [ -n "$run" ] || continue
    read -r status conclusion url <<< "$run"
    if [ "$status" = completed ] && [ "$conclusion" = success ]; then best=$url; break; fi
    detail="$status/$conclusion: $url"
  done
  if [ -n "$best" ]; then item 1 "$label: $best"; else item 0 "$label" "$detail"; fi
done

# Changelog entries since the last release, and the version they ask for.
exclude=()
[ -n "$pr" ] && exclude=(--exclude "$pr")
python3 scripts/release-changelog.py collect "$prev" "$target" "${exclude[@]}" > "$work/entries.json"
missing=$(jq -r '.missing | map("#\(.pr)") | join(", ")' "$work/entries.json")
count=$(jq '[.entries[].pr] + [.none[].pr] | unique | length' "$work/entries.json")
item "$([ -z "$missing" ] && echo 1)" "Every pull request since $prev has a changelog entry ($count with one)" "missing: $missing"
unlabelled=$(jq -r '.unlabelled_breaking | map("#\(.pr)") | join(", ")' "$work/entries.json")
computed=$(python3 scripts/release-changelog.py next-version "$work/entries.json" "${prev#v}")
breaking=$(jq -r '.breaking | map("#\(.)") | join(", ")' "$work/entries.json")
if [ -n "$unlabelled" ]; then
  item 0 "The \`breaking\` labels ask for $version" "breaking entries without the label: $unlabelled"
else
  item "$([ "$computed" = "$version" ] && echo 1)" "The \`breaking\` labels ask for $version (${breaking:-no breaking pull request})" "they ask for $computed"
fi
section=$(git show "$target:CHANGELOG.md" | awk -v v="## [$version]" 'index($0, v) == 1 {p=1; next} p && /^## \[/ {exit} p')
if [ -z "$section" ]; then
  item 0 "CHANGELOG.md has a $version section" "none at ${target:0:12}"
else
  intro=$(grep -cF -- "<!-- release-prep:" <<< "$section" || true)
  item "$([ "$intro" = 0 ] && echo 1)" "CHANGELOG.md has a $version section with its intro" "the intro placeholder is still there"
fi

# Versions.
cargo_v=$(cargo_version_at "$target")
npm_v=$(git show "$target:web/package.json" | jq -r .version)
jsr_v=$(git show "$target:web/jsr.json" | jq -r .version)
item "$([ "$cargo_v" = "$version" ] && [ "$npm_v" = "$version" ] && [ "$jsr_v" = "$version" ] && echo 1)" \
  "Cargo.toml, web/package.json and web/jsr.json say $version" "Cargo.toml $cargo_v, package.json $npm_v, jsr.json $jsr_v"
tagged=$(git rev-parse -q --verify "refs/tags/v$version" || true)
item "$([ -z "$tagged" ] && echo 1)" "v$version is not tagged yet" "it is, at ${tagged:0:12}"

# Fidelity: the number of e2e inputs byte-identical to C (same math) must not drop.
e2e=tests/golden/e2e-baseline.libm-matched.portable.json
now=$(git show "$target:$e2e" | jq '.identical | length')
then=$(git show "$prev:$e2e" 2> /dev/null | jq '.identical | length' || echo 0)
item "$([ "$now" -ge "$then" ] && echo 1)" "e2e fidelity: $now inputs identical to C ($then at $prev)" "it dropped"

echo
if [ "$fails" = 0 ]; then
  echo "**Ready:** tag ${target:0:12} as v$version."
else
  echo "**Not ready:** $fails item(s) open."
fi
[ "$fails" = 0 ]
