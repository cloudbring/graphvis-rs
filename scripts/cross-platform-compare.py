#!/usr/bin/env python3
"""Compares the cross-platform check's outputs (scripts/cross-platform-render.sh,
scripts/cross-platform-wasm.mjs): every platform directory under ROOT must hold the same corpus
hashes and byte-identical sample SVGs.

Writes ROOT/manifest.json (platforms, per-platform agreement, sample hashes) for the site's
Cross-platform page, prints a Markdown summary (for $GITHUB_STEP_SUMMARY), and exits 1 if any
output differs between platforms.

Usage: scripts/cross-platform-compare.py ROOT [--commit SHA] [--run-url URL]
"""
import argparse
import hashlib
import json
import os
import sys


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("root")
    ap.add_argument("--commit", default="")
    ap.add_argument("--run-url", default="")
    a = ap.parse_args()

    dirs = sorted(d for d in os.listdir(a.root) if os.path.isfile(os.path.join(a.root, d, "platform.json")))
    if len(dirs) < 2:
        sys.exit(f"need at least two platform directories in {a.root}, found {dirs}")
    plats = [json.load(open(os.path.join(a.root, d, "platform.json"))) | {"dir": d} for d in dirs]
    hashes = {d: json.load(open(os.path.join(a.root, d, "hashes.json"))) for d in dirs}
    ref = dirs[0]

    keys = sorted(set().union(*hashes.values()))
    corpus_diff = []
    for k in keys:
        vals = {d: hashes[d].get(k) for d in dirs}
        if len(set(map(str, vals.values()))) > 1:
            corpus_diff.append({"file": k, "hashes": vals})

    names = sorted({f[:-4] for d in dirs for f in os.listdir(os.path.join(a.root, d, "samples")) if f.endswith(".svg")})
    samples, sample_diff = [], []
    for n in names:
        sha = {}
        for d in dirs:
            p = os.path.join(a.root, d, "samples", f"{n}.svg")
            sha[d] = hashlib.sha256(open(p, "rb").read()).hexdigest() if os.path.exists(p) else None
        same = len(set(sha.values())) == 1 and None not in sha.values()
        samples.append({"name": n, "sha256": sha, "identical": same})
        if not same:
            sample_diff.append(n)

    for p in plats:
        d = p["dir"]
        p["corpus_files"] = len(hashes[d])
        p["corpus_agree"] = sum(1 for k in keys if str(hashes[d].get(k)) == str(hashes[ref].get(k)))
    manifest = {
        "commit": a.commit,
        "run_url": a.run_url,
        "reference": ref,
        "platforms": plats,
        "corpus_files": len(keys),
        "corpus_differences": corpus_diff,
        "samples": samples,
        "identical": not corpus_diff and not sample_diff,
    }
    json.dump(manifest, open(os.path.join(a.root, "manifest.json"), "w"), indent=1)

    print("## Cross-platform output\n")
    print(f"{len(keys)} corpus inputs (`-Tsvg` hash of every graph) and {len(names)} gallery samples (full SVG bytes).\n")
    print("| platform | runtime | corpus inputs equal to " + ref + " | samples |")
    print("|---|---|---:|---:|")
    for p in plats:
        same_s = sum(1 for s in samples if s["sha256"][p["dir"]] == s["sha256"][ref])
        print(f"| {p['label']} | {p['runtime']} | {p['corpus_agree']} / {len(keys)} | {same_s} / {len(names)} |")
    if manifest["identical"]:
        print("\n**Every platform produced byte-identical output.**")
        return 0
    print(f"\n**Differences:** {len(corpus_diff)} corpus inputs, {len(sample_diff)} samples.")
    for c in corpus_diff[:30]:
        print(f"- `{c['file']}`")
    for n in sample_diff:
        print(f"- sample `{n}`")
    return 1


if __name__ == "__main__":
    sys.exit(main())
