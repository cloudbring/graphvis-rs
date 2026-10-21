#!/usr/bin/env bash
# Skips CI work already done for the same source tree.
#
#   ci-result-cache.sh lookup KIND   writes hit=true|false, run_id and tree to $GITHUB_OUTPUT
#   ci-result-cache.sh name KIND     prints the marker artifact's name for this tree
#
# A workflow that passes uploads an artifact named pass-KIND-TREE (TREE is `git rev-parse
# HEAD^{tree}`, so it names the files tested, not the commit). A later run of the same tree finds
# it and skips the heavy jobs: a squash merge of an up-to-date pull request gives master the tree
# its pull_request run tested, so master does not test it again. Artifacts are repository-wide
# (the Actions cache is per branch, and master cannot read a pull request's cache entries).
# Only markers uploaded by runs of this repository count, never a fork's pull request run.
#
# Needs GH_TOKEN (actions: read), GITHUB_REPOSITORY and GITHUB_REPOSITORY_ID.
set -euo pipefail

cmd=${1:?usage: ci-result-cache.sh lookup|name KIND}
kind=${2:?usage: ci-result-cache.sh lookup|name KIND}
tree=$(git rev-parse 'HEAD^{tree}')
name="pass-$kind-$tree"

case "$cmd" in
  name)
    echo "$name"
    ;;
  lookup)
    run_id=$(gh api "repos/$GITHUB_REPOSITORY/actions/artifacts?name=$name&per_page=100" \
      --jq "[.artifacts[] | select(.expired | not)
             | select(.workflow_run.head_repository_id == $GITHUB_REPOSITORY_ID)]
            | first | .workflow_run.id // empty")
    if [ -n "$run_id" ]; then
      echo "Tree $tree already passed in run $run_id; skipping the jobs it covers."
      { echo "hit=true"; echo "run_id=$run_id"; } >> "$GITHUB_OUTPUT"
    else
      echo "No earlier run passed tree $tree."
      { echo "hit=false"; echo "run_id="; } >> "$GITHUB_OUTPUT"
    fi
    echo "tree=$tree" >> "$GITHUB_OUTPUT"
    ;;
  *)
    echo "unknown command: $cmd" >&2
    exit 2
    ;;
esac
