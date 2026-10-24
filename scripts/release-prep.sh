#!/usr/bin/env bash
# Prepares a release pull request from master (run by an agent; see docs/releasing.md):
#
# 1. collects the `## Changelog entry` of every pull request merged since the last release tag
#    (scripts/release-changelog.py) and refuses to go on while one has none, or while an entry
#    marked breaking comes from a pull request without the `breaking` label;
# 2. picks the version: minor (0.x) if a pull request in the range has the `breaking` label, else
#    patch. A VERSION given on the command line must be that version;
# 3. writes the entries under CHANGELOG.md's "Unreleased" heading and commits them, then runs
#    scripts/bump-version.sh VERSION --pr, which sets the version, dates the section, pushes
#    release/vVERSION and opens the pull request with $GH;
# 4. starts the release workflow's dry run and the fuzzing workflow on that branch (CI and the
#    cross-platform check start with the pull request).
#
# --dry-run prints the version, the CHANGELOG section and what is missing, and changes nothing
# (no label, no commit, no branch). Real runs switch this checkout to a detached origin/master
# first, so they need a clean working tree.
#
# Usage: scripts/release-prep.sh [VERSION] [--dry-run] [--intro TEXT]
#   GH     gh command that writes (default gh); CARGO  cargo command for bump-version.sh
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/release-lib.sh

version="" dry=0 intro=""
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) dry=1 ;;
    --intro) intro=${2:?--intro needs a text}; shift ;;
    -*) die "unknown option $1" ;;
    *) version=$1 ;;
  esac
  shift
done
[ -z "$version" ] || check_version "$version"

fetch_master
head=$(git rev-parse origin/master)
prev=$(prev_tag "$head" "${version:-none}")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

label=$(gh label list --repo "$REPO" --search breaking --json name -q '.[] | select(.name == "breaking") | .name')
[ -n "$label" ] || echo "note: the \`breaking\` label does not exist yet; a real run creates it" >&2

python3 scripts/release-changelog.py collect "$prev" "$head" > "$work/entries.json"
computed=$(python3 scripts/release-changelog.py next-version "$work/entries.json" "${prev#v}")
version=${version:-$computed}
render=(python3 scripts/release-changelog.py render "$work/entries.json" "$version")
[ -n "$intro" ] && render+=(--intro "$intro")
"${render[@]}" > "$work/section.md"

problems=0
report() {
  jq -r --arg prev "$prev" --arg head "${head:0:12}" '
    "Range: \($prev)..\($head), \(.entries | map(.pr) | unique | length) pull requests with entries, \(.none | length) declared not user-visible.",
    "Pull requests labelled breaking: \(if .breaking == [] then "none" else (.breaking | map("#\(.)") | join(", ")) end)",
    "Declared not user-visible: \(if .none == [] then "none" else (.none | map("#\(.pr)") | join(", ")) end)",
    (if .missing != [] then "\nNo changelog entry (add `## Changelog entry` to the pull request body):",
      (.missing[] | "- #\(.pr) \(.title)") else empty end),
    (if .unlabelled_breaking != [] then "\nEntries marked breaking without the `breaking` label (label them, or unmark the entry):",
      (.unlabelled_breaking[] | "- #\(.pr) \(.title)") else empty end),
    (if .breaking_without_entry != [] then "\nLabelled breaking with no Breaking bullet (check the entry says what breaks):",
      (.breaking_without_entry[] | "- #\(.pr) \(.title)") else empty end),
    (if .notes != [] then "\nProse beside the bullets, not copied (check it):",
      (.notes[] | "- #\(.pr): \(.notes | join(" / "))") else empty end),
    (if .without_pr != [] then "\nCommits without a pull request:", (.without_pr[] | "- \(.sha[:12]) \(.subject) (\(.reason))") else empty end)
  ' "$work/entries.json"
}
report
echo
echo "Version: $version (the labels ask for $computed, after ${prev#v})"
[ "$version" = "$computed" ] || { echo "error: $version is not the version the breaking labels ask for ($computed)" >&2; problems=1; }
[ "$(jq '.missing | length' "$work/entries.json")" = 0 ] || problems=1
[ "$(jq '.unlabelled_breaking | length' "$work/entries.json")" = 0 ] || problems=1
git rev-parse -q --verify "refs/tags/v$version" > /dev/null && { echo "error: v$version is already tagged" >&2; problems=1; }
echo
cat "$work/section.md"

if [ "$dry" = 1 ]; then
  echo
  echo "(dry run: nothing written; a real run would $([ "$problems" = 0 ] && echo "open the release pull request" || echo "stop at the problems above"))"
  exit 0
fi
[ "$problems" = 0 ] || die "fix the problems above, then run again"
[ -z "$(git status --porcelain)" ] || die "the working tree has uncommitted changes"
require_bot
# The checkout switches to master below, and bash reads a script as it runs it: the release
# scripts must already be master's.
git diff --quiet "$head" -- scripts/ || die "scripts/ differs from origin/master: run this from master's scripts"

if [ -z "$label" ]; then
  "${GH_CMD[@]}" label create breaking --repo "$REPO" --color B60205 \
    --description "Breaking change to an API, the command line or a format: bumps the minor version while 0.x"
fi
git switch --quiet --detach "$head"
python3 - "$work/section.md" <<'PY'
import re, sys
section = open(sys.argv[1]).read().split("\n", 1)[1]  # bump-version.sh writes the heading
log = open("CHANGELOG.md").read()
m = re.search(r"^## \[Unreleased\]\n(.*?)(?=^## \[)", log, re.M | re.S)
if not m:
    sys.exit("CHANGELOG.md has no Unreleased section")
if m.group(1).strip():
    sys.exit("CHANGELOG.md's Unreleased section is not empty: entries belong in pull request bodies")
log = log[:m.start(1)] + section.rstrip("\n") + "\n\n" + log[m.end(1):]
open("CHANGELOG.md", "w").write(log)
PY
git commit --quiet -am "Changelog for $version"
scripts/bump-version.sh "$version" --pr
branch=release/v$version
"${GH_CMD[@]}" workflow run release.yml --repo "$REPO" --ref "$branch"
"${GH_CMD[@]}" workflow run fuzz.yml --repo "$REPO" --ref "$branch"
pr=$(gh pr view "$branch" --repo "$REPO" --json number -q .number)
echo
echo "Opened #$pr; started the release dry run and fuzzing on $branch."
grep -qF -- "<!-- release-prep:" CHANGELOG.md && echo "Next: write the intro in CHANGELOG.md (the release-prep placeholder) and push once; a push does not restart the dry run or fuzzing: start release.yml and fuzz.yml on $branch again."
echo "Then: scripts/release-ready.sh $version $pr"
