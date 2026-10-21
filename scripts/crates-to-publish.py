#!/usr/bin/env python3
"""Which of the workspace's publishable crates still need VERSION published to crates.io.

Reads `cargo metadata --no-deps --format-version 1` output (METADATA) and asks crates.io's API
whether NAME@VERSION exists for each crate that can be published (publish is not false). Prints
GitHub Actions outputs:

    exclude=--exclude a --exclude b    (the crates already published, for cargo publish --workspace)
    remaining=N                        (how many still need publishing)

The release workflow's crates-io job uses it so that re-running the job after a partial publish
publishes only the rest. Python standard library only.

Usage: scripts/crates-to-publish.py METADATA VERSION
"""
import json
import sys
import urllib.error
import urllib.request

meta, version = json.load(open(sys.argv[1])), sys.argv[2]
crates = sorted(p["name"] for p in meta["packages"] if p["publish"] is None)
if not crates:
    sys.exit("no publishable crates in the metadata")
published = []
for name in crates:
    url = f"https://crates.io/api/v1/crates/{name}/{version}"
    # crates.io's crawler policy asks for a User-Agent that identifies the client.
    req = urllib.request.Request(url, headers={"User-Agent": "graphviz-pure release workflow (https://github.com/graphviz-pure/graphviz-pure)"})
    try:
        with urllib.request.urlopen(req, timeout=30):
            published.append(name)
    except urllib.error.HTTPError as e:
        if e.code != 404:
            raise
    print(f"{name} {version}: {'already published' if name in published else 'to publish'}", file=sys.stderr)
print("exclude=" + " ".join(f"--exclude {n}" for n in published))
print(f"remaining={len(crates) - len(published)}")
