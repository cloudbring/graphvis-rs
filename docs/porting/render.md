# Renderer port (`crates/graphviz-pure-render`)

The output formats are produced by a function-by-function port of the Graphviz
16.1.0 emit pipeline, so SVG, xdot and JSON share one traversal exactly as in C.
Node, label, record and HTML geometry come from `graphviz_pure_geom::node_size` /
`graphviz_pure_geom::html_label` (the init half of `shapes.c`, `labels.c`, `htmltable.c`);
positions come from the layout.

## Module map

| Rust | C source |
|---|---|
| `emit/model.rs` | the post-layout `Agraphinfo_t`/`Agnodeinfo_t`/`Agedgeinfo_t` state: `GD_clust` tree, `ND_coord`/`lw`/`rw`/`ht`, `ND_shape_info`, `ED_spl`, `textlabel_t`/`textspan_t`, `place_root_label`/`place_graph_label` fallbacks, `agfstout` order (head id, then sequence) |
| `emit/job.rs` | `lib/gvc/gvrender.c` (`gvrender_*`, `gvrender_resolve_color`, `gvrender_set_style`), `obj_state_t` stack (`push_obj_state`/`pop_obj_state`), job geometry from `gvRenderJobs` (`init_job_pad`/`margin`/`dpi`/`viewport`/`pagination`, `setup_page`, `emit_graph` scale) |
| `emit/emit.rs` | `lib/common/emit.c`: `emit_graph`, `emit_page`, `emit_background`, `emit_view` (outputorder), `emit_clusters`, `emit_node`, `emit_edge`, `emit_begin_edge`/`emit_end_edge`, `emit_edge_graphics` (multicolor, parallel colors, tapered, ortho rounded corners), `emit_edge_label`, `emit_attachment`, `initObjMapData`/`getObjId`/`preprocessTooltip`, layers (`init_layering`, `parse_layers`, `parse_layerselect`, `selectedLayer`, `*_in_layer`), `parseSegs`, `findStopColor`, `stripedBox`; `emit_label` from `labels.c` |
| `emit/shapes.rs` | `shapes.c` gencode: `poly_gencode`, `point_gencode`, `record_gencode`/`gen_fields`, `checkStyle`, `stylenode`, `penColor`, `findFill`, `Mcircle_hack`, `round_corners` (rounded, diagonals, cylinder), `wedgedEllipse` |
| `emit/corners.rs` | `round_corners` shape cases: note, tab, folder, box3d, component and all SBOL shapes |
| `emit/arrows.rs` | `arrows.c`: `arrow_flags`, `arrow_match_name`, `arrow_gen`, every arrow type with its miter/penwidth corrections, `Bezier` |
| `emit/html.rs` | `htmltable.c` emit half: `emit_html_label`, `emit_html_tbl`, `emit_html_cell`, `emit_html_rules`, `doBorder`, `setFill`, `emit_htextspans`, `initAnchor`/`endAnchor` |
| `emit/geom.rs` | `dotneato_closest`, `splitBSpline`, `computeoffset_p/qr`, `get_gradient_points` (`utils.c`), `taper` (`taper.c`), `ellipticWedge` (`ellipse.c`) |
| `emit/output.rs` | `output.c`: `attach_attrs_and_arrows`, `set_record_rects`, `rec_attach_bb`, `write_plain` |
| `emit/num.rs` | `gvprintdouble`, `gvprintnum`, `agxbuf_trim_zeros`, `%.Ng` |
| `emit/attrs.rs` | `late_*`, `mapbool`, `parse_style`, `strtod`/`strtol` |
| `svg_engine.rs` | `plugin/core/gvrender_core_svg.c` (incl. gradients, layers, anchors, `gv_xml_escape`) |
| `xdot_engine.rs` | xdot half of `plugin/core/gvrender_core_dot.c` (per-state buffers, penwidth/textflags state, gradients) |
| `json.rs` | `plugin/core/gvrender_core_json.c` and the xdot parser of `lib/xdot/xdot.c` |
| `agwrite.rs` | `lib/cgraph/write.c` (`-Tdot`, `-Txdot`, `-Tcanon`) over the cgraph dictionaries recorded by the parser (`Graph::dicts`: `bindattrs`, `attrstmt`/`setattr`, `unviewsubgraphsattr`, `sym->print`, ProtoGraph `label`) |
| `lib.rs` | format dispatch, `safe_dcl`/`agxset` bookkeeping (`sync_dicts`), dot's `mark_lowclusters` edge membership |

`graphviz-pure-layout/src/nop.rs` ports neato's `Nop` path (`init_nop`, `user_pos`,
`user_spline`, `nop_init_graphs`, `nop_init_edges`, `chkBB`, `compute_bb`,
`neato_translate`, `neato_set_aspect`, `place_graph_label`) for `-n`, `-n1`,
`-n2`, `-Knop`/`nop1`/`nop2`. Edges without a usable `pos` go through the
neatogen `spline_edges_` machinery (ports, equivalence classes, straight or
`splines=` routing, clipping).

## Fidelity

The render corpus isolates the renderers: C lays every `tests/wild`,
`tests/fixtures` and `tests/render` graph out (`dot -Tdot`), then both C and
the port render that file in no-layout mode (`neato -n2`). See
`scripts/gen-render-golden.sh` and `gv-diff render`
(`crates/graphviz-pure-diff/src/render.rs`, ratchet `tests/golden/render/baseline.json`,
test `crates/graphviz-pure-diff/tests/render_fidelity.rs`). The SVG generator comment is
the only line ignored. `tests/render/*.gv` are 21 feature graphs (all arrow
types, styles, shapes, gradients, HTML, records, URLs, labels, rotation,
size/ratio, layers, output order).

Byte-identical files, original 508-file corpus (before = legacy renderers with
`-n2` support, after = this port; plain formats exclude 4 nondeterministic C
references):

| format | before | after |
|---|---|---|
| svg | 36 / 508 | 507 / 508 |
| json | 0 / 472 | 472 / 472 |
| json0 | 0 / 508 | 507 / 508 |
| xdot | 2 / 508 | 507 / 508 |
| dot | 207 / 508 | 507 / 508 |
| plain | 276 / 504 | 501 / 504 |
| plain-ext | 260 / 504 | 501 / 504 |

All 21 feature graphs are identical in every format. `-Tcanon` (agwrite on the
parsed input) matches C on 493 of 508 inputs (454 before).

## Known differences

- `-Tplain` of labels changed by `htmlEntityUTF8`/`latin1ToUTF8` that need no
  quoting: `canon` in `write_plain` (`lib/common/output.c`) interns the text,
  `agstrcanon` returns it unquoted and `agstrfree` frees it before it is
  printed, so C prints whatever the freed memory holds (`gv16_graphs_Latin1`,
  `b60`; the `charset_*` Latin-1 cases, `gallery__index_7`, `Symbol`, `b34`,
  `b56` change from run to run and are skipped, `UNSTABLE` in `gv-diff`). The
  port prints the converted label; the garbage is not emulated.
- HTML cell `STYLE="dashed"` borders: C's `xdot_style` walks past the end of a
  one-element style list and prints adjacent string literals (undefined
  behaviour); not reproduced (`tests/fuzz-open/034-html-cell-dashed-style-overread`).
- Images (`image=`, `shapefile=`, `<IMG>`) are never loaded; C draws them only
  when the file exists.
- The SVG generator comment names graphviz-pure.
- Gradient and HTML anchor ids are numbered per `render_dot_all` call, like C's
  process-wide statics for a single invocation.

## Charset

C keeps the input's bytes and converts only label text. `graphviz_pure_core::charset`
gives the port the same model with `String`s: bytes that are not part of valid
UTF-8 become the code points `U+10FF00 + byte` (`decode`), and `encode` turns
them back into bytes. The CLI decodes its input and encodes its output, and
`gv-diff` reads inputs and C references the same way, so attribute values,
names, `-Tdot`/`-Tcanon`/`-Txdot` strings, SVG titles and ids, tooltips and
URLs carry C's bytes (SVG always declares `encoding="UTF-8"`; `gv_xml_escape`
leaves bytes `>= 0x80` alone, and so does the port).

- `make_label`, HTML fallbacks and tooltips (`preprocessTooltip`) convert with
  `latin1ToUTF8` (`charset=latin1`) or `htmlEntityUTF8` (else) on the encoded
  bytes, once, where C does. `htmlEntityUTF8` reproduces C's recovery for
  invalid UTF-8 (the offending byte is taken as Latin-1, possibly leaving
  invalid UTF-8, kept as raw bytes) and its once-per-graph warning.
- Text width is measured per encoded byte, with `textspan_lut.c`'s
  once-per-process "no value for width" warnings.
- HTML labels: expat gets the graph's encoding. Latin-1 bytes are decoded to
  UTF-8, invalid UTF-8 is a fatal error, and `BIG-5` is an unknown encoding,
  so every Big5 HTML label fails. A failed HTML label is plain text with its
  unconverted source afterwards (`lp->html` cleared): `-Tplain` quotes it.
- `charset=big5`: labels convert like UTF-8; `make_simple_label` keeps the byte
  after a `0xA1..=0xFE` byte with it (so a `\` there is not an escape).
  Unknown charsets warn `Unsupported charset` and act as UTF-8.
- `-Txdot` string lengths count bytes; `-Tjson` runs `latin1ToUTF8` over every
  string of a Latin-1 graph (so label text, already UTF-8, is converted again,
  as in C) and escapes only `"`, `\`, `/`, `\b`, `\f`, `\n`, `\r`, `\t`; other
  control characters and bytes are written as is.
- The charset of the graph being processed is kept per thread
  (`charset::begin_layout`/`begin_render`, called by `layout_graph_with_options`
  and `render_graph_named`); `NodeSizeEnv::new` and `-n2` labels use it.
- Not reproduced: HTML lexer error messages (`Error: not well-formed ...`,
  `unknown encoding`); input that really contains `U+10FF00..=U+10FFFF` is
  written out as raw bytes.

## Debugging aid

`scripts/c-probe.c` prints C's exact (`%.17g`) node coordinates, sizes and shape
vertices after `-n2`, for chasing last-bit floating-point differences.
