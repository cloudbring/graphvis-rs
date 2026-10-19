#!/usr/bin/env python3
"""Builds tests/patchwork/*.gv: corpus graphs with `layout=patchwork` forced,
plus hand-written cases that exercise the `area` attribute and nested clusters.

The generated files are checked in; rerun this only to change the corpus, then
regenerate the goldens with scripts/gen-golden.sh.

Usage: python3 -I scripts/gen-patchwork-corpus.py
"""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "patchwork"

HEADER = re.compile(r"^\s*(strict\s+)?(di)?graph\b[^{]*\{", re.M | re.I)
LAYOUT = re.compile(r"\blayout\s*=", re.I)
CLUSTER = re.compile(r"subgraph\s*\"?cluster", re.I)

EXCLUDE = set()


def force(src, engine="patchwork"):
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


AREAS = """graph areas {
%s
a [area=1]; b [area=2]; c [area=3]; d [area=4]; e [area=5];
f [area=0.5]; g [area=10]; h;
}"""

NESTED = """graph nested {
%s
subgraph cluster_1 {
  area=2;
  a [area=3]; b [area=1];
  subgraph cluster_11 { c [area=2]; d; e [area=0.25]; }
}
subgraph cluster_2 { f [area=6]; subgraph cluster_21 { g; } subgraph cluster_22 { } }
subgraph plain { h; i [area=2]; }
j [area=4];
a -- f; g -- j;
}"""

FLAT = "graph flat {\n%s\n" + "\n".join(f"n{i};" for i in range(13)) + "\n}"

TREE = """digraph tree {
%s
subgraph cluster_root {
  subgraph cluster_x { x1 [area=5]; x2 [area=5]; x3; }
  subgraph cluster_y { y1 [area=7]; subgraph cluster_yy { y2; y3 [area=3]; } }
  r [area=2];
}
z [area=9];
}"""


def handwritten():
    c = {}
    c["hw_areas"] = AREAS % ""
    c["hw_nested"] = NESTED % ""
    c["hw_nested_inset"] = NESTED % "inset=5;"
    c["hw_flat"] = FLAT % ""
    c["hw_flat_fontsize"] = FLAT % "node [fontsize=\"\"];"
    c["hw_tree"] = TREE % ""
    c["hw_tree_labels"] = TREE.replace("subgraph cluster_x {", "subgraph cluster_x { label=X;") % 'label="root";'
    c["hw_cluster_area"] = "graph ca {\nsubgraph cluster_e1 { area=4; }\nsubgraph cluster_e2 { }\na;\n}"
    c["hw_single"] = "graph single { a; }"
    c["hw_empty"] = "graph empty { }"
    c["hw_styles"] = AREAS % 'node [style=filled, fillcolor=lightblue];'
    c["hw_size"] = AREAS % 'size="2,2";'
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
    print(f"wrote {n} patchwork cases to {OUT}")


if __name__ == "__main__":
    main()
