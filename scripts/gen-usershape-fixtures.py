#!/usr/bin/env python3
"""Writes small image files of every type C's `imagetype` recognises (`lib/gvc/gvusershape.c`),
with odd headers too, and graphs that use them through `image=`, `shapefile=`, `shape=epsf`
and HTML `<IMG>`, for a differential sweep of the size readers and the image drawing (#347).

Only headers are written: C reads no pixel data, and neither does the port.

  python3 -I scripts/gen-usershape-fixtures.py OUT
  python3 -I scripts/gen-usershape-fixtures.py sweep OUT [--port BIN] [--label NAME]

The first form writes OUT/img/* (the images) and OUT/*.gv (the graphs, which name the
images as `img/NAME`, so run C and the port from OUT). `sweep` runs C with the port's math
(scripts/dot-portable-libm.sh, GVBINDIR ~/src/gv-ref/lutplugins) and the port (default
target/release/graphviz-pure) on every graph and format, from OUT, two at a time under a
timeout that kills each process group, and prints how many outputs and how many stderrs
are identical; with --label it also writes OUT/sweep-NAME.json.
"""
import concurrent.futures as cf
import json
import os
import pathlib
import struct
import subprocess
import sys
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
FORMATS = ["svg", "xdot", "json", "json0", "dot", "canon", "plain"]
TIMEOUT = 20
# Runs ARGV in a process group of its own and kills the whole group on timeout
# (scripts/fuzz-crash.py's supervisor); exits 142 on timeout.
SUPERVISE = r"""
$t = shift; $p = 0;
for my $s (qw(ALRM TERM INT HUP)) { $SIG{$s} = sub { kill 'KILL', -$p if $p; exit($_[0] eq 'ALRM' ? 142 : 143) } }
$p = fork; defined $p or exit 127;
if (!$p) { $SIG{$_} = 'DEFAULT' for qw(ALRM TERM INT HUP); setpgrp(0, 0); alarm $t + 2; exec @ARGV; exit 127 }
setpgrp($p, $p);
alarm $t; waitpid($p, 0);
exit($? & 127 ? 128 + ($? & 127) : $? >> 8);
"""


def png(w, h, truncate=None):
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IEND", b""))
    return data[:truncate] if truncate else data


def gif(w, h):
    return b"GIF89a" + struct.pack("<HH", w, h) + b"\x00\x00\x00" + b"\x00" * 8 + b";"


def jpeg(w, h, sof=0xC0, exif=False, standalone=False, truncate=None):
    out = b"\xff\xd8"
    out += b"\xff\xe0" + struct.pack(">H", 16) + b"JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00"
    if exif:
        body = b"Exif\x00\x00MM\x00\x2a\x00\x00\x00\x08" + b"\x00" * 40
        out += b"\xff\xe1" + struct.pack(">H", len(body) + 2) + body
    if standalone:
        out += b"\xff\xff\xff\x01\xff\xd0"
    out += bytes([0xFF, sof]) + struct.pack(">HBHHB", 17, 8, h, w, 3) + b"\x01\x11\x00" * 3
    out += b"\xff\xd9"
    return out[:truncate] if truncate else out


def bmp(w, h, header_size=40):
    body = struct.pack("<IiiHHIIiiII", header_size, w, h, 1, 24, 0, 0, 2835, 2835, 0, 0)
    return b"BM" + struct.pack("<IHHI", 14 + len(body), 0, 0, 14 + len(body)) + body


def ico(w, h):
    return b"\x00\x00\x01\x00\x01\x00" + bytes([w, h]) + b"\x00" * 14


def webp_vp8(w, h):
    vp8 = b"\x9d\x01\x2a" + struct.pack("<HH", w, h)
    payload = b"WEBP" + b"VP8 " + struct.pack("<I", 10) + b"\x00\x00\x00" + vp8
    return b"RIFF" + struct.pack("<I", len(payload)) + payload + b"\x00" * 8


def webp_vp8x(w, h):
    chunk = b"\x00\x00\x00\x00" + struct.pack("<I", w - 1)[:3] + struct.pack("<I", h - 1)[:3]
    payload = b"WEBP" + b"VP8X" + struct.pack("<I", len(chunk)) + chunk
    return b"RIFF" + struct.pack("<I", len(payload)) + payload + b"\x00" * 8


def webp_vp8l(w, h):
    bits = (w - 1) | ((h - 1) << 14)
    chunk = b"\x2f" + struct.pack("<I", bits)
    payload = b"WEBP" + b"VP8L" + struct.pack("<I", len(chunk)) + chunk
    return b"RIFF" + struct.pack("<I", len(payload)) + payload + b"\x00" * 8


def svg(attrs, decl=True, newline_between=False):
    head = '<?xml version="1.0" encoding="UTF-8"?>\n' if decl else ""
    sep = "\n" if newline_between else " "
    return (head + '<svg xmlns="http://www.w3.org/2000/svg"' + sep + attrs
            + '>\n<rect width="10" height="10"/>\n</svg>\n').encode()


IMAGES = {
    "a.png": png(40, 20),
    "wide.png": png(300, 10),
    "zero.png": png(0, 0),
    "big.png": png(0x7FFFFFFF, 3),
    "huge.png": png(0x80000000, 3),
    "trunc.png": png(40, 20, truncate=22),
    "a.gif": gif(30, 15),
    "trunc.gif": gif(30, 15)[:20],
    "a.jpg": jpeg(64, 48),
    "prog.jpeg": jpeg(33, 22, sof=0xC2),
    "exif.jpg": jpeg(50, 70, exif=True),
    "marks.jpg": jpeg(12, 34, standalone=True),
    "trunc.jpg": jpeg(64, 48, truncate=21),
    "a.bmp": bmp(25, 35),
    "odd.bmp": bmp(25, 35, header_size=0x00050028),
    "neg.bmp": bmp(-25, -35),
    "a.ico": ico(16, 32),
    "zero.ico": ico(0, 0),
    "a.webp": webp_vp8(44, 22),
    "x.webp": webp_vp8x(55, 11),
    "l.webp": webp_vp8l(21, 13),
    "riff.avi": b"RIFF" + struct.pack("<I", 12) + b"AVI LIST" + b"\x00" * 12,
    "in.svg": svg('width="2in" height="1in"'),
    "units.svg": svg('width="96px" height="3pc"'),
    "cm.svg": svg('width="2.5cm" height="10mm"'),
    "plain.svg": svg('width="50" height="25"'),
    "pct.svg": svg('width="100%" height="50%"'),
    "em.svg": svg('width="3em" height="2em"'),
    "vb.svg": svg('viewBox="0 0 120 60"'),
    "vbw.svg": svg('width="30pt" viewBox="0 0 120 60"'),
    "lines.svg": svg('width="80pt"\nheight="40pt"', newline_between=True),
    "nodecl.svg": svg('width="20" height="20"', decl=False),
    "noxmlsvg.xml": b'<?xml version="1.0"?>\n<html><body/></html>\n',
    "a.ps": b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 10 20 110 70\n0 0 moveto\n",
    "far.ps": b"%!PS-Adobe-3.0\n%%Creator: x\r%%BoundingBox: -50 -20 150 30\n",
    "nobb.ps": b"%!PS-Adobe-3.0\n%%Title: no box at all\n",
    "inv.ps": b"%!PS-Adobe-3.0\n%%BoundingBox: 100 100 0 0\n",
    "bin.eps": b"\xc5\xd0\xd3\xc6" + b"\x00" * 28,
    "a.pdf": b"%PDF-1.4\n1 0 obj << /Type /Page /MediaBox [0 0 200 100] >> endobj\n",
    "frac.pdf": b"%PDF-1.7\n<< /MediaBox [ 0.5 0 100.7 50.25 ] >>\n",
    "neg.pdf": b"%PDF-1.7\n<< /MediaBox [-10 0 100 50] >>\n",
    "nobox.pdf": b"%PDF-1.7\n<< /Type /Catalog /Pages 2 0 R >>\n",
    "short.png": b"\x89PNG\r\n\x1a\n",
    "text.txt": b"this is not an image file at all\n",
}

# shape=epsf needs a %%BoundingBox (epsf_init), the others go through imagetype
EPSF = ["a.ps", "far.ps", "nobb.ps", "inv.ps"]


def graphs():
    names = sorted(IMAGES)
    out = {}
    for name in names:
        stem = name.replace(".", "_")
        img = f"img/{name}"
        out[f"image_{stem}"] = (
            "digraph {\n"
            f'  a [image="{img}"];\n'
            f'  b [image="{img}" shape=box label="" imagescale=true];\n'
            f'  c [image="{img}" shape=box width=3 height=2 fixedsize=true imagepos=tl];\n'
            f'  d [image="{img}" imagescale=width imagepos=br labelloc=b label="long label text"];\n'
            f'  e [image="{img}" imagescale=height imagepos=mr labelloc=t shape=hexagon];\n'
            f'  f [image="{img}" imagescale=both peripheries=2 style=filled fillcolor=yellow];\n'
            "  a -> b -> c; a -> d -> e -> f;\n"
            "}\n")
        out[f"shapefile_{stem}"] = (
            "digraph {\n"
            f'  a [shapefile="{img}"];\n'
            f'  b [shape=box shapefile="{img}" style=filled];\n'
            f'  c [shape=custom shapefile="{img}" imagepos=bl];\n'
            "  a -> b -> c;\n"
            "}\n")
        out[f"html_{stem}"] = (
            "digraph {\n"
            f'  a [shape=plain label=<<TABLE><TR><TD><IMG SRC="{img}"/></TD></TR></TABLE>>];\n'
            f'  b [label=<<TABLE><TR><TD FIXEDSIZE="TRUE" WIDTH="50" HEIGHT="30">'
            f'<IMG SRC="{img}" SCALE="TRUE"/></TD><TD><IMG SRC="{img}" SCALE="BOTH"/></TD>'
            f'<TD WIDTH="90" HEIGHT="90"><IMG SRC="{img}" SCALE="WIDTH"/></TD></TR></TABLE>>];\n'
            "  a -> b;\n"
            "}\n")
        out[f"dpi_{stem}"] = (
            "digraph {\n  dpi=150;\n"
            f'  a [image="{img}"];\n'
            f'  b [label=<<TABLE><TR><TD><IMG SRC="{img}"/></TD></TR></TABLE>>];\n'
            "  a -> b;\n"
            "}\n")
    for name in EPSF:
        stem = name.replace(".", "_")
        out[f"epsf_{stem}"] = (
            "digraph {\n"
            f'  a [shape=epsf shapefile="img/{name}"];\n'
            f'  b [shape=epsf shapefile="img/{name}" label="b"];\n'
            "  a -> b;\n"
            "}\n")
    # several types in one graph: the loadimage engine of an earlier image is reused
    out["mixed"] = (
        "digraph {\n"
        '  a [image="img/a.png"]; b [image="img/a.ps"]; c [image="img/a.bmp"];\n'
        '  d [image="img/in.svg"]; e [image="img/a.gif"]; f [image="img/a.pdf"];\n'
        "  a -> b -> c -> d -> e -> f;\n"
        "}\n")
    out["mixed_ps_first"] = (
        "digraph {\n"
        '  a [image="img/a.ps"]; b [image="img/a.png"]; c [image="img/a.webp"];\n'
        "  a -> b -> c;\n"
        "}\n")
    out["rotate"] = (
        "digraph {\n  rotate=90;\n"
        '  a [image="img/a.png"]; b [image="img/wide.png" shape=box imagescale=true];\n'
        "  a -> b;\n"
        "}\n")
    out["cluster_lr"] = (
        "digraph {\n  rankdir=LR;\n"
        '  subgraph cluster_x { a [image="img/a.jpg" labelloc=b]; }\n'
        '  b [image="img/a.gif" imagepos=tc]; a -> b;\n'
        "}\n")
    out["multi_graphs"] = (
        'digraph one { a [image="img/a.png"]; b [image="img/trunc.jpg"]; }\n'
        'digraph two { c [image="img/a.ps"]; d [image="img/trunc.jpg"]; e [image="img/a.bmp"]; }\n'
        'digraph three { imagepath="img"; f [image="trunc.jpg"]; g [image="a.gif"]; }\n')
    out["imagepath"] = (
        'digraph { imagepath="nowhere:img"; a [image="a.png"]; b [image="a.svg"];\n'
        '  c [label=<<TABLE><TR><TD><IMG SRC="in.svg"/></TD></TR></TABLE>>]; }\n')
    out["html_objects"] = (
        "digraph {\n  imagescale=true;\n"
        '  label=<<TABLE><TR><TD WIDTH="100" HEIGHT="20"><IMG SRC="img/a.png"/></TD></TR></TABLE>>;\n'
        '  subgraph cluster_c { imagescale=width;\n'
        '    label=<<TABLE><TR><TD WIDTH="80" HEIGHT="40"><IMG SRC="img/a.gif"/></TD></TR></TABLE>>;\n'
        '    a [image="img/a.jpg" imagescale=height shape=box width=2]; }\n'
        '  a -> b [label=<<TABLE><TR><TD><IMG SRC="img/in.svg" SCALE="both"/></TD></TR></TABLE>>'
        ' imagescale=both];\n'
        "}\n")
    out["neato_images"] = (
        "graph {\n  layout=neato; start=1;\n"
        '  a [image="img/a.png"]; b [image="img/wide.png" shape=box imagescale=true];\n'
        '  c [shapefile="img/a.gif"]; d [shape=epsf shapefile="img/a.ps"];\n'
        "  a -- b -- c -- d -- a;\n"
        "}\n")
    out["epsf_files"] = (
        "digraph {\n"
        '  a [shape=epsf shapefile="img/far.ps"]; b [shape=epsf shapefile="img/a.ps"];\n'
        '  c [shape=epsf shapefile="img/far.ps" URL="x" tooltip="t"]; d [shape=epsf shapefile="img/nobb.ps"];\n'
        '  e [shape=epsf]; a -> b -> c -> d -> e;\n'
        "}\n")
    # EPSF_contents is keyed by the pointer safefile returned: a graph's string (gone with
    # the graph) or findPath's static buffer (which matches every lookup through it)
    out["epsf_graphs"] = (
        'digraph A { a [shape=epsf shapefile="img/a.ps"]; b [shape=epsf shapefile="img/inv.ps"] }\n'
        'digraph B { c [shape=epsf shapefile="img/inv.ps"]; d [shape=epsf shapefile="img/a.ps"] }\n'
        'digraph C { imagepath="img"; e [shape=epsf shapefile="inv.ps"]; f [shape=epsf shapefile="a.ps"] }\n'
        'digraph D { imagepath="."; g [shape=epsf shapefile="img/a.ps"]; h [shape=epsf shapefile="/nonexistent.ps"] }\n'
        'digraph E { i [shape=epsf shapefile="img/a.ps"] }\n')
    out["odd_names"] = (
        "digraph {\n"
        '  a [image="img"]; b [shape=custom]; c [shape=custom shapefile=""];\n'
        '  d [image="img/missing.png"]; e [shape=foo image="img/a.png"];\n'
        '  f [image="img/a.png" peripheries=0 style=filled fillcolor=red];\n'
        '  g [image="img/a.png" shape=point]; h [image="img/a.png" shape=record label="x|y"];\n'
        '  i [image="img/a.png" shape=star]; j [image="img/a.png" shape=cylinder];\n'
        '  k [image="img/a.png" regular=true shape=polygon sides=5 orientation=20];\n'
        "}\n")
    return out


def run(argv, cwd, env=None):
    p = subprocess.run(["perl", "-e", SUPERVISE, str(TIMEOUT)] + argv, cwd=cwd, env=env,
                       capture_output=True, stdin=subprocess.DEVNULL, timeout=TIMEOUT + 10)
    out = b"".join(l for l in p.stdout.splitlines(keepends=True)
                   if not l.startswith(b"<!-- Generated by "))
    return p.returncode, out, p.stderr


def sweep(args):
    out = pathlib.Path(args[0]).resolve()
    port = str(ROOT / "target/release/graphviz-pure")
    label = None
    rest = args[1:]
    while rest:
        if rest[0] == "--port":
            port, rest = rest[1], rest[2:]
        elif rest[0] == "--label":
            label, rest = rest[1], rest[2:]
        else:
            sys.exit(__doc__)
    c_dot = [str(ROOT / "scripts/dot-portable-libm.sh")]
    env = dict(os.environ, GVBINDIR=str(pathlib.Path.home() / "src/gv-ref/lutplugins"))
    cases = [(g.name, fmt) for g in sorted(out.glob("*.gv")) for fmt in FORMATS]

    def one(case):
        name, fmt = case
        c = run(c_dot + [f"-T{fmt}", name], out, env)
        p = run([port, f"-T{fmt}", name], out)
        return f"{name}:{fmt}", {
            "c_status": c[0], "port_status": p[0],
            "out": c[1] == p[1], "err": c[2] == p[2],
        }

    with cf.ThreadPoolExecutor(max_workers=2) as ex:
        results = dict(ex.map(one, cases))
    ok = {k: v for k, v in results.items() if v["c_status"] < 128}
    print(f"{len(results)} runs, C crashed or timed out on {len(results) - len(ok)}")
    print(f"of the other {len(ok)}: output identical {sum(v['out'] for v in ok.values())}, "
          f"stderr identical {sum(v['err'] for v in ok.values())}, "
          f"both {sum(v['out'] and v['err'] for v in ok.values())}")
    print(f"port crashed or timed out: {sum(v['port_status'] >= 101 for v in results.values())}")
    if label:
        (out / f"sweep-{label}.json").write_text(json.dumps(results, indent=1, sort_keys=True))


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "sweep":
        return sweep(sys.argv[2:])
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = pathlib.Path(sys.argv[1])
    (out / "img").mkdir(parents=True, exist_ok=True)
    for name, data in IMAGES.items():
        (out / "img" / name).write_bytes(data)
    for stem, text in graphs().items():
        (out / f"{stem}.gv").write_text(text)


if __name__ == "__main__":
    main()
