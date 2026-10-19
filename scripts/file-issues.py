#!/usr/bin/env python3
"""Files the issues in docs/issues/manifest.json on GitHub and adds them to the project.

For each manifest entry not yet in docs/issues/filed.json it creates the issue (labels from
the manifest), adds it to the GitHub Project, sets the Status, Engine, Category, Severity,
Source, Cases and Fuzz seeds fields, and closes it as "not planned" when the entry has
`close_as_wontfix`. Relative `tests/fuzz-open/...` paths in the body become links to the
default branch, and `{{SITE}}` becomes the documentation site's URL. filed.json maps each
slug to its issue URL, so the script can be re-run safely after a partial failure.

Usage: scripts/file-issues.py [--dry-run] [--repo OWNER/NAME] [--project N] [--owner LOGIN]
"""
import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
MANIFEST = os.path.join(ROOT, "docs", "issues", "manifest.json")
FILED = os.path.join(ROOT, "docs", "issues", "filed.json")


def gh(*args, capture=True):
    r = subprocess.run(["gh", *args], capture_output=capture, text=True)
    if r.returncode != 0:
        sys.exit(f"gh {' '.join(args[:3])} failed: {r.stderr.strip()}")
    return r.stdout.strip()


def body_for(entry, repo, branch, site):
    body = entry["body"].replace("{{SITE}}", site)
    tree = f"https://github.com/{repo}/tree/{branch}/"
    # Bare or backticked tests/fuzz-open/... paths outside code fences become links.
    path = re.compile(r"(?<![\[/(\w])`?(tests/fuzz-open/[\w.-]+(?:/[\w.-]+)*/?)`?")
    parts = body.split("```")
    for i in range(0, len(parts), 2):
        parts[i] = path.sub(lambda m: f"[`{m.group(1)}`]({tree}{m.group(1)})", parts[i])
    return "```".join(parts)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--repo", default="cloudbring/graphvis-rs")
    ap.add_argument("--branch", default="master")
    ap.add_argument("--owner", default="cloudbring")
    ap.add_argument("--project", default="4")
    ap.add_argument("--site", default="https://cloudbring.github.io/graphvis-rs")
    a = ap.parse_args()

    manifest = json.load(open(MANIFEST))
    filed = json.load(open(FILED)) if os.path.exists(FILED) else {}
    todo = [e for e in manifest if e["slug"] not in filed]
    print(f"{len(manifest)} issues in the manifest, {len(filed)} already filed, {len(todo)} to file")
    if a.dry_run:
        for e in todo:
            print(f"  {e['slug']}: {e['title']}  {e['labels']}")
        if todo:
            print("\n--- first body ---\n" + body_for(todo[0], a.repo, a.branch, a.site))
        return

    project_id = gh("project", "view", a.project, "--owner", a.owner, "--format", "json", "--jq", ".id")
    fields = {f["name"]: f for f in json.loads(gh("project", "field-list", a.project, "--owner", a.owner, "--format", "json"))["fields"]}

    def option(field, name):
        for o in fields[field].get("options", []):
            if o["name"] == name:
                return o["id"]
        sys.exit(f"project field {field} has no option {name!r}")

    def set_field(item, field, **value):
        args = ["project", "item-edit", "--id", item, "--project-id", project_id, "--field-id", fields[field]["id"]]
        for k, v in value.items():
            args += [f"--{k.replace('_', '-')}", str(v)]
        gh(*args)

    for e in todo:
        url = gh(
            "issue", "create", "--repo", a.repo, "--title", e["title"],
            "--body", body_for(e, a.repo, a.branch, a.site),
            *sum((["--label", l] for l in e["labels"]), []),
        )
        item = gh("project", "item-add", a.project, "--owner", a.owner, "--url", url, "--format", "json", "--jq", ".id")
        wontfix = e.get("close_as_wontfix", False)
        set_field(item, "Status", single_select_option_id=option("Status", "Won't fix" if wontfix else "Todo"))
        set_field(item, "Engine", single_select_option_id=option("Engine", e["engine"]))
        set_field(item, "Category", single_select_option_id=option("Category", e["category"]))
        set_field(item, "Severity", single_select_option_id=option("Severity", e["severity"]))
        set_field(item, "Source", single_select_option_id=option("Source", "upstream C" if e.get("upstream_c") else "port bug"))
        set_field(item, "Cases", number=e.get("case_count", 0))
        if e.get("seeds"):
            set_field(item, "Fuzz seeds", text=", ".join(str(s) for s in e["seeds"])[:1000])
        if wontfix:
            gh("issue", "close", url, "--repo", a.repo, "--reason", "not planned",
               "--comment", "Documented here and on the site; the port deliberately does not (or cannot) match C in this case. Closing as won't fix.")
        filed[e["slug"]] = url
        with open(FILED, "w") as f:
            json.dump(filed, f, indent=2, sort_keys=True)
            f.write("\n")
        print(f"  {e['slug']}: {url}{' (closed)' if wontfix else ''}")


if __name__ == "__main__":
    main()
