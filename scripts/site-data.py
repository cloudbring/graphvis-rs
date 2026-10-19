#!/usr/bin/env python3
"""Writes site/src/data/fidelity.json, the data behind the site's Fidelity page.

    python3 -I scripts/site-data.py              # count the committed ratchet baselines
    python3 -I scripts/site-data.py --measure    # run gv-diff e2e under both math profiles

Without --measure the numbers come from the baseline files that `cargo test --workspace`
enforces (tests/golden/*baseline*.json): a file listed there is checked on every test run
and fails the build if it stops matching. With --measure the full -Tsvg check is run again
here, for the Apple profile (macOS only) and the portable profile, and the identical sets
are taken from the hashes it writes; the script fails if a measurement disagrees with the
committed baseline, so the page never shows a number the ratchets do not back.

The C reference is only needed to regenerate tests/golden/e2e.json, not for this script:
the e2e check compares against the stored hashes of C's output.
"""

import argparse
import datetime as dt
import json
import platform
import re
import subprocess
import sys
import tempfile
from collections import Counter, OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GOLDEN = ROOT / "tests" / "golden"
OUT = ROOT / "site" / "src" / "data" / "fidelity.json"

# Explanatory text only; every number below is computed.
E2E_CORPORA = OrderedDict(
    [
        ("main", ("tests/wild, tests/fixtures", "Graphs from the wild, the Graphviz gallery and test suite, upstream fixtures and fuzz reproducers, laid out by dot unless the file says otherwise.")),
        ("neato", ("tests/neato", "neato: stress majorization, KK, circuit and subset models, overlap removal, pinned nodes, packing.")),
        ("sfdp", ("tests/sfdp", "sfdp: multilevel force-directed layout with quadtree approximation.")),
        ("circo", ("tests/circo", "circo: circular layout of biconnected components.")),
        ("twopi", ("tests/twopi", "twopi: radial layout around a root.")),
        ("fdp", ("tests/fdp", "fdp: force-directed layout with clusters.")),
        ("osage", ("tests/osage", "osage: cluster packing.")),
        ("patchwork", ("tests/patchwork", "patchwork: squarified treemaps.")),
        ("render", ("tests/render", "Renderer edge cases: shapes, styles, arrows, colour lists, gradients and charsets.")),
    ]
)
LAYOUT_CORPORA = ["main", "neato", "sfdp", "circo", "twopi", "fdp", "osage", "patchwork"]
RENDER_FORMATS = ["svg", "xdot", "dot", "json", "json0", "plain", "plain-ext"]


def load(p):
    with open(p, encoding="utf-8") as f:
        return json.load(f)


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()


def layout_file(corpus, portable):
    stem = "baseline" if corpus == "main" else f"{corpus}-baseline"
    return GOLDEN / f"{stem}.{'portable.json' if portable else 'json'}"


def exact(r):
    # gv_diff::FileReport::exact
    return (
        r.get("error") is None
        and r["missing"] == 0
        and r["bb_ok"]
        and r["nodes_ok"] == r["nodes"]
        and r["edges_ok"] == r["edges"]
        and r["clusters_ok"] == r["clusters"]
    )


def render_unstable():
    """The (format, stem) pairs gv-diff render skips, read from its source."""
    src = (ROOT / "crates" / "gv-diff" / "src" / "render.rs").read_text(encoding="utf-8")
    block = src.split("pub const UNSTABLE", 1)[1].split("];", 1)[0]
    return re.findall(r'\("([^"]+)",\s*"([^"]+)"\)', block)


def fuzz_rounds():
    """The rounds table of docs/porting/fuzzing.md."""
    text = (ROOT / "docs" / "porting" / "fuzzing.md").read_text(encoding="utf-8")
    sec = text.split("## Rounds", 1)[1]
    rows = [l for l in sec.splitlines() if l.startswith("|")]
    header = [c.strip() for c in rows[0].strip("|").split("|")]
    out = []
    for l in rows[2:]:
        cells = [c.strip() for c in l.strip("|").split("|")]
        if len(cells) != len(header):
            break
        row = dict(zip(header, cells))
        out.append(
            {
                "round": row["Round"],
                "seeds": row["Seeds"].replace("–", "-"),
                "cases": int(row["Cases"]),
                "engines": {k: float(row[k]) for k in header[3:-1]},
                "all": float(row["all"]),
            }
        )
    return out


def measure(portable):
    """Runs gv-diff e2e for one math profile; returns the set of identical files."""
    feat = ["--features", "gv-core/portable-math", "--target-dir", "target/portable"] if portable else []
    subprocess.run(["cargo", "build", "--release", "-p", "gv-diff", *feat], cwd=ROOT, check=True)
    exe = ROOT / ("target/portable/release/gv-diff" if portable else "target/release/gv-diff")
    golden = load(GOLDEN / "e2e.json")["files"]
    with tempfile.TemporaryDirectory() as td:
        hp = Path(td) / "hashes.json"
        r = subprocess.run([str(exe), "e2e", "--hashes", str(hp)], cwd=ROOT, capture_output=True, text=True)
        sys.stderr.write(r.stdout[-1500:])
        hashes = load(hp)
    return {k for k, h in hashes.items() if h is not None and golden.get(k) == h}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--measure", action="store_true", help="run gv-diff e2e under both math profiles")
    ap.add_argument("--out", type=Path, default=OUT)
    a = ap.parse_args()

    golden = load(GOLDEN / "e2e.json")
    files = sorted(golden["files"])
    base = {
        "apple": set(load(GOLDEN / "e2e-baseline.json")["identical"]),
        "portable": set(load(GOLDEN / "e2e-baseline.portable.json")["identical"]),
    }
    ident = dict(base)
    measured = {"apple": False, "portable": False}
    if a.measure:
        profiles = ["portable"] + (["apple"] if platform.system() == "Darwin" else [])
        for prof in profiles:
            got = measure(prof == "portable")
            lost = base[prof] - got
            if lost:
                sys.exit(f"{prof}: {len(lost)} files in the committed baseline no longer match C, e.g. {sorted(lost)[:5]}")
            ident[prof] = got
            measured[prof] = True

    total = Counter(f.split("/")[0] for f in files)
    unstable = sorted(golden["unstable"])
    unstable_by = Counter(f.split("/")[0] for f in unstable)
    corpora = []
    for name, (dirs, desc) in E2E_CORPORA.items():
        row = {"name": name, "dirs": dirs, "description": desc, "total": total[name], "unstable": unstable_by[name]}
        for prof in ("apple", "portable"):
            row[prof] = sum(1 for f in ident[prof] if f.split("/")[0] == name)
        corpora.append(row)
    e2e = {
        "corpora": corpora,
        "total": len(files),
        "apple": len(ident["apple"]),
        "portable": len(ident["portable"]),
        "unstable": unstable,
        "files": files,
        "differs": {p: sorted(set(files) - ident[p]) for p in ("apple", "portable")},
    }

    layout = []
    for c in LAYOUT_CORPORA:
        row = {"name": c}
        for prof, portable in (("apple", False), ("portable", True)):
            rep = load(layout_file(c, portable))["files"]
            row["total"] = len(rep)
            row[prof] = sum(1 for r in rep.values() if exact(r))
            row[prof + "_inexact"] = sorted(k for k, r in rep.items() if not exact(r))
        layout.append(row)

    skipped = render_unstable()
    render = []
    for fmt in RENDER_FORMATS:
        row = {"format": fmt, "skipped": sum(1 for f, _ in skipped if f == fmt)}
        for prof, name in (("apple", "baseline.json"), ("portable", "baseline.portable.json")):
            m = load(GOLDEN / "render" / name)["formats"][fmt]
            row["total"] = len(m)
            row[prof] = sum(1 for r in m.values() if r["identical"])
            row[prof + "_differs"] = sorted(k for k, r in m.items() if not r["identical"])
        render.append(row)

    commit = git("rev-parse", "HEAD")
    data = {
        "generated": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "commit": commit,
        "commit_short": commit[:7],
        "commit_date": git("show", "-s", "--format=%cI", "HEAD"),
        "source": "measured" if any(measured.values()) else "baselines",
        "measured": measured,
        "measured_on": f"{platform.system()} {platform.machine()}" if any(measured.values()) else None,
        "oracle": {
            "version": "16.1.0",
            "command": "GVBINDIR=~/src/gv-ref/lutplugins dot -Tsvg FILE",
            "normalisation": "the line starting with '<!-- Generated by ' is dropped from both outputs",
        },
        "e2e": e2e,
        "layout": layout,
        "render": render,
        "fuzz": {"source": "docs/porting/fuzzing.md", "rounds": fuzz_rounds()},
        # Not reproducible from committed files: needs the Graphviz source tree and the C
        # oracle (scripts/dot-broad-validate.sh then gv-diff pairs). Carried from README.md.
        "c_graphs": {
            "source": "README.md (scripts/dot-broad-validate.sh + gv-diff pairs, layout geometry)",
            "inputs": 379,
            "variants": [
                {"name": "base", "flags": "", "exact": 379, "total": 379},
                {"name": "LR", "flags": "-Grankdir=LR", "exact": 379, "total": 379},
                {"name": "ortho", "flags": "-Gsplines=ortho", "exact": 379, "total": 379, "note": "C needs over 60 s on 3 inputs; given longer, they match too"},
                {"name": "concentrate", "flags": "-Gconcentrate=true", "exact": 379, "total": 379},
                {"name": "newrank", "flags": "-Gnewrank=true", "exact": 379, "total": 379},
            ],
        },
    }
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {a.out.relative_to(ROOT)}: e2e apple {e2e['apple']}/{e2e['total']}, portable {e2e['portable']}/{e2e['total']} ({data['source']})")


if __name__ == "__main__":
    main()
