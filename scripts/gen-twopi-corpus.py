#!/usr/bin/env python3
"""Builds tests/twopi/*.gv: corpus graphs with `layout=twopi` forced, plus
hand-written cases that exercise twopi attributes.

The generated files are checked in; rerun this only to change the corpus, then
regenerate the goldens with scripts/gen-golden.sh.

Usage: python3 -I scripts/gen-twopi-corpus.py
"""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "twopi"

HEADER = re.compile(r"^\s*(strict\s+)?(di)?graph\b[^{]*\{", re.M | re.I)
LAYOUT = re.compile(r"\blayout\s*=", re.I)

EXCLUDE = set()


def force_layout(src):
    m = HEADER.search(src)
    if not m:
        return None
    return src[: m.end()] + "\nlayout=twopi;\n" + src[m.end() :]


def corpus_cases():
    """Fixtures (any direction) and undirected wild graphs without their own layout."""
    out = {}
    for f in sorted((ROOT / "tests" / "fixtures").glob("*.gv")):
        out["fx_" + f.stem] = f.read_text(encoding="utf-8", errors="surrogateescape")
    for f in sorted((ROOT / "tests" / "wild").glob("*.gv")):
        src = f.read_text(encoding="utf-8", errors="surrogateescape")
        if re.search(r"^\s*(strict\s+)?graph\b", src, re.M | re.I):
            out["wild_" + f.stem] = src
    return out


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


def tree(depth, extra=""):
    lines = ["graph tree {", extra, "node [shape=circle, width=0.3, fixedsize=true, label=\"\"];"]
    k = 1
    frontier = [0]
    for _ in range(depth):
        nxt = []
        for p in frontier:
            for _ in range(2):
                lines.append(f"t{p} -- t{k};")
                nxt.append(k)
                k += 1
        frontier = nxt
    lines.append("}")
    return "\n".join(lines)


def complete(n, extra=""):
    lines = [f"graph K{n} {{", extra]
    for i in range(n):
        for j in range(i + 1, n):
            lines.append(f"{i} -- {j};")
    lines.append("}")
    return "\n".join(lines)


def cycle(n, extra=""):
    body = " -- ".join(str(i) for i in range(n)) + " -- 0;"
    return f"graph C{n} {{\n{extra}\n{body}\n}}"


PETERSEN = """graph petersen {
%s
node [shape=circle, width=0.4, fixedsize=true];
0 -- 1 -- 2 -- 3 -- 4 -- 0;
5 -- 7 -- 9 -- 6 -- 8 -- 5;
0 -- 5; 1 -- 6; 2 -- 7; 3 -- 8; 4 -- 9;
}"""

CUBE = """graph cube {
%s
a -- b -- c -- d -- a;
e -- f -- g -- h -- e;
a -- e; b -- f; c -- g; d -- h;
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

LOOPS = """graph loops {
%s
a -- a;
a -- a;
a -- b;
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

LABELS = """graph labels {
%s
a -- b [label="ab"];
b -- c [label="bc", headlabel="h", taillabel="t"];
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

BIGLABELS = """graph biglabel {
%s
"a long node name" -- "short";
"short" -- "another long label here";
"another long label here" -- "x";
"x" -- "a long node name";
"x" -- "y"; "y" -- "z"; "z" -- "short";
}"""


BLOCKS = """graph blocks {
%s
a -- b -- c -- d -- a; a -- c;
d -- e -- f -- g -- e;
g -- h; h -- i; h -- j;
c -- k -- l -- m -- k;
b -- n;
}"""

CUTV = """graph cutv {
%s
hub -- a1 -- a2 -- a3 -- hub;
hub -- b1 -- b2 -- hub;
hub -- c1 -- c2 -- c3 -- c4 -- hub;
hub -- d1;
hub -- e1 -- e2;
}"""

CHAIN_OF_CYCLES = """graph cc {
%s
a0 -- a1 -- a2 -- a3 -- a0;
a2 -- b0 -- b1 -- b2 -- b0;
b2 -- c0 -- c1 -- c2 -- c3 -- c4 -- c0;
c3 -- d0;
d0 -- e0 -- e1 -- e2 -- d0;
}"""

STAR = """graph star {
%s
c -- {a b d e f g h i};
}"""

CROSS = """graph cross {
%s
1 -- 2 -- 3 -- 4 -- 5 -- 6 -- 7 -- 8 -- 1;
1 -- 5; 2 -- 6; 3 -- 7; 4 -- 8; 1 -- 3; 5 -- 7;
}"""


DISCONNECTED_NOSINGLE = """graph disc2 {
%s
a -- b -- c -- a;
d -- e -- f -- d;
g -- h;
k -- l -- m -- n -- k;
}"""


def handwritten():
    c = {}
    c["hw_petersen"] = PETERSEN % ""
    c["hw_K5"] = complete(5)
    c["hw_K6_box"] = complete(6, "node [shape=box];")
    c["hw_K8"] = complete(8)
    c["hw_C12"] = cycle(12)
    c["hw_C3"] = cycle(3)
    c["hw_cube"] = CUBE % ""
    c["hw_grid4x4"] = grid(4, 4)
    c["hw_grid6x3"] = grid(6, 3)
    c["hw_tree3"] = tree(3)
    c["hw_tree5"] = tree(5)
    c["hw_star"] = STAR % ""
    c["hw_blocks"] = BLOCKS % ""
    c["hw_cutv"] = CUTV % ""
    c["hw_chain_cycles"] = CHAIN_OF_CYCLES % ""
    c["hw_cross"] = CROSS % ""
    c["hw_disconnected"] = DISCONNECTED % ""
    c["hw_disconnected_pack10"] = DISCONNECTED % "pack=10;"
    c["hw_disconnected_packgraph"] = DISCONNECTED % "pack=true; packmode=\"graph\";"
    c["hw_disconnected_packarray"] = DISCONNECTED % "pack=true; packmode=array;"
    c["hw_clusters"] = CLUSTERS % ""
    c["hw_loops"] = LOOPS % ""
    c["hw_loops_splines"] = LOOPS % "splines=true;"
    c["hw_digraph"] = DIGRAPH % ""
    c["hw_labels"] = LABELS % ""
    c["hw_wheel"] = WHEEL % ""
    c["hw_biglabels"] = BIGLABELS % ""
    c["hw_lenweight"] = LENWEIGHT % ""
    for ov in ["false", "scale", "prism", "true"]:
        c[f"hw_overlap_{ov}"] = OVERLAP % f"overlap={ov};"
        c[f"hw_overlap_{ov}_blocks"] = BLOCKS % f"overlap={ov}; node [shape=box, width=1.5];"
    c["hw_overlap_false_disconnected"] = DISCONNECTED % "overlap=false; node [width=2];"
    for sp in ["true", "false", "polyline", "line", "curved"]:
        c[f"hw_splines_{sp}_wheel"] = WHEEL % f"splines={sp};"
        c[f"hw_splines_{sp}_blocks"] = BLOCKS % f"splines={sp};"
    for rs in ["0.5", "2", "1:2", "0.3:1.5:0.8", "0.01", "2 : 0.5"]:
        c["hw_ranksep_" + re.sub(r"[^0-9a-z]+", "_", rs)] = tree(4, f"ranksep=\"{rs}\";")
    c["hw_ranksep_blocks"] = BLOCKS % "ranksep=\"1.5:0.5\";"
    c["hw_root_graph"] = BLOCKS % "root=h;"
    c["hw_root_graph_tree"] = tree(4, "root=t5;")
    c["hw_root_graph_cutv"] = CUTV % "root=c2;"
    c["hw_root_missing"] = BLOCKS % "root=nosuchnode;"
    c["hw_root_empty"] = BLOCKS % "root=\"\";"
    c["hw_root_empty_disconnected"] = DISCONNECTED % "root=\"\";"
    c["hw_root_node"] = BLOCKS % "k [root=true];"
    c["hw_root_node_tree"] = tree(3, "t9 [root=true];")
    # A node `root` attribute with isolated nodes crashes C 16.1.0 (agxset on a
    # NULL centre), so these use the disconnected graph without singletons.
    c["hw_root_node_disconnected"] = DISCONNECTED_NOSINGLE % "e [root=true]; node [root=false];"
    c["hw_root_both_disconnected"] = DISCONNECTED_NOSINGLE % "root=b; m [root=true];"
    c["hw_weight0"] = BLOCKS % "a -- c [weight=0]; e -- f [weight=0];"
    c["hw_weight0_cycle"] = cycle(8, "edge [weight=0]; 0 -- 4 [weight=1]; 0 -- 1 [weight=1];")
    c["hw_weights"] = BLOCKS % "edge [weight=3]; a -- b [weight=0];"
    c["hw_normalize"] = CUBE % "normalize=true;"
    c["hw_rotate"] = CUBE % "rotate=90;"
    c["hw_ratio_fill"] = CUBE % "ratio=fill; size=\"4,2\";"
    c["hw_size"] = PETERSEN % "size=\"2,2\";"
    c["hw_single"] = "graph single { a; }"
    c["hw_two"] = "graph two { a -- b; }"
    c["hw_empty"] = "graph empty { }"
    c["hw_isolated"] = "graph iso { a; b; c; d; }"
    c["hw_K33"] = """graph K33 { a -- {x y z}; b -- {x y z}; c -- {x y z}; }"""
    c["hw_strict_multi"] = "strict graph sm { a -- b; a -- b; b -- c; c -- a; }"
    c["hw_reverse_multi"] = "digraph rm { a -> b; b -> a; b -> c; c -> b; c -> a; d -> a; d -> d; }"
    return c


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.gv"):
        old.unlink()
    n = 0
    for name, src in {**corpus_cases(), **handwritten()}.items():
        if name in EXCLUDE or LAYOUT.search(src):
            continue
        forced = force_layout(src)
        if forced is None:
            continue
        (OUT / f"{name}.gv").write_text(forced, encoding="utf-8", errors="surrogateescape")
        n += 1
    print(f"wrote {n} twopi cases to {OUT}")


if __name__ == "__main__":
    main()
