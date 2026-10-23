#!/usr/bin/env python3
"""Seeded generator of dot inputs with unusual edge endpoints, for crash fuzzing.

It targets what dot's spline router assumes about path ends: that `beginpath` and
`endpath` made at least one box (`P->boxes[P->nbox - 1]`, the port's
`boxes.last().unwrap()`). The cases mix:

- compass ports (`n`, `se`, `c`, `_`, ...) and record and HTML ports, on tails,
  heads and both ends, including ports that do not exist;
- zero and tiny sizes (`width=0`, `fixedsize=true`, empty labels, `peripheries=0`),
  `shape=point/none/plaintext/plain/record/Mrecord`;
- `headclip/tailclip=false`, `samehead`/`sametail` groups, `arrowhead=none`;
- self-loops with and without ports, several per node, with labels;
- flat edges (rank=same), parallel edges, back edges, clusters, `concentrate`;
- every `rankdir` and `splines` mode, tiny `nodesep`/`ranksep`.

Every case is a pure function of the seed. One statement per line, and a subgraph's
`{` and `}` on lines of their own, so scripts/fuzz-run.py's minimiser can drop lines.

Usage:
  python3 -I scripts/fuzz-gen-endpoints.py --seed 1 --count 1000 --out /tmp/fuzz-ends
  python3 -I scripts/fuzz-gen-endpoints.py --seed 17       # one case to stdout
See docs/porting/fuzzing.md, "Crash campaigns", for running them with scripts/fuzz-crash.py.
"""
import argparse
import pathlib
import random

COMPASS = ["n", "ne", "e", "se", "s", "sw", "w", "nw", "c", "_"]
SHAPES = ["point", "point", "none", "plaintext", "plain", "record", "Mrecord", "box",
          "circle", "ellipse", "diamond", "triangle", "doublecircle", "square", "underline"]
SIZES = ["0", "0", "0.001", "0.01", "0.05", "0.3", "1", "3"]


class Gen:
    def __init__(self, seed):
        self.r = random.Random(seed)
        self.seed = seed
        self.lines = []
        self.nodes = []
        self.ports = {}  # node -> record/HTML port names
        self.clusters = []

    def node(self, name):
        r = self.r
        attrs = []
        shape = r.choice(SHAPES)
        attrs.append(f"shape={shape}")
        ports = []
        if shape in ("record", "Mrecord"):
            k = r.random()
            if k < 0.15:
                lab = ""
            elif k < 0.3:
                lab = "<p0>"
                ports = ["p0"]
            else:
                fields = []
                for i in range(r.randint(1, 4)):
                    ports.append(f"p{i}")
                    fields.append(f"<p{i}> {r.choice(['', 'a', 'xy', ' '])}")
                lab = "|".join(fields)
                if r.random() < 0.4:
                    lab = "{" + lab + "}"
            attrs.append(f'label="{lab}"')
        elif shape in ("plaintext", "none", "plain") and r.random() < 0.4:
            cells = []
            for i in range(r.randint(1, 3)):
                ports.append(f"h{i}")
                cells.append(f'<TD PORT="h{i}">{r.choice(["", "x", "ab"])}</TD>')
            b = r.choice(["0", "1"])
            attrs.append(f'label=<<TABLE BORDER="{b}" CELLPADDING="0"><TR>{"".join(cells)}</TR></TABLE>>')
        elif r.random() < 0.4:
            attrs.append(f'label="{r.choice(["", " ", "x", "long label"])}"')
        if r.random() < 0.5:
            attrs.append(f"width={r.choice(SIZES)}")
        if r.random() < 0.5:
            attrs.append(f"height={r.choice(SIZES)}")
        if r.random() < 0.3:
            attrs.append(f"fixedsize={r.choice(['true', 'shape', 'false'])}")
        if r.random() < 0.15:
            attrs.append(f"peripheries={r.choice(['0', '2'])}")
        if r.random() < 0.08:
            attrs.append(f"fontsize={r.choice(['0', '1', '0.1'])}")
        if r.random() < 0.05:
            attrs.append("margin=" + r.choice(["0", '"0,0"']))
        if r.random() < 0.05:
            attrs.append(f"orientation={r.choice(['90', '45'])}")
        if r.random() < 0.1:
            attrs.append(f"group=g{r.randint(0, 1)}")
        self.ports[name] = ports
        self.lines.append(f"{name} [{', '.join(attrs)}];")

    def port(self, n):
        """A port on node n, written as a `:port` suffix, or ''."""
        r = self.r
        k = r.random()
        if k < 0.35:
            return ""
        ps = self.ports.get(n, [])
        if ps and k < 0.7:
            p = r.choice(ps + ["zz"] if r.random() < 0.1 else ps)
            if r.random() < 0.4:
                return f":{p}:{r.choice(COMPASS)}"
            return f":{p}"
        return f":{r.choice(COMPASS)}"

    def edge(self, a=None, b=None):
        r = self.r
        a = a or r.choice(self.nodes)
        b = b or (a if r.random() < 0.25 else r.choice(self.nodes))
        attrs = []
        if r.random() < 0.3:
            attrs.append(f"headclip={r.choice(['false', 'true'])}")
        if r.random() < 0.3:
            attrs.append(f"tailclip={r.choice(['false', 'true'])}")
        if r.random() < 0.2:
            attrs.append(f"samehead=h{r.randint(0, 2)}")
        if r.random() < 0.2:
            attrs.append(f"sametail=t{r.randint(0, 2)}")
        if r.random() < 0.25:
            attrs.append(f'label="{r.choice(["", "l", "a label", "x"])}"')
        if r.random() < 0.08:
            attrs.append(f'headlabel="{r.choice(["h", ""])}"')
        if r.random() < 0.08:
            attrs.append(f'taillabel="{r.choice(["t", ""])}"')
        if r.random() < 0.1:
            attrs.append(f"dir={r.choice(['back', 'both', 'none'])}")
        if r.random() < 0.1:
            attrs.append(f"arrowhead={r.choice(['none', 'normal', 'dot'])}")
        if r.random() < 0.08:
            attrs.append(f"minlen={r.choice(['0', '2'])}")
        if r.random() < 0.05:
            attrs.append("constraint=false")
        if r.random() < 0.15:
            attrs.append(f"headport={r.choice(COMPASS)}")
        if r.random() < 0.15:
            attrs.append(f"tailport={r.choice(COMPASS)}")
        if self.clusters and r.random() < 0.1:
            attrs.append(f"lhead={r.choice(self.clusters)}")
        if self.clusters and r.random() < 0.1:
            attrs.append(f"ltail={r.choice(self.clusters)}")
        if r.random() < 0.04:
            attrs.append(f"penwidth={r.choice(['0', '5'])}")
        if r.random() < 0.04:
            attrs.append(f"arrowsize={r.choice(['0', '3'])}")
        s = f" [{', '.join(attrs)}]" if attrs else ""
        self.lines.append(f"{a}{self.port(a)} -> {b}{self.port(b)}{s};")

    def build(self):
        r = self.r
        L = self.lines
        L.append(f"// fuzz-gen-endpoints seed={self.seed}")
        L.append(f"{r.choice(['digraph', 'digraph', 'digraph', 'graph'])} G {{")
        if r.random() < 0.6:
            L.append(f"rankdir={r.choice(['TB', 'LR', 'BT', 'RL'])};")
        if r.random() < 0.5:
            L.append(f"splines={r.choice(['true', 'false', 'line', 'polyline', 'ortho', 'curved', 'spline'])};")
        if r.random() < 0.2:
            L.append(f"nodesep={r.choice(['0', '0.01', '0.02', '1'])};")
        if r.random() < 0.2:
            L.append(f"ranksep={r.choice(['0', '0.02', '0.1', '1'])};")
        if r.random() < 0.15:
            L.append("concentrate=true;")
        if r.random() < 0.1:
            L.append("newrank=true;")
        if r.random() < 0.08:
            L.append(f"ordering={r.choice(['out', 'in'])};")
        if r.random() < 0.15:
            L.append("compound=true;")
        n = r.randint(1, 10)
        self.nodes = [f"n{i}" for i in range(n)]
        for v in self.nodes:
            self.node(v)
        flat = []
        if n > 1 and r.random() < 0.35:
            flat = r.sample(self.nodes, r.randint(2, n))
            L.append("subgraph {")
            L.append("  rank=same;")
            for v in flat:
                L.append(f"  {v};")
            L.append("}")
        for c in range(r.choices([0, 1, 2], [6, 3, 1])[0] if n > 1 else 0):
            self.clusters.append(f"cluster_{c}")
            L.append(f"subgraph cluster_{c} {{")
            for v in r.sample(self.nodes, r.randint(1, n)):
                L.append(f"  {v};")
            L.append("}")
        edges = r.randint(1, 3 * n + 2)
        for _ in range(edges):
            if r.random() < 0.15:  # a burst of parallel edges with ports
                a, b = r.choice(self.nodes), r.choice(self.nodes)
                for _ in range(r.randint(2, 4)):
                    self.edge(a, b)
            elif flat and r.random() < 0.3:  # flat edge, often labelled, between ported nodes
                a, b = r.choice(flat), r.choice(flat)
                self.edge(a, b)
            else:
                self.edge()
        if self.lines[1].startswith("graph"):
            self.lines[2:] = [l.replace(" -> ", " -- ") for l in self.lines[2:]]
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
