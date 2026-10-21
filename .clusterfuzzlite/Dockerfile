# ClusterFuzzLite build image for the cargo-fuzz targets in fuzz/ (.github/workflows/fuzz.yml).
# The OSS-Fuzz Rust builder, pinned by digest: a nightly toolchain and cargo-fuzz.
FROM gcr.io/oss-fuzz-base/base-builder-rust:v1@sha256:3428d0a90b1e7efbe2ba947bd1d0c9a70594986b4b6553804547f1fb24cfc92f
COPY . $SRC/graphviz-pure
WORKDIR $SRC/graphviz-pure
COPY .clusterfuzzlite/build.sh $SRC/
