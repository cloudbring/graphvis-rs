#!/usr/bin/env python3
"""Crash campaign: run the port on generated cases, and C on every case the port
panics or hangs on (see docs/porting/fuzzing.md, "Crash campaigns").

scripts/fuzz-run.py compares outputs and skips the port where C crashes; this runner
looks for the port's panics and hangs whatever C does, because the port must not
crash where C does (docs/porting/crashes.md). It runs only the port on most cases, so
it gets through tens of thousands of inputs quickly.

  run      generate cases with GEN (any generator with a `Gen(seed).case()`), run the
           port on each, and C on each panic or hang; one JSON line per case is
           appended to OUT/results.jsonl (use a fresh OUT per run), the failing cases
           go to OUT/hits/
  summary  group a run's hits by (port panic site or timeout, C outcome)
  min      delta-debug one hit while the port's panic site (or hang) and C's outcome
           stay the same; fuzz-run.py's `min` cannot, because it stops at `c_crash`

Usage:
  CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=true CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true cargo build --release -p graphviz-pure-cli
  python3 -I scripts/fuzz-crash.py run --gen scripts/fuzz-gen-ranks.py --seed 1 --count 20000 --out /tmp/fz/ranks
  python3 -I scripts/fuzz-crash.py summary /tmp/fz/ranks
  python3 -I scripts/fuzz-crash.py min /tmp/fz/ranks/hits/s141.gv -o /tmp/fz/s141.min.gv
C_DOT and PORT_BIN choose the binaries (default `dot` on PATH and target/release).
"""
import argparse
import collections
import concurrent.futures as cf
import importlib.util
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
GVBINDIR = os.environ.get("GVBINDIR", str(pathlib.Path.home() / "src/gv-ref/lutplugins"))
C_BIN = os.environ.get("C_DOT", "dot")
PORT = os.environ.get("PORT_BIN", str(ROOT / "target/release/graphviz-pure"))
C_ENV = dict(os.environ, GVBINDIR=GVBINDIR)
TIMEOUT = 20

# Runs ARGV in a process group of its own and kills the whole group when the alarm
# fires or when this supervisor is told to stop, so neither the command nor anything
# it starts (a wrapper script that does not exec) outlives TIMEOUT. The child also
# carries an alarm of its own across exec, in case the supervisor is killed outright.
# Exits with the command's status, 128+N for signal N, and 142 on timeout.
SUPERVISE = r"""
$t = shift; $p = fork; defined $p or exit 127;
if (!$p) { setpgrp(0, 0); alarm $t + 2; exec @ARGV; exit 127 }
for my $s (qw(ALRM TERM INT HUP)) { $SIG{$s} = sub { kill 'KILL', -$p; exit($_[0] eq 'ALRM' ? 142 : 143) } }
alarm $t; waitpid($p, 0);
exit($? & 127 ? 128 + ($? & 127) : $? >> 8);
"""


def run(cmd, path, timeout=TIMEOUT, env=None):
    """Returns (status, stderr); status is 'timeout' or an int (negative: signal)."""
    argv = ["perl", "-e", SUPERVISE, str(int(timeout))] + cmd + ["-Tsvg", str(path)]
    try:
        p = subprocess.run(argv, capture_output=True, timeout=timeout + 10, env=env,
                           stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    st = p.returncode
    if st == 142:
        return "timeout", ""
    if st > 128:  # killed by a signal (directly, or reported by a wrapper shell)
        st = 128 - st
    return st, p.stderr.decode("utf-8", "replace")


def port_hit(st, err):
    """The port's panic site, 'timeout' or 'signal N'; None if it did not crash."""
    m = re.search(r"panicked at ([^:\n]+:\d+):\d+:", err)
    if m:
        return m.group(1)
    if st == "timeout":
        return "timeout"
    if st == 101:
        return "panic"
    if isinstance(st, int) and st < 0:
        return f"signal {-st}"
    return None


def c_outcome(st):
    if st == "timeout":
        return "c_timeout"
    return "c_crash" if st < 0 else ("c_ok" if st == 0 else "c_error")


def check_binaries():
    for name, b in (("PORT_BIN", PORT), ("C_DOT", C_BIN)):
        if not shutil.which(b):
            sys.exit(f"fuzz-crash: {name}={b} is not an executable")


def cmd_run(a):
    check_binaries()
    spec = importlib.util.spec_from_file_location("gen", a.gen)
    gen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gen)
    (a.out / "hits").mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="fuzzcrash") as td:
        tmp = pathlib.Path(td)

        def one(s):
            text = gen.Gen(s).case()
            f = tmp / f"s{s}.gv"
            f.write_text(text, encoding="utf-8")
            st, err = run([PORT], f, a.timeout)
            f.unlink()
            res = {"seed": s, "port": st}
            site = port_hit(st, err)
            if site:
                hit = a.out / "hits" / f"s{s}.gv"
                hit.write_text(text, encoding="utf-8")
                cst, cerr = run([C_BIN], hit, a.timeout, C_ENV)
                m = re.search(r"panicked at [^\n]*\n(.*)", err)
                res.update(site=site, msg=m.group(1)[:200] if m else "", c=c_outcome(cst),
                           c_stderr=cerr[-400:])
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


def cmd_min(a):
    """fuzz-run.py's minimiser, with the outcome replaced by (port site, C outcome)."""
    check_binaries()
    spec = importlib.util.spec_from_file_location("fuzzrun", ROOT / "scripts/fuzz-run.py")
    fr = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fr)

    def key(path):
        st, err = run([PORT], path, a.timeout)
        site = port_hit(st, err)
        return site, (c_outcome(run([C_BIN], path, a.timeout, C_ENV)[0]) if site else None)

    target = key(a.case)
    if target[0] is None:
        sys.exit("the port does not panic or hang on this case")
    fr.classify = lambda path, **kw: ("hit" if key(path) == target else "miss", {})
    res = fr.minimize(a.case, "hit")
    if res is None:
        sys.exit(1)
    if a.o:
        pathlib.Path(a.o).write_text(res, encoding="utf-8")
    else:
        print(res, end="")
    print(f"kept: {target[0]}, {target[1]}", file=sys.stderr)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--gen", type=pathlib.Path, required=True)
    r.add_argument("--seed", type=int, default=1)
    r.add_argument("--count", type=int, default=1000)
    r.add_argument("--out", type=pathlib.Path, required=True)
    r.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    r.add_argument("--timeout", type=int, default=TIMEOUT)
    s = sub.add_parser("summary")
    s.add_argument("out", type=pathlib.Path)
    m = sub.add_parser("min")
    m.add_argument("case")
    m.add_argument("-o")
    m.add_argument("--timeout", type=int, default=TIMEOUT)
    a = ap.parse_args()
    if getattr(a, "timeout", 1) < 1:
        ap.error("--timeout must be at least 1 second")
    {"run": cmd_run, "summary": lambda a: summarize(a.out), "min": cmd_min}[a.cmd](a)


if __name__ == "__main__":
    main()
