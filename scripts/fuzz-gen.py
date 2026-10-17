#!/usr/bin/env python3
"""Seeded generator of random DOT inputs for differential fuzzing against C
Graphviz 16.1.0 (see docs/porting/fuzzing.md and scripts/fuzz-run.py).

Every case is a pure function of (seed, engine, malformed): the same arguments
always produce the same bytes, so a case is identified by its seed alone.

The output keeps one statement per line, and a subgraph's `{` and `}` on lines of
their own, so the minimiser in scripts/fuzz-run.py can drop lines and attributes.

Usage:
  python3 -I scripts/fuzz-gen.py --seed 1 --count 100 --out /tmp/fuzz/cases
  python3 -I scripts/fuzz-gen.py --seed 17                 # one case to stdout
Options: --engine dot|neato|... (default: drawn per case), --malformed RATE
(default 0.08: share of cases that are mutated to be invalid).
"""
import argparse
import pathlib
import random

ENGINES = ["dot", "neato", "fdp", "sfdp", "twopi", "circo", "osage", "patchwork"]
ENGINE_WEIGHTS = [30, 14, 10, 8, 10, 10, 9, 9]

# lib/common/shapes.c Shapes[]
SHAPES = (
    "box polygon ellipse oval circle point egg triangle plaintext plain diamond "
    "trapezium parallelogram house pentagon hexagon septagon octagon "
    "doublecircle doubleoctagon tripleoctagon invtriangle invtrapezium invhouse "
    "Mdiamond Msquare Mcircle rect rectangle square star none underline cylinder "
    "note tab folder box3d component promoter cds terminator utr primersite "
    "restrictionsite fivepoverhang threepoverhang noverhang assembly signature "
    "insulator ribosite rnastab proteasesite proteinstab rpromoter rarrow larrow "
    "lpromoter record Mrecord"
).split()
COMMON_SHAPES = "box ellipse circle record Mrecord plaintext point diamond doublecircle".split()

# lib/common/arrows.c Arrowtypes/Arrowmods
ARROW_BASE = ["normal", "inv", "dot", "odot", "invdot", "invodot", "none", "tee", "empty",
              "invempty", "diamond", "odiamond", "ediamond", "crow", "box", "obox", "open",
              "halfopen", "vee", "curve", "icurve", "lnormal", "rnormal", "olbox", "rtee",
              "ltee", "lcrow", "rvee", "ocurve"]
ARROW_PARTS = ["normal", "inv", "dot", "tee", "box", "crow", "diamond", "vee", "none", "curve"]
ARROW_MODS = ["", "", "o", "l", "r", "ol", "or"]

COLORS = ["red", "blue", "green", "black", "white", "gray", "lightblue", "yellow", "orange",
          "purple", "#ff0000", "#00ff0080", "#123", "#abcdef", "0.5 0.5 0.5", "0.1,0.9,0.7",
          "/blues9/3", "/accent8/5", "transparent", "navy", "gold", "crimson", "forestgreen",
          "lightgray", "pink", "cyan", "magenta", "brown", "salmon", "turquoise"]
FONTS = ["Times-Roman", "Helvetica", "Courier", "Arial", "Helvetica-Bold", "Times-Bold",
         "Courier-Oblique", "Times-Italic", "Palatino", "Bookman", "AvantGarde",
         "NewCenturySchlbk-Roman", "Symbol", "DejaVu Sans", "monospace", "sans-serif",
         "serif", "Verdana", "Georgia", "Arial Bold", "Helvetica Neue", "Courier New"]
WORDS = ["a", "node", "Start", "end", "process data", "x", "y1", "Hello World", "foo_bar",
         "long label with several words in it", "IF", "then", "0", "42", "-1.5", "A&B",
         "x<y", "\"q\"", "tab\there", "π", "αβγ", "日本語", "Ünïcödé", "naïve café",
         "→ arrow", "😀", "€100", "Ω", "ß", "中文标签", "Привет", "עברית", "e=mc²", "…"]
STYLES_NODE = ["filled", "dashed", "dotted", "bold", "rounded", "diagonals", "invis",
               "solid", "filled,rounded", "dashed,filled", "striped", "wedged", "radial",
               "rounded,dashed", "bold,filled", "setlinewidth(3)"]
STYLES_EDGE = ["dashed", "dotted", "bold", "invis", "solid", "tapered", "bold,dashed",
               "setlinewidth(2)"]
STYLES_CLUSTER = ["filled", "dashed", "dotted", "bold", "rounded", "invis", "solid",
                  "filled,rounded", "striped", "radial"]
COMPASS = ["n", "ne", "e", "se", "s", "sw", "w", "nw", "c", "_"]


def q(s):
    """Quotes a DOT string."""
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"' if not s.startswith("\x00") else '"' + s[1:] + '"'


def raw(s):
    """A string written verbatim inside quotes (escape sequences intact)."""
    return "\x00" + s


class Gen:
    def __init__(self, seed, engine=None, malformed=0.08):
        self.r = random.Random(seed)
        self.seed = seed
        r = self.r
        self.engine = engine or r.choices(ENGINES, ENGINE_WEIGHTS)[0]
        self.malformed = r.random() < malformed
        self.lines = []
        self.records = {}  # node -> list of record port names
        self.html_ports = {}  # node -> list of HTML port names
        self.clusters = []

    # ---- value draws ------------------------------------------------------
    def num(self, lo, hi, digits=2):
        v = self.r.uniform(lo, hi)
        return f"{v:.{digits}f}".rstrip("0").rstrip(".") or "0"

    def color(self):
        r = self.r
        if r.random() < 0.08:
            a, b = r.choice(COLORS), r.choice(COLORS)
            return r.choice([f"{a}:{b}", f"{a};0.3:{b}", f"{a}:{b}:{r.choice(COLORS)}"])
        return r.choice(COLORS)

    def text(self):
        r = self.r
        k = r.random()
        if k < 0.45:
            return r.choice(WORDS)
        if k < 0.6:
            sep = r.choice(["\\n", "\\l", "\\r"])
            parts = [r.choice(WORDS).replace("\\", "") for _ in range(r.randint(2, 4))]
            s = sep.join(p.replace('"', "") for p in parts)
            if r.random() < 0.5:
                s += r.choice(["\\n", "\\l", "\\r"])
            return raw(s)
        if k < 0.7:
            return raw(r.choice(["\\N", "\\G", "\\E", "\\T", "\\H", "\\L", "node \\N", "\\N\\n\\G"]))
        if k < 0.75:
            return ""
        if k < 0.85:
            return " ".join(r.choice(WORDS) for _ in range(r.randint(1, 6)))
        return "".join(r.choice("abcdefghijklmnopqrstuvwxyz ABCXYZ0123456789_-.") for _ in range(r.randint(1, 30)))

    def record_label(self, node):
        r = self.r
        ports = []

        def field(depth):
            if depth < 2 and r.random() < 0.25:
                return "{" + "|".join(field(depth + 1) for _ in range(r.randint(1, 3))) + "}"
            s = ""
            if r.random() < 0.6:
                p = f"p{len(ports)}"
                ports.append(p)
                s += f"<{p}> "
            w = r.choice(["a", "b", "field", "x y", "", "long field text", "1", "αβ", "\\N"])
            return s + w

        lab = "|".join(field(0) for _ in range(r.randint(1, 4)))
        if r.random() < 0.3:
            lab = "{" + lab + "}"
        self.records[node] = ports
        return raw(lab)

    def html_label(self, node):
        r = self.r
        ports = []
        if r.random() < 0.3:
            # plain HTML text
            t = r.choice(["<B>bold</B>", "<I>it</I> text", "a<BR/>b", "<FONT POINT-SIZE=\"20\">big</FONT>",
                          "x<SUB>2</SUB>", "<U>u</U><BR ALIGN=\"LEFT\"/>left", "&amp; &lt; &gt;",
                          "<FONT COLOR=\"red\" FACE=\"Courier\">c</FONT>", "<S>strike</S>", "&#945;&beta;"])
            return "<" + t + ">"
        tattrs = []
        for k, vals in [("BORDER", ["0", "1", "2"]), ("CELLBORDER", ["0", "1", "3"]),
                        ("CELLSPACING", ["0", "2", "5"]), ("CELLPADDING", ["0", "2", "6"]),
                        ("BGCOLOR", ["lightyellow", "#eeeeee", "white"]),
                        ("STYLE", ["rounded", "radial"]), ("COLOR", ["blue", "black"]),
                        ("ALIGN", ["LEFT", "RIGHT", "CENTER"]),
                        ("FIXEDSIZE", ["FALSE"]), ("WIDTH", ["40", "100"])]:
            if r.random() < 0.25:
                tattrs.append(f'{k}="{r.choice(vals)}"')
        rows = []
        ncols = r.randint(1, 4)
        for i in range(r.randint(1, 4)):
            cells = []
            for j in range(ncols if r.random() < 0.7 else r.randint(1, ncols)):
                ca = []
                if r.random() < 0.4:
                    p = f"h{len(ports)}"
                    ports.append(p)
                    ca.append(f'PORT="{p}"')
                if r.random() < 0.15:
                    ca.append(f'COLSPAN="{r.randint(1, 3)}"')
                if r.random() < 0.1:
                    ca.append(f'ROWSPAN="{r.randint(1, 2)}"')
                if r.random() < 0.2:
                    ca.append(f'BGCOLOR="{r.choice(["red", "yellow", "lightblue", "white"])}"')
                if r.random() < 0.15:
                    ca.append(f'ALIGN="{r.choice(["LEFT", "RIGHT", "CENTER", "TEXT"])}"')
                if r.random() < 0.1:
                    ca.append(f'SIDES="{r.choice(["LR", "TB", "L", "T"])}"')
                if r.random() < 0.1:
                    ca.append(f'WIDTH="{r.randint(10, 80)}" HEIGHT="{r.randint(10, 40)}"')
                content = r.choice(["a", "cell", "x", "", "<B>b</B>", "long cell text",
                                    "1<BR/>2", "<I>i</I>", "&lt;x&gt;", "αβ",
                                    "<FONT POINT-SIZE=\"8\">small</FONT>"])
                cells.append(f"<TD{(' ' + ' '.join(ca)) if ca else ''}>{content}</TD>")
            rows.append("<TR>" + "".join(cells) + "</TR>")
        self.html_ports[node] = ports
        return f"<<TABLE{(' ' + ' '.join(tattrs)) if tattrs else ''}>" + "".join(rows) + "</TABLE>>"

    def arrow(self):
        r = self.r
        if r.random() < 0.6:
            return r.choice(ARROW_BASE)
        return "".join(r.choice(ARROW_MODS) + r.choice(ARROW_PARTS) for _ in range(r.randint(1, 3)))

    # ---- attribute sets ---------------------------------------------------
    def fmt_attrs(self, attrs):
        out = []
        for k, v in attrs:
            if isinstance(v, str) and v.startswith("<") and v.endswith(">"):
                out.append(f"{k}={v}")
            else:
                out.append(f"{k}={q(v)}")
        return "[" + ", ".join(out) + "]" if out else ""

    def node_attrs(self, name, density):
        r = self.r
        a = []

        def maybe(p, k, f):
            if r.random() < p * density:
                a.append((k, f()))

        shape = None
        if r.random() < 0.5 * density:
            shape = r.choice(SHAPES) if r.random() < 0.5 else r.choice(COMMON_SHAPES)
            a.append(("shape", shape))
        if shape in ("record", "Mrecord") and r.random() < 0.85:
            a.append(("label", self.record_label(name)))
        elif r.random() < 0.08 * density:
            a.append(("label", self.html_label(name)))
        else:
            maybe(0.35, "label", self.text)
        maybe(0.1, "xlabel", self.text)
        maybe(0.15, "width", lambda: self.num(0.01, 3))
        maybe(0.15, "height", lambda: self.num(0.01, 2))
        maybe(0.1, "fixedsize", lambda: r.choice(["true", "false", "shape"]))
        maybe(0.12, "fontname", lambda: r.choice(FONTS))
        maybe(0.15, "fontsize", lambda: self.num(1, 40, 1))
        maybe(0.08, "fontcolor", self.color)
        maybe(0.15, "color", self.color)
        maybe(0.15, "fillcolor", self.color)
        maybe(0.2, "style", lambda: r.choice(STYLES_NODE))
        maybe(0.08, "penwidth", lambda: self.num(0, 5))
        maybe(0.06, "peripheries", lambda: str(r.randint(0, 3)))
        if shape == "polygon" or r.random() < 0.03:
            maybe(1.0, "sides", lambda: str(r.randint(3, 12)))
            maybe(0.5, "skew", lambda: self.num(-1, 1))
            maybe(0.5, "distortion", lambda: self.num(-1, 1))
        maybe(0.05, "orientation", lambda: self.num(0, 360, 0))
        maybe(0.04, "regular", lambda: "true")
        maybe(0.05, "margin", lambda: r.choice(["0", "0.3", "0.1,0.4", "0.05"]))
        maybe(0.05, "labelloc", lambda: r.choice(["t", "b", "c"]))
        maybe(0.05, "URL", lambda: r.choice(["http://example.com/\\N", "a.html", "x?y=1&z=2"]))
        maybe(0.05, "id", lambda: f"id_{name}")
        maybe(0.05, "class", lambda: r.choice(["c1", "c1 c2", "node-cls"]))
        maybe(0.05, "tooltip", self.text)
        maybe(0.03, "gradientangle", lambda: str(r.choice([0, 45, 90, 270])))
        if self.engine == "dot":
            maybe(0.06, "group", lambda: r.choice(["g1", "g2"]))
            maybe(0.03, "ordering", lambda: r.choice(["in", "out"]))
        if self.engine in ("neato", "fdp") and r.random() < 0.08:
            pin = r.random() < 0.4
            a.append(("pos", f"{self.num(-300, 300, 0)},{self.num(-300, 300, 0)}{'!' if pin else ''}"))
            if r.random() < 0.3:
                a.append(("pin", "true"))
        if self.engine == "patchwork":
            maybe(0.4, "area", lambda: self.num(0.1, 10))
        if self.engine == "osage":
            maybe(0.1, "sortv", lambda: str(r.randint(0, 9)))
        return a

    def edge_attrs(self, t, h, density, directed):
        r = self.r
        a = []

        def maybe(p, k, f):
            if r.random() < p * density:
                a.append((k, f()))

        maybe(0.2, "label", self.text)
        maybe(0.05, "xlabel", self.text)
        maybe(0.06, "headlabel", self.text)
        maybe(0.06, "taillabel", self.text)
        maybe(0.03, "labelangle", lambda: self.num(-180, 180, 0))
        maybe(0.03, "labeldistance", lambda: self.num(0, 3))
        maybe(0.03, "labelfloat", lambda: "true")
        maybe(0.03, "decorate", lambda: "true")
        maybe(0.1, "arrowhead", self.arrow)
        maybe(0.06, "arrowtail", self.arrow)
        maybe(0.05, "arrowsize", lambda: self.num(0.2, 3))
        maybe(0.1, "dir", lambda: r.choice(["forward", "back", "both", "none"]))
        maybe(0.15, "color", self.color)
        maybe(0.08, "fontcolor", self.color)
        maybe(0.06, "fontsize", lambda: self.num(4, 30, 1))
        maybe(0.04, "fontname", lambda: r.choice(FONTS))
        maybe(0.12, "style", lambda: r.choice(STYLES_EDGE))
        maybe(0.08, "penwidth", lambda: self.num(0, 5))
        maybe(0.05, "URL", lambda: "http://e.com/\\E")
        maybe(0.04, "id", lambda: f"e_{t}_{h}")
        maybe(0.04, "tooltip", self.text)
        maybe(0.04, "headclip", lambda: "false")
        maybe(0.04, "tailclip", lambda: "false")
        maybe(0.04, "class", lambda: "ecls")
        for port_attr, node in (("headport", h), ("tailport", t)):
            if r.random() < 0.08 * density:
                a.append((port_attr, self.port_for(node, allow_none=False)))
        if self.engine == "dot":
            maybe(0.1, "weight", lambda: str(r.choice([0, 1, 2, 5, 100])))
            maybe(0.08, "minlen", lambda: str(r.choice([0, 1, 2, 3])))
            maybe(0.06, "constraint", lambda: "false")
            maybe(0.03, "samehead", lambda: "sh1")
            maybe(0.03, "sametail", lambda: "st1")
            if self.clusters and r.random() < 0.08:
                a.append(("lhead", r.choice(self.clusters)))
            if self.clusters and r.random() < 0.08:
                a.append(("ltail", r.choice(self.clusters)))
        if self.engine in ("neato", "fdp", "sfdp"):
            maybe(0.1, "len", lambda: self.num(0.1, 4))
            maybe(0.05, "weight", lambda: self.num(0.5, 5))
        return a

    def port_for(self, node, allow_none=True):
        r = self.r
        ports = self.records.get(node, []) + self.html_ports.get(node, [])
        if ports and r.random() < 0.7:
            p = r.choice(ports)
            if r.random() < 0.3:
                p += ":" + r.choice(COMPASS)
            return p
        if r.random() < 0.05:
            return "nosuchport"
        return r.choice(COMPASS)

    def graph_attrs(self, n):
        r = self.r
        e = self.engine
        a = []

        def maybe(p, k, f):
            if r.random() < p:
                a.append((k, f()))

        maybe(0.12, "label", self.text)
        maybe(0.05, "labelloc", lambda: r.choice(["t", "b"]))
        maybe(0.05, "labeljust", lambda: r.choice(["l", "r", "c"]))
        maybe(0.06, "fontname", lambda: r.choice(FONTS))
        maybe(0.06, "fontsize", lambda: self.num(6, 30, 0))
        maybe(0.06, "bgcolor", self.color)
        maybe(0.08, "size", lambda: r.choice(["3,3", "5,5!", "10,2", "1,1", "7.5,10", "4", "20,20!"]))
        maybe(0.06, "ratio", lambda: r.choice(["fill", "compress", "expand", "auto", "0.5", "2", "1"]))
        maybe(0.04, "margin", lambda: r.choice(["0", "0.5", "1,0.2"]))
        maybe(0.05, "pad", lambda: r.choice(["0", "0.5", "1,0.2", "2"]))
        maybe(0.03, "rotate", lambda: "90")
        maybe(0.02, "landscape", lambda: "true")
        maybe(0.04, "center", lambda: "true")
        maybe(0.03, "dpi", lambda: r.choice(["72", "96", "36"]))
        maybe(0.04, "forcelabels", lambda: r.choice(["true", "false"]))
        maybe(0.04, "outputorder", lambda: r.choice(["breadthfirst", "nodesfirst", "edgesfirst"]))
        maybe(0.15, "splines", lambda: r.choice(["true", "false", "line", "polyline", "ortho",
                                                 "curved", "spline", "none", ""]))
        maybe(0.05, "stylesheet", lambda: "s.css")
        maybe(0.03, "class", lambda: "gcls")
        maybe(0.03, "id", lambda: "gid")
        maybe(0.03, "URL", lambda: "http://g.example/")
        maybe(0.03, "esep", lambda: r.choice(["+3", "0.2", "+1,4"]))
        if e != "dot":
            maybe(0.15, "pack", lambda: r.choice(["true", "false", "10", "0"]))
            maybe(0.1, "packmode", lambda: r.choice(["node", "clust", "graph", "array", "array_c2",
                                                     "array_u3t", "array_r", "array_b2"]))
        if e == "dot":
            maybe(0.2, "rankdir", lambda: r.choice(["TB", "LR", "BT", "RL"]))
            maybe(0.12, "nodesep", lambda: self.num(0.02, 2))
            maybe(0.12, "ranksep", lambda: r.choice([self.num(0.02, 2), "1.2 equally", "equally", "0.3"]))
            maybe(0.08, "concentrate", lambda: "true")
            maybe(0.15 if self.clusters else 0.03, "compound", lambda: "true")
            maybe(0.06, "ordering", lambda: r.choice(["in", "out"]))
            maybe(0.1, "newrank", lambda: "true")
            maybe(0.04, "clusterrank", lambda: r.choice(["local", "global", "none"]))
            maybe(0.04, "mclimit", lambda: self.num(0.1, 3))
            maybe(0.03, "nslimit", lambda: self.num(0.1, 3))
            maybe(0.03, "searchsize", lambda: str(r.randint(1, 50)))
            maybe(0.03, "remincross", lambda: "true")
            maybe(0.03, "quantum", lambda: self.num(0.1, 1))
            maybe(0.03, "TBbalance", lambda: r.choice(["min", "max"]))
            maybe(0.06, "pack", lambda: r.choice(["true", "8"]))
            maybe(0.04, "packmode", lambda: r.choice(["node", "clust", "graph", "array"]))
        if e == "neato":
            maybe(0.15, "mode", lambda: r.choice(["major", "KK", "sgd", "hier", "ipsep"]))
            maybe(0.12, "model", lambda: r.choice(["shortpath", "circuit", "subset", "mds"]))
            # an unseeded start=random uses time(NULL) in C: always give a seed
            maybe(0.06, "start", lambda: r.choice(["random7", "self", "regular", "5", "random3"]))
            maybe(0.06, "maxiter", lambda: str(r.choice([1, 10, 50, 200])))
            maybe(0.04, "epsilon", lambda: r.choice(["0.1", "0.0001"]))
            maybe(0.04, "Damping", lambda: self.num(0.1, 0.99))
            maybe(0.03, "defaultdist", lambda: self.num(0.5, 5))
            maybe(0.03, "inputscale", lambda: r.choice(["72", "0"]))
            maybe(0.03, "levelsgap", lambda: self.num(0, 2))
            maybe(0.02, "diredgeconstraints", lambda: "true")
            maybe(0.02, "notranslate", lambda: "true")
        if e in ("neato", "fdp", "sfdp", "circo", "twopi"):
            maybe(0.3, "overlap", lambda: r.choice(["false", "true", "scale", "prism", "compress",
                                                    "vpsc", "ortho", "scalexy", "portho", "voronoi",
                                                    "ipsep", "orthoxy", "prism0", "oscale"]))
            maybe(0.1, "sep", lambda: r.choice(["+4", "0.2", "+10,2", "5"]))
            maybe(0.04, "overlap_scaling", lambda: self.num(-4, 4))
        if e in ("fdp", "sfdp"):
            maybe(0.2, "K", lambda: self.num(0.05, 3))
            maybe(0.05, "maxiter", lambda: str(r.choice([1, 10, 100, 600])))
            # fdp seeds non-random starts with time(NULL)
            maybe(0.04, "start", lambda: r.choice(["random2", "3"]))
        if e == "fdp":
            maybe(0.05, "dim", lambda: "2")
        if e == "sfdp":
            maybe(0.1, "repulsiveforce", lambda: self.num(0.5, 3))
            maybe(0.08, "smoothing", lambda: r.choice(["none", "avg_dist", "graph_dist", "power_dist",
                                                       "rng", "spring", "triangle"]))
            maybe(0.08, "quadtree", lambda: r.choice(["normal", "fast", "none", "true", "false"]))
            maybe(0.05, "beautify", lambda: "true")
            maybe(0.05, "levels", lambda: str(r.randint(0, 5)))
            maybe(0.05, "label_scheme", lambda: str(r.randint(0, 4)))
            maybe(0.04, "rotation", lambda: self.num(0, 360, 0))
        if e == "twopi":
            maybe(0.3, "ranksep", lambda: r.choice(["0.5", "1.5", "0.3:1:2", "2"]))
            maybe(0.05, "rotation", lambda: self.num(0, 360, 0))
        if e == "circo":
            maybe(0.3, "mindist", lambda: self.num(0, 3))
            maybe(0.05, "oneblock", lambda: "true")
        return a

    def cluster_attrs(self):
        r = self.r
        a = []

        def maybe(p, k, f):
            if r.random() < p:
                a.append((k, f()))

        maybe(0.4, "label", self.text)
        maybe(0.25, "style", lambda: r.choice(STYLES_CLUSTER))
        maybe(0.2, "color", self.color)
        maybe(0.15, "fillcolor", self.color)
        maybe(0.1, "bgcolor", self.color)
        maybe(0.08, "penwidth", lambda: self.num(0, 4))
        maybe(0.08, "labeljust", lambda: r.choice(["l", "r"]))
        maybe(0.08, "labelloc", lambda: r.choice(["t", "b"]))
        maybe(0.06, "margin", lambda: r.choice(["0", "4", "20"]))
        maybe(0.05, "peripheries", lambda: str(r.randint(0, 2)))
        maybe(0.08, "fontsize", lambda: self.num(6, 30, 0))
        maybe(0.05, "fontcolor", self.color)
        maybe(0.04, "URL", lambda: "http://c.example/")
        maybe(0.04, "id", lambda: "cid")
        maybe(0.03, "tooltip", self.text)
        if self.engine == "osage":
            maybe(0.2, "pack", lambda: r.choice(["true", "4", "0"]))
            maybe(0.2, "packmode", lambda: r.choice(["array", "array_c2", "array_u", "graph", "node"]))
            maybe(0.1, "sortv", lambda: str(r.randint(0, 5)))
        if self.engine == "patchwork":
            maybe(0.2, "area", lambda: self.num(0.5, 10))
        if self.engine == "dot":
            maybe(0.06, "rank", lambda: r.choice(["same", "min", "max"]))
        return a

    # ---- structure ---------------------------------------------------------
    def node_name(self, i):
        r = self.r
        k = self.name_style
        if k == 0:
            return f"n{i}"
        if k == 1:
            return str(i)
        if k == 2:
            return ["node a", "Ωmega", "x-y", "strict", "graph", "edge", "a b c", "π", "日本"][i] if i < 9 else f"name {i}"
        return f"v{i}"

    def edges_for(self, n, directed):
        r = self.r
        kind = r.choice(["tree", "dag", "cycle", "random", "dense", "chain", "star", "grid",
                         "disconnected", "bipartite", "multi", "empty"])
        es = []
        if n <= 1:
            kind = r.choice(["empty", "multi"])
        if kind == "tree":
            for i in range(1, n):
                es.append((r.randrange(i), i))
        elif kind == "dag":
            m = int(n * r.uniform(1, 2.5))
            for _ in range(m):
                a, b = sorted(r.sample(range(n), 2))
                es.append((a, b))
        elif kind == "cycle":
            for i in range(n):
                es.append((i, (i + 1) % n))
            for _ in range(r.randint(0, n // 3)):
                es.append((r.randrange(n), r.randrange(n)))
        elif kind == "random":
            m = int(n * r.uniform(0.5, 2.5))
            for _ in range(m):
                es.append((r.randrange(n), r.randrange(n)))
        elif kind == "dense":
            nn = min(n, 14)
            p = r.uniform(0.3, 0.9)
            for i in range(nn):
                for j in range(nn):
                    if i != j and r.random() < p and (directed or i < j):
                        es.append((i, j))
        elif kind == "chain":
            for i in range(n - 1):
                es.append((i, i + 1))
        elif kind == "star":
            for i in range(1, n):
                es.append((0, i) if r.random() < 0.7 else (i, 0))
        elif kind == "grid":
            w = max(1, int(n ** 0.5))
            for i in range(n):
                if (i + 1) % w and i + 1 < n:
                    es.append((i, i + 1))
                if i + w < n:
                    es.append((i, i + w))
        elif kind == "disconnected":
            k = r.randint(2, 5)
            for i in range(1, n):
                if i % k:
                    es.append((i - 1, i) if r.random() < 0.8 else (r.randrange(i), i))
        elif kind == "bipartite":
            h = max(1, n // 2)
            for _ in range(n):
                es.append((r.randrange(h), r.randrange(h, n) if n > h else 0))
        elif kind == "multi":
            for _ in range(r.randint(1, 3 * max(n, 1))):
                a = r.randrange(n)
                b = a if r.random() < 0.3 else r.randrange(n)
                es.append((a, b))
        # sprinkle self-loops and multi-edges
        if n and r.random() < 0.2:
            for _ in range(r.randint(1, 3)):
                a = r.randrange(n)
                es.append((a, a))
        if es and r.random() < 0.2:
            for _ in range(r.randint(1, 3)):
                es.append(r.choice(es))
        if len(es) > 600:
            es = es[:600]
        return es

    def build(self):
        r = self.r
        e = self.engine
        size_class = r.choices(["tiny", "small", "medium", "large"], [30, 40, 22, 8])[0]
        n = {"tiny": r.randint(1, 8), "small": r.randint(9, 30), "medium": r.randint(31, 100),
             "large": r.randint(101, 300)}[size_class]
        density = {"tiny": 1.0, "small": 0.7, "medium": 0.35, "large": 0.12}[size_class]
        self.name_style = r.choices([0, 1, 2, 3], [60, 15, 10, 15])[0]
        directed = r.random() < (0.75 if e == "dot" else 0.4)
        strict = r.random() < 0.12
        names = [self.node_name(i) for i in range(n)]
        op = "->" if directed else "--"
        L = self.lines

        # clusters: a random forest of clusters over a subset of the nodes
        cl_members = {}
        cl_parent = {}
        ncl = 0
        if r.random() < (0.45 if e in ("dot", "fdp", "osage", "patchwork") else 0.15):
            ncl = r.randint(1, min(8, max(1, n // 2 + 1)))
        for c in range(ncl):
            nm = f"cluster_{c}" if r.random() < 0.9 else f"cluster{c}"
            self.clusters.append(nm)
            cl_parent[nm] = r.choice(self.clusters[:-1]) if c and r.random() < 0.4 else None
            cl_members[nm] = []
        for i in range(n):
            if self.clusters and r.random() < 0.5:
                cl_members[r.choice(self.clusters)].append(i)
                if r.random() < 0.04:  # overlapping membership
                    cl_members[r.choice(self.clusters)].append(i)

        L.append(f"{'strict ' if strict else ''}{'digraph' if directed else 'graph'} {q('G' if r.random() < 0.8 else 'fuzz ' + str(self.seed))} {{")
        if e != "dot" or r.random() < 0.15:
            L.append(f"layout={e};")
        for k, v in self.graph_attrs(n):
            L.append(f"{k}={self.fmt_attrs([(k, v)])[len(k) + 2:-1]};")
        if r.random() < 0.35:
            L.append("node " + (self.fmt_attrs(self.node_attrs("\\N", 0.6)) or "[]") + ";")
        if r.random() < 0.3:
            L.append("edge " + (self.fmt_attrs(self.edge_attrs("", "", 0.6, directed)) or "[]") + ";")

        # node declarations (attributes) first, so ports are known for edges
        declared = list(range(n))
        r.shuffle(declared)
        in_cluster = {i for m in cl_members.values() for i in m}
        node_lines = {}
        for i in declared:
            at = self.node_attrs(names[i], density)
            if at or i not in in_cluster or r.random() < 0.3:
                node_lines[i] = f"{q(names[i])} {self.fmt_attrs(at)};".replace(" ;", ";")

        def emit_cluster(nm, depth):
            L.append(f"subgraph {q(nm)} {{")
            for k, v in self.cluster_attrs():
                L.append(f"{k}={self.fmt_attrs([(k, v)])[len(k) + 2:-1]};")
            for i in cl_members[nm]:
                L.append(node_lines.pop(i, f"{q(names[i])};"))
            for c in self.clusters:
                if cl_parent.get(c) == nm:
                    emit_cluster(c, depth + 1)
            if r.random() < 0.15 and n:
                a, b = r.randrange(n), r.randrange(n)
                L.append(f"{q(names[a])} {op} {q(names[b])};")
            L.append("}")

        # top-level node lines interleaved with clusters
        for i in sorted(node_lines):
            if i not in in_cluster:
                L.append(node_lines.pop(i))
        for c in self.clusters:
            if cl_parent[c] is None:
                emit_cluster(c, 0)
        for i in sorted(node_lines):
            L.append(node_lines.pop(i))

        # rank constraints
        if e == "dot" and n > 2 and r.random() < 0.25:
            for _ in range(r.randint(1, 3)):
                kind = r.choice(["same", "same", "min", "max", "source", "sink"])
                mem = r.sample(names, min(n, r.randint(1, 4)))
                L.append("{")
                L.append(f"rank={kind};")
                for m in mem:
                    L.append(f"{q(m)};")
                L.append("}")

        # edges
        es = self.edges_for(n, directed)
        i = 0
        while i < len(es):
            t, h = es[i]
            k = r.random()
            tn, hn = q(names[t]), q(names[h])
            if r.random() < 0.06 * density * 3:
                tn += ":" + q(self.port_for(names[t]))
            if r.random() < 0.06 * density * 3:
                hn += ":" + q(self.port_for(names[h]))
            if k < 0.06 and i + 1 < len(es) and es[i + 1][0] == h:
                # chain a -> b -> c
                h2 = es[i + 1][1]
                at = self.edge_attrs(names[t], names[h2], density, directed)
                L.append(f"{tn} {op} {hn} {op} {q(names[h2])} {self.fmt_attrs(at)};".replace(" ;", ";"))
                i += 2
                continue
            if k < 0.10 and n > 3:
                grp = " ".join(q(x) for x in r.sample(names, min(n, r.randint(1, 3))))
                if r.random() < 0.5:
                    L.append(f"{{{grp}}} {op} {hn};")
                else:
                    L.append(f"{tn} {op} subgraph {q('s' + str(i))} {{{grp}}};")
                i += 1
                continue
            at = self.edge_attrs(names[t], names[h], density, directed)
            L.append(f"{tn} {op} {hn} {self.fmt_attrs(at)};".replace(" ;", ";"))
            i += 1
        if e == "twopi" and n and r.random() < 0.4:
            L.append(f"root={q(r.choice(names))};")
        L.append("}")
        if r.random() < 0.02:
            # a second graph in the same file
            L.append(f"{'digraph' if directed else 'graph'} second {{")
            L.append(f"layout={e};")
            L.append(f"x {op} y {op} z;")
            L.append("}")

    def malform(self):
        """Mutates a generated case into a (mildly) invalid one."""
        r = self.r
        L = self.lines
        k = r.randrange(12)
        if k == 0 and len(L) > 2:  # drop a line (often a brace)
            del L[r.randrange(len(L))]
        elif k == 1:  # truncate
            cut = r.randrange(1, len(L) + 1)
            del L[cut:]
            if L and r.random() < 0.5:
                L[-1] = L[-1][: r.randrange(len(L[-1]) + 1)]
        elif k == 2:  # garbage token
            L.insert(r.randrange(1, len(L)), r.choice(["@@", "->", "[", "]]", "= ;", "a -> ;",
                                                       "\"unterminated", "node [shape=box",
                                                       "subgraph {", "}", "edge [a=", "1.2.3"]))
        elif k == 3:  # bad attribute values
            bad = r.choice(["shape=bogus", "color=notacolor", "rankdir=XX", "fontsize=-3",
                            "width=abc", "splines=weird", "arrowhead=foo", "style=\"weird()\"",
                            "dir=xyz", "overlap=bogus", "nodesep=-1", "size=abc", "ratio=xyz",
                            "fillcolor=\"/nosuchscheme/3\"", "penwidth=-2", "peripheries=-1",
                            "fontname=\"\"", "labelloc=q", "height=0", "width=-1",
                            "style=\"filled,unknownstyle\"", "packmode=bogus", "mode=bogus",
                            "model=bogus", "ranksep=-2", "sides=0", "minlen=-1", "weight=-1",
                            "weight=1.5", "K=-1", "mindist=-1", "root=nosuchnode",
                            "lhead=nosuchcluster", "ltail=cluster_x", "pos=\"a,b\"",
                            "headport=zz", "label=<<B>unclosed>", "label=<<TABLE><TR></TABLE>>",
                            "label=<&bogus;>", "color=\"red:blue:\"", "arrowtail=xxx",
                            "samehead=\"\"", "rotate=45", "dpi=0", "pad=-1", "margin=-1"])
            idx = r.randrange(1, max(2, len(L)))
            line = L[idx] if idx < len(L) else ""
            if "[" in line and line.rstrip().endswith("];"):
                L[idx] = line.rstrip()[:-2] + ", " + bad + "];"
            else:
                L.insert(idx, bad + ";")
        elif k == 4:  # undefined port / bad edge op
            L.insert(len(L) - 1 if L else 0, r.choice(["a:nosuch -> b;", "a -- b;", "a -> b;",
                                                       "x:y:z:w -> q;", "\"\" -> \"\";",
                                                       "edge [headport=zz]; p -> q;"]))
        elif k == 5:  # unbalanced braces
            L.insert(r.randrange(1, max(2, len(L))), r.choice(["{", "}", "subgraph cluster_bad {"]))
        elif k == 6:  # bad HTML
            L.insert(len(L) - 1 if L else 0, r.choice([
                "h1 [label=<<TABLE><TR><TD>x</TD></TABLE>>];",
                "h2 [label=<<TABLE><TR><TD PORT=\"p\">x</TD></TR></TABLE>>]; h2:q -> h1;",
                "h3 [label=<<FONT>a</B>>];", "h4 [label=<a<BR>b>];",
                "h5 [label=<<TABLE><TR><TD COLSPAN=\"0\">x</TD></TR></TABLE>>];",
                "h6 [label=<<TABLE BORDER=\"-1\"><TR><TD>x</TD></TR></TABLE>>];",
                "h7 [label=<<TABLE><TR><TD><TABLE><TR><TD>in</TD></TR></TABLE></TD></TR></TABLE>>];",
                "h8 [label=<<IMG SRC=\"nosuch.png\"/>>];"]))
        elif k == 7:  # bad record
            L.insert(len(L) - 1 if L else 0, r.choice([
                "r1 [shape=record, label=\"{a|b\"];", "r2 [shape=record, label=\"a}|b\"];",
                "r3 [shape=record, label=\"<p>\"]; r3:p -> r3;", "r4 [shape=record, label=\"|||\"];",
                "r5 [shape=record, label=\"<a> x|<a> y\"]; r5:a -> r5;",
                "r6 [shape=Mrecord, label=\"\\\\{x\\\\}\"];"]))
        elif k == 8:  # wrong edge op for graph kind
            for i, line in enumerate(L):
                if "->" in line or "--" in line:
                    L[i] = line.replace("->", "--") if "->" in line else line.replace("--", "->")
                    break
        elif k == 9:  # duplicate attribute keys
            L.insert(1, "node [shape=box, shape=circle, label=\"a\", label=\"b\"];")
        elif k == 10:  # stray keyword
            L.insert(r.randrange(1, max(2, len(L))), r.choice(["strict;", "digraph;", "graph [];",
                                                               "node;", "edge;", "subgraph;"]))
        else:  # empty file / only a header
            if r.random() < 0.5:
                L[:] = []
            else:
                L[:] = [L[0]] if L else []

    def case(self):
        self.build()
        if self.malformed:
            self.malform()
        return "\n".join(self.lines) + ("\n" if self.lines else "")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--count", type=int, default=0, help="write COUNT cases (seed, seed+1, ...)")
    ap.add_argument("--out", type=pathlib.Path)
    ap.add_argument("--engine", choices=ENGINES)
    ap.add_argument("--malformed", type=float, default=0.08)
    a = ap.parse_args()
    if not a.count:
        print(Gen(a.seed, a.engine, a.malformed).case(), end="")
        return
    a.out.mkdir(parents=True, exist_ok=True)
    for s in range(a.seed, a.seed + a.count):
        g = Gen(s, a.engine, a.malformed)
        (a.out / f"s{s}.gv").write_text(g.case(), encoding="utf-8")


if __name__ == "__main__":
    main()
