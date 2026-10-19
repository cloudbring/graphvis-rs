# C Graphviz 16.1.0 on Linux, built like the macOS reference (scripts/gen-golden.sh): clang,
# no Pango, and a plugin directory with only the core, dot and neato plugins, so C measures
# text with its built-in font tables. Used to measure how far C on Linux is from C on macOS,
# and how far the port's portable build is from C on Linux (docs/porting/libm.md).
#
#   docker build -t gv-c-linux -f scripts/c-oracle-linux/Dockerfile <dir holding graphviz-16.1.0.tar.xz>
#   scripts/c-oracle-linux/run.sh
FROM rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      clang bison flex pkg-config libexpat1-dev zlib1g-dev libltdl-dev xz-utils \
    && rm -rf /var/lib/apt/lists/*
COPY graphviz-16.1.0.tar.xz /src/
RUN cd /src && tar xf graphviz-16.1.0.tar.xz && cd graphviz-16.1.0 \
    && CC=clang CXX=clang++ CFLAGS=-O2 CXXFLAGS=-O2 ./configure --prefix=/opt/gv \
         --without-pangocairo --without-gdk --without-gdk-pixbuf --without-gtk --without-gts \
         --without-x --without-gd --without-rsvg --without-ghostscript --without-poppler \
         --without-webp --without-lasi --without-qt --disable-swig --disable-tcl \
         --disable-perl --disable-python --disable-man-pdfs \
    && make -j"$(nproc)" && make install
RUN mkdir /opt/gv/lutplugins \
    && cp -P /opt/gv/lib/graphviz/libgvplugin_core.so* /opt/gv/lib/graphviz/libgvplugin_dot_layout.so* \
         /opt/gv/lib/graphviz/libgvplugin_neato_layout.so* /opt/gv/lutplugins/ \
    && GVBINDIR=/opt/gv/lutplugins /opt/gv/bin/dot -c
ENV PATH=/opt/gv/bin:$PATH GVBINDIR=/opt/gv/lutplugins
