#!/usr/bin/env python3
"""Builds tests/osage/*.gv: corpus graphs with `layout=osage` forced, plus
hand-written cases that exercise osage attributes (clusters, sortv,
pack/packmode, array modes).

The generated files are checked in; rerun this only to change the corpus, then
regenerate the goldens with scripts/gen-golden.sh.

Usage: python3 -I scripts/gen-osage-corpus.py
"""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "osage"

HEADER = re.compile(r"^\s*(strict\s+)?(di)?graph\b[^{]*\{", re.M | re.I)
LAYOUT = re.compile(r"\blayout\s*=", re.I)
CLUSTER = re.compile(r"subgraph\s*\"?cluster", re.I)

EXCLUDE = set()


def force(src, engine="osage"):
    m = HEADER.search(src)
    if not m:
        return None
    return src[: m.end()] + f"\nlayout={engine};\n" + src[m.end() :]


def corpus_cases():
    """Fixtures and wild graphs with clusters."""
    out = {}
    for f in sorted((ROOT / "tests" / "fixtures").glob("*.gv")):
        out["fx_" + f.stem] = f.read_text(encoding="utf-8", errors="surrogateescape")
    for f in sorted((ROOT / "tests" / "wild").glob("*.gv")):
        src = f.read_text(encoding="utf-8", errors="surrogateescape")
        if CLUSTER.search(src):
            out["wild_" + f.stem] = src
    return out


CLUSTERS = """graph clus {
%s
subgraph cluster_a { a1; a2; a3; a1 -- a2; }
subgraph cluster_b { b1; b2 [shape=box, width=2]; b3; }
subgraph cluster_c { label="C"; c1; }
d; e [shape=circle];
a1 -- b1; d -- e;
}"""

NESTED = """digraph nested {
%s
subgraph cluster_outer {
  label="outer";
  o1; o2;
  subgraph cluster_inner { label="inner"; i1; i2; i3; }
  subgraph cluster_inner2 { j1 -> j2; }
}
subgraph cluster_side { s1; s2; }
x -> o1; x -> s1; i3 -> s2;
y; z;
}"""

SORTV = """graph sv {
%s
subgraph cluster_a { sortv=3; a1; a2; }
subgraph cluster_b { sortv=1; b1; }
subgraph cluster_c { sortv=2; c1; c2; c3; }
n1 [sortv=5]; n2 [sortv=0]; n3 [sortv=4];
}"""

MANY = "graph many {\n%s\n" + "\n".join(
    f"n{i} [width={0.3 + (i % 4) * 0.2}, height={0.3 + (i % 3) * 0.15}];" for i in range(17)
) + "\n}"

EMPTY = """graph emptyc {
%s
subgraph cluster_empty { }
subgraph cluster_lbl { label="only a label"; }
subgraph cluster_one { a; }
b;
}"""


def handwritten():
    c = {}
    c["hw_clusters"] = CLUSTERS % ""
    c["hw_nested"] = NESTED % ""
    c["hw_many"] = MANY % ""
    c["hw_empty_clusters"] = EMPTY % ""
    c["hw_sortv_array_u"] = SORTV % 'packmode="array_u";'
    c["hw_sortv_nouser"] = SORTV % 'packmode="array";'
    for pm in ["array", "array_c", "array_t", "array_b", "array_l", "array_r",
               "array_c3", "array_r2", "array_cl4", "array_rt2", "array_ub3",
               "node", "graph", "clust", "array_i"]:
        c[f"hw_packmode_{pm}"] = MANY % f'packmode="{pm}";'
        c[f"hw_packmode_{pm}_clusters"] = CLUSTERS % f'packmode="{pm}";'
    c["hw_pack_0"] = CLUSTERS % "pack=0;"
    c["hw_pack_30"] = CLUSTERS % "pack=30;"
    c["hw_pack_false"] = CLUSTERS % "pack=false;"
    c["hw_cluster_pack"] = CLUSTERS.replace("subgraph cluster_a {", "subgraph cluster_a { pack=20; packmode=array_c1;") % ""
    c["hw_splines_true"] = CLUSTERS % "splines=true;"
    c["hw_splines_none"] = CLUSTERS % "splines=none;"
    c["hw_ratio"] = CLUSTERS % 'ratio=fill; size="3,3";'
    c["hw_label"] = CLUSTERS % 'label="root label";'
    c["hw_label_wide"] = EMPTY.replace("label=\"only a label\"", "label=\"a very very very long cluster label\"") % ""
    c["hw_single"] = "graph single { a; }"
    c["hw_empty"] = "graph empty { }"
    c["hw_edges"] = "digraph e { a -> b -> c -> a; b -> b; a -> c [label=x]; }"
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
    print(f"wrote {n} osage cases to {OUT}")


if __name__ == "__main__":
    main()
