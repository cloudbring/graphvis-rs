#!/usr/bin/env python3
"""Downloads the raw GitHub Actions and pull request data that ci-stats/report.py reads.

    scripts/ci-stats/collect.py [--since YYYY-MM-DD] [--out DIR]

Writes, under DIR (default target/ci-stats):
  runs.jsonl   every workflow run created since the date, each with its jobs and their steps
  prs.jsonl    every pull request merged since the date, with its timeline: commits, reviews,
               merge queue entries and removals, and the merge

Completed runs and merged pull requests do not change, so a second collect only downloads
what the cache does not hold. Needs `gh` signed in with read access to the repository.
"""
import argparse
import concurrent.futures as cf
import datetime as dt
import json
import pathlib
import subprocess
import sys

REPO = "graphviz-pure/graphviz-pure"


def gh(*args, input=None):
    out = subprocess.run(["gh", *args], check=True, capture_output=True, text=True, input=input)
    return out.stdout


def gh_json(path):
    return json.loads(gh("api", path))


def paged(path, key):
    # gh --paginate with --jq prints one JSON value per line.
    out = gh("api", "--paginate", path, "--jq", f".{key}[] | tojson")
    return [json.loads(line) for line in out.splitlines() if line]


def load(path):
    if not path.exists():
        return {}
    return {r["id"]: r for r in map(json.loads, path.read_text().splitlines())}


def save(path, rows):
    path.write_text("".join(json.dumps(r, separators=(",", ":")) + "\n" for r in rows))


RUN_FIELDS = ("id", "name", "event", "status", "conclusion", "head_branch", "head_sha",
              "created_at", "run_started_at", "updated_at", "run_attempt", "html_url")
JOB_FIELDS = ("id", "name", "status", "conclusion", "created_at", "started_at", "completed_at",
              "runner_name", "labels")


def fetch_jobs(run):
    jobs = paged(f"repos/{REPO}/actions/runs/{run['id']}/jobs?filter=all&per_page=100", "jobs")
    run["jobs"] = [
        {**{k: j.get(k) for k in JOB_FIELDS},
         "steps": [{k: s.get(k) for k in ("name", "conclusion", "started_at", "completed_at")}
                   for s in j.get("steps") or []]}
        for j in jobs
    ]
    return run


def collect_runs(out, since):
    path = out / "runs.jsonl"
    cache = load(path)
    # The API returns at most 1000 runs per query, so ask one day at a time.
    runs = []
    day = since
    today = dt.date.today()
    while day <= today:
        runs += paged(f"repos/{REPO}/actions/runs?created={day.isoformat()}&per_page=100",
                      "workflow_runs")
        day += dt.timedelta(days=1)
    todo, keep = [], []
    for r in runs:
        r = {k: r.get(k) for k in RUN_FIELDS} | {
            "pull_requests": [p["number"] for p in r.get("pull_requests") or []]}
        old = cache.get(r["id"])
        if old and old["status"] == "completed" and old["run_attempt"] == r["run_attempt"]:
            keep.append(old)
        else:
            todo.append(r)
    print(f"runs: {len(runs)} ({len(keep)} cached, fetching jobs for {len(todo)})",
          file=sys.stderr)
    with cf.ThreadPoolExecutor(8) as pool:
        keep += pool.map(fetch_jobs, todo)
    keep.sort(key=lambda r: r["created_at"])
    save(path, keep)


PR_QUERY = """
query($q: String!, $after: String) {
  search(query: $q, type: ISSUE, first: 50, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes { ... on PullRequest {
      number title createdAt mergedAt headRefName author { login }
      additions deletions changedFiles
      closingIssuesReferences(first: 5) { nodes { number createdAt } }
      timelineItems(first: 200, itemTypes: [PULL_REQUEST_COMMIT, PULL_REQUEST_REVIEW,
          ADDED_TO_MERGE_QUEUE_EVENT, REMOVED_FROM_MERGE_QUEUE_EVENT, MERGED_EVENT,
          READY_FOR_REVIEW_EVENT, HEAD_REF_FORCE_PUSHED_EVENT]) {
        nodes {
          __typename
          ... on PullRequestCommit { commit { oid committedDate pushedDate } }
          ... on PullRequestReview { state submittedAt author { login } }
          ... on AddedToMergeQueueEvent { createdAt }
          ... on RemovedFromMergeQueueEvent { createdAt reason }
          ... on MergedEvent { createdAt }
          ... on ReadyForReviewEvent { createdAt }
          ... on HeadRefForcePushedEvent { createdAt afterCommit { oid } }
        }
      }
    } }
  }
}"""


def collect_prs(out, since):
    path = out / "prs.jsonl"
    cache = load(path)
    owner, name = REPO.split("/")
    q = f"repo:{REPO} is:pr is:merged merged:>={since.isoformat()}"
    prs, after = [], None
    while True:
        args = ["api", "graphql", "-f", f"query={PR_QUERY}", "-f", f"q={q}"]
        if after:
            args += ["-f", f"after={after}"]
        data = json.loads(gh(*args))["data"]["search"]
        for n in data["nodes"]:
            n["id"] = n["number"]
            n["timeline"] = n.pop("timelineItems")["nodes"]
            n["issues"] = n.pop("closingIssuesReferences")["nodes"]
            prs.append(n)
        if not data["pageInfo"]["hasNextPage"]:
            break
        after = data["pageInfo"]["endCursor"]
    for p in prs:
        cache[p["id"]] = p
    print(f"merged pull requests: {len(prs)}", file=sys.stderr)
    save(path, sorted(cache.values(), key=lambda p: p["mergedAt"]))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--since", type=dt.date.fromisoformat,
                    default=dt.date.today() - dt.timedelta(days=7))
    ap.add_argument("--out", type=pathlib.Path, default=pathlib.Path("target/ci-stats"))
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    collect_runs(a.out, a.since)
    collect_prs(a.out, a.since)


if __name__ == "__main__":
    main()
