#!/usr/bin/env bash
# Runs the corpus through C Graphviz on Linux (the gv-c-linux image, see Dockerfile here) and
# through the port's Linux build, in the container, and writes to OUT (default /tmp/gv-c-linux):
#   e2e.linux-c.json      C-on-Linux hashes (gv-diff e2e --generate --runs 5)
#   port-vs-linux-c.txt   the port (portable math) against C on Linux
#   e2e.linux-c-libm-matched.json, port-vs-linux-c-libm-matched.txt
#                         the same with C running on the port's portable math functions
#   e2e.linux-c-apple-libc.json, port-vs-linux-c-apple-libc.txt
#                         and with Apple's qsort, rand and srand as well (feature apple-libc)
# Compare e2e.linux-c.json with tests/golden/e2e.json (C on macOS) to see how far C on Linux
# is from C on macOS.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-/tmp/gv-c-linux}"
IMAGE="${IMAGE:-gv-c-linux}"
mkdir -p "$OUT"
# The sources go in through a volume rather than a bind mount, so this works whatever
# directories the Docker VM shares.
docker volume rm -f gv-c-linux-src >/dev/null
COPYFILE_DISABLE=1 tar -C "$ROOT" -cf - --no-xattrs --exclude ./target --exclude ./site/node_modules --exclude ./web/node_modules \
    --exclude ./.git . \
  | docker run --rm -i -v gv-c-linux-src:/repo "$IMAGE" tar -xf - -C /repo
docker run --rm -v gv-c-linux-src:/repo -v "$IMAGE-target":/target --name gv-c-linux-run \
  -e CARGO_TARGET_DIR=/target -w /repo "$IMAGE" bash -c '
    set -e
    mkdir -p /repo/out
    cargo build --release -q -p graphviz-pure-diff -p graphviz-pure-portable-libm
    /target/release/gv-diff e2e --generate --runs 5 --golden /repo/out/e2e.linux-c.json
    echo "{\"identical\":[]}" > /tmp/none.json
    /target/release/gv-diff e2e --golden /repo/out/e2e.linux-c.json --baseline /tmp/none.json \
      --list --hashes /repo/out/port-hashes.json > /repo/out/port-vs-linux-c.txt 2>/dev/null || true
    /target/release/gv-diff e2e --generate --runs 5 --c-dot scripts/dot-portable-libm.sh \
      --golden /repo/out/e2e.linux-c-libm-matched.json
    /target/release/gv-diff e2e --golden /repo/out/e2e.linux-c-libm-matched.json --baseline /tmp/none.json \
      --list > /repo/out/port-vs-linux-c-libm-matched.txt 2>/dev/null || true
    CARGO_TARGET_DIR=/target/apple-libc cargo build --release -q -p graphviz-pure-portable-libm --features apple-libc
    GV_PORTABLE_LIBM=/target/apple-libc/release/libgraphviz_pure_portable_libm.so /target/release/gv-diff e2e \
      --generate --runs 5 --c-dot scripts/dot-portable-libm.sh --golden /repo/out/e2e.linux-c-apple-libc.json
    /target/release/gv-diff e2e --golden /repo/out/e2e.linux-c-apple-libc.json --baseline /tmp/none.json \
      --list > /repo/out/port-vs-linux-c-apple-libc.txt 2>/dev/null || true
  '
docker run --rm -v gv-c-linux-src:/repo "$IMAGE" tar -C /repo/out -cf - . | tar -C "$OUT" -xf -
tail -14 "$OUT/port-vs-linux-c.txt"
tail -14 "$OUT/port-vs-linux-c-libm-matched.txt"
tail -14 "$OUT/port-vs-linux-c-apple-libc.txt"
