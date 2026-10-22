#!/usr/bin/env python3
"""Extreme numeric attribute values, C Graphviz 16.1.0 versus the port.

Sets one numeric attribute at a time (or, with --mix, a few at once) to a value
at or past the edge of what C parses: INT_MAX, INT_MAX + 1, -1, 0, 1e300, nan,
-inf, digit strings hundreds of characters long, and so on. Every case is laid
out by C and the port with the same engine and compared on exit status, stderr
and -Tplain output. Positions (`pos`) are left out: the "Extreme coordinates"
cause in docs/porting/crashes.md covers them.

  gen      write the cases to a directory (one .gv per case)
  run      generate and compare; one JSON line per case in OUT/results.jsonl,
           and every case that is not `same` copied to OUT/diff/
  summary  group a run's results by outcome, attribute and panic site

Usage:
  python3 -I scripts/attr-extremes.py gen --out /tmp/ax/cases [--engine dot]
  python3 -I scripts/attr-extremes.py run --out /tmp/ax/r1 [--engine dot] [--attr weight]
  python3 -I scripts/attr-extremes.py run --out /tmp/ax/mix --mix 2000 --seed 1
  python3 -I scripts/attr-extremes.py summary /tmp/ax/r1

Build the port with overflow checks so that wrapping arithmetic panics:
  CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=true CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true \\
    cargo build --release -p graphviz-pure-cli

C runs as $C_DOT (default `dot`) with GVBINDIR (default ~/src/gv-ref/lutplugins);
the port is $PORT_BIN (default target/release/graphviz-pure). --jobs defaults to
half the cores. Outcomes:
  same          exit status, stderr and stdout agree
  stderr_diff   exit status and stdout agree, stderr differs
  plain_diff    stdout differs           exit_diff     exit status differs
  port_crash    the port panicked or died from a signal
  port_timeout  the port exceeded the timeout (C finished)
  both_timeout  both exceeded it         c_timeout     only C did
  c_crash       C died from a signal (the port's result is in the record)
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

ROOT = pathlib.Path(__file__).resolve().parent.parent
GVBINDIR = os.environ.get("GVBINDIR", str(pathlib.Path.home() / "src/gv-ref/lutplugins"))
C_BIN = os.environ.get("C_DOT", "dot")
PORT = os.environ.get("PORT_BIN", str(ROOT / "target/release/graphviz-pure"))
ENGINES = ["dot", "neato", "fdp", "sfdp", "twopi", "circo", "osage", "patchwork"]

# Values at the edges of C's int and double parsing (atoi, strtol, strtod, sscanf).
VALUES = [
    "2147483647", "2147483648", "-2147483648", "-2147483649", "4294967296",
    "9223372036854775808", "-1", "0", "1e300", "-1e300", "1e-300", "nan", "inf", "-inf",
    "9" * 400, "-" + "9" * 400, "0." + "0" * 400 + "1",
]

# Attribute, scopes (G root graph, C cluster, N node, E edge), and whether it is a
# point ("x,y") as well as a single number. From lib/common/const.h, the
# late_int/late_double callers and agget(g, "...") reads in lib/.
ATTRS = [
    # dot
    ("nslimit", "G", False), ("nslimit1", "G", False), ("mclimit", "G", False),
    ("searchsize", "G", False), ("nodesep", "G", False), ("ranksep", "G", False),
    ("remincross", "G", False), ("phase", "G", False), ("weight", "E", False),
    ("minlen", "E", False), ("group", "N", False), ("ordering", "G", False),
    # graph-wide drawing
    ("size", "G", True), ("ratio", "G", False), ("page", "G", True), ("pad", "G", True),
    ("margin", "GCN", True), ("dpi", "G", False), ("resolution", "G", False),
    ("quantum", "G", False), ("rotate", "G", False), ("landscape", "G", False),
    ("pack", "G", False), ("center", "G", False), ("label_scheme", "G", False),
    ("nojustify", "G", False), ("fontsize", "GCNE", False), ("peripheries", "CN", False),
    ("penwidth", "CNE", False), ("gradientangle", "GCN", False),
    # neato, fdp, sfdp, circo, twopi
    ("maxiter", "G", False), ("epsilon", "G", False), ("K", "GC", False),
    ("levels", "G", False), ("levelsgap", "G", False), ("repulsiveforce", "G", False),
    ("Damping", "G", False), ("defaultdist", "G", False), ("mindist", "G", False),
    ("start", "G", False), ("esep", "G", True), ("sep", "G", True), ("dim", "G", False),
    ("dimen", "G", False), ("inputscale", "G", False), ("overlap_scaling", "G", False),
    ("voro_margin", "G", False), ("quadtree", "G", False), ("smoothing", "G", False),
    ("rotation", "G", False), ("T0", "G", False), ("len", "E", False),
    ("samplepoints", "G", False), ("sortv", "GCN", False), ("radius", "G", False),
    ("overlap", "G", False),
    # nodes
    ("sides", "N", False), ("skew", "N", False), ("distortion", "N", False),
    ("orientation", "GN", False), ("width", "N", False), ("height", "N", False),
    ("area", "CN", False), ("vertices", "N", False),
    # edges
    ("arrowsize", "E", False), ("labelangle", "E", False), ("labeldistance", "E", False),
    ("labelfontsize", "E", False),
]

# Small graphs with the features the attributes act on: clusters, labels, a
# record, a polygon, head/tail labels, a flat edge, a self-loop.
BASES = {
    "small": """digraph G {
  label="G";
  a -> b [label="ab", headlabel="h", taillabel="t"];
  b -> c;
  a -> c;
  c -> c;
  subgraph cluster_x {
    label="X";
    d;
    e [shape=polygon];
  }
  c -> d;
  d -> e;
  {
    rank=same;
    f [shape=record, label="<p>f|g"];
    a;
  }
  f:p -> b;
%s}
""",
    "tiny": """digraph G {
  a -> b;
%s}
""",
}

SCOPE_DECL = {"G": "  %s=%s;\n", "N": "  node [%s=%s];\n", "E": "  edge [%s=%s];\n"}


def quote(v):
    return '"%s"' % v


def place(text, attr, scope, value):
    """Puts attr=value at the scope's place in a base graph."""
    if scope == "C":
        return text.replace('    label="X";\n', '    label="X";\n    %s=%s;\n' % (attr, quote(value)), 1)
    return text.replace("%s}\n", SCOPE_DECL[scope] % (attr, quote(value)) + "%s}\n", 1)


def values_for(point):
    return VALUES + (["%s,%s" % (v, v) for v in VALUES] if point else [])


def header(engine, desc):
    return "/* attr-extremes: engine=%s %s */\n" % (engine, desc)


def finish(text):
    return text.replace("%s}\n", "}\n")


def single_cases(engines, attrs, bases):
    """Every (engine, attribute, scope, value, base): one extreme attribute per case."""
    for engine in engines:
        for attr, scopes, point in ATTRS:
            if attrs and attr not in attrs:
                continue
            for scope in scopes:
                for vi, v in enumerate(values_for(point)):
                    for b in bases:
                        if scope == "C" and "cluster" not in BASES[b]:
                            continue
                        name = "%s-%s-%s-v%d-%s" % (engine, attr, scope, vi, b)
                        text = finish(place(BASES[b], attr, scope, v))
                        yield name, engine, header(engine, "attr=%s scope=%s value=%s base=%s"
                                                   % (attr, scope, v[:40], b)) + text


def mix_cases(engines, seed, count):
    """Seeded cases with 2 to 4 extreme attributes each on the small base."""
    for s in range(seed, seed + count):
        r = random.Random(s)
        engine = engines[s % len(engines)] if len(engines) > 1 else engines[0]
        text = BASES["small"]
        picks = []
        for attr, scopes, point in r.sample(ATTRS, r.randint(2, 4)):
            scope = r.choice(scopes)
            v = r.choice(values_for(point))
            text = place(text, attr, scope, v)
            picks.append("%s@%s=%s" % (attr, scope, v[:40]))
        yield "mix-s%d-%s" % (s, engine), engine, header(engine, " ".join(picks)) + finish(text)


def run(cmd, path, timeout, env=None):
    """Returns (status, stdout, stderr); status is 'timeout' or an int (negative: signal)."""
    try:
        p = subprocess.run(cmd + [str(path)], capture_output=True, timeout=timeout, env=env,
                           stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return "timeout", b"", b""
    return p.returncode, p.stdout, p.stderr


def norm_err(err, path):
    s = err.decode("utf-8", "replace").replace(str(path), "FILE")
    return re.sub(r"^(dot|graphviz-pure): ", "PROG: ", s, flags=re.M)


PANIC_RE = re.compile(r"panicked at ([^\n]+?):(\d+):\d+:\n([^\n]*)")


def compare(path, engine, timeout):
    args = ["-K" + engine, "-Tplain"]
    c = run([C_BIN] + args, path, timeout, env=dict(os.environ, GVBINDIR=GVBINDIR))
    p = run([PORT] + args, path, timeout * 2)
    d = {"c_status": c[0], "port_status": p[0]}
    perr = p[2].decode("utf-8", "replace")
    m = PANIC_RE.search(perr)
    if m:
        d["panic"] = "%s:%s %s" % (m.group(1), m.group(2), m.group(3)[:120])
    if c[0] == "timeout":
        return ("both_timeout" if p[0] == "timeout" else "c_timeout"), d
    if p[0] == "timeout":
        return "port_timeout", d
    if m or p[0] == 101 or (isinstance(p[0], int) and p[0] < 0):
        return "port_crash", d
    if c[0] < 0:
        return "c_crash", d
    if c[0] != p[0]:
        d["c_err"], d["port_err"] = norm_err(c[2], path)[-300:], perr[-300:]
        return "exit_diff", d
    if c[1] != p[1]:
        return "plain_diff", d
    if norm_err(c[2], path) != norm_err(p[2], path):
        d["c_err"], d["port_err"] = norm_err(c[2], path)[-300:], perr[-300:]
        return "stderr_diff", d
    return "same", d


def cmd_gen(a):
    a.out.mkdir(parents=True, exist_ok=True)
    n = 0
    for name, _, text in cases(a):
        (a.out / (name + ".gv")).write_text(text, encoding="utf-8")
        n += 1
    print("%d cases in %s" % (n, a.out))


def cases(a):
    engines = [a.engine] if a.engine else ENGINES
    if a.mix:
        return mix_cases(engines, a.seed, a.mix)
    return single_cases(engines, set(a.attr or []), a.base or list(BASES))


def cmd_run(a):
    (a.out / "diff").mkdir(parents=True, exist_ok=True)
    work = a.out / "work"
    work.mkdir(exist_ok=True)

    def one(case):
        name, engine, text = case
        p = work / (name + ".gv")
        p.write_text(text, encoding="utf-8")
        oc, d = compare(p, engine, a.timeout)
        if oc != "same":
            (a.out / "diff" / p.name).write_text(text, encoding="utf-8")
        p.unlink()
        return {"case": name, "engine": engine, "outcome": oc, **d}

    n = 0
    with open(a.out / "results.jsonl", "a") as f, cf.ThreadPoolExecutor(a.jobs) as ex:
        for rec in ex.map(one, cases(a)):
            f.write(json.dumps(rec) + "\n")
            f.flush()
            n += 1
            if rec["outcome"] not in ("same", "stderr_diff", "plain_diff"):
                print("[%d] %s %s %s" % (n, rec["case"], rec["outcome"], rec.get("panic", "")),
                      flush=True)
    summarize(a.out)


def attr_of(case):
    parts = case.split("-")
    return "mix" if parts[0] == "mix" else "%s@%s" % (parts[1], parts[2])


def summarize(out):
    recs = [json.loads(l) for l in open(out / "results.jsonl")]
    print("%d cases" % len(recs))
    for oc, k in collections.Counter(r["outcome"] for r in recs).most_common():
        print("  %-13s %d" % (oc, k))
    sites = collections.defaultdict(list)
    for r in recs:
        if r["outcome"] == "port_crash":
            sites[r.get("panic", "signal %s" % r["port_status"])].append(r["case"])
    if sites:
        print("panic sites:")
        for site, cs in sorted(sites.items(), key=lambda kv: -len(kv[1])):
            print("  %4d  %s\n        e.g. %s" % (len(cs), site, " ".join(cs[:3])))
    by = collections.defaultdict(collections.Counter)
    for r in recs:
        if r["outcome"] not in ("same",):
            by[r["outcome"]][(r["engine"], attr_of(r["case"]))] += 1
    for oc in ("port_timeout", "exit_diff", "plain_diff", "stderr_diff", "c_crash", "c_timeout",
               "both_timeout"):
        if by.get(oc):
            print("%s by engine, attribute:" % oc)
            for (e, at), k in by[oc].most_common(40):
                print("  %4d  %-9s %s" % (k, e, at))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("gen", "run"):
        s = sub.add_parser(name)
        s.add_argument("--out", type=pathlib.Path, required=True)
        s.add_argument("--engine", choices=ENGINES)
        s.add_argument("--attr", action="append", help="only this attribute (repeatable)")
        s.add_argument("--base", action="append", choices=list(BASES))
        s.add_argument("--mix", type=int, default=0, help="COUNT seeded cases with several attributes")
        s.add_argument("--seed", type=int, default=1)
        if name == "run":
            s.add_argument("--timeout", type=float, default=10)
            s.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    s = sub.add_parser("summary")
    s.add_argument("out", type=pathlib.Path)
    a = ap.parse_args()
    {"gen": cmd_gen, "run": cmd_run, "summary": lambda a: summarize(a.out)}[a.cmd](a)


if __name__ == "__main__":
    main()
