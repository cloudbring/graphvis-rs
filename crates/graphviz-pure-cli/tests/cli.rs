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
    assert!(ok && err.contains("version"));
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
