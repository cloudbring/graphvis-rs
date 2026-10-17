#!/usr/bin/env python3
"""Builds tests/neato/*.gv: corpus graphs with `layout=neato` forced, plus
hand-written cases that exercise neato attributes.

The generated files are checked in; rerun this only to change the corpus, then
regenerate the goldens with scripts/gen-golden.sh.

Usage: python3 -I scripts/gen-neato-corpus.py
"""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "neato"

HEADER = re.compile(r"^\s*(strict\s+)?(di)?graph\b[^{]*\{", re.M | re.I)
LAYOUT = re.compile(r"\blayout\s*=", re.I)

# C output for these is not reproducible run to run (multispline reads
# uninitialised state after a Pshortestpath failure), so they cannot be goldens.
EXCLUDE = {"wild_gv16_undirected_ngk10_4_793340"}


def force_neato(src):
    m = HEADER.search(src)
    if not m:
        return None
    return src[: m.end()] + "\nlayout=neato;\n" + src[m.end() :]


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


def handwritten():
    c = {}
    c["hw_petersen"] = PETERSEN % ""
    c["hw_K5"] = complete(5)
    c["hw_K6_box"] = complete(6, "node [shape=box];")
    c["hw_C12"] = cycle(12)
    c["hw_cube"] = CUBE % ""
    c["hw_grid4x4"] = grid(4, 4)
    c["hw_grid6x3"] = grid(6, 3)
    c["hw_tree3"] = tree(3)
    c["hw_tree5"] = tree(5)
    c["hw_disconnected"] = DISCONNECTED % ""
    c["hw_disconnected_nopack"] = DISCONNECTED % "pack=false;"
    c["hw_disconnected_pack10"] = DISCONNECTED % "pack=10;"
    c["hw_disconnected_packgraph"] = DISCONNECTED % "pack=true; packmode=\"graph\";"
    c["hw_disconnected_packarray"] = DISCONNECTED % "pack=true; packmode=array;"
    c["hw_clusters"] = CLUSTERS % ""
    c["hw_loops"] = LOOPS % ""
    c["hw_loops_splines"] = LOOPS % "splines=true;"
    c["hw_digraph"] = DIGRAPH % ""
    c["hw_labels"] = LABELS % ""
    c["hw_pos"] = POS % ""
    c["hw_pos_scale"] = POS % "inputscale=72;"
    c["hw_pospin"] = POSPIN % ""
    c["hw_pospin_notranslate"] = POSPIN % "notranslate=true;"
    c["hw_lenweight"] = LENWEIGHT % ""
    c["hw_wheel"] = WHEEL % ""
    c["hw_biglabels"] = BIGLABELS % ""
    for mode in ["KK", "sgd", "hier", "major"]:
        c[f"hw_mode_{mode}_petersen"] = PETERSEN % f"mode={mode};"
        c[f"hw_mode_{mode}_lenweight"] = LENWEIGHT % f"mode={mode};"
    c["hw_mode_KK_disconnected"] = DISCONNECTED % "mode=KK;"
    c["hw_mode_KK_grid"] = grid(4, 4, "mode=KK;")
    c["hw_mode_sgd_tree"] = tree(4, "mode=sgd;")
    for model in ["circuit", "subset", "mds", "shortpath"]:
        c[f"hw_model_{model}_petersen"] = PETERSEN % f"model={model};"
        c[f"hw_model_{model}_lenweight"] = LENWEIGHT % f"model={model};"
    # model=circuit with mode=KK segfaults in C 16.1.0; not included.
    for ov in ["false", "scale", "scalexy", "compress", "prism", "voronoi", "ortho", "orthoxy", "vpsc", "true", "ipsep"]:
        c[f"hw_overlap_{ov}"] = OVERLAP % f"overlap={ov};"
    c["hw_overlap_false_sep"] = OVERLAP % "overlap=false; sep=\"+10\";"
    c["hw_overlap_prism0"] = OVERLAP % "overlap=prism0;"
    c["hw_overlap_scale_biglabels"] = BIGLABELS % "overlap=scale;"
    c["hw_overlap_false_biglabels"] = BIGLABELS % "overlap=false;"
    c["hw_overlap_false_K6"] = complete(6, "overlap=false; node [shape=box];")
    for sp in ["true", "false", "polyline", "line", "curved"]:
        c[f"hw_splines_{sp}_wheel"] = WHEEL % f"splines={sp};"
        c[f"hw_splines_{sp}_cube"] = CUBE % f"splines={sp};"
    c["hw_splines_true_overlap_false"] = OVERLAP % "splines=true; overlap=false;"
    c["hw_start_7"] = PETERSEN % "start=7;"
    c["hw_start_random3"] = PETERSEN % "start=random3;"
    c["hw_start_self"] = PETERSEN % "start=self;"
    c["hw_start_regular"] = PETERSEN % "start=regular;"
    c["hw_KK_start_42"] = CUBE % "mode=KK; start=42;"
    c["hw_epsilon"] = PETERSEN % "epsilon=0.01;"
    c["hw_maxiter"] = PETERSEN % "maxiter=5;"
    c["hw_KK_maxiter"] = PETERSEN % "mode=KK; maxiter=20;"
    c["hw_KK_damping"] = PETERSEN % "mode=KK; Damping=0.5;"
    c["hw_defaultdist"] = DISCONNECTED % "pack=false; defaultdist=3;"
    c["hw_defaultdist_KK"] = DISCONNECTED % "pack=false; mode=KK; defaultdist=2;"
    c["hw_normalize"] = CUBE % "normalize=true;"
    c["hw_normalize_30"] = CUBE % "normalize=30;"
    c["hw_rotate"] = CUBE % "rotate=90;"
    c["hw_landscape"] = CUBE % "landscape=true;"
    c["hw_ratio_fill"] = CUBE % "ratio=fill; size=\"4,2\";"
    c["hw_ratio_compress"] = CUBE % "ratio=compress; size=\"2,2\";"
    c["hw_ratio_2"] = CUBE % "ratio=2;"
    c["hw_size"] = PETERSEN % "size=\"2,2\";"
    c["hw_esep"] = OVERLAP % "splines=true; esep=5;"
    c["hw_dim3"] = CUBE % "dim=3;"
    c["hw_single"] = "graph single { a; }"
    c["hw_two"] = "graph two { a -- b; }"
    c["hw_empty"] = "graph empty { }"
    c["hw_isolated"] = "graph iso { a; b; c; d; }"
    c["hw_K33"] = """graph K33 { a -- {x y z}; b -- {x y z}; c -- {x y z}; }"""
    c["hw_strict_multi"] = "strict graph sm { a -- b; a -- b; b -- c; c -- a; }"
    return c

RECORDS = """digraph rec {
%s
node [shape=record];
s1 [label="<f0> left|<f1> mid|<f2> right"];
s2 [label="<f0> one|<f1> two"];
s3 [label="hello\\nworld |{ b |{c|<here> d|e}| f}| g | h"];
s1:f1 -> s2:f0;
s1:f2 -> s3:here;
s2:f1 -> s3:f0;
}"""

XLABELS = """graph xl {
%s
a [xlabel="ext a"]; b [xlabel="ext b"]; c;
a -- b [xlabel="ex"];
b -- c [label="mid", labelangle=30, headlabel="H", taillabel="T"];
c -- a [label="ca"];
}"""

def randgraph(n, m, seed, extra=""):
    import random
    r = random.Random(seed)
    lines = [f"graph r{n}_{seed} {{", extra]
    edges = set()
    for i in range(1, n):
        j = r.randrange(i)
        edges.add((j, i))
    while len(edges) < m:
        a, b = r.randrange(n), r.randrange(n)
        if a != b:
            edges.add((min(a, b), max(a, b)))
    for a, b in sorted(edges):
        lines.append(f"n{a} -- n{b};")
    lines.append("}")
    return "\n".join(lines)


def handwritten2():
    c = {}
    c["hw2_overlap_voronoi_K6"] = complete(6, "overlap=voronoi; node [shape=box];")
    c["hw2_overlap_voronoi_biglabels"] = BIGLABELS % "overlap=voronoi;"
    c["hw2_overlap_oscale"] = OVERLAP % "overlap=oscale;"
    c["hw2_overlap_scale_K6"] = complete(6, "overlap=scale; node [shape=box];")
    c["hw2_overlap_scalexy_K6"] = complete(6, "overlap=scalexy; node [shape=box];")
    c["hw2_overlap_false_grid"] = grid(5, 5, "overlap=false; node [shape=box, width=0.6];")
    c["hw2_overlap_false_tree"] = tree(4, "overlap=false; node [width=0.6, fixedsize=false];")
    c["hw2_records"] = RECORDS % ""
    c["hw2_records_splines"] = RECORDS % "splines=true;"
    c["hw2_xlabels"] = XLABELS % ""
    c["hw2_xlabels_noforce"] = XLABELS % "forcelabels=false;"
    c["hw2_concentrate"] = "digraph conc { concentrate=true; a -> b; b -> a; a -> c; c -> b; }"
    c["hw2_KK_subset"] = PETERSEN % "mode=KK; model=subset;"
    c["hw2_sgd_subset"] = PETERSEN % "mode=sgd; model=subset;"
    c["hw2_KK_pos"] = POS % "mode=KK;"
    c["hw2_pin_components"] = """graph pc {
a [pos="0,0!"]; b [pos="1,1!"]; a -- b;
c -- d -- e -- c;
f -- g;
}"""
    c["hw2_packclust"] = CLUSTERS % "pack=true; packmode=\"clust\";"
    c["hw2_packarray_u"] = DISCONNECTED % "pack=true; packmode=\"array_u\";"
    c["hw2_packarray_c2"] = DISCONNECTED % "pack=true; packmode=\"array_c2\";"
    for i, (n, m) in enumerate([(30, 40), (50, 70), (80, 120), (120, 160)]):
        c[f"hw2_random_{n}"] = randgraph(n, m, 17 + i)
    c["hw2_random_30_splines"] = randgraph(30, 40, 17, "splines=true;")
    c["hw2_random_50_overlap"] = randgraph(50, 70, 18, "overlap=false; node [shape=box];")
    c["hw2_random_30_KK"] = randgraph(30, 40, 17, "mode=KK;")
    c["hw2_random_30_sgd"] = randgraph(30, 40, 17, "mode=sgd;")
    c["hw2_random_30_circuit"] = randgraph(30, 40, 17, "model=circuit;")
    c["hw2_random_30_self"] = randgraph(30, 40, 17, "start=self;")
    c["hw2_graph_label"] = CUBE % "label=\"A cube\"; labelloc=t;"
    c["hw2_graph_label_wide"] = "graph gl { label=\"a very very long graph label indeed\"; a -- b; }"
    c["hw2_cluster_label"] = CLUSTERS % "subgraph cluster_c { label=\"C\"; x -- y; }"
    c["hw2_ports_compass"] = "graph pcmp { node [shape=box]; a:n -- b:s; b:e -- c:w; c:ne -- a:sw; a:e -- a:w; }"
    c["hw2_multi_splines"] = "graph ms { splines=true; a -- b; a -- b; a -- b; b -- c; }"
    c["hw2_shapes_mix"] = """graph sm { a [shape=triangle]; b [shape=hexagon]; c [shape=star]; d [shape=doublecircle]; e [shape=cylinder]; f [shape=point]; a -- b -- c -- d -- e -- f -- a; a -- d; }"""
    c["hw2_penwidth_arrows"] = "digraph pa { edge [penwidth=3, arrowsize=1.5]; a -> b [arrowhead=diamond]; b -> c [arrowhead=dot]; c -> a [arrowhead=crow, dir=both, arrowtail=tee]; }"
    return c


ORTHO_MODES = ["ortho", "ortho_yx", "orthoxy", "orthoyx", "portho", "portho_yx", "porthoxy", "porthoyx"]


def handwritten3():
    """Cases for constraint.c (ortho*), vpsc, mode=hier/ipsep and multisplines."""
    c = {}
    for ov in ORTHO_MODES:
        c[f"hw3_{ov}_overlap"] = OVERLAP % f"overlap={ov};"
        c[f"hw3_{ov}_biglabels"] = BIGLABELS % f"overlap={ov};"
        c[f"hw3_{ov}_K6"] = complete(6, f"overlap={ov}; node [shape=box];")
        c[f"hw3_{ov}_random50"] = randgraph(50, 70, 18, f"overlap={ov}; node [shape=box];")
    # equal coordinates share a constraint node (grid), digraph edge lookup,
    # additive and multiplicative sep
    c["hw3_ortho_grid"] = grid(4, 4, "overlap=ortho; node [shape=box, width=1.5];")
    c["hw3_portho_grid"] = grid(4, 4, "overlap=portho; node [shape=box, width=1.5];")
    c["hw3_ortho_digraph"] = DIGRAPH % "overlap=ortho; node [shape=box, width=1.5];"
    c["hw3_portho_digraph"] = DIGRAPH % "overlap=portho; node [shape=box, width=1.5];"
    c["hw3_ortho_sep_add"] = OVERLAP % "overlap=ortho; sep=\"+12\";"
    c["hw3_ortho_sep_scale"] = OVERLAP % "overlap=ortho; sep=0.4;"
    c["hw3_portho_sep_scale"] = OVERLAP % "overlap=portho_yx; sep=\"0.3,0.6\";"
    c["hw3_ortho_random120"] = randgraph(120, 160, 20, "overlap=ortho; node [shape=box];")
    c["hw3_ortho_clusters"] = CLUSTERS % "overlap=orthoxy; node [shape=box, width=1];"
    # overlap=vpsc (lib/vpsc removeoverlaps)
    c["hw3_vpsc_biglabels"] = BIGLABELS % "overlap=vpsc;"
    c["hw3_vpsc_K6"] = complete(6, "overlap=vpsc; node [shape=box];")
    c["hw3_vpsc_random50"] = randgraph(50, 70, 18, "overlap=vpsc; node [shape=box];")
    c["hw3_vpsc_random120"] = randgraph(120, 160, 20, "overlap=vpsc; node [shape=box];")
    c["hw3_vpsc_random200"] = randgraph(200, 260, 5, "overlap=vpsc; node [shape=box, width=1];")
    c["hw3_vpsc_grid"] = grid(4, 4, "overlap=vpsc; node [shape=box, width=1.5];")
    c["hw3_vpsc_grid_tall"] = grid(5, 3, "overlap=vpsc; node [shape=box, height=1.2];")
    c["hw3_vpsc_sep_add"] = OVERLAP % "overlap=vpsc; sep=\"+12\";"
    c["hw3_vpsc_sep_scale"] = OVERLAP % "overlap=vpsc; sep=0.4;"
    c["hw3_vpsc_clusters"] = CLUSTERS % "overlap=vpsc; node [shape=box, width=1];"
    c["hw3_vpsc_tree"] = tree(4, "overlap=vpsc; node [shape=box, width=1.2];")
    # mode=hier (DiG-CoLa)
    c["hw3_hier_digraph"] = DIGRAPH % "mode=hier;"
    c["hw3_hier_tree"] = tree(4, "mode=hier;").replace("graph", "digraph", 1).replace("--", "->")
    c["hw3_hier_levelsgap"] = DIGRAPH % "mode=hier; levelsgap=1;"
    c["hw3_hier_levelsgap_neg"] = PETERSEN % "mode=hier; levelsgap=-0.5;"
    c["hw3_hier_start_random"] = PETERSEN % "mode=hier; start=random5;"
    c["hw3_hier_start_rand_lg"] = DIGRAPH % "mode=hier; start=3; levelsgap=0.5;"
    c["hw3_hier_subset"] = PETERSEN % "mode=hier; model=subset;"
    c["hw3_hier_circuit"] = PETERSEN % "mode=hier; model=circuit;"
    c["hw3_hier_mds"] = LENWEIGHT % "mode=hier; model=mds;"
    c["hw3_hier_random30"] = randgraph(30, 40, 17, "mode=hier;")
    c["hw3_hier_random80"] = randgraph(80, 120, 19, "mode=hier;")
    c["hw3_hier_dirnone"] = "digraph dn { mode=hier; a -> b -> c; c -> d [dir=none]; a -> d; d -> e; }"
    c["hw3_hier_nodir"] = "graph nd { mode=hier; edge [dir=none]; a -- b -- c -- a; c -- d; }"
    c["hw3_hier_dim3"] = DIGRAPH % "mode=hier; dim=3;"
    c["hw3_hier_disconnected"] = DISCONNECTED % "mode=hier;"
    c["hw3_hier_overlap"] = OVERLAP % "mode=hier; overlap=false;"
    c["hw3_hier_maxiter"] = PETERSEN % "mode=hier; maxiter=3;"
    return c


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.gv"):
        old.unlink()
    n = 0
    for name, src in {**corpus_cases(), **handwritten(), **handwritten2(), **handwritten3()}.items():
        if name in EXCLUDE or LAYOUT.search(src):
            continue
        forced = force_neato(src)
        if forced is None:
            continue
        (OUT / f"{name}.gv").write_text(forced, encoding="utf-8", errors="surrogateescape")
        n += 1
    print(f"wrote {n} neato cases to {OUT}")


if __name__ == "__main__":
    main()
