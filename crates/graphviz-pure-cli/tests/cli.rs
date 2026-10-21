use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn run(args: &[&str], stdin: &str) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A run rejected on its arguments may exit before reading stdin.
    if let Err(e) = child.stdin.take().unwrap().write_all(stdin.as_bytes()) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "{e}");
    }
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn undirected_graph_has_no_arrowhead() {
    let (out, _, ok) = run(&["-Tsvg"], "graph{a--b}");
    assert!(ok);
    assert_eq!(out.matches("<polygon").count(), 1, "only the background");
    let (out, _, _) = run(&["-Tsvg"], "digraph{a->b}");
    assert_eq!(out.matches("<polygon").count(), 2);
}

#[test]
fn unknown_flag_is_rejected() {
    let (_, err, ok) = run(&["-Z"], "graph{}");
    assert!(!ok);
    assert!(err.contains("option -Z unrecognized"), "{err}");
}

#[test]
fn global_attrs_output_file_and_separate_args() {
    let path = tmp("cli_out.svg");
    let (_, _, ok) = run(
        &[
            "-T",
            "svg",
            "-o",
            path.to_str().unwrap(),
            "-Nshape=box",
            "-Gbgcolor=yellow",
            "-Ecolor=red",
        ],
        "digraph G{a->b}\ndigraph H{c->d}",
    );
    assert!(ok);
    let svg = std::fs::read_to_string(&path).unwrap();
    assert_eq!(svg.matches("<svg").count(), 2, "every graph is rendered");
    assert!(!svg.contains("<ellipse"));
    assert!(svg.contains("fill=\"yellow\""));
    assert!(svg.contains("stroke=\"red\""));
}

#[test]
fn multiple_input_files_are_each_rendered() {
    let a = tmp("cli_a.gv");
    let b = tmp("cli_b.gv");
    std::fs::write(&a, "digraph{x->y}").unwrap();
    std::fs::write(&b, "graph{p--q}").unwrap();
    let (out, _, ok) = run(&["-Tsvg", a.to_str().unwrap(), b.to_str().unwrap()], "");
    assert!(ok);
    assert_eq!(out.matches("<svg").count(), 2);
}

#[test]
fn version_and_help_exit_successfully() {
    let (_, err, ok) = run(&["-V"], "");
    let version = concat!("graphviz-pure version ", env!("CARGO_PKG_VERSION"), " ");
    assert!(ok && err.contains(version), "{err}");
    let (out, _, ok) = run(&["-?"], "");
    assert!(ok && out.contains("Usage:"));
}

#[test]
fn no_layout_mode_keeps_positions() {
    let laid = "digraph{a [pos=\"127,118\"]; b [pos=\"127,18\"]; a -> b [pos=\"e,127,36 127,100 127,80 127,60 127,46\"]}";
    let (out, _, ok) = run(&["-Kneato", "-n2", "-Tdot"], laid);
    assert!(ok);
    // translated so the bounding box starts at the origin
    assert!(out.contains("pos=\"27,118\""), "{out}");
    assert!(
        out.contains("pos=\"e,27,36 27,100 27,80 27,60 27,46\""),
        "{out}"
    );
    let (_, err, ok) = run(&["-Kneato", "-n0", "-Tdot"], laid);
    assert!(!ok && err.contains("Invalid parameter"));
    let (out, _, ok) = run(&["-Knop2", "-Tplain"], laid);
    assert!(ok && out.starts_with("graph 1 "), "{out}");
}

#[test]
fn png_and_jpeg_output_are_images_at_graphviz_sizes() {
    let png = tmp("raster.png");
    let (_, err, ok) = run(
        &["-Tpng", "-o", png.to_str().unwrap()],
        "digraph { a -> b }",
    );
    assert!(ok, "{err}");
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[1..4], b"PNG");
    // 62 x 116 pt at 96 dpi, as C's -Tpng.
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 83);
    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 155);
    let jpg = tmp("raster.jpg");
    let (_, err, ok) = run(
        &["-Tjpg", "-o", jpg.to_str().unwrap()],
        "digraph { a -> b }",
    );
    assert!(ok, "{err}");
    assert_eq!(&std::fs::read(&jpg).unwrap()[..2], &[0xFF, 0xD8]);
}

// Fuzz case 003-pack-filledge-zero-point-spline-panic (issue #12), its repro.gv verbatim.
// Clipping leaves both edges between the overlapping pinned nodes with an empty spline. C
// crashes on it (exit 138, SIGBUS), so there is no C reference output and the fidelity
// ratchets never render it: this test is the regression check. Without the empty-list guard
// in `pack::fill_edge` the port panics with "index out of bounds: the len is 0 but the index
// is 0" and exits 101.
#[test]
fn packed_edges_with_empty_splines_do_not_panic() {
    let src = "graph G {
layout=neato;
splines=polyline;
pack=true;
n1 [pos=\"0,0!\"];
n9 [pos=\"0.2,0.1!\"];
n1 -- n9;
n9 -- n1;
n14;
}
";
    for fmt in ["-Tsvg", "-Tdot", "-Tpng"] {
        let (_, err, ok) = run(&[fmt], src);
        assert!(ok && !err.contains("panicked"), "{fmt}: {err}");
    }
}

// Fuzz case 004-neato-router-record-obstacle-height (issue #13), its repro.gv verbatim. C's
// `makeObstacle` (lib/neatogen/neatosplines.c:447-449) builds a record's obstacle from the
// root field box `fld->b`, not from `ND_height`, which carries record_init's +1pt. With the
// taller box the port routed `a -- b` as a curve around the record instead of C's straight
// line.
#[test]
fn neato_record_obstacle_uses_field_box() {
    let src = "graph G {
layout=neato;
splines=true;
a [pos=\"0,0!\"];
b [pos=\"3,0!\"];
r [shape=record, label=\"a|b\", pos=\"1.5,0.3!\"];
a -- b;
}
";
    let (out, err, ok) = run(&["-Kneato", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains(r#"d="M54.36,-18C95.79,-18 174.02,-18 215.53,-18""#),
        "{out}"
    );
}

// The circo variant of issue #13 (repro-circo.gv). With C's record box the guiding polygon
// does not triangulate, so `Pshortestpath` fails, genroute warns (multispline.c:703) and
// the edge falls back to a straight line.
#[test]
fn circo_record_obstacle_falls_back_like_c() {
    let src = "graph G {
layout=circo;
splines=spline;
n7 [shape=record];
n1 -- n7:n;
n2;
}
";
    let (out, err, ok) = run(&["-Kcirco", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains(r#"d="M126.28,-22.32C89.29,-27.6 27,-36.5 27,-36.5""#),
        "{out}"
    );
    assert_eq!(
        err,
        "lib/pathplan/shortest.c:193: destination point not in any triangle\n\
         Warning: Could not create control points for multiple spline for edge (n1,n7)\n"
    );
}

// Fuzz case 014-dot-pack-failed-component-not-absorbed (issue #23), its repro.gv verbatim.
// `n9` is both in `cluster3` and in a `rank=source` set, so mincross fails in the second
// pack component (`_cc_1`; the empty `cluster0` is `_cc_0`). C (`doDot`, dotinit.c) returns
// the failure and renders the half-made drawing: the component's nodes are the root's own
// objects, so they keep the sizes `dot_init_node_edge` gave them (`rx="27"`), and the
// messages name the component. The port used to drop the failed component's records
// (`rx="0"`), name the root `G` and omit the warning.
#[test]
fn failed_pack_component_keeps_its_node_sizes() {
    let src = "graph \"G\" {
pack=\"true\";
subgraph \"cluster0\" {
}
subgraph \"cluster3\" {
\"n1\" [];
\"n8\" [];
\"n9\" [];
}
{
rank=source;
\"n7\";
\"n9\";
}
\"n1\":\"c\" -- \"n7\";
\"n8\" -- \"n9\" [];
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(!ok);
    assert_eq!(
        err,
        "Warning: n9 was already in a rankset, deleted from cluster _cc_1\n\
         Error: install_in_rank, line 1158: _cc_1 %0 rank 2 i = 0 an = 0\n"
    );
    let sized = r#"<ellipse fill="none" stroke="black" cx="0" cy="0" rx="27" ry="18"/>"#;
    assert_eq!(out.matches(sized).count(), 4, "{out}");
}

// Fuzz case 017-dot-pack-curved-edge-subpoint (issue #26), its repro.gv verbatim. In a pack
// component, C's `clip_and_install` grows `GD_bb(agraphof(tail))`, the root graph, so the
// component's box stays at its `dot_position` value (y up to 180) while its edges are routed.
// The second curved edge bends towards that box's centre (`get_centroid`, routespl.c). The
// port grew the component's box with the first edge's spline (to 180.32), which moved the
// bend of `n59:sw -- n72`. C 16.1.0 output below.
#[test]
fn packed_curved_edge_bends_towards_component_box_without_splines() {
    let src = "graph \"G\" {
splines=\"curved\";
pack=\"8\";
\"n26\":\"n\" -- \"n59\":\"c\";
\"n59\":\"sw\" -- \"n72\";
\"n61\";
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains("d=\"M26.98,-74.98C25.94,-53.57 25.46,-43.7 25.55,-36.45\""),
        "{out}"
    );
}

// Fuzz case 007-dot-port-endpoint-attr-sym-id (issue #16), its repro.gv verbatim. The flat
// edge `a:sw -> b` is routed by the nested layout of `make_flat_adj_edges`, where C's stale
// `E_arrowsz` reads the aux graph attribute whose id equals the main `arrowsize` id. C's
// `mkport` (lib/cgraph/grammar.y) declares `tailport` when it meets `a:sw`, so that id lands
// on `tailport` ("sw", not a number: arrow size 1). Without the declaration the id lands on
// `fontsize` (27.3), the edge is clipped to almost nothing and the graph is 44pt tall.
#[test]
fn port_endpoint_declares_its_edge_attribute_like_mkport() {
    let src = "digraph G {
edge [constraint=false];
a:sw -> b [fontsize=27.3];
c -> d [arrowsize=2];
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains("<svg width=\"278pt\" height=\"56pt\""),
        "{out}"
    );
    assert!(
        out.contains(
            "<path fill=\"none\" stroke=\"black\" \
             d=\"M12.98,-14.75C-4.84,8.98 37.44,-1.06 68.55,-13.3\"/>"
        ),
        "{out}"
    );
}

// Fuzz case 013-dot-pack-component-missing-attr-dicts (issue #22), its repro.gv verbatim.
// The flat adjacent edge `25:ne -> 26` is routed through make_flat_adj_edges' auxiliary
// graph, whose setState keeps the main graph's N_penwidth/E_penwidth/E_arrowsz symbols and
// reads them by id (lib/dotgen/dotsplines.c). A pack component is a subgraph of the root and
// shares its dictionaries, so `xlabel` on 22 and `penwidth` on 24 (other components) shift
// those ids. Without the root dictionaries in `CComp::to_graph` the edge moved by about 1pt.
#[test]
fn pack_component_keeps_root_attribute_dictionaries() {
    let src = "digraph \"G\" {
pack=\"true\";
node [width=\"2.7\"];
\"22\" [xlabel=\"\u{df}\"];
\"24\" [penwidth=\"4.79\"];
\"25\" [fontsize=\"2\"];
\"25\":\"ne\" -> \"26\" [style=\"bold,dashed\", minlen=\"0\"];
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    // C Graphviz 16.1.0.
    assert!(
        out.contains("d=\"M114.95,-111.75C149.34,-141.07 206.35,-128.53 249.55,-113.61\""),
        "{out}"
    );
    assert!(
        out.contains("points=\"249.13,-117.47 257.37,-110.81 246.77,-110.88 249.13,-117.47\""),
        "{out}"
    );
}

// Fuzz case 005-xlabels-hilbert-key-signed-order (issue #14). The drawings are wider than
// 32768pt, so some objects' Hilbert keys have bit 31 set. C stores the key in an `int`
// (`HDict_t.key`) and orders the keys with `icompare` on `int` (lib/label/xlabels.c:28-35),
// so those keys sort first; sorting them as `u32` builds a different R-tree and moves the
// label by about its width. The inputs are corpus files whose C -Tdot goldens the fdp and
// circo ratchets also check; the coordinates below are C's.
#[test]
fn xlabel_hilbert_keys_are_ordered_as_signed_int() {
    let fdp = include_str!("../../../tests/fdp/fuzz_xlabels_hilbert_signed_key.gv");
    let (out, err, ok) = run(&["-Tdot"], fdp);
    assert!(ok, "{err}");
    assert!(out.contains("10 -> 0\t[lp=\"14095,11133\""), "fdp: {out}");
    let circo = include_str!("../../../tests/circo/fuzz_xlabels_hilbert_signed_key.gv");
    let (out, err, ok) = run(&["-Tdot"], circo);
    assert!(ok, "{err}");
    assert!(
        out.contains("n68 -- n98\t[head_lp=\"34855,2663.7\""),
        "circo: {out}"
    );
}

// Fuzz case 015-dot-pack-unplaced-edge-label-shifted-off-page (issue #24), its repro.gv
// verbatim. With `splines=none` dot places no edge label (`set` stays false), but the label
// keeps its position: `(0,0)`, moved by the pack offset in `shiftEdge` (lib/pack/pack.c).
// `edge_in_box` (lib/common/emit.c) tests `ED_label(e)` without looking at `set`, so C finds
// the packed label off the page and does not emit the edge, while without pack the label sits
// at the origin and C writes an empty edge group. The port used to lose the unset position.
#[test]
fn dot_pack_unplaced_edge_label_off_page_skips_edge() {
    let src = "digraph G {
splines=none;
pack=true;
c;
a -> b [label=\"lbl\"];
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(!out.contains("class=\"edge\""), "{out}");
    // -Tplain writes the edge with the shifted, unset label position (C 16.1.0).
    let (out, err, ok) = run(&["-Kdot", "-Tplain"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains(" lbl -0.069444 -0.88889 solid black\n"),
        "{out}"
    );
    let (out, err, ok) = run(
        &["-Kdot", "-Tsvg"],
        "digraph G { splines=none; c; a -> b [label=\"lbl\"]; }",
    );
    assert!(ok, "{err}");
    assert_eq!(out.matches("class=\"edge\"").count(), 1, "{out}");
}

// Fuzz case 009-xlabels-dtobag-equal-key-order (issue #18), its repro.gv verbatim. Five
// nodes share the pinned position, so several label objects get the same Hilbert key.
// `placeLabels` (lib/label/xlabels.c:xlhdxload/xlspdxload) loads the R-tree in the order of a
// cdt `Dtobag`, whose splay tree puts a duplicate key just before the equal element its
// search stops at (lib/cdt/dttree.c), so equal keys are not simply in reverse insertion
// order. With the old sort the edge label moved one candidate step down (y="-52.88").
#[test]
fn xlabels_equal_hilbert_keys_follow_dtobag_order() {
    let src = r#"graph "G" {
layout=neato;
node [pos="31,-83!"];
"n0" [];
"n1" [label=<<TABLE BORDER="0" COLOR="black"><TR><TD BGCOLOR="lightblue">x</TD><TD WIDTH="28" HEIGHT="30">&lt;x&gt;</TD><TD><FONT POINT-SIZE="8">small</FONT></TD></TR></TABLE>>];
"n3" [shape="star"];
"n7" [height="1.68"];
"n12" [pos="-241,74"];
"n1" -- "n5" [label="\E", headport="w"];
}
"#;
    let (out, err, ok) = run(&["-Kneato", "-Tsvg"], src);
    assert!(ok, "{err}");
    // C Graphviz 16.1.0's line for the edge label.
    let c = r#"<text xml:space="preserve" text-anchor="middle" x="18.66" y="-69.68" font-family="Times,serif" font-size="14.00">n1&#45;&#45;n5</text>"#;
    assert!(out.contains(c), "{out}");
}

// Fuzz case 012-dot-flat-adj-labelfloat (issue #21), its repro.gv verbatim. A flat edge
// between adjacent nodes with a port is routed by a nested dot layout
// (`make_flat_adj_edges`). There C's `setState` looks `E_label_float` up as "label_float"
// (lib/dotgen/dotsplines.c:745), so the nested layout ignores `labelfloat=true`. The
// expected values are C 16.1.0's -Tsvg output; passing `labelfloat` through gives a
// 49.8pt-high graph with the label at y=-33.2.
#[test]
fn flat_adjacent_edge_ignores_labelfloat_like_c() {
    let src = "digraph G {
{rank=same; \"2\" -> \"10\" [label=\"x\", tailport=\"e\", labelfloat=\"true\"];}
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    for want in [
        "translate(4 50.35)",
        "d=\"M54,-18C62.09,-18 60.83,-26.42 68.5,-29 71.45,-29.99 72.42,-29.41 75.5,-29 \
         77.39,-28.75 79.31,-28.43 81.24,-28.07\"",
        "points=\"81.77,-31.54 90.75,-25.92 80.23,-24.71 81.77,-31.54\"",
        "x=\"72\" y=\"-33.75\"",
    ] {
        assert!(out.contains(want), "missing {want}:\n{out}");
    }
}

// Issue #21, the other side of C's `label_float` lookup: an aux edge floats its label when
// the main graph sets `label_float`. C 16.1.0 then draws what the port drew for the
// repro above before the fix (label at y=-33.2).
#[test]
fn flat_adjacent_edge_floats_label_from_label_float_like_c() {
    let src = "digraph G {
edge [label_float=\"true\"];
{rank=same; \"2\" -> \"10\" [label=\"x\", tailport=\"e\"];}
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    for want in [
        "translate(4 49.8)",
        "d=\"M54,-18C63.19,-18 62.67,-26.46 71.5,-29 75.17,-30.06 79.03,-30.33 82.88,-30.08\"",
        "x=\"72\" y=\"-33.2\"",
    ] {
        assert!(out.contains(want), "missing {want}:\n{out}");
    }
}

// Issue #21: `label_float` declared after the edge. cgraph gives the existing edge ""
// (not the later default), `cloneEdge` copies that, and the label does not float.
#[test]
fn label_float_declared_after_the_edge_does_not_float_like_c() {
    let src = "digraph G {
{rank=same; \"2\" -> \"10\" [label=\"x\", tailport=\"e\"];}
edge [label_float=\"true\"];
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(out.contains("translate(4 50.35)"), "{out}");
    assert!(out.contains("x=\"72\" y=\"-33.75\""), "{out}");
}

// Fuzz case 018-render-interpolate-points-fma (issue #27), its repro.gv verbatim. C's
// `interpolate_pointf` (`lib/common/geomprocs.h:115`, called by `alloc_interpolation_points`
// in `lib/common/shapes.c`) computes `p + t * (q - p)`, which Apple clang fuses (`fmla.2d`).
// Unfused, the rarrow's shaft corners round to 10.48 instead of C's 10.47.
#[test]
fn round_corners_interpolation_is_fused_like_clang() {
    let src = "digraph G{layout=neato; notranslate=true; \"n45\" [shape=\"rarrow\", height=\"1.73\", fontsize=\"27.3\", pos=\"0,0!\"];}\n";
    let (out, err, ok) = run(&["-Kneato", "-Tsvg"], src);
    assert!(ok, "{err}");
    let c_points = "points=\"10.47,-56.28 -28.48,-56.28 -28.48,56.28 10.47,56.28 \
                    10.47,62.28 28.48,0 10.47,-62.28 10.47,-56.28\"";
    assert!(out.contains(c_points), "{out}");
}

/// Issue #29 (was open case `tests/fuzz-open/020-parser-keyword-as-attribute-value`).
/// scan.l lexes `node`, `edge`, `graph`, `digraph`, `subgraph` and `strict`
/// (case-insensitively) as keyword tokens, and grammar.y's `atom : T_atom | qatom`
/// takes none of them, so C 16.1.0 reports a syntax error, writes no output and
/// exits 1 when one appears unquoted as an attribute name, an attribute value or a
/// port. The first input is the case's repro.gv verbatim; C says
/// `syntax error in line 2 near 'node'`. The message wording is issue #31.
#[test]
fn unquoted_keyword_atoms_are_syntax_errors() {
    for src in [
        "digraph G {\npackmode=node;\na [label=edge];\na:graph -> b;\n}\n",
        "digraph G { a [label=Edge] }\n",
        "digraph G { a [node=1] }\n",
        "digraph G { a:graph -> b }\n",
        "digraph G { a -> b:c:SUBGRAPH }\n",
    ] {
        let (out, err, ok) = run(&["-Tsvg"], src);
        assert!(!ok, "{src:?} should fail");
        assert!(out.is_empty(), "{src:?} wrote output");
        assert!(err.contains("syntax error"), "{src:?}: {err}");
    }
}

/// Issue #29: the quoted keywords are `T_qatom` in C and keep working. The
/// `-Tdot` writer quotes them again (`agcanonstr`), so the output reads back.
#[test]
fn quoted_keyword_atoms_still_parse() {
    let src = "digraph G {\npackmode=\"node\";\na [label=\"edge\", \"node\"=\"1\"];\na:\"graph\" -> b;\n}\n";
    let (out, err, ok) = run(&["-Tdot"], src);
    assert!(ok, "{err}");
    assert!(out.contains("packmode=\"node\""), "{out}");
    assert!(out.contains("label=\"edge\""), "{out}");
    assert!(out.contains("\"node\"=1"), "{out}");
    assert!(out.contains("a:\"graph\" -> b"), "{out}");
}

// Issue #31, fuzz case 022-stderr-syntax-error-message: a DOT syntax error is reported in
// the words of cgraph's `aagerror` (lib/cgraph/scan.l), on the line where bison read the
// rejected lookahead token, and dot exits 1. Expected stderr is C Graphviz 16.1.0's.
#[test]
fn syntax_errors_are_reported_as_c_does() {
    // repro.gv: the line is the end of input, where the unterminated string ends
    let path = tmp("cli_syntax_repro.gv");
    std::fs::write(&path, "digraph G {\n a -> b\n c [label=\"abc\n\n]\n d\n").unwrap();
    let (out, err, ok) = run(&["-Tsvg", path.to_str().unwrap()], "");
    assert!(!ok && out.is_empty());
    assert_eq!(
        err,
        format!(
            "Error: {}: syntax error in line 7 scanning a quoted string (missing endquote? \
             longer than 16384?)\nString starting:\"abc\n\n]\n d\n\n",
            path.display()
        )
    );
    // repro2.gv, read from stdin
    let (_, err, ok) = run(&["-Tsvg"], "digraph G {\n a -> b\n c -> ;\n}\n");
    assert!(!ok);
    assert_eq!(err, "Error: <stdin>: syntax error in line 3 near ';'\n");
    let (_, err, ok) = run(&["-Tsvg"], "digraph G {\n a -> b\n");
    assert!(!ok);
    assert_eq!(err, "Error: <stdin>: syntax error in line 3\n");
    // a `# N "file"` line directive renames the input
    let (_, err, ok) = run(&["-Tsvg"], "digraph G {\n# 100 \"other.gv\"\n a -> ;\n}\n");
    assert!(!ok);
    assert_eq!(err, "Error: other.gv: syntax error in line 100 near ';'\n");
    // the graphs before the error are rendered; line numbers restart with each graph
    let (out, err, ok) = run(&["-Tsvg"], "digraph G { a }\ndigraph H { b -> ; }\n");
    assert!(!ok);
    assert_eq!(out.matches("<svg").count(), 1);
    assert_eq!(err, "Error: <stdin>: syntax error in line 1 near ';'\n");
}

// `%.80s` of the string read so far may end inside a UTF-8 sequence: C writes the bytes.
#[test]
fn unterminated_string_excerpt_is_cut_at_80_bytes() {
    let src = format!("digraph G {{\n a [label=\"x{}\n", "\u{e9}".repeat(43));
    let mut child = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
        .arg("-Tsvg")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(src.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let mut expected = b"Error: <stdin>: syntax error in line 3 scanning a quoted string \
        (missing endquote? longer than 16384?)\nString starting:\"x"
        .to_vec();
    expected.extend("\u{e9}".repeat(39).as_bytes());
    expected.extend(b"\xc3\n");
    assert_eq!(out.stderr, expected);
}

// cgraph's `default_usererrf` (lib/cgraph/agerror.c) writes control characters other than
// white space as `\ooo`: in the near text, the string excerpt and a directive's file name.
#[test]
fn syntax_error_escapes_control_characters() {
    let (_, err, ok) = run(&["-Tsvg"], "digraph { a \u{1} }");
    assert!(!ok);
    assert_eq!(err, "Error: <stdin>: syntax error in line 1 near '\\001'\n");
    let (_, err, _) = run(&["-Tsvg"], "digraph { a \u{7f} }");
    assert_eq!(err, "Error: <stdin>: syntax error in line 1 near '\\177'\n");
    let (_, err, _) = run(&["-Tsvg"], "digraph {\n a [label=\"a\u{1b}b\tc\r\n");
    assert_eq!(
        err,
        "Error: <stdin>: syntax error in line 3 scanning a quoted string (missing endquote? \
         longer than 16384?)\nString starting:\"a\\033b\tc\r\n\n"
    );
    let (_, err, _) = run(&["-Tsvg"], "digraph {\n# 3 \"a\u{1}b\"\n a -> ;\n}\n");
    assert_eq!(err, "Error: a\\001b: syntax error in line 3 near ';'\n");
}

/// C: `Warning: syntax ambiguity - badly delimited number '1a' in line 2 of <stdin> splits
/// into two tokens` before the error (scan.l `chkNum`). The port splits the token the same way
/// but does not warn.
#[test]
#[ignore = "issue #97: scan.l chkNum's badly-delimited-number warning is not ported"]
fn badly_delimited_number_warns() {
    let (_, err, _) = run(&["-Tsvg"], "digraph G {\n a -> b -> 1a ; c -> ;\n}\n");
    assert_eq!(
        err,
        "Warning: syntax ambiguity - badly delimited number '1a' in line 2 of <stdin> splits \
         into two tokens\nError: <stdin>: syntax error in line 2 near ';'\n"
    );
}

/// C: a backslash before CR LF in a quoted string is kept with the CR and LF (scan.l:140
/// ignores only a backslash before LF; rule 142 keeps the backslash). The port drops all three.
#[test]
#[ignore = "issue #112: backslash + CRLF in a quoted string is dropped"]
fn backslash_crlf_in_quoted_string_is_kept() {
    let (out, err, ok) = run(&["-Tdot"], "digraph G { a [label=\"x\\\r\ny\"] }");
    assert!(ok, "{err}");
    assert!(out.contains("label=\"x\\\r\ny\""), "{out}");
}

/// Fuzz case 019-neato-polyline-compass-port-endpoint (issue #28), minimised to fixed
/// positions. The head port `10:n` lies on the node boundary, so `spline_edges_` sends the
/// edge to the GTS router (`makeMultiSpline`, `lib/neatogen/multispline.c`). The two
/// stacked octagon obstacles are mirror images, so the shortest triangle path from the
/// head can pass either side of the `9`-bottom/`10`-top diagonal at an equal length.
///
/// Expected (C 16.1.0): in `triPath` both head edges have exactly the same length
/// (20.005238055389295, from triangle centres summed in GTS's vertex order by
/// `triCenter`, `multispline.c:391`), the first inserted (left) one is popped first and
/// the guiding polygon is the left sleeve. Its bends are not corners next to the end
/// points, so `tweakEnd` (`multispline.c:658`) leaves the head on the node centre:
/// `e,29.16,36.482 29.126,71.526 ...`.
///
/// Actual (port): the port's constrained Delaunay triangulation has the same triangles but
/// a different vertex rotation per face (GTS's `gts_triangle_vertices` order follows its
/// incremental construction behind `tri`/`mkSurface`, `lib/neatogen/delaunay.c:140,454`),
/// so the centres differ by an ulp, the right head edge is 1 ulp shorter and the path
/// takes the right sleeve. Its last bend is the obstacle's top corner, so `tweakEnd` moves
/// the head 0.1pt right: `e,29.26,36.483 29.026,71.859 ...`.
#[test]
#[ignore = "issue #28: triangle-path tie decided by GTS's face vertex order, which the port's CDT does not reproduce"]
fn neato_polyline_compass_port_routes_like_c() {
    let src = "digraph G { splines=\"polyline\"; \"10\" [width=0.81, pos=\"100,18\"]; \"9\" [pos=\"100,90\"]; \"9\" -> \"10\":\"n\"; }";
    let (out, err, ok) = run(&["-Kneato", "-n", "-Tdot"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains(
            "pos=\"e,29.16,36.482 29.126,71.526 29.134,63.677 29.142,54.763 29.149,47.889\""
        ),
        "{out}"
    );
}

// Fuzz case 010-dot-pack-single-component-ignores-cc-subgraph (issue #19). With pack set and a
// single connected component, C's doDot (lib/dotgen/dotinit.c:466-475) lays out the root with
// `dotLayout(g)` while the `_cc_0` subgraph made by `cccomps` is still a child of the root.
// `_cc_0` inherits the root's `rank=max` and holds every node, so it is a rank set and a and b
// share a rank (C: 134x44pt, side by side). The port used to lay out the plain root, where the
// root's own `rank` is ignored (62x116pt, b below a).
#[test]
fn pack_single_component_lays_out_with_cc_subgraph() {
    let (out, err, ok) = run(
        &["-Kdot", "-Tsvg"],
        "digraph G {\npack=true;\nrank=max;\na -> b;\n}\n",
    );
    assert!(ok, "{err}");
    assert!(
        out.contains("<svg width=\"134pt\" height=\"44pt\""),
        "{out}"
    );
    assert!(out.contains("cx=\"27\" cy=\"-18\""), "{out}");
    assert!(out.contains("cx=\"99\" cy=\"-18\""), "{out}");
}

// Fuzz case 001-dot-cluster-rank-max-build-skeleton-panic (issue #10) as generated, with
// `pack="8"`. Through the `_cc_0` subgraph of issue #19 C ranks the clusters so that
// build_skeleton finds every rank leader; it exits 0 with the mark_clusters warning
// (lib/dotgen/cluster.c:318) and this layout. The pack-less input crashes C.
#[test]
fn pack_single_component_cluster_node_in_rank_max_set_lays_out_like_c() {
    let src = "strict digraph \"G\" {
pack=\"8\";
\"n148\";
subgraph \"cluster_1\" {
\"n165\";
\"n181\" [];
\"n235\";
subgraph \"cluster4\" {
\"n102\" [];
\"n225\" [];
}
}
{
rank=max;
\"n235\";
\"n148\";
}
{\"n15\" \"n181\" \"n205\"} -> \"n102\";
\"n148\" -> \"n164\";
\"n164\" -> \"n165\";
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert_eq!(
        err,
        "Warning: n235 was already in a rankset, deleted from cluster G\n"
    );
    assert!(
        out.contains("<svg width=\"418pt\" height=\"212pt\""),
        "{out}"
    );
    assert!(
        out.contains("points=\"79.35,-16 79.35,-68 234.35,-68 234.35,-16 79.35,-16\""),
        "cluster4: {out}"
    );
    assert!(out.contains("cx=\"379.35\" cy=\"-114\""), "n235: {out}");
}

// Issue #19 under newrank. `dot2_rank`'s compile_samerank also walks `_cc_0`'s clone of
// cluster_a; `set_parent` → `node_induce` empties the clone (cluster_a already holds a and
// b), so dot_mincross prints "removing empty cluster" (lib/dotgen/mincross.c:346).
#[test]
fn pack_single_component_newrank_removes_empty_cluster_clone() {
    let (out, err, ok) = run(
        &["-Kdot", "-Tsvg"],
        "digraph G { pack=true; newrank=true; subgraph cluster_a { a -> b; } b -> c; }\n",
    );
    assert!(ok, "{err}");
    assert_eq!(err, "Warning: removing empty cluster\n");
    assert!(
        out.contains("<svg width=\"94pt\" height=\"204pt\""),
        "{out}"
    );
}

// As above with a root rank=max, which the emptied clone inherits: its union_all finds no
// node, and C's `union_one(NULL, GD_maxrep(clust))` returns NULL (lib/dotgen/rank.c:615-621,
// 684). The port unwrapped the missing leader and panicked. C: one rank, 222x76pt.
#[test]
fn pack_single_component_newrank_rank_max_empty_clone_does_not_panic() {
    let (out, err, ok) = run(
        &["-Kdot", "-Tsvg"],
        "digraph G { pack=true; newrank=true; rank=max; subgraph cluster_a { a -> b; } b -> c; }\n",
    );
    assert!(ok, "{err}");
    assert_eq!(err, "Warning: removing empty cluster\n");
    assert!(
        out.contains("<svg width=\"222pt\" height=\"76pt\""),
        "{out}"
    );
    for cx in ["43", "115", "187"] {
        assert!(out.contains(&format!("cx=\"{cx}\" cy=\"-34\"")), "{out}");
    }
}

// The same empty-cluster rank set without pack: cluster_b's nodes are already in
// cluster_a, so node_induce empties it before its rank=max is compiled (newrank).
#[test]
fn newrank_rank_max_cluster_emptied_by_node_induce_does_not_panic() {
    let (out, err, ok) = run(
        &["-Kdot", "-Tsvg"],
        "digraph G { newrank=true; subgraph cluster_a { a -> b } \
         subgraph cluster_b { rank=max; a; b } b -> c; }\n",
    );
    assert!(ok, "{err}");
    assert_eq!(err, "Warning: removing empty cluster\n");
    assert!(
        out.contains("<svg width=\"94pt\" height=\"204pt\""),
        "{out}"
    );
}

// With a non-default ratio C's doDot lays out the root with dotLayout(g) even with several
// components (lib/dotgen/dotinit.c:466-494), and the `_cc_<i>` subgraphs exist meanwhile.
// Each inherits rank=max, so a, b, c and d share one rank (C: 216x34pt). Without pack they
// would take two ranks.
#[test]
fn pack_ratio_several_components_lays_out_with_cc_subgraphs() {
    let (out, err, ok) = run(
        &["-Kdot", "-Tsvg"],
        "digraph G { pack=true; ratio=fill; size=\"3,3\"; rank=max; a -> b; c -> d; }\n",
    );
    assert!(ok, "{err}");
    assert!(
        out.contains("<svg width=\"216pt\" height=\"34pt\""),
        "{out}"
    );
    for cx in ["27", "99", "171", "243"] {
        assert!(out.contains(&format!("cx=\"{cx}\" cy=\"-18\"")), "{out}");
    }
}

/// The `<title>`s of the cluster groups of an SVG, in drawing order.
fn svg_cluster_titles(svg: &str) -> Vec<&str> {
    svg.split("class=\"cluster\">\n<title>")
        .skip(1)
        .map(|s| &s[..s.find("</title>").unwrap()])
        .collect()
}

// Issue #25 (fuzz case 016-dot-pack-cluster-list-from-subgraph-tree). With pack/packmode,
// dot's `copyClusterInfo` (lib/dotgen/dotinit.c) rebuilds `GD_clust(root)` in component
// order, appending a cluster once per component that kept it, and `emit_clusters`
// (lib/common/emit.c) draws that list as it is. A node of two non-nested clusters puts
// both clusters in two components, each of which keeps `cluster_0`, so C draws it twice;
// components whose clusters are not in declaration order are drawn in component order.
// The renderer used to rebuild the list from the subgraph tree (once each, declaration
// order).
#[test]
fn packed_dot_draws_the_root_cluster_list_of_copy_cluster_info() {
    let (out, _, ok) = run(
        &["-Kdot", "-Tsvg"],
        "digraph G {\npack=8;\nsubgraph cluster_0 { a }\nsubgraph cluster_1 { a }\n}\n",
    );
    assert!(ok);
    assert_eq!(svg_cluster_titles(&out), ["cluster_0", "cluster_0"]);
    assert_eq!(
        out.matches("<g id=\"clust1\" class=\"cluster\">").count(),
        2
    );
    let (out, _, ok) = run(
        &["-Kdot", "-Tsvg"],
        "graph G {\npackmode=\"node\";\nsubgraph cluster_0 { a }\nsubgraph cluster_3 { b }\n\
         subgraph cluster_5 { c }\nx -- a;\nx -- c;\n}\n",
    );
    assert!(ok);
    assert_eq!(
        svg_cluster_titles(&out),
        ["cluster_0", "cluster_5", "cluster_3"]
    );
}

// The reserved cluster-list key written in the input is an ordinary attribute to C: it
// must not reorder another engine's clusters, nor (inherited by the subgraphs, so a
// cluster lists itself) make the cluster tree cyclic, which overflowed the stack.
#[test]
fn input_cluster_list_key_is_ignored() {
    let src =
        "graph G { \"\u{1}clustlist\"=\"1 0\"; subgraph cluster_a {a} subgraph cluster_b {b} }\n";
    for engine in ["-Kneato", "-Kfdp", "-Kosage", "-Kpatchwork", "-Kdot"] {
        let (out, err, ok) = run(&[engine, "-Tsvg"], src);
        assert!(ok, "{engine}: {err}");
        assert_eq!(
            svg_cluster_titles(&out),
            ["cluster_a", "cluster_b"],
            "{engine}"
        );
    }
}
