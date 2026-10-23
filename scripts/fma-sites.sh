#!/usr/bin/env bash
# Lists every C source line where the reference build of Graphviz (Homebrew 16.1.0 on macOS
# arm64) computes a fused multiply-add or calls `__sincos_stret` (docs/porting/libm.md,
# "Fused multiply-adds").
#
#   scripts/fma-sites.sh ~/src/gv-ref/graphviz-16.1.0.tar.xz /tmp/fma-sites
#
# Homebrew's dylibs have no debug information, so this builds the same sources with the same
# compiler and `-Os -g` (Homebrew's flags), checks that the fused instructions per function
# match the installed dylibs, and maps each one to its source line with atos. Writes
# OUT/fma-sites.tsv: C file:line, inlined function, instructions, C source text. macOS only.
# GTS (Homebrew's libgts, used by neato) is not covered; docs/porting/libm.md says how it was checked.
set -euo pipefail
TARBALL="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
OUT="$2"
LIBS="lib/gvc/.libs/libgvc.7 lib/pathplan/.libs/libpathplan.4 plugin/core/.libs/libgvplugin_core.8
      plugin/dot_layout/.libs/libgvplugin_dot_layout.8 plugin/neato_layout/.libs/libgvplugin_neato_layout.8"
FUSED='\t(fmadd|fmsub|fnmadd|fnmsub|fmla|fmls)(\.[0-9a-z]+)?\t|___sincos_stret'
mkdir -p "$OUT"
cd "$OUT"
if [ ! -f src/config.h ]; then
  mkdir -p src
  tar -xf "$TARBALL" -C src --strip-components=1
  (cd src && ./configure --with-gts --without-pangocairo --without-gdk --without-gdk-pixbuf \
     --without-gtk --without-x --without-gd --without-rsvg --without-ghostscript --without-poppler \
     --without-webp --without-lasi --without-qt --disable-swig --disable-tcl --disable-perl \
     --disable-python --disable-man-pdfs >configure.log)
fi
make -C src -j3 CFLAGS="-g -Os" CXXFLAGS="-g -Os" >make.log
per_function() {
  awk -v re="$FUSED" '/^_[A-Za-z0-9_.$]+:$/{fn=$1} $0 ~ re {c[fn]++} END{for (k in c) print k, c[k]}' "$1" | sort
}
: >fma-sites.libs
for p in $LIBS; do
  b=$(basename "$p")
  otool -tv "src/$p.dylib" >"$b.s"
  ref=/opt/homebrew/lib/$b.dylib
  [ -f "$ref" ] || ref=/opt/homebrew/lib/graphviz/$b.dylib
  if ! diff <(per_function "$b.s") <(otool -tv "$ref" | per_function /dev/stdin) >/dev/null; then
    echo "$b: the fused instructions per function differ from $ref; the map would be wrong" >&2
    exit 1
  fi
  dsymutil "src/$p.dylib" -o "$b.dSYM"
  grep -E "$FUSED" "$b.s" | awk '{print $1, ($2 == "bl" ? "sincos" : $2)}' >"$b.insn"
  cut -d' ' -f1 "$b.insn" | xargs xcrun atos -i -o "$b.dSYM/Contents/Resources/DWARF/$b.dylib" -arch arm64 >"$b.atos"
  echo "$b" >>fma-sites.libs
done
# shellcheck disable=SC2046
python3 -I - src $(cat fma-sites.libs) >fma-sites.tsv <<'EOF'
import collections, os, re, sys
src = sys.argv[1]
files = collections.defaultdict(list)
for d in ("lib", "plugin"):
    for root, _, names in os.walk(os.path.join(src, d)):
        for n in names:
            files[n].append(os.path.join(root, n))
frame = re.compile(r"^(.*) \(in [^)]*\) \(([^:]+):(\d+)\)$")
sites = collections.defaultdict(collections.Counter)
fns = collections.defaultdict(set)
for lib in sys.argv[2:]:
    insns = [l.split()[1] for l in open(f"{lib}.insn")]
    # `atos -i` prints the inlined frames of each address, innermost first, then a blank line.
    blocks = open(f"{lib}.atos").read().strip("\n").split("\n\n")
    assert len(blocks) == len(insns), lib
    for insn, block in zip(insns, blocks):
        fn, name, n = frame.match(block.splitlines()[0]).groups()
        sites[(name, int(n))][insn] += 1
        fns[(name, int(n))].add(fn)
for (name, n), insns in sorted(sites.items()):
    paths = [p for p in files[name] if 0 < n <= len(open(p, errors="replace").readlines())]
    text = " || ".join(open(p, errors="replace").readlines()[n - 1].strip() for p in paths) or "?"
    where = ",".join(os.path.relpath(p, src) for p in paths) or name
    print(f"{where}:{n}\t{','.join(sorted(fns[(name, n)]))}\t{dict(insns)}\t{text}")
EOF
wc -l fma-sites.tsv
