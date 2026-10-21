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

/// Issue #17 (fuzz case `008-neato-router-star-cylinder-no-obstacle`): star and
/// cylinder nodes are `SH_POLY` in C (`star_fns` and `cylinder_fns` use
/// `poly_init`), so `makeObstacle` gives them a polygon obstacle and the
/// neato-family spline router routes edges around them. The expected paths are
/// C Graphviz 16.1.0's output.
#[test]
fn neato_splines_route_around_star_and_cylinder_nodes() {
    let src = "graph G {
layout=neato;
splines=true;
a [pos=\"0,0!\"];
b [pos=\"3,0!\"];
s [shape=star, pos=\"1.5,0!\"];
c [pos=\"0,1.5!\"];
d [pos=\"3,1.5!\"];
y [shape=cylinder, pos=\"1.5,1.5!\"];
a -- b;
c -- d;
}
";
    let (out, err, ok) = run(&["-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains("<svg width=\"278pt\" height=\"188pt\""),
        "{out}"
    );
    assert!(
        out.contains(
            "d=\"M42.81,-67.3C62.71,-84.29 98.7,-109.87 135,-109.87 171.3,-109.87 207.29,-84.29 227.19,-67.3\""
        ),
        "a -- b must go around the star:\n{out}"
    );
    assert!(
        out.contains("d=\"M52.33,-167.2C67.41,-170.83 87,-174.94 104.68,-176.77 "),
        "c -- d must go around the cylinder:\n{out}"
    );
}

/// Issue #17, circo variant: with the star's obstacle, C's guiding polygon
/// makes `Pshortestpath` fail in `genroute` (`lib/neatogen/multispline.c`),
/// which warns and falls back to a straight edge.
#[test]
fn circo_star_obstacle_falls_back_to_straight_edge_with_warning() {
    let src = "graph G {
layout=circo;
splines=true;
n16 [shape=star];
n2:w -- n13;
}
";
    let (out, err, ok) = run(&["-Tsvg"], src);
    assert!(ok, "{err}");
    assert_eq!(
        err,
        "lib/pathplan/shortest.c:193: destination point not in any triangle\n\
         Warning: Could not create control points for multiple spline for edge (n2,n13)\n"
    );
    assert!(
        out.contains("<svg width=\"344pt\" height=\"150pt\""),
        "{out}"
    );
    assert!(
        out.contains("d=\"M155.98,-18C155.98,-18 237.75,-18 281.83,-18\""),
        "{out}"
    );
}

/// Issue #17, ortho: with `isOrtho`, C's `makeObstacle` gives every `SH_POLY`
/// node, star and cylinder included, its outline box without the margin
/// (`lib/neatogen/neatosplines.c:345-376`). Grown star and cylinder outlines
/// made the touch test fall back to straight lines (osage), or let a star whose
/// box overlaps a point reach the ortho router, which panicked (neato).
#[test]
fn ortho_star_and_cylinder_obstacles_are_outline_boxes() {
    let src = "graph G {
layout=osage;
splines=ortho;
a [shape=cylinder];
s [shape=star];
a -- b -- c -- a;
s -- b;
}
";
    let (out, err, ok) = run(&["-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains(
            "d=\"M168.47,-111C177.84,-111 186,-111 186,-111 186,-111 186,0 186,0 186,0 55,0 55,0 55,0 55,-8.72 55,-17.58\""
        ),
        "c -- a must be routed orthogonally:\n{out}"
    );

    let src = "graph G {
layout=neato;
splines=ortho;
s [shape=star, pos=\"0,0!\"];
p [shape=point, width=0.05, pos=\"0.4,0.42!\"];
q [pos=\"2,0!\"];
r [pos=\"0,2!\"];
s -- q;
p -- r;
q -- r;
}
";
    let (out, err, ok) = run(&["-Tsvg"], src);
    assert!(ok && !err.contains("panicked"), "{err}");
    assert!(
        out.contains("d=\"M94.2,-52.53C118.85,-52.53 150.11,-52.53 172.09,-52.53\""),
        "touching boxes fall back to straight edges:\n{out}"
    );
}

/// Issue #17: a cylinder's obstacle repeats vertices, so some of its sides are
/// in no triangle. C's `addEndpoint` (`lib/neatogen/multispline.c:889`, `:897`)
/// then aborts on `findMap`'s `assert(ip)`; the port looped in `mk_poly`. It
/// now treats this as a failed multispline and falls back like other failures.
#[test]
fn cylinder_port_endpoint_does_not_hang_the_router() {
    let src = "graph G { layout=neato; splines=true; a [shape=cylinder]; a:n -- b; c -- a:w; }";
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
    // The SVG is a few KB, well under the pipe buffer, so polling cannot block.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            panic!("the router must terminate");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    let (svg, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{err}");
    assert_eq!(svg.matches("class=\"edge\"").count(), 2, "{svg}");
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

// Fuzz case 002-dot-ortho-concentrate-ignored-edge (issue #11), its repro.gv verbatim.
// Under concentrate, class2 marks `13 -> 14` IGNORED, yet orthoEdges (lib/ortho/ortho.c)
// routes it as the first edge of the {13, 14} pair. C draws it (emit_edge_graphics has no type
// check) but attach_attrs_and_arrows (lib/common/output.c) gives it no `pos`. The port used to
// drop its spline before rendering, so -Tsvg lost the edge. The expected bytes are C's.
#[test]
fn ortho_concentrate_draws_routed_ignored_edge() {
    let src = "digraph G {
splines=\"ortho\";
concentrate=\"true\";
\"8\" -> \"9\";
\"8\" -> \"13\";
\"9\" -> \"14\";
\"14\" -> \"13\";
\"13\" -> \"14\";
}
";
    let (svg, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    let edge = "<!-- 13&#45;&gt;14 -->
<g id=\"edge5\" class=\"edge\">
<title>13&#45;&gt;14</title>
<path fill=\"none\" stroke=\"black\" d=\"M40.5,-33.98C40.5,-33.98 40.5,-62.55 40.5,-62.55\"/>
<polygon fill=\"black\" stroke=\"black\" points=\"37,-62.55 40.5,-72.55 44,-62.55 37,-62.55\"/>
</g>
";
    assert!(svg.contains(edge), "{svg}");
    assert!(!svg.contains("<title>14&#45;&gt;13</title>"), "{svg}");
    let (dot, err, ok) = run(&["-Kdot", "-Tdot"], src);
    assert!(ok, "{err}");
    assert!(
        dot.contains("\t13 -> 14;\n") && dot.contains("\t14 -> 13;\n"),
        "{dot}"
    );
    // The layout places the labels of the IGNORED edge to draw them, but C writes back no
    // position for it: the input head_lp and xlp stay, and no pos appears.
    let (dot, err, ok) = run(
        &["-Kdot", "-Tdot"],
        &src.replace(
            "\"13\" -> \"14\";",
            "\"13\" -> \"14\" [headlabel=H xlabel=X head_lp=\"1,2\" xlp=\"3,4\"];",
        ),
    );
    assert!(ok, "{err}");
    let edge = "\t13 -> 14\t[head_lp=\"1,2\",
\t\theadlabel=H,
\t\txlabel=X,
\t\txlp=\"3,4\"];
";
    assert!(dot.contains(edge), "{dot}");
    // Label positions with no label to place stay as well.
    let (dot, err, ok) = run(
        &["-Kdot", "-Tdot"],
        &src.replace(
            "\"13\" -> \"14\";",
            "\"13\" -> \"14\" [head_lp=\"1,2\" tail_lp=\"5,6\" xlp=\"7,8\"];",
        ),
    );
    assert!(ok, "{err}");
    let edge = "\t13 -> 14\t[head_lp=\"1,2\",
\t\ttail_lp=\"5,6\",
\t\txlp=\"7,8\"];
";
    assert!(dot.contains(edge), "{dot}");
}

// Fuzz case 011-dot-render-node-lw-rw-ht (issue #20), its repro.gv verbatim. With
// `splines=""` dot_splines_ returns before resetRW, so the self-loop space dot_position added
// to ND_rw stays (lw=27, rw=45). Mcircle_hack draws the diagonals from ND_rw alone; C's
// chords run from x+29.76 to x-29.76, and a renderer that halves `width` draws them from
// x+23.81 to x-23.81.
#[test]
fn diagonals_use_the_self_loop_widened_right_width() {
    let src = "graph \"G\" {
splines=\"\";
\"n7\" [style=\"diagonals\"];
\"n7\" -- \"n7\":\"ne\" [];
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(out.contains("points=\"56.76,-31.5 -2.76,-31.5\""), "{out}");
    assert!(out.contains("points=\"56.76,-4.5 -2.76,-4.5\""), "{out}");
}

// Issue #20, minimised from fuzz seed 21064. mark_clusters deletes `0` from the cluster
// with a warning, and install_in_rank then abandons the layout, so no postprocessing resets
// the node sizes: ND_lw+ND_rw and ND_ht keep the rotated rankdir=RL values (36 and 54) and
// C draws the ellipses rx=18 ry=27, and prints width=0.5 height=0.75.
#[test]
fn abandoned_flipped_layout_keeps_rotated_node_sizes() {
    let src = "digraph G {
rankdir=RL;
2 [label=\"\"];
subgraph cluster_0 {
0;
1 [shape=diamond];
2 -> 2;
}
{rank=source; 0;}
1 -> 2 [dir=back];
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(!ok);
    assert!(
        err.contains("Warning: 0 was already in a rankset, deleted from cluster G\n"),
        "{err}"
    );
    assert!(err.contains("Error: install_in_rank"), "{err}");
    assert_eq!(out.matches("rx=\"18\" ry=\"27\"").count(), 2, "{out}");
    let (out, _, _) = run(&["-Kdot", "-Tdot"], src);
    assert_eq!(out.matches("height=0.75").count(), 3, "{out}");
    assert_eq!(out.matches("width=0.5").count(), 3, "{out}");
}

// Fuzz case 001-dot-cluster-rank-max-build-skeleton-panic (issue #10), without its `pack="8"`
// line. Node n235 of cluster_1 is also in a top-level rank=max set; ranking leaves n102 of
// cluster4 on rank 2 while cluster4 spans rank 1 only. C's build_skeleton
// (lib/dotgen/cluster.c:365-366) reads GD_rankleader(subg)[ND_rank(v)], gets NULL and crashes
// (SIGSEGV, exit 139, no output) after printing mark_clusters' "already in a rankset"
// warning, so only that line can be compared. The port used to panic on the same lookup
// ("called `Option::unwrap()` on a `None` value", exit 101); it now fails the layout with an
// error. The case as generated, with `pack="8"`, is
// `pack_single_component_cluster_node_in_rank_max_set_lays_out_like_c` (issue #19).
const ISSUE10_GRAPH: &str = "strict digraph \"G\" {
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

#[test]
fn cluster_node_in_rank_max_set_does_not_panic_build_skeleton() {
    for fmt in ["-Tsvg", "-Tdot"] {
        let (_, err, ok) = run(&["-Kdot", fmt], ISSUE10_GRAPH);
        assert!(
            !ok,
            "{fmt}: C crashes here, the port reports a layout error"
        );
        assert_eq!(
            err,
            "Warning: n235 was already in a rankset, deleted from cluster G\n\
             Error: <stdin>: Layout computation error in dot: build_skeleton: no rank leader \
             for rank 2 of node n102 in cluster cluster4\n",
            "{fmt}"
        );
    }
}

// Issue #30 (tests/fuzz-open/021-stderr-missing-warnings): C warnings the port left out.
// Every expected string below is C Graphviz 16.1.0's stderr for the same input.

/// stderr of `-Tsvg -K<engine>` for `src`, with the run's success.
fn warnings(engine: &str, src: &str) -> (String, bool) {
    let k = format!("-K{engine}");
    let (_, err, ok) = run(&["-Tsvg", &k], src);
    (err, ok)
}

/// The issue's reproducer, verbatim: an illegal HTML attribute, an unknown port and an edge
/// label with no spline to place it on.
#[test]
fn issue30_reproducer_warnings() {
    let src = "graph G {
 splines=none
 a [xlabel=q, label=<<TABLE FOO=\"1\"><TR><TD>x</TD></TR></TABLE>>]
 a:nosuch -- b [label=x]
}
";
    let (err, ok) = warnings("dot", src);
    assert!(ok);
    assert_eq!(
        err,
        "Warning: Illegal attribute FOO in <TABLE> - ignored\n\
         in label of node a\n\
         Warning: node a, port nosuch unrecognized\n\
         Warning: no position for edge with label x\n"
    );
}

/// `unrecognized` and the compass warnings of `poly_port` and `record_port`
/// (`lib/common/shapes.c:362-365`, `2894-2896`, `2909-2910`, `3748-3752`), once per
/// `chkPort` call of `common_init_edge`, in `agfstnode`/`agfstout` order.
#[test]
fn issue30_unknown_port_warnings() {
    let src = "digraph G {
 a -> b:nosuch
 r [shape=record, label=\"<f0> x|<f1> y\"]
 r:f0:zz -> r:bad
 r:f1:s -> a:ne
 h [label=<<TABLE><TR><TD PORT=\"p\">x</TD></TR></TABLE>>]
 h:p:qq -> h:missing
 c [shape=box]
 c:wat -> a
 a -> c:wat
}
";
    let want = "Warning: node b, port nosuch unrecognized\n\
                Warning: node c, port wat unrecognized\n\
                Warning: node r, port f0, unrecognized compass point 'zz' - ignored\n\
                Warning: node r, port bad unrecognized\n\
                Warning: node h, port p, unrecognized compass point 'qq' - ignored\n\
                Warning: node h, port missing unrecognized\n\
                Warning: node c, port wat unrecognized\n";
    for engine in ["dot", "neato", "fdp", "circo"] {
        assert_eq!(warnings(engine, src).0, want, "{engine}");
    }
    // patchwork_init_edge never calls common_init_edge, so C resolves no ports.
    assert_eq!(warnings("patchwork", src).0, "");
}

/// `doAttrs` and its attribute actions (`lib/common/htmllex.c:80-552`; the `doInt`
/// messages have no newline), `size_html_cell`'s fixed-size check
/// (`lib/common/htmltable.c:1141-1153`), then `make_label`'s `in label of ...`
/// (`lib/common/labels.c:148-160`).
#[test]
fn issue30_html_attribute_warnings() {
    let src = "digraph { a [label=<<TABLE BORDER=\"-1\" STYLE=\"rounded,wiggly\" SIDES=\"LTx\" \
COLUMNS=\"x\"><TR><TD COLSPAN=\"0\" ALIGN=\"up\" FIXEDSIZE=\"TRUE\" WIDTH=\"3\" HEIGHT=\"3\">long \
text</TD></TR></TABLE>>]; b [label=<<FONT POINT-SIZE=\"big\">f</FONT><BR CLEAR=\"all\"/>>]; \
a -> b [label=<<TABLE FOO=\"1\"><TR><TD>x</TD></TR></TABLE>>] }";
    let want = "Warning: BORDER value -1 < 0 - too small - ignored\
                Warning: Illegal value wiggly for STYLE - ignored\n\
                Warning: Unrecognized character 'x' (120) in sides attribute\n\
                Warning: Unknown value x for COLUMNS - ignored\n\
                Warning: COLSPAN value cannot be 0 - ignored\n\
                Warning: Illegal value up for ALIGN in TD - ignored\n\
                Warning: cell size too small for content\n\
                in label of node a\n\
                Warning: Improper POINT-SIZE value big - ignored\
                Warning: Illegal attribute CLEAR in <BR> - ignored\n\
                in label of node b\n\
                Warning: Illegal attribute FOO in <TABLE> - ignored\n\
                in label of edge a -> b\n";
    for engine in ["dot", "neato"] {
        let (err, ok) = warnings(engine, src);
        assert!(ok, "{engine}");
        assert_eq!(err, want, "{engine}");
    }
}

/// A root graph label is made by `graph_init` (`lib/common/input.c:711`), before the
/// engine's node and edge labels, so its diagnostics come first.
#[test]
fn issue30_root_label_warnings_come_first() {
    let src = "digraph G { label=<<TABLE X=\"1\"><TR><TD>g</TD></TR></TABLE>>; \
a [label=<<TABLE Y=\"1\"><TR><TD>a</TD></TR></TABLE>>] }";
    let want = "Warning: Illegal attribute X in <TABLE> - ignored\n\
                in label of graph G\n\
                Warning: Illegal attribute Y in <TABLE> - ignored\n\
                in label of node a\n";
    for engine in ["dot", "neato", "patchwork"] {
        assert_eq!(warnings(engine, src).0, want, "{engine}");
    }
}

/// `addXLabels` (`lib/common/postproc.c:500-504`, `517`, `532`, `547`): an unset label of an
/// edge without a spline is reported with its `lp->text` (escapes substituted, an HTML label
/// without its delimiters), and the edge's remaining labels are skipped.
#[test]
fn issue30_unplaced_edge_label_warnings() {
    let src = "digraph G { splines=none; a [xlabel=q]; a -> b [label=\"x\\E\", taillabel=t, \
headlabel=h, xlabel=xx]; b -> c [headlabel=<<b>h2</b>>]; c -> d [taillabel=t3, xlabel=x3]; }";
    let want = "Warning: no position for edge with label xa->b\n\
                Warning: no position for edge with head label <b>h2</b>\n\
                Warning: no position for edge with tail label t3\n";
    for engine in ["dot", "neato", "fdp", "twopi", "osage"] {
        assert_eq!(warnings(engine, src).0, want, "{engine}");
    }
}

/// `spline_edges_` (`lib/neatogen/neatosplines.c:623`, `:625`) and `compoundEdges`
/// (`lib/fdpgen/clusteredges.c:238-246`) when node obstacles overlap.
#[test]
fn issue30_touching_nodes_warnings() {
    let pinned = "a [pos=\"0,0!\"]; b [pos=\"0.1,0!\"]; c [pos=\"2,0!\"]; a -- c;";
    assert_eq!(
        warnings("neato", &format!("graph G {{ splines=true; {pinned} }}")).0,
        "Warning: some nodes with margin (3.20,3.20) touch - falling back to straight line \
         edges\n"
    );
    assert_eq!(
        warnings("neato", &format!("graph G {{ splines=ortho; {pinned} }}")).0,
        "Warning: the bounding boxes of some nodes touch - falling back to straight line \
         edges\n"
    );
    let compound = "graph G { splines=compound; pack=1; a [pos=\"0,0!\"]; b [pos=\"2,0!\"]; \
d [pos=\"2.1,0!\"]; c [pos=\"4,0!\"]; a -- c; }";
    assert_eq!(
        warnings("fdp", compound).0,
        "Warning: compoundEdges: nodes touch - falling back to straight line edges\n\
         pack value 1 is smaller than esep (3.200,3.200)\n\
         Warning: some nodes with margin (3.20,3.20) touch - falling back to straight line \
         edges\n"
    );
}

/// Node shape diagnostics of `common_init_node`: `user_shape`'s unknown-shape warning, once
/// per name (`lib/common/shapes.c:3961`), `record_init`'s `bad label format` error, once
/// per node (`shapes.c:3682`), and `poly_init`'s fixed-size check (`shapes.c:2115-2118`).
#[test]
fn issue30_node_shape_warnings() {
    let src = "digraph Named { a [shape=foo]; a2 [shape=foo]; a3 [shape=Box]; \
b [shape=record, label=\"{a|\"]; b2 [shape=record, label=\"{a|\"]; \
c [fixedsize=true, width=0.1, label=\"long label\"]; c3 [fixedsize=true, width=3, label=ok] }";
    let want = "Warning: using box for unknown shape foo\n\
                Warning: using box for unknown shape Box\n\
                Error: bad label format {a|\n\
                Error: bad label format {a|\n\
                Warning: node 'c', graph 'Named' size too small for label\n";
    for engine in ["dot", "neato"] {
        let (err, ok) = warnings(engine, src);
        assert!(!ok, "{engine}: an error sets the exit status");
        assert_eq!(err, want, "{engine}");
    }
}

/// Each label is made, and reported, once in C, even when two labels of one object, or two
/// parallel edges, have the same text.
#[test]
fn issue30_identical_labels_each_warn() {
    let font = "<<FONT Q=\"1\">x</FONT>>";
    let src = format!(
        "digraph G {{ a [label={font} xlabel={font}]; \
         a -> b [label={font} headlabel={font} taillabel={font}] }}"
    );
    let node = "Warning: Illegal attribute Q in <FONT> - ignored\nin label of node a\n";
    let edge = "Warning: Illegal attribute Q in <FONT> - ignored\nin label of edge a -> b\n";
    let want = format!("{node}{node}{edge}{edge}{edge}");
    for engine in ["dot", "neato", "circo"] {
        assert_eq!(warnings(engine, &src).0, want, "{engine}");
    }
    let src = format!("digraph {{ a -> b [label={font}]; a -> b [label={font}] }}");
    assert_eq!(warnings("dot", &src).0, format!("{edge}{edge}"));
}

/// Every graph of a file is laid out, and its labels reported, on its own. Only the
/// unknown-shape warning is once per process (`user_shape` adds the name to `Shapes`).
#[test]
fn issue30_warnings_repeat_per_graph() {
    let g = "digraph G { a [label=<<TABLE Q=\"1\"><TR><TD>x</TD></TR></TABLE>> shape=foo]; \
a:zz -> b; c [shape=record label=\"{a|\"]; d [fixedsize=true width=0.1 label=\"long label\"] }\n";
    let per_graph = "Warning: Illegal attribute Q in <TABLE> - ignored\n\
                     in label of node a\n\
                     Error: bad label format {a|\n\
                     Warning: node 'd', graph 'G' size too small for label\n\
                     Warning: node a, port zz unrecognized\n";
    let want = format!("Warning: using box for unknown shape foo\n{per_graph}{per_graph}");
    for engine in ["dot", "neato"] {
        let (err, ok) = warnings(engine, &format!("{g}{g}"));
        assert!(!ok, "{engine}");
        assert_eq!(err, want, "{engine}");
    }
}

/// `neato -n`: neato_init_node_edge runs first whatever the level, so each edge's label
/// warnings are followed by its port warnings, and `-n2` warns about the ports of edges that
/// keep their input splines.
#[test]
fn issue30_neato_nop_port_warnings() {
    let src = "digraph G { node [pos=\"0,0\"]; a [pos=\"0,0\" label=<<FONT A=\"1\">a</FONT>>]; \
b [pos=\"100,0\"]; c [pos=\"10,0\"]; a:zz -> b [label=x pos=\"0,0 30,0 60,0 100,0\" lp=\"50,10\"]; \
b -> c:yy [label=<<FONT B=\"1\">y</FONT>>]; }";
    let want = "Warning: Illegal attribute A in <FONT> - ignored\n\
                in label of node a\n\
                Warning: node a, port zz unrecognized\n\
                Warning: Illegal attribute B in <FONT> - ignored\n\
                in label of edge b -> c\n\
                Warning: node c, port yy unrecognized\n";
    for n in ["-n1", "-n2"] {
        assert_eq!(run(&["-Kneato", n, "-Tsvg"], src).1, want, "{n}");
    }
    let src = "digraph G { a [pos=\"0,0\"]; b [pos=\"100,0\"]; \
a:zz -> b:yy [pos=\"e,80,0 0,0 30,0 60,0 80,0\"]; b -> a [label=x pos=\"e,0,0 100,0 60,0 30,0 0,0\"] }";
    let want = "Warning: node a, port zz unrecognized\nWarning: node b, port yy unrecognized\n";
    for n in ["-n1", "-n2"] {
        assert_eq!(run(&["-Kneato", n, "-Tsvg"], src).1, want, "{n}");
    }
}

/// `sidesfn` prints each unrecognized byte with `%c`, so a UTF-8 character gives one warning
/// per byte, written raw, with the byte's signed `char` value.
#[test]
fn issue30_sides_warning_raw_bytes() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
        .args(["-Tsvg"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            "digraph { a [label=<<TABLE SIDES=\"é\"><TR><TD>x</TD></TR></TABLE>>] }".as_bytes(),
        )
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let mut want = b"Warning: Unrecognized character '\xC3' (-61) in sides attribute\n".to_vec();
    want.extend_from_slice(b"Warning: Unrecognized character '\xA9' (-87) in sides attribute\n");
    assert_eq!(out.stderr, want);
}

/// `lexerror` (`lib/common/htmllex.c:67-72`) does not stop expat, so an empty unknown
/// element is reported by both `startElement` and `endElement`; other forms once.
#[test]
fn issue30_unknown_empty_html_element_reported_twice() {
    let err = "Error: Unknown HTML element <FOO> on line 1 \n";
    for (label, n) in [
        (
            "<TABLE A=\"1\"><FOO Z=\"1\"/><TR><TD>x</TD></TR></TABLE>",
            2,
        ),
        ("<FOO/>", 2),
        ("<FOO>x</FOO>", 1),
    ] {
        let warn = if label.contains("A=") {
            "Warning: Illegal attribute A in <TABLE> - ignored\n"
        } else {
            ""
        };
        let want = format!("{warn}{}in label of node a\n", err.repeat(n));
        let (got, ok) = warnings("dot", &format!("digraph {{ a [label=<{label}>] }}"));
        assert!(!ok, "{label}");
        assert_eq!(got, want, "{label}");
    }
}

/// `make_flat_adj_edges` (`lib/dotgen/dotsplines.c:1144-1151`) skips flat edges between
/// adjacent nodes when one is a record, with a warning printed once per process.
#[test]
fn issue30_flat_adjacent_record_edge_warning() {
    let src = "graph { {rank=same; a -- b [label=x]; c -- d [label=y];} b [shape=record]; \
c [shape=Mrecord] }";
    let (err, ok) = warnings("dot", src);
    assert!(!ok);
    assert_eq!(
        err,
        "Warning: flat edge between adjacent nodes one of which has a record shape - replace \
         records with HTML-like labels\n  Edge a -- b\n\
         Error: lost a b edge\n\
         Error: lost c d edge\n"
    );
}

/// fdp's `deriveGraph` (`lib/fdpgen/layout.c:409-413`) labels a graph's clusters before it
/// lays out each of them, so a cluster's label warnings come after all its siblings'.
#[test]
fn issue30_fdp_cluster_label_warning_order() {
    let tbl = |a: &str| format!("<<TABLE {a}=\"1\"><TR><TD>x</TD></TR></TABLE>>");
    let src = format!(
        "graph G {{ subgraph cluster_a {{ label={}; subgraph cluster_a1 {{ label={}; p }} q }} \
         subgraph cluster_b {{ label={}; subgraph cluster_b1 {{ label={}; r }} s }} x -- s; }}",
        tbl("A1"),
        tbl("A11"),
        tbl("B1"),
        tbl("B11")
    );
    let w = |a: &str, g: &str| {
        format!("Warning: Illegal attribute {a} in <TABLE> - ignored\nin label of graph {g}\n")
    };
    let want = [
        w("A1", "cluster_a"),
        w("B1", "cluster_b"),
        w("A11", "cluster_a1"),
        w("B11", "cluster_b1"),
    ]
    .concat();
    assert_eq!(warnings("fdp", &src).0, want);
}

/// fdp's `splines and cluster edges not supported` (`lib/fdpgen/layout.c:1048-1051`) and
/// osage's array packing without `sortv` (`lib/osage/osageinit.c:100-106`, no newline).
#[test]
fn issue30_fdp_and_osage_layout_warnings() {
    assert_eq!(
        warnings(
            "fdp",
            "graph G { splines=true; subgraph cluster_a { x } y -- cluster_a }"
        )
        .0,
        "Warning: splines and cluster edges not supported - using line segments\n"
    );
    let src = "graph G { packmode=\"array_u\"; subgraph cluster_a { packmode=\"array_u\"; x; \
subgraph cluster_c { z } } subgraph cluster_b { y } }";
    let w = |g: &str| {
        format!(
            "Warning: Graph {g} has array packing with user values but no \"sortv\" attributes \
             are defined."
        )
    };
    let want = ["cluster_c", "cluster_a", "cluster_b", "G"].map(w).concat();
    assert_eq!(warnings("osage", src).0, want);
}

/// Issue #173: `cluster_inner4` is repeated under three parents and each copy holds j1 and
/// j2. osage's `layout` (`lib/osage/osageinit.c`) places them in the first copy, packs
/// nothing for the others and leaves them a NaN box, so the edges into `cluster_outer` get
/// NaN control points. `update_bb_bz` (`lib/common/emit.c`) splits such a segment forever
/// and C's stack overflows; the port must render the graph.
#[test]
fn osage_repeated_cluster_does_not_overflow_update_bb_bz() {
    for fmt in ["-Tsvg", "-Tdot"] {
        let (out, err, ok) = run(
            &[fmt],
            "digraph{layout=osage{o2\n  subgraph cluster_inner4{j1 j2}}subgraph cluster_outer{{label=\"outer\"\
         subgraph cluster_inner4{j1 j2}}->o1}subgraph cluster_inner4{j1 j2}}\n",
        );
        assert!(ok, "{fmt}: {err}");
        if fmt == "-Tdot" {
            assert!(out.contains("j1 -> o1\t[pos="), "{out}");
        }
    }
}

/// fdp's cluster edges (`checkCompound`, `lib/common/utils.c:855-900`): cycles and edges
/// within one cluster tree are dropped with a warning. `agcontains` looks at every
/// ancestor, so a cluster two levels down counts as inside.
#[test]
fn issue30_fdp_cluster_edge_warnings() {
    for (src, want) in [
        (
            "graph G { subgraph cluster_b { b1 } cluster_b -- cluster_b; x -- cluster_b }",
            "Warning: cluster cycle cluster_b -- cluster_b not supported\n",
        ),
        (
            "graph G { subgraph cluster_a { a1; subgraph cluster_a2 { a2 } } \
             cluster_a -- cluster_a2; cluster_a2 -- cluster_a; x -- cluster_a }",
            "Warning: head cluster cluster_a2 inside tail cluster cluster_a\n\
             Warning: tail cluster cluster_a2 inside head cluster cluster_a\n",
        ),
        (
            "graph G { subgraph cluster_a { a1 } a1 -- cluster_a; cluster_a -- a1; x -- cluster_a }",
            "Warning: tail node a1 inside head cluster cluster_a\n\
             Warning: head node a1 inside tail cluster cluster_a\n",
        ),
        (
            "graph G { subgraph cluster_a { subgraph cluster_b { subgraph cluster_c { c } } } \
             cluster_a -- cluster_c; x -- cluster_a }",
            "Warning: head cluster cluster_c inside tail cluster cluster_a\n",
        ),
    ] {
        assert_eq!(warnings("fdp", src).0, want, "{src}");
    }
}

/// neato ortho obstacles (`makeObstacle` with `isOrtho`) keep `margin = {0}`: a scaling
/// `sep`/`esep` shrinks each box to its node's centre, so touching boxes are not reported.
#[test]
fn issue30_neato_ortho_scaled_margin_does_not_touch() {
    let src = "graph G { splines=ortho; esep=0.5; a [pos=\"0,0!\"]; b [pos=\"0.5,0!\"]; \
c [pos=\"3,2!\"]; a -- c; b -- c }";
    assert!(!warnings("neato", src).0.contains("touch"));
    let src = src.replace("esep=0.5", "esep=\"+0.5\"");
    assert!(
        warnings("neato", &src)
            .0
            .contains("the bounding boxes of some nodes touch")
    );
}

/// A `fixedsize=shape` node's ortho obstacle is its polygon's box (`polyBB`,
/// `lib/neatogen/neatosplines.c:351-358`), not the node size that holds the label.
#[test]
fn issue30_neato_ortho_fixedshape_obstacle_is_the_polygon() {
    let src = "graph { splines=ortho; k [shape=box fixedsize=shape width=0.3 \
label=\"a very wide label\" pos=\"0,0!\"]; b [pos=\"0.9,0.3!\"]; c [pos=\"0,2!\"]; k -- c; b -- c }";
    assert!(!warnings("neato", src).0.contains("touch"));
}

/// Issue #88: graph_init makes the root graph label before dot sizes the nodes, so the
/// label's font warnings come first.
#[test]
fn issue88_root_label_font_warnings_first() {
    let src = "graph G {\nlabel=\"tab\there\";\na [fontname=\"Bookman\"];\n}\n";
    let want = "Warning: Warning: no value for width of ASCII character 9. Falling back to 0\n\
                Warning: Warning: no hard-coded metrics for 'Bookman'.  Falling back to 'Times' \
                metrics\n";
    for engine in ["dot", "neato", "circo"] {
        assert_eq!(warnings(engine, src).0, want, "{engine}");
    }
}

/// Not ported: HTML `<IMG>` sources are never loaded, so `size_html_img`'s error
/// (`lib/common/htmltable.c:1088-1093`, after `gvusershape_file_access`'s warning at
/// `lib/gvc/gvusershape.c:700`) is missing and the run exits 0 where C exits 1.
#[test]
#[ignore = "issue #130: HTML IMG file errors (No or improper image file)"]
fn issue30_html_missing_image_error() {
    let src = "digraph { a [label=<<TABLE><TR><TD><IMG SRC=\"nope.png\"/></TD></TR></TABLE>>] }";
    let (err, ok) = warnings("dot", src);
    assert!(!ok);
    assert_eq!(
        err,
        "Warning: No such file or directory while opening nope.png\n\
         Error: No or improper image file=\"nope.png\"\n\
         in label of node a\n"
    );
}

/// Not ported: user shapes are never loaded, so `poly_init`'s missing-shapefile warning
/// (`lib/common/shapes.c:2030-2032`) and `epsf_init`'s (`lib/common/psusershape.c:118`)
/// are missing.
#[test]
#[ignore = "issue #130: shapefile/epsf user-shape warnings"]
fn issue30_missing_shapefile_warnings() {
    let src = "digraph { a4 [shape=custom]; a5 [shape=epsf] }";
    assert_eq!(
        warnings("dot", src).0,
        "Warning: No or improper shapefile=\"<nil>\" for node \"a4\"\n\
         Warning: shapefile not set or not found for epsf node a5\n"
    );
}
