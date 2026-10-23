#!/usr/bin/env python3
"""Crash campaign: run the port on generated cases, and C on every case the port
panics or hangs on (see docs/porting/fuzzing.md, "Crash campaigns").

scripts/fuzz-run.py compares outputs and skips the port where C crashes; this runner
looks for the port's panics and hangs whatever C does, because the port must not
crash where C does (docs/porting/crashes.md). It runs only the port on most cases, so
it gets through tens of thousands of inputs quickly.

  run      generate cases with GEN (any generator with a `Gen(seed).case()`), run the
           port on each, and C on each panic or hang; one JSON line per case in
           OUT/results.jsonl, the failing cases in OUT/hits/
  summary  group a run's hits by (port panic site or timeout, C outcome)

Usage:
  CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=true CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true cargo build --release -p graphviz-pure-cli
  python3 -I scripts/fuzz-crash.py run --gen scripts/fuzz-gen-ranks.py --seed 1 --count 20000 --out /tmp/fz/ranks
  python3 -I scripts/fuzz-crash.py summary /tmp/fz/ranks
Then minimise a hit with `scripts/fuzz-run.py min` (outcome `port_crash` only where C
does not crash), or by hand.
"""
import argparse
import collections
import concurrent.futures as cf
import importlib.util
import json
import os
import pathlib
import re
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
GVBINDIR = os.environ.get("GVBINDIR", str(pathlib.Path.home() / "src/gv-ref/lutplugins"))
C_BIN = os.environ.get("C_DOT", "dot")
PORT = os.environ.get("PORT_BIN", str(ROOT / "target/release/graphviz-pure"))
C_ENV = dict(os.environ, GVBINDIR=GVBINDIR)


def run(cmd, path, timeout, env=None):
    """Returns (status, stderr); status is 'timeout' or an int (negative: signal).

    The child runs under an alarm set before exec (`perl -e alarm`), so it dies after
    `timeout` seconds even if this runner is killed first and cannot reap it."""
    t = int(timeout)
    argv = ["perl", "-e", "alarm shift; exec @ARGV or exit 127", str(t)] + cmd + ["-Tsvg", str(path)]
    try:
        p = subprocess.run(argv, capture_output=True, timeout=t + 5, env=env, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    if p.returncode == -14:  # SIGALRM: the alarm fired
        return "timeout", ""
    return p.returncode, p.stderr.decode("utf-8", "replace")


def c_outcome(st):
    if st == "timeout":
        return "c_timeout"
    return "c_crash" if st < 0 else ("c_ok" if st == 0 else "c_error")


def cmd_run(a):
    spec = importlib.util.spec_from_file_location("gen", a.gen)
    gen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gen)
    (a.out / "hits").mkdir(parents=True, exist_ok=True)
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="fuzzcrash"))

    def one(s):
        text = gen.Gen(s).case()
        f = tmp / f"s{s}.gv"
        f.write_text(text, encoding="utf-8")
        st, err = run([PORT], f, a.timeout)
        m = re.search(r"panicked at ([^:\n]+:\d+):\d+:\n(.*)", err)
        res = {"seed": s, "port": st}
        if st == "timeout" or m or (isinstance(st, int) and st < 0):
            hit = a.out / "hits" / f"s{s}.gv"
            hit.write_text(text, encoding="utf-8")
            cst, cerr = run([C_BIN], hit, a.timeout, C_ENV)
            res.update(site=m.group(1) if m else ("timeout" if st == "timeout" else f"signal {-st}"),
                       msg=m.group(2)[:200] if m else "", c=c_outcome(cst), c_stderr=cerr[-400:])
        f.unlink()
        return res

    with cf.ThreadPoolExecutor(a.jobs) as ex, open(a.out / "results.jsonl", "a") as out:
        for res in ex.map(one, range(a.seed, a.seed + a.count)):
            out.write(json.dumps(res) + "\n")
            if "site" in res:
                print(res["seed"], res["site"], res["c"], flush=True)
    summarize(a.out)


def summarize(out):
    rows = [json.loads(l) for l in open(out / "results.jsonl")]
    hits = [r for r in rows if "site" in r]
    print(f"{len(rows)} cases, {len(hits)} port panics or hangs")
    groups = collections.defaultdict(list)
    for r in hits:
        groups[(r["site"], r["c"])].append(r["seed"])
    for (site, c), seeds in sorted(groups.items(), key=lambda kv: -len(kv[1])):
        print(f"{len(seeds):6d}  {site}  {c}  e.g. s{min(seeds)}")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--gen", type=pathlib.Path, required=True)
    r.add_argument("--seed", type=int, default=1)
    r.add_argument("--count", type=int, default=1000)
    r.add_argument("--out", type=pathlib.Path, required=True)
    r.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    r.add_argument("--timeout", type=int, default=20)
    s = sub.add_parser("summary")
    s.add_argument("out", type=pathlib.Path)
    a = ap.parse_args()
    if a.cmd == "run":
        cmd_run(a)
    else:
        summarize(a.out)


if __name__ == "__main__":
    main()
