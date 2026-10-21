#!/usr/bin/env python3
"""Writes the seed corpora of the cargo-fuzz targets from the repository's test graphs.

    python3 -I fuzz/seed-corpus.py OUT_DIR

OUT_DIR/dot gets every tests/**/*.gv no longer than the fuzz input limit (fuzz/src/lib.rs
MAX_INPUT), including the tests/**/fuzz_*.gv reproducers; it seeds `parse` and the
`render_*` targets. OUT_DIR/html gets the HTML labels (`label=<...>`) found in them, without
the outer delimiters, for `parse_html`. File names are content hashes, so equal inputs are
kept once.
"""

import hashlib
import pathlib
import sys

MAX_INPUT = 8192
ROOT = pathlib.Path(__file__).resolve().parent.parent


def html_labels(src: bytes):
    """The contents of `= <...>` values, matching nested angle brackets."""
    i = 0
    while (i := src.find(b"<", i)) != -1:
        j = i - 1
        while j >= 0 and src[j : j + 1].isspace():
            j -= 1
        if j < 0 or src[j : j + 1] != b"=":
            i += 1
            continue
        depth, k = 0, i
        while k < len(src):
            if src[k : k + 1] == b"<":
                depth += 1
            elif src[k : k + 1] == b">":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        if depth != 0:
            return
        yield src[i + 1 : k]
        i = k + 1


def write(dir_: pathlib.Path, data: bytes) -> None:
    (dir_ / hashlib.sha1(data).hexdigest()).write_bytes(data)


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = pathlib.Path(sys.argv[1])
    dot, html = out / "dot", out / "html"
    dot.mkdir(parents=True, exist_ok=True)
    html.mkdir(parents=True, exist_ok=True)
    for path in sorted((ROOT / "tests").rglob("*.gv")):
        src = path.read_bytes()
        if len(src) > MAX_INPUT:
            continue
        write(dot, src)
        for label in html_labels(src):
            if label.strip():
                write(html, label)
    print(f"{out}: {sum(1 for _ in dot.iterdir())} DOT, {sum(1 for _ in html.iterdir())} HTML seeds")


if __name__ == "__main__":
    main()
