#!/usr/bin/env python3
"""Builds tests/sfdp/*.gv: corpus graphs with `layout=sfdp` forced, larger
generated graphs (grids, trees, random sparse graphs), and hand-written cases
that exercise sfdp attributes.

The generated files are checked in; rerun this only to change the corpus, then
regenerate the goldens with scripts/gen-golden.sh.

Usage: python3 -I scripts/gen-sfdp-corpus.py
"""
import pathlib
import random
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "sfdp"

HEADER = re.compile(r"^\s*(strict\s+)?(di)?graph\b[^{]*\{", re.M | re.I)
LAYOUT = re.compile(r"\blayout\s*=", re.I)

# C output for these is not reproducible run to run (smoothing=rng walks a GTS
# hash table keyed by pointers).
EXCLUDE = {"wild_gv16_undirected_ngk10_4_793340", "attr_smoothing_rng"}


def force_sfdp(src):
    m = HEADER.search(src)
    if not m:
        return None
    return src[: m.end()] + "\nlayout=sfdp;\n" + src[m.end() :]


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


def tree(depth, arity=2, extra="", shape='shape=circle, width=0.3, fixedsize=true, label=""'):
    lines = ["graph tree {", extra, f"node [{shape}];"]
    k = 1
    frontier = [0]
    for _ in range(depth):
        nxt = []
        for p in frontier:
            for _ in range(arity):
                lines.append(f"t{p} -- t{k};")
                nxt.append(k)
                k += 1
        frontier = nxt
    lines.append("}")
    return "\n".join(lines)


def random_graph(n, extra_edges, seed, extra="", shape="shape=point"):
    """Connected: a random spanning tree plus `extra_edges` random edges."""
    rng = random.Random(seed)
    edges = set()
    for i in range(1, n):
        edges.add((rng.randrange(i), i))
    while len(edges) < n - 1 + extra_edges:
        a, b = rng.randrange(n), rng.randrange(n)
        if a != b and (a, b) not in edges and (b, a) not in edges:
            edges.add((a, b))
    lines = [f"graph rnd{n} {{", extra, f"node [{shape}];"]
    lines += [f"{a} -- {b};" for a, b in sorted(edges)]
    lines.append("}")
    return "\n".join(lines)


def power_law(n, seed, extra=""):
    """Preferential attachment tree with many leaves (power-law degrees)."""
    rng = random.Random(seed)
    targets = [0]
    lines = [f"graph pl{n} {{", extra, "node [shape=point];"]
    for i in range(1, n):
        t = rng.choice(targets)
        lines.append(f"{t} -- {i};")
        targets += [t, i]
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

DISCONNECTED = """graph disc {
%s
a -- b -- c -- a;
d -- e -- f -- d;
g -- h;
i; j;
k -- l -- m -- n -- k;
}"""

STAR = """graph star {
%s
hub -- {a b c d e f g h i j k l};
a -- x1 -- x2 -- x3 -- a;
}"""

DIGRAPH = """digraph dg {
%s
a -> b -> c -> d -> a;
a -> c;
b -> d [dir=both];
e -> a [arrowhead=none];
d -> f -> g;
}"""

LABELS = """graph labels {
%s
a -- b [label="ab"];
b -- c [label="bc", headlabel="h", taillabel="t"];
c -- a;
c -- d -- e -- c;
a [label="node A"]; b [shape=box]; c [shape=diamond];
}"""

OVERLAP = """graph ov {
%s
node [shape=box, width=1.2, height=0.6, fixedsize=true];
a -- b; a -- c; a -- d; a -- e; a -- f; b -- c; d -- e; f -- g; g -- h; h -- a;
}"""

WEIGHTS = """graph w {
%s
a -- b [weight=5]; b -- c [weight=0.5]; c -- d; d -- a [weight=3];
a -- e; e -- f [weight=2]; f -- b;
}"""

ELABELS = """graph el {
%s
a -- "|edgelabel|ab" -- b;
b -- "|edgelabel|bc" -- c;
c -- "|edgelabel|ca" -- a;
c -- d -- e -- c;
d -- "|edgelabel|da" -- a;
e -- f; f -- g; g -- "|edgelabel|ga" -- a;
}"""

LOOPS = """graph loops {
%s
a -- a; a -- b; a -- b; b -- c; c -- c [label="self"]; c -- d; d -- a;
}"""


def handwritten():
    c = {}
    c["hw_petersen"] = PETERSEN % ""
    c["hw_K5"] = complete(5)
    c["hw_K8_box"] = complete(8, "node [shape=box];")
    c["hw_C12"] = cycle(12)
    c["hw_C60"] = cycle(60, "node [shape=point];")
    c["hw_star"] = STAR % ""
    c["hw_disconnected"] = DISCONNECTED % ""
    c["hw_disconnected_pack20"] = DISCONNECTED % "pack=20;"
    c["hw_disconnected_packarray"] = DISCONNECTED % "packmode=array;"
    c["hw_digraph"] = DIGRAPH % ""
    c["hw_labels"] = LABELS % ""
    c["hw_weights"] = WEIGHTS % ""
    c["hw_loops"] = LOOPS % ""
    c["hw_single"] = "graph single { a; }"
    c["hw_two"] = "graph two { a -- b; }"
    c["hw_three"] = "graph three { a -- b -- c; }"
    c["hw_empty"] = "graph empty { }"
    c["hw_isolated"] = "graph iso { a; b; c; d; }"
    c["hw_strict_multi"] = "strict graph sm { a -- b; a -- b; b -- c; c -- a; }"
    # Larger generated graphs.
    c["gen_grid10x10"] = grid(10, 10)
    c["gen_grid20x20"] = grid(20, 20)
    c["gen_grid8x40"] = grid(8, 40)
    c["gen_tree7"] = tree(7)
    c["gen_tree4_ternary"] = tree(4, 3)
    c["gen_tree9"] = tree(9, 2, "", "shape=point")
    c["gen_rnd100"] = random_graph(100, 40, 1, "", "shape=circle, width=0.2, fixedsize=true, label=\"\"")
    c["gen_rnd300"] = random_graph(300, 150, 2)
    c["gen_rnd800"] = random_graph(800, 500, 3)
    c["gen_rnd2000"] = random_graph(2000, 1000, 4)
    c["gen_powerlaw300"] = power_law(300, 5)
    c["gen_powerlaw1000"] = power_law(1000, 6)
    # Attributes.
    for k in ["0.3", "2"]:
        c[f"attr_K_{k}"] = PETERSEN % f"K={k};"
    for r in ["0.5", "2", "3"]:
        c[f"attr_repulsiveforce_{r}"] = PETERSEN % f"repulsiveforce={r};"
    c["attr_repulsiveforce_2_grid"] = grid(10, 10, "repulsiveforce=2;")
    for lv in ["0", "1", "2"]:
        c[f"attr_levels_{lv}"] = grid(10, 10, f"levels={lv};")
    for sm in ["avg_dist", "graph_dist", "power_dist", "spring", "triangle", "rng"]:
        c[f"attr_smoothing_{sm}"] = random_graph(60, 30, 7, f"smoothing={sm};")
    for qt in ["none", "normal", "fast", "2"]:
        c[f"attr_quadtree_{qt}"] = random_graph(120, 60, 8, f"quadtree={qt};")
    c["attr_beautify"] = STAR % "beautify=true;"
    c["attr_beautify_tree"] = tree(4, 3, "beautify=true;")
    for rot in ["30", "90", "-45"]:
        c[f"attr_rotation_{rot}"] = PETERSEN % f"rotation={rot};"
    for ov in ["false", "true", "prism", "prism0", "prism5", "scale", "scalexy", "compress", "voronoi", "ortho", "vpsc"]:
        c[f"attr_overlap_{ov}"] = OVERLAP % f"overlap={ov};"
    c["attr_overlap_false_grid"] = grid(8, 8, "overlap=false; node [shape=box, label=\"node\"];")
    c["attr_overlap_false_labels"] = LABELS % "overlap=false;"
    c["attr_overlap_false_sep"] = OVERLAP % "overlap=false; sep=\"+12\";"
    c["attr_overlap_false_noshrink"] = OVERLAP % "overlap=false; overlap_shrink=false;"
    for sc in ["-2", "-8", "0", "3"]:
        c[f"attr_overlap_scaling_{sc}"] = OVERLAP % f"overlap=prism; overlap_scaling={sc};"
    c["attr_overlap_scaling_default_ov"] = OVERLAP % "overlap_scaling=-1;"
    c["attr_start_7"] = PETERSEN % "start=7;"
    c["attr_start_random42"] = PETERSEN % "start=random42;"
    # label_scheme: edge-label nodes. With overlap=false C 16.1.0 crashes for
    # schemes 1 and 3 and diverges for 2, so only scheme 4 is run with it.
    for ls in ["1", "2", "3", "4"]:
        c[f"attr_label_scheme_{ls}"] = ELABELS % f"label_scheme={ls};"
    c["attr_label_scheme_4_overlap"] = ELABELS % "label_scheme=4; overlap=false;"
    c["attr_dim3"] = PETERSEN % "dim=3;"
    for sp in ["true", "polyline", "curved", "false"]:
        c[f"attr_splines_{sp}"] = OVERLAP % f"splines={sp};"
    c["attr_normalize"] = PETERSEN % "normalize=true;"
    c["attr_size"] = grid(10, 10, "size=\"3,3\";")
    c["attr_ratio_fill"] = PETERSEN % "ratio=fill; size=\"5,3!\";"
    return c


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.gv"):
        old.unlink()
    n = 0
    for name, src in {**corpus_cases(), **handwritten()}.items():
        if name in EXCLUDE or LAYOUT.search(src):
            continue
        forced = force_sfdp(src)
        if forced is None:
            continue
        (OUT / f"{name}.gv").write_text(forced, encoding="utf-8", errors="surrogateescape")
        n += 1
    print(f"wrote {n} sfdp cases to {OUT}")


if __name__ == "__main__":
    main()
