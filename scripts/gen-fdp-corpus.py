#!/usr/bin/env python3
"""Builds tests/fdp/*.gv: corpus graphs with `layout=fdp` forced, plus
hand-written cases that exercise fdp attributes (clusters, K, maxiter, start,
pos/pin, overlap, splines, sep/esep).

The generated files are checked in; rerun this only to change the corpus, then
regenerate the goldens with scripts/gen-golden.sh.

Usage: python3 -I scripts/gen-fdp-corpus.py
"""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fdp"

HEADER = re.compile(r"^\s*(strict\s+)?(di)?graph\b[^{]*\{", re.M | re.I)
LAYOUT = re.compile(r"\blayout\s*=", re.I)
CLUSTER = re.compile(r"subgraph\s*\"?cluster", re.I)

# C 16.1.0 aborts on these (assertion in compoundEdges).
EXCLUDE = {"hw_crossing_compound", "hw_deep_compound", "hw_compound_two"}


def force(src, engine="fdp"):
    m = HEADER.search(src)
    if not m:
        return None
    return src[: m.end()] + f"\nlayout={engine};\n" + src[m.end() :]


def corpus_cases():
    """Fixtures, undirected wild graphs and wild graphs with clusters."""
    out = {}
    for f in sorted((ROOT / "tests" / "fixtures").glob("*.gv")):
        out["fx_" + f.stem] = f.read_text(encoding="utf-8", errors="surrogateescape")
    for f in sorted((ROOT / "tests" / "wild").glob("*.gv")):
        src = f.read_text(encoding="utf-8", errors="surrogateescape")
        if re.search(r"^\s*(strict\s+)?graph\b", src, re.M | re.I) or CLUSTER.search(src):
            out["wild_" + f.stem] = src
    return out


def complete(n, extra=""):
    lines = [f"graph K{n} {{", extra]
    for i in range(n):
        for j in range(i + 1, n):
            lines.append(f"{i} -- {j};")
    lines.append("}")
    return "\n".join(lines)


def grid(n, m, extra=""):
    lines = [f"graph grid{n}x{m} {{", extra, "node [shape=point];"]
    for i in range(n):
        for j in range(m):
            if j + 1 < m:
                lines.append(f"n{i}_{j} -- n{i}_{j + 1};")
            if i + 1 < n:
                lines.append(f"n{i}_{j} -- n{i + 1}_{j};")
    lines.append("}")
    return "\n".join(lines)


PETERSEN = """graph petersen {
%s
node [shape=circle, width=0.4, fixedsize=true];
0 -- 1 -- 2 -- 3 -- 4 -- 0;
5 -- 7 -- 9 -- 6 -- 8 -- 5;
0 -- 5; 1 -- 6; 2 -- 7; 3 -- 8; 4 -- 9;
}"""

DISCONNECTED = """graph disc {
%s
a -- b -- c -- a;
d -- e -- f -- d;
g -- h;
i; j;
k -- l -- m -- n -- k;
}"""

CLUSTERS = """graph clus {
%s
subgraph cluster_a { a1 -- a2 -- a3 -- a1; }
subgraph cluster_b { b1 -- b2 -- b3; }
a1 -- b1;
c -- a2;
}"""

NESTED = """graph nested {
%s
subgraph cluster_outer {
  label="outer";
  o1 -- o2;
  subgraph cluster_inner {
    label="inner";
    i1 -- i2 -- i3;
  }
  o2 -- i1;
}
subgraph cluster_side { s1 -- s2; s2 -- s3; }
x -- o1;
x -- s1;
i3 -- s3;
y;
}"""

DEEP = """digraph deep {
%s
subgraph cluster_l1 {
  a;
  subgraph cluster_l2 {
    b;
    subgraph cluster_l3 { c -> d; }
  }
}
subgraph cluster_r { e -> f; }
a -> b -> c;
d -> e;
f -> a;
g -> d;
}"""

CROSSING = """graph cross {
%s
subgraph cluster_0 { a -- b -- c; }
subgraph cluster_1 { d -- e -- f; }
subgraph cluster_2 { g -- h; }
a -- d; b -- e; c -- f;
a -- g; f -- h; c -- h;
}"""

EMPTYCLUST = """graph emptyc {
%s
subgraph cluster_empty { label="nothing"; }
subgraph cluster_one { a; }
a -- b;
}"""

LABELS = """graph labels {
%s
label="graph label";
a -- b [label="ab"];
b -- c [label="bc"];
c -- a;
a [label="node A"]; b [shape=box]; c [shape=diamond];
}"""

POS = """graph pos {
%s
a [pos="0,0!"];
b [pos="2,0!"];
c [pos="1,1.5"];
d; e;
a -- b -- c -- a;
c -- d -- e -- a;
}"""

POSPIN = """graph pospin {
%s
node [pin=true];
a [pos="0,0"];
b [pos="100,0"];
c [pos="50,80"];
d [pin=false];
a -- b -- c -- a; c -- d; d -- a;
}"""

LENWEIGHT = """graph lenweight {
%s
a -- b [len=2];
b -- c [len=0.5];
c -- d [weight=5];
d -- a [len=3, weight=0.5];
a -- c;
b -- e [len=1.5];
}"""

WHEEL = """graph wheel {
%s
node [shape=box];
hub -- r1; hub -- r2; hub -- r3; hub -- r4; hub -- r5; hub -- r6;
r1 -- r2 -- r3 -- r4 -- r5 -- r6 -- r1;
}"""

OVERLAP = """graph ov {
%s
node [shape=box, width=1.2, height=0.6, fixedsize=true];
a -- b; a -- c; a -- d; a -- e; a -- f; b -- c; d -- e;
}"""

LOOPS = """graph loops {
%s
a -- a;
a -- b;
a -- b;
b -- c;
c -- c [label="self"];
}"""

DIGRAPH = """digraph dg {
%s
a -> b -> c -> d -> a;
a -> c;
b -> d [dir=both];
e -> a [arrowhead=none];
}"""


def handwritten():
    c = {}
    c["hw_petersen"] = PETERSEN % ""
    c["hw_K5"] = complete(5)
    c["hw_K8_box"] = complete(8, "node [shape=box];")
    c["hw_grid5x5"] = grid(5, 5)
    c["hw_grid8x8"] = grid(8, 8)
    c["hw_disconnected"] = DISCONNECTED % ""
    c["hw_disconnected_nopack"] = DISCONNECTED % "pack=false;"
    c["hw_disconnected_pack20"] = DISCONNECTED % "pack=20;"
    c["hw_disconnected_packarray"] = DISCONNECTED % "packmode=array;"
    c["hw_disconnected_packgraph"] = DISCONNECTED % 'packmode="graph";'
    c["hw_clusters"] = CLUSTERS % ""
    c["hw_clusters_labels"] = CLUSTERS.replace("a1 -- a2", "label=A; a1 -- a2").replace("b1 -- b2", "label=\"cluster B\"; labelloc=b; b1 -- b2") % ""
    c["hw_clusters_margin"] = CLUSTERS.replace("a1 -- a2", "margin=20; a1 -- a2") % ""
    c["hw_nested"] = NESTED % ""
    c["hw_nested_K1"] = NESTED % "K=1;"
    c["hw_nested_compound"] = NESTED % "splines=compound;"
    c["hw_nested_splines"] = NESTED % "splines=true;"
    c["hw_nested_polyline"] = NESTED % "splines=polyline;"
    c["hw_deep"] = DEEP % ""
    c["hw_deep_compound"] = DEEP % "splines=compound;"
    c["hw_crossing"] = CROSSING % ""
    c["hw_crossing_compound"] = CROSSING % "splines=compound;"
    c["hw_crossing_maxiter50"] = CROSSING % "maxiter=50;"
    c["hw_emptycluster"] = EMPTYCLUST % ""
    c["hw_labels"] = LABELS % ""
    c["hw_K_05"] = PETERSEN % "K=0.5;"
    c["hw_K_2"] = DISCONNECTED % "K=2;"
    c["hw_maxiter_10"] = PETERSEN % "maxiter=10;"
    c["hw_maxiter_1000"] = PETERSEN % "maxiter=1000;"
    c["hw_T0"] = PETERSEN % "T0=2;"
    c["hw_start_7"] = PETERSEN % "start=7;"
    c["hw_start_random42"] = PETERSEN % "start=random42;"
    c["hw_pos"] = POS % ""
    c["hw_pos_scale"] = POS % "inputscale=72;"
    c["hw_pospin"] = POSPIN % ""
    c["hw_lenweight"] = LENWEIGHT % ""
    c["hw_wheel"] = WHEEL % ""
    c["hw_loops"] = LOOPS % ""
    c["hw_digraph"] = DIGRAPH % ""
    for ov in ["false", "true", "scale", "prism", "0:", "3:", "20:prism", "0:prism", "compress", "scalexy"]:
        name = ov.replace(":", "_colon_") or "empty"
        c[f"hw_overlap_{name}"] = OVERLAP % f'overlap="{ov}";'
    c["hw_overlap_sep"] = OVERLAP % 'sep="+20";'
    c["hw_overlap_sep_scale"] = OVERLAP % 'sep="0.5";'
    for sp in ["true", "false", "polyline", "line", "curved", "none", "compound", "ortho"]:
        c[f"hw_splines_{sp}_wheel"] = WHEEL % f"splines={sp};"
    c["hw_splines_ortho_clusters"] = NESTED % "splines=ortho;"
    c["hw_esep"] = OVERLAP % "splines=true; esep=5;"
    c["hw_clusters_splines_true"] = CLUSTERS % "splines=true;"
    c["hw_clusters_compound"] = CLUSTERS % "splines=compound;"
    c["hw_clusters_pos"] = CLUSTERS.replace("a1 -- a2", 'a1 [pos="0,0!"]; a1 -- a2') % ""
    c["hw_dim3"] = PETERSEN % "dim=3;"
    c["hw_normalize"] = WHEEL % "normalize=true;"
    c["hw_size"] = PETERSEN % 'size="2,2";'
    c["hw_ratio_fill"] = PETERSEN % 'ratio=fill; size="4,2";'
    c["hw_single"] = "graph single { a; }"
    c["hw_two"] = "graph two { a -- b; }"
    c["hw_empty"] = "graph empty { }"
    c["hw_isolated"] = "graph iso { a; b; c; d; }"
    c["hw_strict_multi"] = "strict graph sm { a -- b; a -- b; b -- c; c -- a; }"
    c["hw_multiedge_cluster"] = """graph mc {
subgraph cluster_x { p; q; }
r -- p; r -- p; r -- q; s -- q; s -- r;
}"""
    c["hw_compound_mid"] = """graph c1 { splines=compound;
subgraph cluster_mid { m1 -- m2 -- m3 -- m1; m4 -- m1; }
a -- m1; b -- m2; a -- b; a -- c; c -- b; c -- m3; d -- a; d -- b; d -- c;
}"""
    c["hw_compound_two"] = """graph c4 { splines=compound;
subgraph cluster_l { l1 -- l2; l2 -- l3; }
subgraph cluster_r { r1 -- r2; }
l1 -- r1; l3 -- r2; x -- l2; x -- r2; y -- x; y -- l1;
}"""
    c["hw_clusteredge"] = """graph ce1 {
subgraph cluster_a { a1 -- a2; } subgraph cluster_b { b1; b2; }
x -- cluster_a; cluster_a -- cluster_b; cluster_b -- y; x -- y;
}"""
    c["hw_clusteredge_nested"] = """digraph ce2 { splines=true;
subgraph cluster_o { o1; subgraph cluster_i { i1 -> i2; } }
cluster_i -> z; z -> cluster_o; o1 -> i1;
}"""
    c["hw_clusteredge_inside"] = """digraph ce3 {
subgraph cluster_a { a1 -> a2; }
a1 -> cluster_a; cluster_a -> b; b -> cluster_a; cluster_a -> cluster_a;
}"""
    return c


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.gv"):
        old.unlink()
    n = 0
    for name, src in {**corpus_cases(), **handwritten()}.items():
        if name in EXCLUDE or LAYOUT.search(src):
            continue
        forced = force(src)
        if forced is None:
            continue
        (OUT / f"{name}.gv").write_text(forced, encoding="utf-8", errors="surrogateescape")
        n += 1
    print(f"wrote {n} fdp cases to {OUT}")


if __name__ == "__main__":
    main()
