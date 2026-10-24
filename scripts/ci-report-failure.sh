#!/usr/bin/env bash
# Reports a failed run that nobody waits on (master and scheduled runs): opens an issue, or
# comments on the open issue with the same title, so a failure that lasts across several merges
# stays one issue.
#
#   ci-report-failure.sh TITLE LABELS BODY     LABELS is comma-separated
#
# Needs GH_TOKEN (issues: write) and GH_REPO.
set -euo pipefail

title=${1:?usage: ci-report-failure.sh TITLE LABELS BODY}
labels=${2:?usage: ci-report-failure.sh TITLE LABELS BODY}
body=${3:?usage: ci-report-failure.sh TITLE LABELS BODY}

number=$(gh issue list --state open --search "in:title \"$title\"" --json number,title --limit 50 |
  jq -r --arg t "$title" 'map(select(.title == $t)) | first | .number // empty')
if [ -n "$number" ]; then
  gh issue comment "$number" --body "$body"
else
  gh issue create --title "$title" --label "$labels" --body "$body"
fi
