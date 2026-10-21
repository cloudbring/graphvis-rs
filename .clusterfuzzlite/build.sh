#!/bin/bash -eu
# Builds the cargo-fuzz targets for ClusterFuzzLite and installs each one into $OUT with its
# seed corpus (tests/**/*.gv), the DOT dictionary and its libFuzzer options.
cd "$SRC/graphviz-pure/fuzz"
# Each target links the whole port with thin LTO; ten links at once run out of memory.
export CARGO_BUILD_JOBS=2
cargo fuzz build -O --debug-assertions

python3 -I seed-corpus.py "$WORK/seeds"
(cd "$WORK/seeds/dot" && zip -q -r "$WORK/dot_seed_corpus.zip" .)
(cd "$WORK/seeds/html" && zip -q -r "$WORK/html_seed_corpus.zip" .)

for src in fuzz_targets/*.rs; do
  name=$(basename "${src%.rs}")
  cp "target/x86_64-unknown-linux-gnu/release/$name" "$OUT/"
  cp dot.dict "$OUT/$name.dict"
  cp fuzz.options "$OUT/$name.options"
  if [ "$name" = parse_html ]; then
    cp "$WORK/html_seed_corpus.zip" "$OUT/${name}_seed_corpus.zip"
  else
    cp "$WORK/dot_seed_corpus.zip" "$OUT/${name}_seed_corpus.zip"
  fi
done
