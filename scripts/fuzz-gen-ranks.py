#!/usr/bin/env python3
"""Seeded generator of dot inputs with unusual rank shapes, for crash fuzzing.

It targets what dot's C assumes about ranks: that every rank of every graph and
cluster holds a node (`GD_rank(g)[r].v[0]`, the port's `rv(..).unwrap()`), and that
every cluster reaching position has its left and right nodes (`GD_ln`/`GD_rn`).
Those break when a ranking fails, when a cluster loses nodes after it was ranked,
or when a flat label needs a rank above the top one. So the cases mix:

- nested and sibling clusters that share nodes, empty clusters;
- `rank=min/max/same/source/sink` sets that contradict each other or a cluster;
- labelled flat edges on the top rank, parallel and reversed;
- `minlen=0`, `constraint=false`, `weight=0`, back edges;
- `newrank`, `clusterrank`, `pack`/`packmode`, `compound` with `lhead`/`ltail`,
  `concentrate`, `ordering`, `rankdir`, `splines`, small `nslimit`/`mclimit`.

Every case is a pure function of the seed. One statement per line, and a subgraph's
`{` and `}` on lines of their own, so scripts/fuzz-run.py's minimiser can drop lines.

Usage:
  python3 -I scripts/fuzz-gen-ranks.py --seed 1 --count 1000 --out /tmp/fuzz-ranks
  python3 -I scripts/fuzz-gen-ranks.py --seed 17           # one case to stdout
See docs/porting/fuzzing.md, "Crash campaigns", for running them with scripts/fuzz-crash.py.
"""
import argparse
import pathlib
import random

RANKS = ["same", "min", "max", "source", "sink"]
SPLINES = ["", "", "true", "false", "line", "polyline", "ortho", "curved", "compound"]


class Gen:
    def __init__(self, seed):
        self.r = random.Random(seed)
        self.seed = seed
        self.lines = []
        self.nodes = []
        self.clusters = []  # cluster names, for lhead/ltail

    def label(self):
        r = self.r
        return r.choice(["l", "label", "a long edge label", "x\\ny", "", "L"])

    def graph_attrs(self):
        r = self.r
        L = self.lines
        p = lambda k, vs, pr: L.append(f"{k}={r.choice(vs)};") if r.random() < pr else None
        p("newrank", ["true", "false"], 0.4)
        p("clusterrank", ["local", "global", "none"], 0.2)
        p("compound", ["true"], 0.3)
        p("concentrate", ["true", "false"], 0.3)
        p("pack", ["true", "false", "0", "4", "20"], 0.2)
        p("packmode", ["\"node\"", "clust", "\"graph\"", "array", "array_c2", "array_u"], 0.1)
        p("rankdir", ["TB", "LR", "BT", "RL"], 0.3)
        p("ranksep", ["0", "0.02", "0.5", "1.5 equally", "equally"], 0.15)
        p("nodesep", ["0", "0.02", "0.25", "1"], 0.15)
        p("ordering", ["out", "in"], 0.15)
        p("splines", [s for s in SPLINES if s], 0.25)
        p("remincross", ["true", "false"], 0.1)
        p("mclimit", ["0", "0.01", "1", "5"], 0.1)
        p("nslimit", ["0", "1", "0.01"], 0.08)
        p("nslimit1", ["0", "1", "0.01"], 0.08)
        p("searchsize", ["0", "1", "30"], 0.08)
        p("rank", ["same", "min", "max"], 0.05)

    def node_name(self, i):
        return f"n{i}"

    def pick(self, k=None):
        r = self.r
        k = k or r.randint(1, min(4, len(self.nodes)))
        return r.sample(self.nodes, min(k, len(self.nodes)))

    def rankset(self, ind):
        r = self.r
        L = self.lines
        name = r.choice(["", f"r{len(L)} "])
        L.append(f"{ind}subgraph {name}{{")
        L.append(f"{ind}  rank={r.choice(RANKS)};")
        for n in self.pick():
            L.append(f"{ind}  {n};")
        L.append(f"{ind}}}")

    def cluster(self, depth, ind):
        r = self.r
        L = self.lines
        name = f"cluster_{len(self.clusters)}"
        self.clusters.append(name)
        L.append(f"{ind}subgraph {name} {{")
        if r.random() < 0.2:
            L.append(f"{ind}  rank={r.choice(RANKS)};")
        if r.random() < 0.3:
            L.append(f"{ind}  label=\"{self.label()}\";")
        if r.random() < 0.1:
            L.append(f"{ind}  margin={r.choice(['0', '20'])};")
        # an empty cluster now and then
        if r.random() > 0.08:
            for n in self.pick(r.randint(1, 4)):
                L.append(f"{ind}  {n};")
        if r.random() < 0.35:
            self.rankset(ind + "  ")
        if depth < 3:
            for _ in range(r.choices([0, 1, 2], [5, 4, 1])[0]):
                self.cluster(depth + 1, ind + "  ")
        if r.random() < 0.3:
            self.edge(ind + "  ")
        L.append(f"{ind}}}")

    def edge(self, ind, a=None, b=None, flat=False):
        r = self.r
        a = a or r.choice(self.nodes)
        b = b or r.choice(self.nodes)
        attrs = []
        if flat or r.random() < 0.3:
            attrs.append(f"label=\"{self.label()}\"")
        if r.random() < 0.2:
            attrs.append(f"minlen={r.choice(['0', '0', '2', '3'])}")
        if r.random() < 0.1:
            attrs.append("constraint=false")
        if r.random() < 0.08:
            attrs.append(f"weight={r.choice(['0', '5', '100'])}")
        if r.random() < 0.08:
            attrs.append(f"dir={r.choice(['back', 'both', 'none'])}")
        if self.clusters and r.random() < 0.15:
            attrs.append(f"lhead={r.choice(self.clusters)}")
        if self.clusters and r.random() < 0.15:
            attrs.append(f"ltail={r.choice(self.clusters)}")
        if r.random() < 0.05:
            attrs.append(f"samehead=h{r.randint(0, 1)}")
        if r.random() < 0.05:
            attrs.append(f"xlabel=\"{self.label()}\"")
        if r.random() < 0.05:
            attrs.append(f"headlabel=\"{self.label()}\"")
        s = f" [{', '.join(attrs)}]" if attrs else ""
        self.lines.append(f"{ind}{a} -> {b}{s};")

    def top_rank_flat_labels(self):
        """Labelled flat edges among rank=min/source nodes: parallel, reversed, chained."""
        r = self.r
        L = self.lines
        top = self.pick(r.randint(2, 4))
        L.append("subgraph {")
        L.append(f"  rank={r.choice(['min', 'source', 'same'])};")
        for n in top:
            L.append(f"  {n};")
        L.append("}")
        for _ in range(r.randint(1, 4)):
            a, b = r.sample(top, 2) if len(top) > 1 else (top[0], top[0])
            k = r.random()
            if k < 0.3:  # unlabelled then labelled parallel edge (merge_oneway)
                L.append(f"{a} -> {b};")
                self.edge("", a, b, flat=True)
            elif k < 0.55:  # reversed pair (flat_rev)
                self.edge("", a, b, flat=True)
                self.edge("", b, a, flat=r.random() < 0.7)
            else:
                self.edge("", a, b, flat=True)

    def build(self):
        r = self.r
        L = self.lines
        L.append(f"// fuzz-gen-ranks seed={self.seed}")
        L.append(f"{r.choice(['digraph', 'digraph', 'strict digraph'])} G {{")
        self.graph_attrs()
        n = r.randint(2, 12)
        self.nodes = [self.node_name(i) for i in range(n)]
        if r.random() < 0.2:
            L.append(f"node [shape={r.choice(['box', 'point', 'plaintext', 'circle'])}];")
        if r.random() < 0.1:
            L.append(f"edge [minlen={r.choice(['0', '2'])}];")
        for _ in range(r.choices([0, 1, 2, 3], [2, 4, 3, 1])[0]):
            self.cluster(0, "")
        for _ in range(r.choices([0, 1, 2, 3], [3, 4, 2, 1])[0]):
            self.rankset("")
        if r.random() < 0.45:
            self.top_rank_flat_labels()
        for _ in range(r.randint(0, n + 4)):
            self.edge("")
        # isolated components for pack
        if r.random() < 0.15:
            for i in range(r.randint(1, 3)):
                L.append(f"z{i}a -> z{i}b;")
        L.append("}")

    def case(self):
        self.build()
        return "\n".join(self.lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--count", type=int, default=0, help="write COUNT cases (seed, seed+1, ...)")
    ap.add_argument("--out", type=pathlib.Path)
    a = ap.parse_args()
    if not a.count:
        print(Gen(a.seed).case(), end="")
        return
    a.out.mkdir(parents=True, exist_ok=True)
    for s in range(a.seed, a.seed + a.count):
        (a.out / f"s{s}.gv").write_text(Gen(s).case(), encoding="utf-8")


if __name__ == "__main__":
    main()
