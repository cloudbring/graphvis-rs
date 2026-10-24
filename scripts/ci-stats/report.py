#!/usr/bin/env python3
"""Reads what ci-stats/collect.py downloaded and reports how long a change waits for CI.

    scripts/ci-stats/report.py [--dir target/ci-stats] [--since YYYY-MM-DD] [--html FILE]

Prints the measures below and, with --html, writes them as one self-contained page.

  feedback     per pushed commit, minutes from the push to the last required check passing,
               and to every check finishing (what `gh pr checks --watch` waits for)
  cycles       per merged pull request, the commits that ran CI, the force-pushes (rebases)
  lead time    per merged pull request, minutes with CI running, without (the agent working or
               waiting for review), and in the merge queue
  critical     per job, the median minutes on pull requests and whether a merge waits for it
  yield        per job, failures on pull requests and in the queue against runner minutes:
               what each job catches for the time it costs
"""
import argparse
import collections as C
import datetime as dt
import html
import json
import pathlib
import statistics as st

# Checks the master ruleset requires (gh api repos/OWNER/REPO/rules/branches/master).
REQUIRED = {
    "Format, Clippy, Test & WASM Check",
    "WASM end-to-end (Node)",
    "Supply chain (cargo-deny, zizmor)",
    "Compare every platform's output",
    "Build the documentation site",
}
# The required checks wait on these jobs (cross-platform.yml: compare needs native and wasm).
FEEDS_REQUIRED = {"Linux x86-64", "Linux arm64", "Windows x86-64", "macOS arm64",
                  "WebAssembly (Node.js)"}
GATED = ("CI", "Cross-platform", "Fuzzing")


def t(s):
    return dt.datetime.fromisoformat(s.replace("Z", "+00:00")) if s else None


def mins(a, b):
    return (b - a).total_seconds() / 60


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p * len(xs)))] if xs else 0.0


def union_minutes(spans, lo, hi):
    spans = sorted((max(a, lo), min(b, hi)) for a, b in spans if b > lo and a < hi)
    total, cur = 0.0, None
    for a, b in spans:
        if cur and a <= cur[1]:
            cur = (cur[0], max(cur[1], b))
        else:
            if cur:
                total += mins(*cur)
            cur = (a, b)
    return total + (mins(*cur) if cur else 0.0)


def feedback(runs, since):
    """Per (day, commit) on pull requests: minutes to required green and to all checks done."""
    by = C.defaultdict(list)
    for r in runs:
        if r["event"] == "pull_request" and r["name"] in GATED and t(r["created_at"]) >= since:
            by[r["head_sha"]].append(r)
    out, cancelled = [], 0
    for rs in by.values():
        if any(r["conclusion"] == "cancelled" for r in rs):
            cancelled += 1
            continue
        if any(r["conclusion"] == "failure" for r in rs):
            continue
        jobs = [j for r in rs for j in r["jobs"] if j["completed_at"]]
        req = [j for j in jobs if j["name"] in REQUIRED]
        if {j["name"] for j in req} != REQUIRED:
            continue
        t0 = min(t(r["created_at"]) for r in rs)
        last = max(jobs, key=lambda j: j["completed_at"])
        out.append({"day": t0.date().isoformat(),
                    "required": mins(t0, max(t(j["completed_at"]) for j in req)),
                    "all": mins(t0, t(last["completed_at"])),
                    "last": last["name"]})
    return out, cancelled


def jobs_table(runs, since):
    dur, fails, total, minutes = C.defaultdict(list), C.Counter(), C.Counter(), C.Counter()
    for r in runs:
        if r["event"] not in ("pull_request", "merge_group") or t(r["created_at"]) < since:
            continue
        for j in r["jobs"]:
            if not (j["started_at"] and j["completed_at"]) or j["conclusion"] not in (
                    "success", "failure"):
                continue
            d = mins(t(j["started_at"]), t(j["completed_at"]))
            if d < 0.05:  # skipped by the result cache
                continue
            k = (r["name"], j["name"])
            total[k] += 1
            minutes[k] += d
            fails[k] += j["conclusion"] == "failure"
            if r["event"] == "pull_request" and j["conclusion"] == "success":
                dur[k].append(d)
    rows = []
    for k in total:
        if total[k] < 5 or k[0] not in GATED:
            continue
        rows.append({"workflow": k[0], "job": k[1], "runs": total[k], "failures": fails[k],
                     "minutes": minutes[k], "p50": st.median(dur[k]) if dur[k] else 0.0,
                     "blocks": k[1] in REQUIRED or k[1] in FEEDS_REQUIRED})
    return sorted(rows, key=lambda r: -r["p50"])


def lead_times(runs, prs, since):
    spans = C.defaultdict(list)
    for r in runs:
        if r["event"] == "pull_request" and r["name"] in GATED:
            spans[r["head_branch"]].append((t(r["created_at"]), t(r["updated_at"])))
    shas = C.defaultdict(set)
    for r in runs:
        if r["event"] == "pull_request" and r["name"] == "CI":
            shas[r["head_branch"]].add((r["head_sha"], t(r["created_at"])))
    out = []
    for p in prs:
        c, m = t(p["createdAt"]), t(p["mergedAt"])
        if m < since:
            continue
        tl = p["timeline"]
        queued = [t(e["createdAt"]) for e in tl if e["__typename"] == "AddedToMergeQueueEvent"]
        q = queued[-1] if queued else m
        ci = union_minutes(spans[p["headRefName"]], c, q)
        out.append({
            "pr": p["number"], "title": p["title"], "total": mins(c, m), "ci": ci,
            "other": mins(c, q) - ci, "queue": mins(q, m),
            "cycles": sum(1 for _, at in shas[p["headRefName"]] if c - dt.timedelta(minutes=5)
                          <= at <= m),
            "force_pushes": sum(e["__typename"] == "HeadRefForcePushedEvent" for e in tl),
        })
    return out


def summarize(fb, cancelled, jobs, lt):
    req = [f["required"] for f in fb]
    allc = [f["all"] for f in fb]
    s = {
        "commits": len(fb), "cancelled": cancelled,
        "required_p50": st.median(req) if req else 0, "required_p90": pct(req, .9),
        "all_p50": st.median(allc) if allc else 0, "all_p90": pct(allc, .9),
        "last_to_finish": C.Counter(f["last"] for f in fb).most_common(3),
        "merged": len(lt),
        "cycles_mean": st.mean(x["cycles"] for x in lt) if lt else 0,
        "force_pushes": sum(x["force_pushes"] for x in lt),
        "lead_p50": st.median(x["total"] for x in lt) if lt else 0,
        "share": {k: sum(x[k] for x in lt) / max(1e-9, sum(x["total"] for x in lt))
                  for k in ("ci", "other", "queue")},
        "failures": sum(j["failures"] for j in jobs),
        "job_runs": sum(j["runs"] for j in jobs),
    }
    return s


def text(s, jobs):
    p = print
    p(f"Feedback, {s['commits']} commits that finished ({s['cancelled']} cancelled by a newer push):")
    p(f"  push -> required checks green  p50 {s['required_p50']:5.1f} min   p90 {s['required_p90']:5.1f}")
    p(f"  push -> every check finished   p50 {s['all_p50']:5.1f} min   p90 {s['all_p90']:5.1f}")
    p(f"  last to finish: {s['last_to_finish']}")
    p(f"Merged pull requests: {s['merged']}, lead time p50 {s['lead_p50']:.0f} min;"
      f" {s['cycles_mean']:.1f} CI cycles each; {s['force_pushes']} force-pushes")
    p("  share of lead time: " + ", ".join(f"{k} {v:.0%}" for k, v in s["share"].items()))
    p(f"Failures: {s['failures']} of {s['job_runs']} job runs (pull requests and the queue)")
    p(f"  {'job':45} {'p50':>5} {'runs':>5} {'fail':>4} {'runner-min':>10}  blocks merge")
    for j in jobs:
        p(f"  {j['job'][:45]:45} {j['p50']:5.1f} {j['runs']:5} {j['failures']:4}"
          f" {j['minutes']:10.0f}  {'yes' if j['blocks'] else ''}")


# The reference categorical palette (dataviz skill), light and dark steps; slots 1-4.
CSS = """
.viz-root{color-scheme:light;--surface:#fcfcfb;--ink:#0b0b0b;--ink2:#52514e;--grid:#e6e5e1;
--s1:#2a78d6;--s2:#eb6834;--s3:#1baf7a;--s4:#eda100;font:14px/1.45 system-ui,sans-serif;
background:var(--surface);color:var(--ink);padding:24px;max-width:980px;margin:auto}
@media (prefers-color-scheme:dark){:root:where(:not([data-theme="light"])) .viz-root{
color-scheme:dark;--surface:#1a1a19;--ink:#f0efec;--ink2:#c3c2b7;--grid:#33332f;
--s1:#3987e5;--s2:#d95926;--s3:#199e70;--s4:#c98500}}
h1{font-size:20px;margin:0 0 4px}h2{font-size:15px;margin:28px 0 8px}
.sub{color:var(--ink2);margin:0 0 16px}.tiles{display:grid;grid-template-columns:repeat(4,1fr);gap:12px}
.tile{border:1px solid var(--grid);border-radius:8px;padding:12px}.tile b{font-size:26px;display:block}
.tile span{color:var(--ink2);font-size:12px}table{border-collapse:collapse;width:100%;font-size:13px}
td,th{padding:4px 8px;border-bottom:1px solid var(--grid);text-align:right}td:first-child,th:first-child{text-align:left}
th{color:var(--ink2);font-weight:500}svg text{fill:var(--ink2);font-size:11px}.legend{display:flex;gap:16px;
color:var(--ink2);font-size:12px;margin:4px 0}.sw{display:inline-block;width:10px;height:10px;border-radius:2px;margin-right:4px}
rect:hover,circle:hover{opacity:.8}
"""


def esc(x):
    return html.escape(str(x))


def trend_svg(fb):
    days = sorted({f["day"] for f in fb})
    if not days:
        return ""
    pts = {d: [f["required"] for f in fb if f["day"] == d] for d in days}
    series = [("p50", "var(--s1)", lambda v: st.median(v)), ("p90", "var(--s2)", lambda v: pct(v, .9))]
    W, H, L, B = 900, 220, 40, 30
    top = max(40, max(pct(v, .9) for v in pts.values()) * 1.1)
    x = lambda i: L + (W - L - 60) * (i / max(1, len(days) - 1))
    y = lambda v: H - B - (H - B - 10) * v / top
    g = [f'<line x1="{L}" x2="{W-60}" y1="{y(v)}" y2="{y(v)}" stroke="var(--grid)"/>'
         f'<text x="{L-6}" y="{y(v)+4}" text-anchor="end">{v}</text>' for v in range(0, int(top) + 1, 10)]
    g += [f'<text x="{x(i)}" y="{H-10}" text-anchor="middle">{d[5:]}</text>' for i, d in enumerate(days)]
    for name, col, f in series:
        xy = [(x(i), y(f(pts[d]))) for i, d in enumerate(days)]
        g.append(f'<polyline fill="none" stroke="{col}" stroke-width="2" points="'
                 + " ".join(f"{a:.1f},{b:.1f}" for a, b in xy) + '"/>')
        for (a, b), d in zip(xy, days):
            g.append(f'<circle cx="{a:.1f}" cy="{b:.1f}" r="5" fill="{col}" stroke="var(--surface)" '
                     f'stroke-width="2"><title>{d} {name}: {f(pts[d]):.1f} min '
                     f'({len(pts[d])} commits)</title></circle>')
        g.append(f'<text x="{xy[-1][0]+10}" y="{xy[-1][1]+4}" style="fill:var(--ink)">{name}</text>')
    return f'<svg viewBox="0 0 {W} {H}" width="100%" role="img" aria-label="Minutes from push to required checks green, per day">{"".join(g)}</svg>'


def jobs_svg(jobs):
    rows = [j for j in jobs if j["p50"] >= 0.5]
    W, rh, L = 900, 24, 300
    top = max(j["p50"] for j in rows) * 1.05
    g = []
    for i, j in enumerate(rows):
        w = (W - L - 50) * j["p50"] / top
        col = "var(--s1)" if j["blocks"] else "var(--s4)"
        g.append(f'<text x="{L-8}" y="{i*rh+16}" text-anchor="end" style="fill:var(--ink)">'
                 f'{esc(j["workflow"])} / {esc(j["job"][:34])}</text>'
                 f'<rect x="{L}" y="{i*rh+4}" width="{w:.1f}" height="16" rx="4" fill="{col}">'
                 f'<title>{esc(j["job"])}: median {j["p50"]:.1f} min, {j["failures"]} failures in '
                 f'{j["runs"]} runs</title></rect>'
                 f'<text x="{L+w+6:.1f}" y="{i*rh+16}">{j["p50"]:.1f}</text>')
    return (f'<div class="legend"><span><i class="sw" style="background:var(--s1)"></i>a merge waits for it</span>'
            f'<span><i class="sw" style="background:var(--s4)"></i>advisory (agents still wait with --watch)</span></div>'
            f'<svg viewBox="0 0 {W} {len(rows)*rh+8}" width="100%" role="img" aria-label="Median job minutes on pull requests">{"".join(g)}</svg>')


def lead_svg(lt):
    rows = sorted(lt, key=lambda x: -x["total"])[:40]
    W, rh, L = 900, 14, 60
    top = max(x["total"] for x in rows)
    parts = [("ci", "var(--s1)", "CI running"), ("other", "var(--s2)", "no CI running (agent working, review)"),
             ("queue", "var(--s3)", "merge queue")]
    g = []
    for i, x in enumerate(rows):
        g.append(f'<text x="{L-6}" y="{i*rh+11}" text-anchor="end">#{x["pr"]}</text>')
        at = L
        for k, col, name in parts:
            w = (W - L - 10) * x[k] / top
            if w > 2.5:
                g.append(f'<rect x="{at:.1f}" y="{i*rh+2}" width="{w-2:.1f}" height="10" rx="2" fill="{col}">'
                         f'<title>#{x["pr"]} {esc(x["title"][:60])}: {name} {x[k]:.0f} min '
                         f'of {x["total"]:.0f}; {x["cycles"]} CI cycles</title></rect>')
            at += w
    legend = "".join(f'<span><i class="sw" style="background:{c}"></i>{n}</span>' for _, c, n in parts)
    return (f'<div class="legend">{legend}</div><svg viewBox="0 0 {W} {len(rows)*rh+4}" width="100%" '
            f'role="img" aria-label="Lead time of the slowest merged pull requests">{"".join(g)}</svg>')


def page(s, fb, jobs, lt, since):
    tiles = [
        (f'{s["required_p50"]:.0f} / {s["required_p90"]:.0f} min', "push → required checks green, p50 / p90"),
        (f'{s["cycles_mean"]:.1f}', f'CI cycles per merged PR ({s["force_pushes"]} force-pushes)'),
        (f'{s["cancelled"]}', f'commits whose CI a newer push cancelled (vs {s["commits"]} finished)'),
        (f'{s["failures"]} / {s["job_runs"]}', "job runs that failed: what CI caught"),
    ]
    tiles_html = "".join(f'<div class="tile"><b>{esc(a)}</b><span>{esc(b)}</span></div>' for a, b in tiles)
    table = "".join(
        f'<tr><td>{esc(j["workflow"])} / {esc(j["job"])}</td><td>{j["p50"]:.1f}</td><td>{j["runs"]}</td>'
        f'<td>{j["failures"]}</td><td>{j["minutes"]:.0f}</td>'
        f'<td>{(1000*j["failures"]/j["minutes"]) if j["minutes"] else 0:.1f}</td><td>{"yes" if j["blocks"] else ""}</td></tr>'
        for j in jobs if j["p50"] >= 0.3 or j["failures"])
    share = " · ".join(f"{n} {s['share'][k]:.0%}" for k, n in
                       (("ci", "CI running"), ("other", "no CI running"), ("queue", "merge queue")))
    return f"""<!doctype html><meta charset="utf-8"><title>CI feedback, graphviz-pure</title>
<style>{CSS}</style><div class="viz-root">
<h1>How long a change waits for CI</h1>
<p class="sub">graphviz-pure, runs and merged pull requests since {since.date()} · scripts/ci-stats</p>
<div class="tiles">{tiles_html}</div>
<h2>Push → required checks green, minutes per day</h2>{trend_svg(fb)}
<h2>Median job minutes on pull requests: the critical path</h2>{jobs_svg(jobs)}
<h2>Lead time of the 40 slowest merged pull requests</h2>
<p class="sub">Share of all merged PRs' lead time: {share}</p>{lead_svg(lt)}
<h2>What each job catches for its cost</h2>
<table><tr><th>job</th><th>p50 min</th><th>runs</th><th>failures</th><th>runner-min</th><th>failures / 1000 min</th><th>blocks merge</th></tr>{table}</table>
</div>"""


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", type=pathlib.Path, default=pathlib.Path("target/ci-stats"))
    ap.add_argument("--since", type=lambda s: dt.datetime.fromisoformat(s + "T00:00:00+00:00"),
                    default=dt.datetime.now(dt.timezone.utc) - dt.timedelta(days=2))
    ap.add_argument("--html", type=pathlib.Path)
    a = ap.parse_args()
    runs = [json.loads(line) for line in (a.dir / "runs.jsonl").open()]
    prs = [json.loads(line) for line in (a.dir / "prs.jsonl").open()]
    fb, cancelled = feedback(runs, a.since)
    jobs = jobs_table(runs, a.since)
    lt = lead_times(runs, prs, a.since)
    s = summarize(fb, cancelled, jobs, lt)
    text(s, jobs)
    if a.html:
        a.html.write_text(page(s, fb, jobs, lt, a.since))
        print(f"wrote {a.html}")


if __name__ == "__main__":
    main()
