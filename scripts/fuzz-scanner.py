#!/usr/bin/env python3
"""Byte-level differential sweep of cgraph's scanner (lib/cgraph/scan.l, io.c): C
Graphviz 16.1.0 versus the port, on short random inputs that mix byte-order marks,
NULs, CR/LF, backslashes, numbers running into names, comments, line directives and
non-UTF-8 bytes.

Each case is a pure function of its seed. Both programs run `-Tcanon` on the case as
a file; the exit status, stdout and stderr must agree (`stderr_order`: the same lines in
another order). -Tcanon does no layout in the
port (#108), so C's layout warnings (text width, invalid UTF-8 in a label) are left
out of the stderr comparison.

Usage:
  python3 -I scripts/fuzz-scanner.py --seed 1 --count 5000 --out /tmp/scan
  python3 -I scripts/fuzz-scanner.py --seed 1 --count 5000 --out /tmp/scan --baseline MASTER_BIN
  python3 -I scripts/fuzz-scanner.py --print 17            # one case to stdout
"""
import argparse
import collections
import concurrent.futures as cf
import json
import os
import pathlib
import random
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
GVBINDIR = os.environ.get("GVBINDIR", str(pathlib.Path.home() / "src/gv-ref/lutplugins"))
C_BIN = os.environ.get("C_DOT", "dot")
PORT_BIN = os.environ.get("PORT_BIN", str(ROOT / "target/release/graphviz-pure"))

BOM = b"\xef\xbb\xbf"
# Bytes the scanner treats specially, and the tokens around them.
NOISE = [
    BOM, BOM, b"\0", b"\0", b"\r", b"\n", b"\r\n", b"\\", b"\\\n", b"\\\r\n", b"\\\\",
    b"\\\"", b"2x", b"1.2.3", b"1..", b".5.", b"-.5x", b"1.", b"-1", b".5", b"-", b".",
    b"3\xc3\xa9", b"\xc3\xa9", b"\xff", b"5\xff", b"/*", b"*/", b"//", b"#", b"@",
    b"# 3 \"f.gv\"\n", b"#line 9\n", b" ", b"\t", b"\x0b", b"\x0c", b"node", b"NODE",
    b"edge", b"graph", b"subgraph", b"strict", b"\"", b"<", b">", b"->", b"--", b"+", b";",
]
ATOMS = [b"a", b"b", b"c1", b"_x", b"2", b"-3.5", b".7", b"\"q\"", b"\"x y\"", b"<h>", b"\xc3\xa9t\xc3\xa9"]


def atom(r):
    a = r.choice(ATOMS)
    if r.random() < 0.5:
        a = noisy(r, a)
    return a


def noisy(r, s):
    """`s` with a few noise fragments spliced in."""
    for _ in range(r.randint(1, 3)):
        i = r.randint(0, len(s))
        s = s[:i] + r.choice(NOISE) + s[i:]
    return s


def case(seed):
    r = random.Random(seed)
    if r.random() < 0.25:
        # free sequence of fragments after a header
        body = b"".join(r.choice(NOISE + ATOMS + [b" ", b"{", b"}", b"[", b"]", b"="])
                        for _ in range(r.randint(1, 14)))
        return r.choice([b"digraph{", b"graph G {", b""]) + body + r.choice([b"}", b"}\n", b""])
    kind = r.choice([b"digraph", b"graph"])
    op = b"->" if kind == b"digraph" else b"--"
    stmts = []
    for _ in range(r.randint(1, 5)):
        k = r.random()
        if k < 0.35:
            stmts.append(atom(r) + b" " + op + b" " + atom(r))
        elif k < 0.6:
            stmts.append(atom(r) + b" [label=" + atom(r) + b"]")
        elif k < 0.8:
            stmts.append(b"\"" + noisy(r, b"lab el") + b"\"")
        else:
            stmts.append(atom(r))
    sep = r.choice([b"; ", b"\n", b"\r\n", b" "])
    # no `;` straight after `{`: an empty statement is #143 (grammar), not the scanner
    lead = r.choice([b"\n", b"\r\n", b" "])
    text = kind + b" G {" + lead + sep.join(stmts) + sep + b"}\n"
    if r.random() < 0.5:
        text = noisy(r, text)
    if r.random() < 0.2:
        text += r.choice([b"digraph H { z }\n", b"graph { 2y }", b"\0digraph K {k}\n"])
    return text


LAYOUT_WARNING = re.compile(rb"^Warning: (Warning: no value for width|Invalid \d-byte UTF8|UTF8 codes > 4 bytes)")


def run(cmd, path):
    env = dict(os.environ, GVBINDIR=GVBINDIR)
    try:
        p = subprocess.run(cmd + ["-Tcanon", str(path)], capture_output=True, env=env, timeout=20)
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return "timeout", b"", b""


def strip_layout(err):
    return b"".join(l for l in err.splitlines(keepends=True) if not LAYOUT_WARNING.match(l))


def outcome_of(c, p):
    if isinstance(c[0], int) and c[0] < 0:
        outcome = "c_crash"
    elif c[0] == "timeout":
        outcome = "c_timeout"
    elif isinstance(p[0], int) and p[0] < 0 or p[0] == 101 or p[0] == "timeout":
        outcome = "port_crash"
    elif c[0] != p[0]:
        outcome = "exit_diff"
    elif c[1] != p[1]:
        outcome = "canon_diff"
    elif strip_layout(c[2]) != strip_layout(p[2]):
        same_lines = sorted(strip_layout(c[2]).splitlines()) == sorted(strip_layout(p[2]).splitlines())
        # the port reads every graph before the first layout and prints the syntax error
        # last (the job pipeline, not the scanner)
        outcome = "stderr_order" if same_lines else "stderr_diff"
    else:
        outcome = "same"
    return outcome


def compare(seed, out, port, baseline):
    path = out / "cases" / f"s{seed}.gv"
    path.write_bytes(case(seed))
    c = run([C_BIN], path)
    res = {"seed": seed, "outcome": outcome_of(c, run([port], path))}
    if baseline:
        res["baseline"] = outcome_of(c, run([baseline], path))
    if res["outcome"] != "same":
        (out / "diff").mkdir(exist_ok=True)
        (out / "diff" / path.name).write_bytes(path.read_bytes())
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--count", type=int, default=1000)
    ap.add_argument("--out", type=pathlib.Path)
    ap.add_argument("--port", default=PORT_BIN)
    ap.add_argument("--baseline", metavar="BIN", help="another port binary (say, master's) to count against")
    ap.add_argument("--jobs", type=int, default=os.cpu_count())
    ap.add_argument("--print", type=int, metavar="SEED")
    a = ap.parse_args()
    if a.print is not None:
        sys.stdout.buffer.write(case(a.print))
        return
    (a.out / "cases").mkdir(parents=True, exist_ok=True)
    seeds = range(a.seed, a.seed + a.count)
    with cf.ThreadPoolExecutor(a.jobs) as ex:
        results = list(ex.map(lambda s: compare(s, a.out, a.port, a.baseline), seeds))
    with open(a.out / "results.jsonl", "w") as f:
        for res in results:
            f.write(json.dumps(res) + "\n")
    counts = collections.Counter(res["outcome"] for res in results)
    print(f"{len(results)} inputs: " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))
    if a.baseline:
        base = collections.Counter(res["baseline"] for res in results)
        print("baseline: " + ", ".join(f"{k} {v}" for k, v in sorted(base.items())))
        fixed = [r["seed"] for r in results if r["baseline"] != "same" and r["outcome"] == "same"]
        broken = [r["seed"] for r in results if r["baseline"] == "same" and r["outcome"] != "same"]
        print(f"now same, were not: {len(fixed)}; were same, now not: {len(broken)} {broken[:20]}")


if __name__ == "__main__":
    main()
