#!/usr/bin/env bash
# Prepares a release: sets VERSION in Cargo.toml (the workspace version and the version
# requirements of the workspace's own crates), web/package.json, jsr.json and Cargo.lock, and moves the
# "Unreleased" entries of CHANGELOG.md under "## [VERSION] - today". With --pr it commits that
# on a release/vVERSION branch and opens the pull request. See docs/versioning.md.
#
# Usage: scripts/bump-version.sh VERSION [--pr]
set -euo pipefail
version="${1:?usage: scripts/bump-version.sh VERSION [--pr]}"
pr="${2:-}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "not a SemVer version: $version" >&2; exit 1; }
cd "$(dirname "$0")/.."
old=$(awk -F'"' '/^\[workspace.package\]/{p=1} p && /^version =/{print $2; exit}' Cargo.toml)
[ -n "$(git status --porcelain)" ] && { echo "the working tree has uncommitted changes" >&2; exit 1; }
grep -q '^## \[Unreleased\]' CHANGELOG.md || { echo "CHANGELOG.md has no Unreleased section" >&2; exit 1; }

python3 - "$old" "$version" <<'PY'
import datetime, json, re, sys
old, new = sys.argv[1], sys.argv[2]
toml = open("Cargo.toml").read()
toml = re.sub(r'(\[workspace\.package\][^\[]*?\nversion = ")[^"]+(")', rf"\g<1>{new}\g<2>", toml, count=1)
toml = re.sub(r'(graphviz-pure[\w-]* = \{ path = "[^"]+", version = ")[^"]+(")', rf"\g<1>{new}\g<2>", toml)
open("Cargo.toml", "w").write(toml)
for path in ("web/package.json", "jsr.json"):
    pkg = json.load(open(path))
    pkg["version"] = new
    open(path, "w").write(json.dumps(pkg, indent=2) + "\n")
log = open("CHANGELOG.md").read()
today = datetime.date.today().isoformat()
log = log.replace("## [Unreleased]\n", f"## [Unreleased]\n\n## [{new}] - {today}\n", 1)
base = "https://github.com/graphviz-pure/graphviz-pure"
links = re.compile(r"^\[Unreleased\]: .*$", re.M)
prev = re.search(r"^\[(\d[^\]]*)\]: ", log, re.M)
log = links.sub(
    f"[Unreleased]: {base}/compare/v{new}...HEAD\n"
    + (f"[{new}]: {base}/compare/v{prev.group(1)}...v{new}" if prev else f"[{new}]: {base}/releases/tag/v{new}"),
    log, count=1)
open("CHANGELOG.md", "w").write(log)
PY
cargo update --workspace --quiet
echo "version $old -> $version"
git --no-pager diff --stat

if [ "$pr" = --pr ]; then
  git switch -c "release/v$version"
  git commit -am "Release $version"
  git push -u origin "release/v$version"
  gh pr create --base master --title "Release $version" \
    --body "Sets the version to $version and dates the changelog. After merging, tag the merge commit: \`git tag -s v$version -m v$version && git push origin v$version\` (docs/releasing.md)."
fi
