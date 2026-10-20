#!/usr/bin/env bash
# Fetches the Cross-platform workflow's results (the cross-platform-site artifact) into
# site/public/cross-platform/ for the site's Cross-platform page. pages.yml does this in CI;
# run it by hand for a local preview.
#
# Usage: scripts/site-cross-platform.sh [RUN_ID]   (default: the latest run on master)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="${GITHUB_REPOSITORY:-cloudbring/graphvis-rs}"
RUN="${1:-$(gh run list -R "$REPO" --workflow cross-platform.yml --branch master --status completed \
  --limit 1 --json databaseId --jq '.[0].databaseId')}"
[ -n "$RUN" ] || { echo "no Cross-platform run found" >&2; exit 1; }
OUT="$ROOT/site/public/cross-platform"
rm -rf "$OUT"
gh run download "$RUN" -R "$REPO" -n cross-platform-site -D "$OUT"
echo "site/public/cross-platform: run $RUN"
