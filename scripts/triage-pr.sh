#!/usr/bin/env bash
# First look at an inbound pull request, without building or running any of it.
#
#   scripts/triage-pr.sh 61
#
# Fetches the PR head into refs/pr/N (no checkout), then prints who sent it, what CI says and
# the risk report from scripts/pr-risk.py (this checkout's copy, not the PR's). Building the PR
# runs its build.rs files and tests on your machine: only do that once this report and your
# own read of the diff are clean (docs/pull-requests.md).
set -euo pipefail

n=${1:?usage: scripts/triage-pr.sh PR-NUMBER}
cd "$(git rev-parse --show-toplevel)"
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
base=$(gh pr view "$n" --json baseRefName -q .baseRefName)

git fetch -q origin "$base" "+refs/pull/$n/head:refs/pr/$n"
mb=$(git merge-base "origin/$base" "refs/pr/$n")
assoc=$(gh api "repos/$repo/pulls/$n" -q .author_association)

gh pr view "$n" --json number,title,url,author,isCrossRepository,maintainerCanModify,commits,closingIssuesReferences \
  -q '"# #\(.number) \(.title)\n\(.url)\n\nauthor: \(.author.login) ('"$assoc"', \(if .isCrossRepository then "fork" else "branch in this repo" end)); maintainers can push to it: \(.maintainerCanModify)\ncommits: \(.commits | length), authored as: \([.commits[].authors[].email] | unique | join(", "))\ncloses: \([.closingIssuesReferences[].number | "#\(.)"] | join(", ") | if . == "" then "nothing" else . end)"'

login=$(gh pr view "$n" --json author -q .author.login)
if [[ $login != app/* ]]; then
  since=$(date -u -v-30d +%Y-%m-%d 2>/dev/null || date -u -d '30 days ago' +%Y-%m-%d)
  gh api "users/$login" -q '"account: created \(.created_at[:10]), \(.public_repos) public repos, \(.followers) followers"'
  prs=$(gh api "search/issues?q=author:$login+type:pr+created:>=$since&per_page=1" -q .total_count)
  here=$(gh api "search/issues?q=author:$login+type:pr+repo:$repo&per_page=1" -q .total_count)
  echo "pull requests opened on GitHub in the last 30 days: $prs; to this repo ever: $here"
  if (( prs > 60 )); then
    echo "  -> more than two a day: likely an automated agent. Judge the change on its own; expect little follow-up."
  fi
fi

echo
echo "## CI"
# A fork's runs wait for a maintainer to approve them in the Actions tab before any check appears.
gh pr checks "$n" 2>&1 || true
echo

# Each new action pin must be the commit its comment names: `uses: owner/repo@SHA # vX.Y.Z`.
pins=$(git diff "$mb" "refs/pr/$n" -- .github | sed -n 's|^+ *-\{0,1\} *uses: *\([^@ ]*\)@\([0-9a-f]\{40\}\) *# *\(v[^ ]*\).*|\1 \2 \3|p' | sort -u)
if [[ -n $pins ]]; then
  echo "## Action pins"
  while read -r action sha tag; do
    upstream=$(cut -d/ -f1-2 <<<"$action") # github/codeql-action/upload-sarif lives in github/codeql-action
    got=$(git ls-remote "https://github.com/$upstream" "refs/tags/$tag" "refs/tags/$tag^{}" | tail -1 | cut -f1)
    if [[ $got == "$sha" ]]; then echo "- ok: $action $tag"; else echo "- MISMATCH: $action@$sha claims $tag, which is ${got:-missing}"; fi
  done <<<"$pins"
  echo
fi

python3 -I scripts/pr-risk.py "$mb" "refs/pr/$n"
