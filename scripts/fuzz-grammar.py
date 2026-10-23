#!/usr/bin/env python3
"""Token-level differential sweep of cgraph's grammar (lib/cgraph/grammar.y): C Graphviz
16.1.0 versus the port, on corpus files with a few tokens deleted, repeated, swapped or
replaced, and inserted from the grammar's terminals.

Each case is a pure function of its seed: the seed picks a small corpus file under tests/
and its mutations. Both programs run `-Tcanon` on the case as a file; the exit status,
stdout and stderr must agree. A syntax error is compared by its first line, which holds
the `near` token (`syntax_diff`). `stderr_order`: the same lines in another order. Layout
warnings C prints and the port does not are left out of the stderr comparison, as in
fuzz-scanner.py.

Usage:
  python3 -I scripts/fuzz-grammar.py --seed 1 --count 5000 --out /tmp/grammar
  python3 -I scripts/fuzz-grammar.py --seed 1 --count 5000 --out /tmp/grammar --baseline MASTER_BIN
  python3 -I scripts/fuzz-grammar.py --print 17            # one case to stdout
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
C_BIN = os.environ.get("C_DOT", "/opt/homebrew/bin/dot")
PORT_BIN = os.environ.get("PORT_BIN", str(ROOT / "target/release/graphviz-pure"))

# scan.l's tokens, roughly: comments and whitespace are kept as separators, strings whole.
TOKEN = re.compile(
    rb'"(?:[^"\\]|\\.)*"|<(?:[^<>]|<[^<>]*>)*>|/\*.*?\*/|//[^\n]*|#[^\n]*|->|--'
    rb"|[A-Za-z_\x80-\xff][A-Za-z_0-9\x80-\xff]*|-?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)|\s+|.",
    re.S,
)
# grammar.y's terminals, and atoms of each kind (T_atom names and numbers, T_qatom strings).
TERMINALS = [
    b";", b",", b"=", b"[", b"]", b"{", b"}", b":", b"+", b"->", b"--", b"subgraph",
    b"node", b"edge", b"graph", b"digraph", b"strict", b"a", b"b", b"x1", b"2", b"-1.5",
    b'"q"', b'""', b"<h>", b"<>", b"key", b"color", b"red", b"cluster_c", b"@",
]
SEPARATOR = re.compile(rb"\s+|/\*.*|//.*|#.*", re.S)


def corpus():
    files = []
    for d in ["wild", "neato", "fdp", "twopi", "sfdp", "circo", "osage", "patchwork", "render", "fixtures"]:
        files.extend(sorted((ROOT / "tests" / d).glob("**/*.gv")))
    return [f for f in files if f.stat().st_size <= 1500]


FILES = None


def case(seed):
    global FILES
    if FILES is None:
        FILES = corpus()
    r = random.Random(seed)
    toks = TOKEN.findall(FILES[r.randrange(len(FILES))].read_bytes())
    # the positions of real tokens (not separators)
    for _ in range(r.randint(1, 3)):
        real = [i for i, t in enumerate(toks) if not SEPARATOR.fullmatch(t)]
        if not real:
            break
        i = r.choice(real)
        op = r.random()
        if op < 0.25:
            del toks[i]
        elif op < 0.4:
            toks.insert(i, toks[i] + b" ")
        elif op < 0.55:
            j = r.choice(real)
            toks[i], toks[j] = toks[j], toks[i]
        elif op < 0.75:
            toks[i] = r.choice(TERMINALS)
        else:
            toks.insert(i, r.choice(TERMINALS) + b" ")
    return b"".join(toks)


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


def syntax_line(err):
    for line in err.splitlines():
        if b"syntax error in line" in line or b"memory exhausted in line" in line:
            return line
    return None


def outcome_of(c, p):
    if isinstance(c[0], int) and c[0] < 0:
        return "c_crash"
    if c[0] == "timeout":
        return "c_timeout"
    if isinstance(p[0], int) and p[0] < 0 or p[0] == 101 or p[0] == "timeout":
        return "port_crash"
    cs, ps = syntax_line(c[2]), syntax_line(p[2])
    if cs != ps:
        return "syntax_diff"
    if c[0] != p[0]:
        return "exit_diff"
    if c[1] != p[1]:
        return "canon_diff"
    if strip_layout(c[2]) != strip_layout(p[2]):
        same_lines = sorted(strip_layout(c[2]).splitlines()) == sorted(strip_layout(p[2]).splitlines())
        return "stderr_order" if same_lines else "stderr_diff"
    return "same"


def compare(seed, out, port, baseline):
    path = out / "cases" / f"s{seed}.gv"
    path.write_bytes(case(seed))
    c = run([C_BIN], path)
    res = {"seed": seed, "outcome": outcome_of(c, run([port], path)), "syntax": syntax_line(c[2]) is not None}
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
    ap.add_argument("--jobs", type=int, default=2)
    ap.add_argument("--print", type=int, metavar="SEED")
    a = ap.parse_args()
    if a.print is not None:
        sys.stdout.buffer.write(case(a.print))
        return
    (a.out / "cases").mkdir(parents=True, exist_ok=True)
    seeds = range(a.seed, a.seed + a.count)
    with cf.ThreadPoolExecutor(min(a.jobs, 2)) as ex:
        results = list(ex.map(lambda s: compare(s, a.out, a.port, a.baseline), seeds))
    with open(a.out / "results.jsonl", "w") as f:
        for res in results:
            f.write(json.dumps(res) + "\n")
    counts = collections.Counter(res["outcome"] for res in results)
    errors = sum(res["syntax"] for res in results)
    print(f"{len(results)} inputs ({errors} syntax errors in C): "
          + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))
    if a.baseline:
        base = collections.Counter(res["baseline"] for res in results)
        print("baseline: " + ", ".join(f"{k} {v}" for k, v in sorted(base.items())))
        fixed = [r["seed"] for r in results if r["baseline"] != "same" and r["outcome"] == "same"]
        broken = [r["seed"] for r in results if r["baseline"] == "same" and r["outcome"] != "same"]
        print(f"now same, were not: {len(fixed)}; were same, now not: {len(broken)} {broken[:20]}")


if __name__ == "__main__":
    main()
