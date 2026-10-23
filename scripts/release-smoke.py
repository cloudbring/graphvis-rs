#!/usr/bin/env python3
"""Release smoke test: the binary must reproduce, byte for byte, every gallery sample
(site/src/data/samples.json) as committed in site/public/samples/NAME/rust.svg. Every build
computes with the same math library, so every platform writes those bytes. A release that does
not is not published.

Usage: scripts/release-smoke.py BINARY
"""
import json
import os
import subprocess
import sys

binary = sys.argv[1]
root = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
samples = json.load(open(os.path.join(root, "site/src/data/samples.json")))
print(subprocess.run([binary, "-V"], capture_output=True, text=True).stderr.strip())
bad = []
for s in samples:
    d = os.path.join(root, "site/public/samples", s["name"])
    r = subprocess.run([binary, f"-K{s['engine']}", "-Tsvg", os.path.join(d, s["wasm_input"])], capture_output=True)
    want = open(os.path.join(d, "rust.svg"), "rb").read()
    if r.stdout != want:
        bad.append(s["name"])
print(f"{len(samples) - len(bad)}/{len(samples)} samples byte-identical")
if bad:
    sys.exit("differ: " + ", ".join(bad))
