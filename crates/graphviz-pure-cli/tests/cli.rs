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

// Issue #181: neato -n2's user_spline (lib/neatogen/neatoinit.c) skips one byte after a
// segment of an edge `pos`. The port sliced the `str` there and panicked when the byte began a
// multi-byte character. C rejects the spline with a warning, routes the edge and exits 0;
// its stderr is Graphviz 16.1.0's.
#[test]
fn edge_pos_ending_in_multibyte_char_does_not_panic() {
    let src = "digraph { a [pos=\"0,0\"]; b [pos=\"30,30\"]; \
               a -> b [pos=\"0,0 10,10 20,20 30,30é\"] }";
    let (out, err, ok) = run(&["-Kneato", "-n2", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert_eq!(
        err,
        "Warning: pos attribute for edge (a,b) doesn't have 3n+1 points\n"
    );
    assert!(out.contains("<path"), "the edge is still drawn");
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

// Issue #165: every engine's `is_a_cluster` sliced the subgraph name at byte 7, which panics
// ("byte index 7 is not a char boundary") when byte 7 falls inside a multi-byte character.
// C's `strncasecmp(agnameof(g), "cluster", 7)` compares bytes, and C draws these graphs.
// "CLUSTERé" is still a cluster, as in C.
#[test]
fn subgraph_name_with_multibyte_char_at_byte_7_does_not_panic() {
    let src = "graph { subgraph \"aaaaaaé\" { a [pos=\"0,0\"] } \
               subgraph \"CLUSTERé\" { b [pos=\"1,1\"] } }\n";
    let engines = [
        "dot",
        "neato",
        "fdp",
        "sfdp",
        "twopi",
        "circo",
        "osage",
        "patchwork",
        "nop",
    ];
    for engine in engines {
        for fmt in ["-Tsvg", "-Tjson"] {
            let (out, err, ok) = run(&[&format!("-K{engine}"), fmt, "-Gpack=true"], src);
            assert!(ok && !err.contains("panicked"), "{engine} {fmt}: {err}");
            // The engines that draw this cluster in C (circo, twopi and sfdp ignore clusters;
            // nop has no cluster `bb` to draw).
            let draws = ["dot", "neato", "fdp", "osage", "patchwork"].contains(&engine);
            if fmt == "-Tsvg" && draws {
                assert_eq!(
                    out.matches("class=\"cluster\"").count(),
                    1,
                    "{engine}: {out}"
                );
            }
        }
    }
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

// Issue #166: getAdjustMode (lib/neatogen/adjust.c) matches the "prism" prefix with
// strncasecmp, a byte comparison. The port sliced the `str` at byte 5 and panicked when that
// byte fell inside a multi-byte character. C's stderr is Graphviz 16.1.0's.
#[test]
fn overlap_prism_prefix_with_multibyte_char_does_not_panic() {
    for engine in ["neato", "fdp", "sfdp", "twopi", "circo"] {
        let (err, ok) = warnings(engine, "graph { overlap=\"prisé\"; a -- b }");
        assert!(ok, "{engine}: {err}");
        assert_eq!(
            err, "Warning: Unrecognized overlap value \"prisé\" - using false\n",
            "{engine}"
        );
        // A real "prism" prefix still matches whatever follows it.
        let (err, ok) = warnings(engine, "graph { overlap=\"PRISMé\"; a -- b }");
        assert!(ok, "{engine}: {err}");
        assert_eq!(err, "", "{engine}");
    }
}

// Issue #166, same root cause: neato's ipsep mode matches diredgeconstraints="hier..." with
// strncasecmp(str, "hier", 4) (lib/neatogen/neatoinit.c, majorization).
#[test]
fn diredgeconstraints_hier_prefix_with_multibyte_char_does_not_panic() {
    let src = "digraph { mode=ipsep; diredgeconstraints=\"hieé\"; a -> b; b -> c }";
    let (err, ok) = warnings("neato", src);
    assert!(ok, "{err}");
    assert_eq!(err, "");
}

// Issue #196: at 1e17 `(int)floor(x / Cell)` saturates to INT_MAX (INT_MIN at -1e17), and
// gridRepulse's `doNeighbor(grid, i + 1, ...)` (lib/fdpgen/tlayout.c) wraps under clang where a
// plain Rust `+` panics in debug builds. The expected output is C Graphviz 16.1.0's.
#[test]
fn fdp_nodes_pinned_near_1e17_lay_out_like_c() {
    let cases = [
        (
            "graph { layout=fdp; a [pos=\"1e17,1e17!\"]; b [pos=\"1e17,1.0000001e17!\"] }",
            "graph 1 1e+17 1e+17\n\
             node a 1e+17 1e+17 0.75 0.5 a solid ellipse black lightgrey\n\
             node b 1e+17 1e+17 0.75 0.5 b solid ellipse black lightgrey\n\
             stop\n",
        ),
        (
            "graph { layout=fdp; a [pos=\"-1e17,-1e17!\"]; b [pos=\"-1e17,-1.0000001e17!\"] }",
            "graph 1 1e+17 1e+17\n\
             node a 0 1e+10 0.75 0.5 a solid ellipse black lightgrey\n\
             node b 0 0 0.75 0.5 b solid ellipse black lightgrey\n\
             stop\n",
        ),
    ];
    for (src, expected) in cases {
        let (out, err, ok) = run(&["-Tplain"], src);
        assert!(ok, "{src}: {err}");
        assert_eq!(out, expected, "{src}");
    }
}

// Extreme coordinates (docs/porting/crashes.md): C's `sscanf("%lf")` reads `nan` and `inf` in
// `pos`, and a finite `pos` can overflow once scaled to points. Where C produces output (often
// a component at NaN) the port matches it; where C crashes or hangs, the port must not. These
// are the routes by engine; none may panic or hang.
#[test]
fn non_finite_and_huge_positions_do_not_panic_in_any_engine() {
    let engines = [
        "dot",
        "neato",
        "fdp",
        "sfdp",
        "circo",
        "twopi",
        "osage",
        "patchwork",
    ];
    // 1e308 inches overflows to infinity once multiplied by 72.
    for v in ["nan", "inf", "-inf", "1e308"] {
        for engine in engines {
            for splines in ["spline", "curved", "ortho"] {
                let src = format!(
                    "graph {{ layout={engine}; splines={splines}; pack=false; \
                     a [pos=\"0,0!\"]; x [pos=\"{v},0!\"]; a -- x; x -- y [label=L]; \
                     subgraph cluster_c {{ label=C; c [pos=\"5,{v}\"] }} c -- a }}"
                );
                let (out, err, _) = run(&["-Tsvg"], &src);
                assert!(!err.contains("panicked"), "{src}: {err}");
                assert!(out.contains("</svg>"), "{src}: {err}");
            }
        }
    }
    for (flag, v) in [
        ("-n", "nan"),
        ("-n2", "nan"),
        ("-n2", "inf"),
        ("-n2", "1e400"),
    ] {
        let src = format!(
            "digraph {{ a [pos=\"{v},0\"]; b [pos=\"30,30\" xlabel=Q xlp=\"{v},0\"]; \
             a -> b [pos=\"0,0 {v},10 20,20 30,30\" label=L lp=\"{v},3\"]; \
             subgraph cluster_z {{ bb=\"0,0,{v},9\"; z [pos=\"4,4\"] }} }}"
        );
        let (out, err, ok) = run(&["-Kneato", flag, "-Tsvg"], &src);
        assert!(ok, "{flag} {src}: {err}");
        assert!(out.contains("</svg>"), "{flag} {src}: {err}");
    }
}

// Issue #228: neato splines=ortho with a NaN or infinite pin. C fails an assertion in
// ortho's list.c (exit 134); the port panicked in `convert_sp_to_route` until ortho got a
// failure boundary (#209, #248). The NaN component is drawn as C draws one with any other
// `splines`: its nodes are at NaN and the SVG leaves them out; `b` is drawn. Here the input
// side: the position reaches ortho as C reads it.
#[test]
fn ortho_with_a_non_finite_pin_does_not_panic() {
    for v in ["nan", "inf"] {
        let src = format!(
            "graph {{ layout=neato; pack=false; splines=ortho; a [pos=\"{v},0!\"]; \
             x [pos=\"1,0!\"]; a -- x; b }}"
        );
        let (out, err, ok) = run(&["-Tsvg"], &src);
        assert!(ok, "{src}: {err}");
        assert_eq!(err, "", "{src}");
        assert!(out.contains("<title>b</title>"), "{src}: {out}");
    }
}

// Issue #227: curved edges between pins 1e16 inches apart. `update_bb_bz` refines each
// segment until its control points are within 2pt of its chord, which the coordinates there
// cannot resolve, and C never finishes. The port stops a segment after 2^28 splits (about 5 s
// of C) and takes the rest whole; the graph bb still contains the curves. (neato's
// `compute_bb` rebuilds the bb afterwards; fdp keeps `update_bb_bz`'s.)
#[test]
fn curved_edges_between_far_pins_finish() {
    for (engine, bb) in [
        ("neato", "0,0,8.6241e+17,8.64e+17"),
        ("fdp", "-1.4241e+17,-1.08e+17,7.2e+17,7.2e+17"),
    ] {
        let src = format!(
            "graph {{ layout={engine}; pack=false; splines=curved; a [pos=\"0,0!\"]; \
             x [pos=\"1e16,0!\"]; y [pos=\"0,1e16!\"]; a -- x; a -- y }}"
        );
        let (out, err, ok) = run(&["-Tdot"], &src);
        assert!(ok, "{src}: {err}");
        assert!(out.contains(&format!("bb=\"{bb}\"")), "{src}: {out}");
    }
    // A complete graph of far pins (on a circle of radius 2e14 the same graph kept C running
    // for 18 minutes) must finish; once a segment runs out of splits, the later ones are
    // taken whole.
    let mut src = String::from("graph { layout=fdp; pack=false; splines=curved;");
    for i in 0..6 {
        let t = std::f64::consts::PI / 3.0 * f64::from(i);
        src.push_str(&format!(
            " n{i} [pos=\"{:?},{:?}!\"];",
            1.5e15 * t.cos(),
            1.5e15 * t.sin()
        ));
        for j in 0..i {
            src.push_str(&format!(" n{j} -- n{i};"));
        }
    }
    src.push('}');
    let (_, err, ok) = run(&["-Tdot"], &src);
    assert!(ok, "{err}");
}

// The budget is per segment, not per layout: these 100 segments each finish within it but
// need 3.9e8 splits in all, and C (8 s) and the port print the same bb. The bb is C Graphviz
// 16.1.0's; a per-layout budget ran out at segment 66 and printed `0,-8.5824e+15,...`.
#[test]
fn many_curved_edges_c_finishes_keep_c_bb() {
    let mut src = String::from(
        "graph { layout=fdp; pack=false; splines=curved; top [pos=\"0,300000000000000.0!\"];",
    );
    for k in 0..100 {
        let l = 1e14 * (1.0 + f64::from(k) * 0.02);
        src.push_str(&format!(
            " l{k} [pos=\"{:?},0!\"]; r{k} [pos=\"{l:?},0!\"]; l{k} -- r{k};",
            -l
        ));
    }
    src.push('}');
    let (out, err, ok) = run(&["-Tdot"], &src);
    assert!(ok, "{err}");
    assert!(
        out.contains("bb=\"0,-6.4368e+15,4.2912e+16,2.16e+16\""),
        "{out}"
    );
}

// Issue #239: a pin at -inf gives a component an infinite box, so computeStep's discriminant
// is NaN and C fails `assert(d >= 0)` (exit 134; neato segfaults); the port filled grid
// cells without end. It now skips packing, as C does where `stepSize <= 0`.
#[test]
fn packing_a_component_with_an_infinite_pin_is_skipped() {
    for engine in ["fdp", "neato"] {
        let src = format!("digraph {{ layout={engine} n0 [pos=\"-inf,1e8!\"] n2 n1->n0 }}");
        let (out, err, ok) = run(&["-Tsvg"], &src);
        assert!(ok, "{engine}: {err}");
        assert_eq!(
            err, "Warning: components too large to pack; packing skipped\n",
            "{engine}"
        );
        assert!(out.contains("</svg>"), "{engine}");
    }
}

// Issue #240: overlap=portho* with pins from about 3e6 inches. C's initItem (constraint.c)
// converts the position to `int` through 64 bits and keeps the low 32, so it wraps, and
// its `int` arithmetic after that wraps too, where the port's panicked. C lays these out;
// the expected output is C Graphviz 16.1.0's. The last two have overlapping nodes, so
// overlap removal runs on the wrapped positions.
#[test]
fn portho_with_a_pin_at_1e12_lays_out_like_c() {
    let cases = [
        (
            "graph { layout=neato overlap=portho_yx a -- b; b [pos=\"1e12,0!\"] }",
            [
                "bb=\"0,0,3.6e+13,52.362\"",
                "pos=\"3.6e+13,18\"",
                "a -- b\t[pos=\"54.312,34.362 3.621e+07,34.362 3.6e+13,18 3.6e+13,18\"]",
            ],
        ),
        (
            "graph { layout=neato overlap=portho_yx a -- b; b [pos=\"-inf,1e12!\"] }",
            [
                "bb=\"0,0,-4.295e+09,7.2e+13\"",
                "pos=\"nan,7.2e+13\"",
                "a -- b\t[pos=\"nan,18.047 nan,2.1458e+06 nan,7.2e+13 nan,7.2e+13\"]",
            ],
        ),
    ];
    for (src, expected) in cases {
        let (out, err, ok) = run(&["-Tdot"], src);
        assert!(ok, "{src}: {err}");
        for e in expected {
            assert!(out.contains(e), "{src}: {e}\n{out}");
        }
    }
    let plain = [
        (
            "graph { overlap=portho a [pos=\"1e12,0!\"]; d [pos=\"-1e12,0!\"]; \
             b [pos=\"0,0!\"]; c [pos=\"0.1,0!\"] }",
            "graph 1 2.4722 1.7222\n\
             node a 2.0972 1.4722 0.75 0.5 a solid ellipse black lightgrey\n\
             node d 0.375 1.4722 0.75 0.5 d solid ellipse black lightgrey\n\
             node b 1.2361 0.86111 0.75 0.5 b solid ellipse black lightgrey\n\
             node c 1.2361 0.25 0.75 0.5 c solid ellipse black lightgrey\n\
             stop\n",
        ),
        (
            "graph { overlap=portho; a [pos=\"2e7,0!\"]; b [pos=\"0,0!\"]; c [pos=\"0.1,0!\"] }",
            "graph 1 1.6111 1.7222\n\
             node a 1.2361 1.4722 0.75 0.5 a solid ellipse black lightgrey\n\
             node b 0.375 0.86111 0.75 0.5 b solid ellipse black lightgrey\n\
             node c 0.375 0.25 0.75 0.5 c solid ellipse black lightgrey\n\
             stop\n",
        ),
    ];
    for (src, expected) in plain {
        let (out, err, ok) = run(&["-Kneato", "-Tplain"], src);
        assert!(ok, "{src}: {err}");
        assert_eq!(out, expected, "{src}");
    }
}

// overlap=vpsc: C's `Rectangle` and `Node` (lib/vpsc/generate-constraints.cpp) assert
// `x <= X` and `width < 1e40`, which a pin at NaN, infinity or 1e300 (infinite as the `float`
// coordinates removeoverlaps works on) fails, and C aborts; the port's solver never settled.
// The port skips overlap removal with a warning. Where the boxes pass and only the solver's
// cost is NaN, C's `while (fabs(lastcost - cost) > 0.0001)` ends the loop, and so does the
// port's: an unpinned node at 1e300 lays out as C's (all at NaN).
#[test]
fn vpsc_overlap_removal_with_extreme_positions_finishes() {
    for v in ["nan", "inf", "1e300"] {
        let src =
            format!("graph {{ layout=neato; overlap=vpsc; a [pos=\"{v},0!\"]; a -- b; b -- c }}");
        let (_, err, ok) = run(&["-Tsvg"], &src);
        assert!(ok, "{src}: {err}");
        assert_eq!(
            err, "Warning: node positions are not finite; overlap removal skipped\n",
            "{src}"
        );
    }
    let src = "digraph { layout=fdp; overlap=vpsc; n3 [pos=\"1,1e300\"]; n3->n4 }";
    let (out, err, ok) = run(&["-Tdot"], src);
    assert!(ok, "{err}");
    assert!(out.contains("bb=\"0,0,104.5,-4.295e+09\""), "{out}");
    assert!(out.contains("pos=\"77.499,nan\""), "{out}");
}

// Issue #220: C reads `pos` with the C library's sscanf("%lf"), which takes hexadecimal
// floats and `nan(chars)`. The expected values are C Graphviz 16.1.0's output.
#[test]
fn neato_n_reads_hex_and_nan_chars_positions_like_c() {
    let src = "digraph { a [pos=\"0x10,0\"]; b [pos=\"30,30\"]; a -> b }";
    let (out, err, ok) = run(&["-Kneato", "-n2", "-Tdot"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains("pos=\"27,18\"") && out.contains("pos=\"41,48\""),
        "{out}"
    );
    let edge = |p: &str| {
        format!(
            "digraph {{ a [pos=\"0,0\"]; b [pos=\"30,30\"]; a -> b [pos=\"{p},0 10,10 20,20 30,30\"] }}"
        )
    };
    let (out, err, ok) = run(&["-Kneato", "-n2", "-Tsvg"], &edge("0x10"));
    assert!(ok && err.is_empty(), "{err}");
    assert!(out.contains("d=\"M43,-18C37,-28 47,-38 57,-48\""), "{out}");
    let (out, err, ok) = run(&["-Kneato", "-n2", "-Tsvg"], &edge("nan(1)"));
    assert!(ok && err.is_empty(), "{err}");
    assert!(out.contains("d=\"Mnan,-18C37,-28 47,-38 57,-48\""), "{out}");
}

// Issue #220 (fdp): initialPositions' sscanf("%lf,%lf%c") reads `0x1p3` as 8.
#[test]
fn fdp_reads_a_hex_float_pos_like_c() {
    let (out, err, ok) = run(
        &["-Tdot"],
        "graph { layout=fdp; a [pos=\"0x1p3,2\"]; a -- b }",
    );
    assert!(ok && err.is_empty(), "{err}");
    assert!(out.contains("pos=\"89.637,24.219\""), "{out}");
}

// fdp's chkPos (lib/fdpgen/layout.c) warns about a cluster `coords` it cannot read.
#[test]
fn fdp_warns_about_unreadable_cluster_coords_like_c() {
    let src = "graph { layout=fdp; subgraph cluster_a { coords=\"1,2,x\"; a } a -- b }";
    let (_, err, ok) = run(&["-Tdot"], src);
    assert!(ok, "{err}");
    assert_eq!(
        err,
        "Warning: graph cluster_a, coord 1,2,x, expected four doubles\n"
    );
}

// Issue #245: with dim=3, user_pos (lib/neatogen/neatoinit.c) reads `z` when `pos` has two
// values, or else draws one with jitter3d, and addZ writes every node's `z` back. The
// expected values are C Graphviz 16.1.0's output.
#[test]
fn neato_dim3_reads_and_writes_z_like_c() {
    let src = "graph { layout=neato; dim=3; a [pos=\"1,2\" z=\"3\"]; a -- b; b -- c }";
    let (out, err, ok) = run(&["-Tdot"], src);
    assert!(ok, "{err}");
    assert!(out.contains("bb=\"0,0,66.816,77.847\""), "{out}");
    for z in ["z=66.978896", "z=2.061028", "z=-69.039923"] {
        assert!(out.contains(z), "{z}: {out}");
    }
    // The -n path runs user_pos and addZ too.
    let src = "graph { dim=3; a [pos=\"10,20,5\" z=\"3\"]; b [pos=\"30,40\"]; a -- b }";
    let (out, err, ok) = run(&["-Kneato", "-n", "-Tdot"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains("z=5.000000") && out.contains("z=57.090927"),
        "{out}"
    );
}

// input.c's graph_init reads `ranksep` with sscanf("%lf"): for a blank value it returns EOF
// and assigns nothing, so the rank separation keeps the `nodesep` just read.
#[test]
fn blank_ranksep_takes_the_nodesep_like_c() {
    let (out, err, ok) = run(&["-Tdot"], "digraph { ranksep=\"\"; nodesep=1.5; a -> b }");
    assert!(ok, "{err}");
    assert!(out.contains("bb=\"0,0,54,180\""), "{out}");
}

// getdoubles2ptf (lib/common/input.c) rounds `size` to whole points.
#[test]
fn fractional_size_is_rounded_to_points_like_c() {
    let src = "digraph { size=\"1.013,1.017\"; a -> b -> c; a -> c }";
    let (out, err, ok) = run(&["-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains("transform=\"scale(0.388298 0.388298)"),
        "{out}"
    );
}

// init_job_viewport (lib/common/emit.c) reads `viewport="8,4,8,4"` with its second sscanf,
// "%lf,%lf,%lf,%[^,]%c", as a focus node named `4`; there is none, so the focus stays at the
// centre of the drawing.
#[test]
fn viewport_with_an_unknown_focus_node_keeps_the_centre_like_c() {
    let (out, err, ok) = run(&["-Tsvg"], "graph { viewport=\"8,4,8,4\"; a -- b }");
    assert!(ok, "{err}");
    assert!(out.contains("translate(-26.5 54.25)"), "{out}");
}

// Issue #245 with mode=sgd: initial_positions' randompos also draws the third coordinate
// (jitter3d), which addZ prints.
#[test]
fn neato_sgd_dim3_draws_z_like_c() {
    let src =
        "graph { layout=neato; mode=sgd; dim=3; node [z=\"\"]; a [pos=\"1,2!\"]; a -- b; b -- c }";
    let (out, err, ok) = run(&["-Tdot"], src);
    assert!(ok, "{err}");
    for z in ["z=85.636391", "z=60.106840", "z=0.127218"] {
        assert!(out.contains(z), "{z}: {out}");
    }
}

// colorxlate's sscanf("%lf%lf%lf%lf") reads an HSV color written with hexadecimal floats.
#[test]
fn hsv_color_with_hex_floats_like_c() {
    let src = "graph { a [style=filled, fillcolor=\"0x1p-1 1 1\"] }";
    let (out, err, ok) = run(&["-Tsvg"], src);
    assert!(ok, "{err}");
    assert!(out.contains("fill=\"#00ffff\""), "{out}");
}

// graph_init: dpi = late_double(dpi, late_double(resolution, 0, 0), 0), so an unreadable
// `dpi` falls back to `resolution`.
#[test]
fn unreadable_dpi_falls_back_to_resolution_like_c() {
    let (out, err, ok) = run(&["-Tsvg"], "graph { dpi=\"x\"; resolution=144; a }");
    assert!(ok, "{err}");
    assert!(
        out.contains("<svg width=\"124pt\" height=\"88pt\""),
        "{out}"
    );
}

// fdp's initialPositions (lib/fdpgen/fdpinit.c) warns about a `pos` it cannot read.
#[test]
fn fdp_warns_about_an_unreadable_pos_like_c() {
    let src = "graph { layout=fdp; a [pos=\"zz,0!\"]; a -- b }";
    let (_, err, ok) = run(&["-Tsvg"], src);
    assert!(ok, "{err}");
    assert_eq!(
        err,
        "Warning: node a, position zz,0!, expected two floats\n"
    );
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

// Issue #191. The repeated cluster gives the osage graph an infinite bb, and
// `xlhorder`'s `floor(log2(inf)) + 1` overflowed the Hilbert order in a debug build.
// C fails an assertion in `xlhdxload` (lib/label/xlabels.c) on these inputs; the port
// computes C's order (0), stops where C asserts without placing a label (#301), and exits
// cleanly.
#[test]
fn xlabels_with_an_infinite_bb_do_not_panic() {
    let cluster = "subgraph cluster_inner4{j1 j2}";
    let mut failures = Vec::new();
    for attrs in [
        "edge[label=L]",
        "edge[headlabel=L]",
        "j1[xlabel=X]",
        "splines=curved edge[label=L]",
        "splines=polyline edge[label=L]",
    ] {
        let src = format!(
            "digraph{{layout=osage {attrs}{{o2 {cluster}}}subgraph cluster_outer{{{{label=\"outer\"{cluster}}}->o1}}{cluster}}}"
        );
        let (_, err, ok) = run(&["-Tsvg"], &src);
        if !ok || err.contains("panicked") {
            failures.push(format!("{attrs}: {err}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// Issue #167, found by the fuzzer. `b` and `l` are articulation points that blocktree.c's
// `dfs` places in other blocks, so the block {i, k} has no edge of its own. C's
// `clone_graph` (lib/circogen/blockpath.c) zeroes every clone's `DEGREE`, and `getList`
// sorts the two nodes at degree 0; the port kept degrees only for nodes with an edge and
// panicked in `cmpDegree`. The input is a corpus file whose C -Tdot golden the circo ratchet
// also checks; the positions below are C's.
#[test]
fn circo_block_without_edges_lays_out_like_c() {
    let src = include_str!("../../../tests/circo/fuzz_blockpath_edgeless_block.gv");
    let (out, err, ok) = run(&["-Tdot"], src);
    assert!(ok, "{err}");
    assert!(out.contains("pos=\"373.36,289.22\""), "i: {out}");
    assert!(out.contains("pos=\"303.9,249.11\""), "k: {out}");
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

/// Issue #201, reduced: neato pins `a` at NaN, so `a -- x` gets NaN control points, while
/// `b` keeps `bb` finite and the segment falls outside it. `update_bb_bz`
/// (`lib/common/emit.c`) then splits the segment with `Bezier` until C's stack overflows
/// (SIGSEGV); the issue's second pin at 1e12 is not needed. The port leaves `bb` as it was.
#[test]
fn neato_nan_pin_edge_does_not_overflow_update_bb_bz() {
    let src = "graph { layout=neato; pack=false; a [pos=\"nan,0!\"]; a -- x; b }\n";
    for fmt in ["-Tsvg", "-Tdot"] {
        let (out, err, ok) = run(&[fmt], src);
        assert!(ok, "{fmt}: {err}");
        if fmt == "-Tdot" {
            assert!(out.contains("bb=\"0,0,54,68.723\""), "{out}");
            assert!(out.contains("a -- x\t[pos="), "{out}");
        }
    }
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

// Issue #168, from the coverage-guided fuzzer. mode=ipsep always lays out the connected
// components separately, and cluster_map (lib/neatogen/neatoinit.c) takes the clusters from
// the root graph: a cluster member in another component gets the index agnnodes(g), and C fails
// bitarray_set's "out of bounds access" assertion (exit 134). The port used to carry that index
// into generateNonoverlapConstraints and panic under overlap=ipsep (without it the index
// silently named a cluster dummy variable); it now keeps each component's own members.
#[test]
fn neato_ipsep_clusters_in_other_components_do_not_panic() {
    for src in [
        "graph { layout=neato; mode=ipsep; overlap=ipsep; subgraph cluster_a { a } subgraph cluster_b { b } a -- c }",
        // one cluster split across two components
        "graph { layout=neato; mode=ipsep; overlap=ipsep; subgraph cluster_a { a b } a -- c }",
    ] {
        let (out, err, ok) = run(&["-Tsvg"], src);
        assert!(ok, "{src}: {err}");
        assert_eq!(err, "", "{src}");
        assert_eq!(out.matches("class=\"node\"").count(), 3, "{src}: {out}");
    }
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

/// Issue #138: with a rotated box (or `shape=none`) and a compass port, both
/// endpoints get triangle edges but `triPath` never reaches the tail, so the
/// `dad` walk in C's `mkPoly` (`lib/neatogen/multispline.c:968-971`) stops on a
/// node that is its own parent and aborts on `assert(nxt != dad[nxt])`, or, when
/// node 0 was reached, on `edgeToSeg`'s `assert(0)` (which one varies between C
/// runs). The port looped forever in the first case. It now treats both as a
/// failed multispline and falls back like other failures, on every
/// neato-family engine. On twopi the record graph takes the `edgeToSeg` path.
#[test]
fn mk_poly_unreached_tail_does_not_hang_the_router() {
    let srcs = [
        (
            "graph G { splines=true; a [shape=box, orientation=90]; b:se -- a; }",
            1,
        ),
        (
            "graph G { splines=polyline; a [shape=none, orientation=90, peripheries=2]; b:se -- a; }",
            1,
        ),
        (
            "graph G { splines=true; n0 [shape=record, orientation=45, peripheries=2]; \
             n1 [shape=box, orientation=90]; n1:e -- n1:sw; n1:c -- n0:n; n0:nw -- n0:e; \
             n1:s -- n0:s; n1:c -- n0:se; }",
            5,
        ),
    ];
    for layout in ["neato", "fdp", "sfdp", "circo", "twopi"] {
        for (src, edges) in srcs {
            let mut child = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
                .args(["-Tsvg", &format!("-K{layout}")])
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
                    panic!("{layout}: the router must terminate on {src}");
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let out = child.wait_with_output().unwrap();
            let (svg, err) = (
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
            assert!(out.status.success(), "{layout}: {err}");
            assert_eq!(
                svg.matches("class=\"edge\"").count(),
                edges,
                "{layout}: {svg}"
            );
        }
    }
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

/// Issue #169: with mode=hier, `compute_apsp_packed`'s integer BFS truncates every len
/// below 1 to 0 (`lib/neatogen/bfs.c`), so the stress Laplacian is all zero, or all NaN
/// with levelsgap > 0. Every desired place in `constrained_majorization_new_with_gaps`
/// (`lib/neatogen/quad_prog_solve.c`) is then NaN, the block comes out empty, and C fails
/// the `gv_list_get` bounds assertion. The port indexed the empty block at `len - 1` and
/// panicked; it must lay the graph out.
#[test]
fn neato_hier_zero_laplacian_does_not_panic() {
    for src in [
        "graph{layout=neato\nmode=hier\nb--e[len=0.5]}\n",
        "digraph{layout=neato\nmode=hier\nlevelsgap=1\na->b->c[len=0.5]}\n",
        "graph{layout=neato\nmode=hier\nstart=random5\ndim=3\nb--e[len=0.5]}\n",
    ] {
        let (out, err, ok) = run(&["-Tsvg"], src);
        assert!(ok, "{src}: {err}");
        assert!(out.contains("</svg>"), "{src}: {out}");
        assert!(!out.contains("NaN"), "{src}: {out}");
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

// Issue #114: after `trouble in init_rank` C segfaults in mincross's `furthestnode` (exit
// 139). The port panicked indexing `GD_rank` in `maximal_bbox` with a node of `g -> h`'s
// chain that mincross never placed (its `ND_rank` is still an x coordinate);
// `make_regular_edge` now fails the layout there.
#[test]
fn init_rank_trouble_unplaced_node_fails_the_layout() {
    let src = "digraph G {
rank=sink;
subgraph cluster0 { a; b; c; d }
subgraph cluster_1 { e; subgraph cluster_2 { f } }
subgraph cluster_3 { }
{ d }
{ e }
{ }
f -> e;
g -> h [minlen=2];
d -> c;
i -> h [label=x];
subgraph s { rank=source; a; h; subgraph cluster_in { } }
}
";
    for fmt in ["-Tsvg", "-Tdot"] {
        let (_, err, ok) = run(&["-Kdot", fmt], src);
        assert!(!ok, "{fmt}: {err}");
        assert!(!err.contains("panicked"), "{fmt}: {err}");
        assert!(
            err.starts_with(
                "Warning: a was already in a rankset, deleted from cluster G\n\
                 Warning: c was already in a rankset, deleted from cluster G\n\
                 Warning: d was already in a rankset, deleted from cluster G\n\
                 Warning: e was already in a rankset, deleted from cluster G\n\
                 Error: trouble in init_rank\n\
                 \tc 3\n\
                 \t%0 1\n\
                 \tf 1\n\
                 \ti 1\n\
                 \tg 1\n"
            ),
            "{fmt}: {err}"
        );
        assert!(
            err.ends_with(
                "Error: <stdin>: Layout computation error in dot: make_regular_edge: edge g -> \
                 h reaches a node on rank 112, outside the ranks 0 to 5\n"
            ),
            "{fmt}: {err}"
        );
    }
}

// `trouble in init_rank` alone does not stop C: a node in both a min and a max set is
// reported, then laid out (exit 1). Issue #114's fix must leave this drawing as C's.
#[test]
fn init_rank_trouble_without_lost_nodes_lays_out_like_c() {
    let (out, err, ok) = run(
        &["-Kdot", "-Tdot"],
        "digraph G { {rank=max; a} {rank=min; a} b -> a; }",
    );
    assert!(!ok, "{err}");
    assert_eq!(err, "Error: trouble in init_rank\n\ta 1\n\tb 1\n");
    assert!(out.contains("bb=\"0,0,126,36\""), "{out}");
    assert!(out.contains("pos=\"99,18\""), "{out}");
    assert!(out.contains("pos=\"27,18\""), "{out}");
    assert!(
        out.contains("pos=\"e,71.75,18 54.281,18 56.283,18 58.284,18 60.286,18\""),
        "{out}"
    );
}

// With `splines=false` the same kind of unplaced node is drawn: C's `makeLineEdge` reads no
// rank, so `make_regular_edge` must not check one (issue #114 review). stderr and the SVG
// are C 16.1.0's.
#[test]
fn init_rank_trouble_unplaced_node_with_straight_lines_draws_like_c() {
    let src = "digraph G {
splines=false;
rank=sink;
subgraph cluster0 { a; b; c; d }
subgraph cluster_1 { e; subgraph cluster_2 { f } }
{ d }
{ e }
f -> e;
g -> h [minlen=2];
d -> c;
d -> h [minlen=0];
i -> h [label=x];
subgraph s { rank=source; a; h; subgraph cluster_in { } }
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(!ok, "{err}");
    assert_eq!(
        err,
        "Warning: a was already in a rankset, deleted from cluster G\n\
         Warning: c was already in a rankset, deleted from cluster G\n\
         Warning: d was already in a rankset, deleted from cluster G\n\
         Warning: e was already in a rankset, deleted from cluster G\n\
         Error: trouble in init_rank\n\
         \tc 3\n\
         \t%0 1\n\
         \tf 1\n\
         \ti 1\n\
         \tg 1\n"
    );
    for want in [
        "cx=\"376\" cy=\"-91\"",
        "d=\"M376,-205.51C376,-183.57 376,-146.71 376,-120.75\"",
        "d=\"M127.6,-33.88C149.38,-59.32 192.25,-109.39 217.99,-139.46\"",
        "d=\"M239,-205.6C239,-201.81 239,-197.71 239,-193.62\"",
    ] {
        assert!(out.contains(want), "{want}\n{out}");
    }
}

// Issue #252: C checks two weight sums for overflow, `virtual_weight` (mincross.c:1725) and
// `x_cutval` (ns.c:1060), and exits 1 with its error after writing the graphs before. The
// port fails the layout with C's message, and writes nothing more.
#[test]
fn weight_overflow_fails_the_layout_like_c() {
    for (src, msg, before) in [
        (
            "digraph A { x }\ndigraph B { a -> c [weight=2000000000 minlen=3] }\ndigraph C { y }",
            "overflow when calculating virtual weight of edge",
            "graph 1 0.75 0.5\nnode x 0.375 0.25 0.75 0.5 x solid ellipse black lightgrey\nstop\n",
        ),
        (
            "digraph { a -> b [weight=2000000000]; a -> c [weight=2000000000]; \
             b -> d [weight=2000000000]; c -> d [weight=2000000000] }",
            "overflow when computing edge weight sum",
            "",
        ),
    ] {
        let (out, err, ok) = run(&["-Tplain"], src);
        assert!(!ok, "{src}");
        assert_eq!(
            err,
            format!("Error: <stdin>: Layout computation error in dot: {msg}\n")
        );
        assert_eq!(out, before, "{src}");
    }
}

// Weights that wrap negative where C has no check leave a tree edge with a negative cut value
// and no edge to replace it. C passes the NULL edge to `update` and crashes (ns.c:710); the
// port fails the layout there. Before, it went on and panicked in mincross (with a cluster)
// or class2 (with newrank).
#[test]
fn wrapped_weights_without_an_entering_edge_fail_the_layout() {
    for src in [
        "digraph { subgraph cluster0 { n0 } n0 -> n2 [weight=1073741824]; \
         n0 -> n0 [weight=268435456]; n1 -> n2 [weight=268435456 minlen=0]; \
         n1 -> n0 [weight=2000000000]; n0 -> n1 [weight=0] }",
        "digraph { newrank=true; n3 -> n2 [weight=1000 label=x]; \
         n5 -> n1 [weight=100000000 minlen=2]; n0 -> n2 [weight=1073741824]; \
         n0 -> n2 [weight=1000 minlen=3]; n0 -> n1 [weight=2000000000]; \
         n5 -> n1 [weight=2147483647] }",
    ] {
        let (out, err, ok) = run(&["-Tplain"], src);
        assert!(!ok, "{src}");
        assert_eq!(out, "", "{src}");
        assert_eq!(
            err,
            "Error: <stdin>: Layout computation error in dot: network simplex: no edge can \
             enter the tree for a tree edge with a negative cut value\n",
            "{src}"
        );
    }
}

// C's `largeMinlen` (position.c:64) reports an aux edge longer than INT_MAX before clamping
// it, and the error makes dot exit 1 after drawing.
#[test]
fn overlong_aux_edge_is_reported_like_c() {
    let (out, err, ok) = run(&["-Tplain"], "graph { nodesep=\"1e8\"; a; b }");
    assert!(!ok);
    assert_eq!(
        err,
        "Error: Edge length 2147483701.000000 larger than maximum 2147483647 allowed.\n\
         Check for overwide node(s).\n"
    );
    assert_eq!(
        out,
        "graph 1 2.9826e+07 0.5\nnode a 0.375 0.25 0.75 0.5 a solid ellipse black lightgrey\n\
         node b 2.9826e+07 0.25 0.75 0.5 b solid ellipse black lightgrey\nstop\n"
    );
}

// Issue #271: C's `int` sums on huge `ranksep` (`edgelabel_ranks`), `nodesep` and `margin`
// (network simplex's `init_rank` and `init_graph`) have no check; the reference build wraps
// and draws. The port wraps too, instead of panicking with overflow checks. Its output and
// stderr are C 16.1.0's.
#[test]
fn huge_rank_and_node_separations_wrap_like_c() {
    let (out, err, ok) = run(
        &["-Tplain"],
        "digraph { ranksep=\"2147483647\"; a -> b [label=x] }",
    );
    assert!(ok, "{err}");
    assert_eq!(
        out,
        "graph 1 0.75 1.4556\nnode a 0.375 1.2056 0.75 0.5 a solid ellipse black lightgrey\n\
         node b 0.375 0.25 0.75 0.5 b solid ellipse black lightgrey\n\
         edge a b 4 0.375 0.95274 0.375 0.86469 0.375 0.76237 0.375 0.66396 x 0.42361 0.72778 \
         solid black\nstop\n"
    );
    let line = "Error: Edge length 2147483674.000000 larger than maximum 2147483647 allowed.\n\
                Check for overwide node(s).\n";
    let (out, err, ok) = run(
        &["-Tplain"],
        "digraph { margin=\"2147483647\"; subgraph cluster_x { a } b }",
    );
    assert!(!ok);
    assert_eq!(err, line.repeat(3));
    assert_eq!(
        out,
        "graph 1 2.9826e+07 5.9652e+07\n\
         node a 0.375 2.9826e+07 0.75 0.5 a solid ellipse black lightgrey\n\
         node b 1.375 2.9826e+07 0.75 0.5 b solid ellipse black lightgrey\nstop\n"
    );
    // Two INT_MAX minimum lengths in a chain wrap the ranks; C fails `assert(Maxrank >= 0)`
    // in `TB_balance` (ns.c:823), and the port fails the layout there.
    let (out, err, ok) = run(
        &["-Tplain"],
        "digraph { a -> b [minlen=2147483647]; b -> c [minlen=2147483647] }",
    );
    assert!(!ok);
    assert_eq!(out, "");
    assert_eq!(
        err,
        "Error: <stdin>: Layout computation error in dot: network simplex: the ranks \
         overflowed, the highest rank is below the lowest\n"
    );
}

// Issue #233: inside cluster2, `rank=sink` and the nested `rank=min` set contradict each
// other, network simplex prints `trouble in init_rank` and leaves no node of the cluster on
// rank 0. C fails `assert(leader != NULL)` in `cluster_leader` (rank.c:316, exit 134). The
// port fails the layout where the cluster's ranking failed, for a top-level cluster and
// for a nested one.
#[test]
fn cluster_ranking_trouble_fails_the_layout() {
    for src in [
        "digraph G { subgraph cluster2 { rank=sink; k; f; { a } { rank=min; a; j } } \
         j -> f; f -> k; }",
        "digraph G { subgraph cluster1 { subgraph cluster2 { rank=sink; k; f; { a } \
         { rank=min; a; j } } } j -> f; f -> k; }",
    ] {
        let (_, err, ok) = run(&["-Kdot", "-Tsvg"], src);
        assert!(!ok, "{src}: {err}");
        assert_eq!(
            err,
            "Error: trouble in init_rank\n\tk 1\n\ta 1\n\tf 1\n\
             Error: <stdin>: Layout computation error in dot: cluster_leader: the ranking of \
             cluster cluster2 failed: none of its nodes is on rank 0\n",
            "{src}"
        );
    }
}

// Issue #232, input A: in the root's `build_ranks`, a later component's cluster rank
// leaders are installed past the end of the rank array. C's `install_in_rank` reports it
// (mincross.c:1190), `dot_layout` abandons the layout and the SVG has the nodes and no edge
// paths (exit 1). stderr and SVG are C 16.1.0's.
#[test]
fn install_in_rank_past_the_root_slots_abandons_the_layout_like_c() {
    let src = "digraph G {
rank=sink;
subgraph cluster0 { a; b; c; d }
subgraph cluster_1 { e; f }
{ d }
subgraph cluster9 { g; i }
{ e }
{ i }
f -> e;
g -> h;
i -> g;
d -> c;
i -> h [label=x];
{ rank=source; a; h }
}
";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(!ok, "{err}");
    assert_eq!(
        err,
        "Warning: a was already in a rankset, deleted from cluster G\n\
         Warning: c was already in a rankset, deleted from cluster G\n\
         Warning: d was already in a rankset, deleted from cluster G\n\
         Warning: e was already in a rankset, deleted from cluster G\n\
         Warning: i was already in a rankset, deleted from cluster G\n\
         Error: trouble in init_rank\n\
         \tc 2\n\t%0 1\n\tf 1\n\t%0 1\n\tg 2\n\t%0 1\n\
         Error: install_in_rank, line 1190: GD_rank(g)[2].v + ND_order(%0) [0] > \
         GD_rank(g)[2].av + GD_rank(Root)[2].an [2]\n"
    );
    assert!(
        out.contains("<ellipse fill=\"none\" stroke=\"black\" cx=\"1\" cy=\"0\""),
        "{out}"
    );
    assert!(!out.contains("<path"), "{out}");
}

// Issue #232, input B and a cluster route: a rank gets more nodes than it has slots. C
// stores past the array and fails `assert(GD_rank(g)[r].n <= GD_rank(g)[r].an)`
// (mincross.c:1165, exit 134); the port fails the layout before the store.
#[test]
fn install_in_rank_on_a_full_rank_fails_the_layout() {
    let root = "digraph G {
rank=sink;
subgraph cluster0 { a; b; c; d }
b -> f;
subgraph cluster_1 { e; f }
{ d }
{ e }
k -> a;
f -> e;
d -> c;
i -> h [label=x];
}
";
    let clust = "digraph G { subgraph cluster6 { b e c d subgraph cluster73 { e subgraph \
                 cluster69 { e { rank=source; a } } } subgraph cluster77 { b { rank=min; a } } \
                 } { rank=sink; b a c } c -> d [label=x]; }";
    for (src, want) in [
        (
            root,
            "Warning: c was already in a rankset, deleted from cluster G\n\
             Warning: d was already in a rankset, deleted from cluster G\n\
             Warning: e was already in a rankset, deleted from cluster G\n\
             Error: <stdin>: Layout computation error in dot: install_in_rank: rank 4 of G is \
             full (2 allocated), no slot for %0\n",
        ),
        (
            clust,
            "Warning: b was already in a rankset, deleted from cluster G\n\
             Warning: c was already in a rankset, deleted from cluster G\n\
             Warning: a was already in a rankset, deleted from cluster G\n\
             Error: <stdin>: Layout computation error in dot: install_in_rank: rank 0 of \
             cluster6 is full (1 allocated), no slot for %0\n",
        ),
    ] {
        let (_, err, ok) = run(&["-Kdot", "-Tsvg"], src);
        assert!(!ok, "{src}: {err}");
        assert_eq!(err, want, "{src}");
    }
}

// Issue #231: a cluster below the root loses all its nodes after it was registered: to an
// earlier sibling cluster (newrank's `set_parent` registers it before `node_induce`), or to
// a rank set, through `mark_clusters` at its parent's level (the second input empties
// `cluster4` by taking `b` from `cluster1`; the third panicked in `clust_ht`). C keeps the
// cluster, and `expand_cluster` -> `build_ranks` dereferences its NULL node list
// (mincross.c:1227, exit 139). The port fails the layout there.
#[test]
fn cluster_emptied_by_membership_rules_fails_the_layout() {
    let err_for = |c: &str| {
        format!(
            "Error: <stdin>: Layout computation error in dot: expand_cluster: cluster {c} has \
             no nodes left (a rank set or an earlier cluster took them)\n"
        )
    };
    for (src, want) in [
        (
            "digraph G { newrank=true; subgraph cluster1 { subgraph cluster2 { c } \
             subgraph cluster4 { c } } }",
            err_for("cluster4"),
        ),
        (
            "digraph G { concentrate=true; subgraph cluster1 { c; subgraph cluster4 { b } } \
             { rank=same; b } b -> d [minlen=3]; }",
            "Warning: b was already in a rankset, deleted from cluster G\n".to_string()
                + &err_for("cluster4"),
        ),
        (
            "digraph G { concentrate=true; subgraph cluster39 { e subgraph cluster10 { h d a b \
             { rank=same; c d f e } subgraph cluster73 { c { rank=max; a } { rank=min; a b } } \
             { rank=source; a f b d } } subgraph cluster29 { g f subgraph cluster95 { f e a g \
             { rank=source; a g h } } { g f h } } } g -> e; f -> e [label=x,constraint=false,\
             weight=0]; }",
            "Warning: a was already in a rankset, deleted from cluster cluster10\n\
             Warning: b was already in a rankset, deleted from cluster cluster10\n"
                .to_string()
                + &err_for("cluster73"),
        ),
        (
            "digraph G { rank=max; subgraph cluster47 { rank=sink; subgraph cluster75 { k; } \
             { rank=sink; f k } } { rank=source; d c } a; { rank=source; c } b -> j; }",
            "Warning: k was already in a rankset, deleted from cluster cluster47\n".to_string()
                + &err_for("cluster75"),
        ),
    ] {
        let (_, err, ok) = run(&["-Kdot", "-Tsvg"], src);
        assert!(!ok, "{src}: {err}");
        assert_eq!(err, want, "{src}");
    }
}

// A cluster keeps its rank range when `mark_clusters` takes the nodes of one of its ranks
// for a rank set. On that rank `save_vlist` saves the root's node just right of the
// cluster. C goes on with that node. Here it is `b` itself, so `cluster0`'s box reaches
// down to `b`'s rank. The port draws the same.
#[test]
fn cluster_rank_emptied_by_a_rank_set_draws_like_c() {
    let (out, err, ok) = run(
        &["-Kdot", "-Tsvg"],
        "digraph { subgraph cluster0 { a -> b } {rank=same; b; x} }",
    );
    assert!(ok, "{err}");
    assert_eq!(
        err,
        "Warning: b was already in a rankset, deleted from cluster %1\n"
    );
    assert!(
        out.contains("points=\"8,-18 8,-116 78,-116 78,-18 8,-18\""),
        "{out}"
    );
}

// A rank of `cluster50` loses its node `g` to the rank set, and `save_vlist` saves for it
// the rank leader of `cluster38`, which `cluster38`'s expansion then frees. C reads the
// freed leader in `rec_reset_vlists` -> `furthestnode`. Here it segfaults; on other inputs
// it finishes, with output that is the same on every run, so the port does not stop there
// (see docs/porting/crashes.md). It must not panic.
#[test]
fn cluster_rank_saved_leader_freed_does_not_panic() {
    let src = "digraph G { subgraph cluster50 { c -> g; } { rank=min; g } subgraph cluster38 { \
               j -> a; b -> k; } f -> b; e -> i; k -> h; }";
    for fmt in ["-Tsvg", "-Tdot"] {
        let (_, err, _) = run(&["-Kdot", fmt], src);
        assert!(!err.contains("panicked"), "{fmt}: {err}");
        assert!(
            err.starts_with("Warning: g was already in a rankset, deleted from cluster G\n"),
            "{fmt}: {err}"
        );
    }
}

// The `d` attributes of an SVG's paths, in order.
fn svg_path_data(svg: &str) -> Vec<&str> {
    svg.split(" d=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .collect()
}

// Issue #230: a labelled flat edge on rank 0 that is not its class's representative (a
// parallel `a -> b` behind an unlabelled one, or a `b -> a` merged by `flat_rev`) sits in
// `ND_other`. C's `flat_edges` makes rank -1 only for `ND_flat_in` edges, then `flat_node`
// reads `GD_rank(g)[-1]` and segfaults. The port makes rank -1 for these edges too. The
// layout then fails as C's does for the same edges on a lower rank (the `z -> a` input,
// whose stderr and paths are C 16.1.0's).
#[test]
fn labelled_flat_multi_edge_on_rank_0_gets_a_label_rank() {
    let lost = "Error: in routesplines, illegal values of prev 1 and next 1, line 378\n\
                Error: lost a b edge\n";
    for (src, want) in [
        (
            "{ rank=same; a; b; c } a -> b; a -> b [label=x]; a -> c;",
            lost,
        ),
        (
            "rankdir=LR; { rank=same; a; b; c } a -> b; a -> b [label=x]; a -> c;",
            lost,
        ),
        (
            "ordering=out; { rank=same; a; b; c } a -> b; a -> b [label=x]; a -> c;",
            lost,
        ),
        (
            "newrank=true; rank=sink; a -> b; a -> b [label=x]; a -> c;",
            lost,
        ),
        (
            "{ rank=same; a; b; c } a -> b; b -> a [label=x]; a -> c;",
            "Error: in routesplines, illegal values of prev 1 and next 1, line 378\n\
             Error: lost b a edge\n",
        ),
        (
            "subgraph cluster0 { { rank=same; a; b; c } } a -> b; a -> b [label=x]; a -> c;",
            "",
        ),
    ] {
        let src = format!("digraph G {{ {src} }}");
        let (out, err, ok) = run(&["-Kdot", "-Tsvg"], &src);
        assert_eq!(err, want, "{src}");
        assert_eq!(ok, want.is_empty(), "{src}");
        assert!(!ok || out.contains(">x</text>"), "{src}: {out}");
    }
    let src = "digraph G { z -> a; { rank=same; a; b; c } a -> b; a -> b [label=x]; a -> c; }";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(!ok);
    assert_eq!(err, lost);
    assert_eq!(
        svg_path_data(&out),
        [
            "M27,-88.65C27,-76.96 27,-61.19 27,-47.62",
            "M42.58,-33.16C54.48,-43.63 71.89,-56.66 90,-62.4 112.88,-69.65 121.8,-71.51 144,\
             -62.4 154.14,-58.24 163.22,-50.57 170.52,-42.76",
            "M54.42,-18C62.4,-18 70.37,-18 78.35,-18",
        ]
    );
}

// Issue #241: after `trouble in init_rank` nothing normalised the ranks, and the top rank
// is 1, not 0. `flat_edges` tested rank 0 for labelled flat edges, made no rank above the
// top one, and `flat_node` read a rank that was never allocated (C segfaults there). The
// port runs the test on the top rank, and the label is drawn.
#[test]
fn labelled_flat_edge_on_an_unnormalised_top_rank_gets_a_label_rank() {
    let src = "digraph G { { rank=min; b a e } { rank=max; e } a -> b [label=x,weight=0]; \
               c -> d; a -> c; d -> b; }";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(!ok);
    assert_eq!(err, "Error: trouble in init_rank\n\tb 1\n\td 2\n\tc 1\n");
    assert!(out.contains(">x</text>"), "{out}");
    // C draws a labelled flat edge on a lower rank when the top rank is 2; stderr and
    // paths are C 16.1.0's.
    let src = "digraph G { { rank=min; b a e } { rank=max; e } c -> d; a -> c; d -> b; \
               { rank=same; c x y } c -> y [label=z]; }";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(!ok);
    assert_eq!(err, "Error: trouble in init_rank\n\tb 1\n\td 2\n\tc 1\n");
    assert_eq!(
        svg_path_data(&out),
        [
            "M173,-75.97C161.46,-65.86 145.67,-52.02 132.55,-40.52",
            "M92.03,-33C80.5,-43.1 64.72,-56.94 51.58,-68.45",
            "M135.49,-18C142.99,-18 151.27,-18 159.27,-18",
            "M54.42,-91C62.4,-91 70.37,-91 78.35,-91",
        ]
    );
}

// Issue #247: with `concentrate=true`, `arrow_flags` gives an edge with `ED_conc_opp_flag`
// the arrowheads of an opposing edge, found by `agfindedge`. Among several opposing edges
// cgraph's splay tree returns the first on its search path, not the first created. The port
// took the first created, and lost an arrowhead and its clip, or drew the wrong type. Each
// input after the first two needs one more of the searches that shape the tree: the
// parser's search before each insert, `agcontains` in mincross (`ordering=out`), and
// `cccomps` (`pack`). In the last, a `key` names an edge, whose id sorts after every
// anonymous one. The expected lines are C 16.1.0's.
#[test]
fn concentrated_edge_takes_the_arrowheads_of_the_opposing_edge_cgraph_finds() {
    for (src, want) in [
        (
            "d -> f [tailport=sw,dir=back]; g -> d [tailport=n,minlen=2,dir=back]; \
             g -> d [headport=s]; g -> d; d -> g [minlen=4];",
            &[
                "<path fill=\"none\" stroke=\"black\" d=\"M101.76,-206.18C102.36,-188.74 \
                 103.37,-166.07 105,-146 107.77,-112 112.62,-73.06 116.05,-47.36\"/>",
                "<polygon fill=\"black\" stroke=\"black\" points=\"98.27,-205.96 101.46,-216.07 \
                 105.27,-206.18 98.27,-205.96\"/>",
            ][..],
        ),
        (
            "d -> g; g -> d [arrowhead=dot]; g -> d [arrowhead=box];",
            &[
                "<path fill=\"none\" stroke=\"black\" d=\"M27,-61.38C27,-56.85 27,-52.14 \
                 27,-47.59\"/>",
                "<polygon fill=\"black\" stroke=\"black\" points=\"31,-71.2 23,-71.2 23,-63.2 \
                 31,-63.2 31,-71.2\"/>",
            ][..],
        ),
        (
            "subgraph cluster1 { h; c } subgraph cluster2 { a label=C2; } b -> d; \
             b -> d [dir=back]; d -> b; a -> d; b -> d; c -> d;",
            &[
                "<path fill=\"none\" stroke=\"black\" d=\"M154,-71.7C154,-64.41 154,-55.73 \
               154,-47.54\"/>",
            ][..],
        ),
        (
            "ordering=out; g -> h; h -> b; b -> h [dir=back]; b -> h [tailport=se]; \
             g -> h [label=x];",
            &[
                "<path fill=\"none\" stroke=\"black\" d=\"M31.2,-62.22C30.73,-57.48 \
                 30.25,-52.58 29.78,-47.85\"/>",
                "<polygon fill=\"black\" stroke=\"black\" points=\"27.69,-62.34 32.16,-71.95 \
                 34.66,-61.65 27.69,-62.34\"/>",
            ][..],
        ),
        (
            "pack=true; subgraph cluster0 { d } b -> e; e -> b; b -> e [dir=none]; d -> e;",
            &[
                "<path fill=\"none\" stroke=\"black\" d=\"M43,-71.7C43,-64.41 43,-55.73 \
               43,-47.54\"/>",
            ][..],
        ),
        (
            "a -> c; c -> a [key=zz]; c -> a [arrowhead=dot]; b -> a;",
            &[
                "<path fill=\"none\" stroke=\"black\" d=\"M27,-60.24C27,-56.01 27,-51.66 \
                 27,-47.44\"/>",
                "<polygon fill=\"black\" stroke=\"black\" points=\"23.5,-60.18 27,-70.18 \
                 30.5,-60.18 23.5,-60.18\"/>",
            ][..],
        ),
        // `edge [key=d]` names nothing (`bindattrs` skips `key`), and a key starting with
        // `%` leaves the edge anonymous (`agmapnametoid`).
        (
            "a -> c; subgraph s { edge [key=d]; c -> a [arrowhead=inv]; } \
             c -> a [arrowhead=dot]; b -> a;",
            &[
                "<path fill=\"none\" stroke=\"black\" d=\"M27,-62.8C27,-57.78 27,-52.5 \
                 27,-47.41\"/>",
                "<ellipse fill=\"black\" stroke=\"black\" cx=\"27\" cy=\"-67.2\" rx=\"4\" \
                 ry=\"4\"/>",
            ][..],
        ),
        (
            "a -> c; c -> a [key=\"%zz\"]; c -> a [arrowhead=dot]; b -> a;",
            &[
                "<path fill=\"none\" stroke=\"black\" d=\"M27,-62.8C27,-57.78 27,-52.5 \
                 27,-47.41\"/>",
                "<ellipse fill=\"black\" stroke=\"black\" cx=\"27\" cy=\"-67.2\" rx=\"4\" \
                 ry=\"4\"/>",
            ][..],
        ),
    ] {
        let src = format!("digraph G {{ concentrate=true; {src} }}");
        let (out, err, ok) = run(&["-Kdot", "-Tsvg"], &src);
        assert!(ok, "{src}: {err}");
        for w in want {
            assert!(out.contains(w), "{src}: {w}\n{out}");
        }
    }
    // Undirected: `dir=both` draws both arrowheads of the named edges' group.
    let src = "graph G { concentrate=true; b -- a [key=k3]; a -- b [arrowtail=inv,dir=none]; \
               a -- b [key=k1,dir=both]; }";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    for w in [
        "<path fill=\"none\" stroke=\"black\" d=\"M27,-60.24C27,-56.01 27,-51.66 27,-47.44\"/>",
        "<polygon fill=\"black\" stroke=\"black\" points=\"30.5,-47.62 27,-37.62 23.5,-47.62 \
         30.5,-47.62\"/>",
    ] {
        assert!(out.contains(w), "{w}\n{out}");
    }
    // `-Ekey=q` gives every edge a `key` attribute, but names none of them.
    let src = "digraph G { concentrate=true; a -> c; c -> a [key=zz]; c -> a [arrowhead=dot]; \
               b -> a; }";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg", "-Ekey=q"], src);
    assert!(ok, "{err}");
    assert!(
        out.contains(
            "<polygon fill=\"black\" stroke=\"black\" points=\"23.5,-60.18 27,-70.18 \
             30.5,-60.18 23.5,-60.18\"/>"
        ),
        "{out}"
    );
}

// Issue #242: `rebuild_vlists` warns when concentration leaves a cluster no node of its own
// on a rank (`conc.c:187`). stderr is C 16.1.0's.
#[test]
fn concentrate_warns_about_a_degenerate_cluster_rank() {
    let src = "digraph G { rank=same; concentrate=true; compound=true; \
               subgraph cluster31 { d f } { rank=same; e } e -> f; \
               e -> a [minlen=1,constraint=false]; e -> f [weight=0]; e -> f; \
               e -> d [constraint=false]; d -> a [label=x]; e -> f; d -> f; }";
    let (_, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok);
    assert_eq!(err, "Warning: degenerate concentrated rank cluster31,1\n");
}

// Issue #234: with `concentrate=true`, a cluster edge with other ports shares its chain
// (`interclrep`) and is routed alone through `make_regular_edge`'s `hackflag`, which follows
// the chain as it was before `dot_concentrate` rewired it. Where the walk stops at a
// concentrator, C ends the path at the real head with the concentrator's merge flag
// (`dotsplines.c:1845`), and `conc_slope` on a head with no out-edges segfaults. Where
// `mergevirtual` deleted the chain's node, C walks from a node with no edges and segfaults
// in `straight_len` (the last input but one). The last input gives the head an out-edge: C
// draws it, and the port's paths are C 16.1.0's.
#[test]
fn concentrated_cluster_edge_routed_alone_does_not_panic() {
    for src in [
        "subgraph cluster1 { c } a -> c [minlen=2]; a -> c [tailport=s]; b -> c [constraint=false];",
        "rankdir=LR; subgraph cluster1 { c } a -> c [minlen=2]; a -> c [tailport=s]; \
         b -> c [constraint=false];",
        "newrank=true; subgraph cluster1 { c } a -> c [minlen=2]; a -> c [tailport=s]; \
         b -> c [constraint=false];",
        "ordering=out; subgraph cluster1 { c } a -> c [minlen=2]; a -> c [tailport=s]; \
         b -> c [constraint=false];",
        "subgraph cluster1 { c } a -> c [minlen=2]; a -> c [tailport=s]; b -> c [minlen=2];",
        "subgraph cluster1 { a } a -> c [minlen=2]; a -> c [tailport=s]; a -> d [minlen=2];",
        "subgraph cluster0 { c } a -> c [tailport=s,minlen=2]; a -> c; a -> c;",
    ] {
        let src = format!("digraph G {{ concentrate=true; {src} }}");
        let (out, err, ok) = run(&["-Kdot", "-Tsvg"], &src);
        assert!(ok, "{src}: {err}");
        assert_eq!(err, "", "{src}");
        assert!(out.contains("<title>a:s&#45;&gt;c</title>"), "{src}: {out}");
    }
    // The deleted-node route draws both edge groups from `a` to `c`, with arrowheads.
    let src = "digraph G { concentrate=true; subgraph cluster0 { c } \
               a -> c [tailport=s,minlen=2]; a -> c; a -> c; }";
    let (out, _, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok);
    assert_eq!(
        svg_path_data(&out),
        [
            "M43,-125C43,-104.43 43,-81.22 43,-63.53",
            "M43,-124.81C43,-108.27 43,-82.98 43,-63.44",
        ]
    );
    assert_eq!(out.matches("<polygon fill=\"black\"").count(), 2, "{out}");
    let src = "digraph G { concentrate=true; subgraph cluster1 { c } a -> c [minlen=2]; \
               a -> c [tailport=s]; b -> c [constraint=false]; c -> d; }";
    let (out, err, ok) = run(&["-Kdot", "-Tsvg"], src);
    assert!(ok, "{err}");
    assert_eq!(
        svg_path_data(&out),
        [
            "M43,-71.7C43,-64.41 43,-55.73 43,-47.54",
            "M45.46,-180.93C46.39,-170.36 46.49,-156.63 43,-145",
            "M43,-144C40.65,-136.18 39.93,-127.42 39.99,-119.31",
            "M43,-181C43,-152.52 43,-139.25 43,-119.54",
            "M94.84,-186.89C74.83,-175.29 46.64,-157.11 43,-145",
        ]
    );
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

// Issue #170 (cargo-fuzz render_circo): a route segment that lies in no channel of the
// ortho maze. C's chanSearch (lib/ortho/ortho.c:388/390) fails `assert (chani)` or
// `assert (cp)` (here `cp`) and aborts (exit 134, no output). The port used to panic on the
// same lookup ("chanSearch: no channel", exit 101); it now abandons ortho routing, as C's
// orthoEdges does when shortPath or assignTracks fails, and the edges are drawn as straight
// lines.
const ISSUE170_GRAPH: &str = "graph{layout=circo
oneblock=true
splines=ortho
t5--t12--t13
t6--t14}
";

#[test]
fn circo_ortho_segment_in_no_channel_does_not_panic_chan_search() {
    let (out, err, ok) = run(&["-Tsvg"], ISSUE170_GRAPH);
    assert!(
        ok,
        "C crashes here, the port falls back to straight lines: {err}"
    );
    assert_eq!(
        err,
        "Warning: orthogonal routing found no channel for an edge segment - \
         edges not routed orthogonally\n"
    );
    assert_eq!(out.matches("class=\"edge\"").count(), 3, "{out}");
    // Straight edges: each path is a single cubic, not an ortho polyline.
    let paths: Vec<&str> = out
        .split("<path fill=\"none\" stroke=\"black\" d=\"")
        .skip(1)
        .map(|p| p.split('"').next().unwrap())
        .collect();
    assert_eq!(paths.len(), 3, "{out}");
    for d in paths {
        assert_eq!(d.matches('C').count(), 1, "{d}");
    }
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

// Issue #199, reduced from `tests/golden/sfdp/attr_label_scheme_4_overlap.dot`. sfdp throws
// the nodes out to ~1e143 (C too), and a segment's trapezoid then has no upper neighbour in
// `lib/ortho/trapezoid.c:update_trapezoid`. C reads `LIST_GET(tr, SIZE_MAX)`: depending on
// the run it fails the `gv_list_get_` bounds assertion (exit 134) or goes on to fail
// `add_segment` ("trapezoid segment construction failed") and draws the edges straight. The
// port panicked; now `add_segment` fails and the edges fall back to straight lines.
#[test]
fn ortho_trapezoid_without_upper_neighbour_does_not_panic() {
    let src = "graph {
	label_scheme=4 layout=sfdp overlap=false splines=ortho
	a -- \"|edgelabel|ab\"
	\"|edgelabel|ab\" -- b
	b -- \"|edgelabel|bc\"
	\"|edgelabel|bc\" -- c
	\"|edgelabel|ca\" [width=1.6798]
	c -- \"|edgelabel|ca\"
	\"|edgelabel|ca\" -- a
	e
	d -- e
	d -- \"|edgelabel|da\"
	e -- c
	\"|edgelabel|da\" -- a
	f -- g
	g -- \"|edgelabel|ga\"
	\"|edgelabel|ga\" -- a
}
";
    let (out, err, ok) = run(&["-Tdot"], src);
    assert!(ok && !err.contains("panicked"), "{err}");
    let edges: Vec<&str> = out.lines().filter(|l| l.contains(" -- ")).collect();
    assert_eq!(edges.len(), 13, "{out}");
    for e in edges {
        let pos = e.split("pos=\"").nth(1).unwrap().trim_end_matches("\"];");
        assert_eq!(pos.split(' ').count(), 4, "a straight line: {e}");
    }
    // Only the fallback draws a diagonal; an ortho route is axis-aligned.
    let ab = out
        .lines()
        .find(|l| l.contains("a -- \"|edgelabel|ab\""))
        .unwrap();
    let pts: Vec<Vec<&str>> = ab
        .split("pos=\"")
        .nth(1)
        .unwrap()
        .trim_end_matches("\"];")
        .split(' ')
        .map(|p| p.split(',').collect())
        .collect();
    assert!(
        pts[0][0] != pts[3][0] && pts[0][1] != pts[3][1],
        "a diagonal: {ab}"
    );
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

/// A byte-order mark is ignored wherever it stands alone (scan.l:125), and is part of a name
/// when a letter or digit follows it: `LETTER` holds \200-\377, so `{NAME}` is the longer
/// match (#117).
#[test]
fn byte_order_mark_follows_scan_l() {
    let (out, err, ok) = run(&["-Tcanon"], "digraph { \u{feff} a \u{feff}}");
    assert!(ok, "{err}");
    assert_eq!(out, "digraph {\n\tnode [label=\"\\N\"];\n\ta;\n}\n");
    let (_, err, ok) = run(&["-Tcanon"], "\u{feff}digraph { a }");
    assert!(!ok);
    assert_eq!(
        err,
        "Error: <stdin>: syntax error in line 1 near '\u{feff}digraph'\n"
    );
}

/// cgraph reads its input with `fgets` and `strlen` (`lib/cgraph/io.c:24`), so a NUL hides the
/// rest of its line, newline included, and a line that starts with a NUL ends the input (#120).
#[test]
fn nul_hides_the_rest_of_its_line() {
    let (_, err, _) = run(&["-Tcanon"], "digraph { a \0 b -> ; }\n c -> ;");
    assert_eq!(err, "Error: <stdin>: syntax error in line 1 near ';'\n");
    let (_, err, _) = run(&["-Tcanon"], "digraph { \"x\0y\" -> ; }");
    assert_eq!(
        err,
        "Error: <stdin>: syntax error in line 1 scanning a quoted string (missing endquote? \
         longer than 16384?)\nString starting:\"x\n"
    );
    // The next graph is read from the line after the closing brace, as fgets reads it.
    let (out, err, ok) = run(
        &["-Tcanon"],
        "digraph{a\0junk\n}\ndigraph{b}\n\0\ndigraph{c}\n",
    );
    assert!(ok, "{err}");
    assert_eq!(
        out,
        "digraph {\n\tnode [label=\"\\N\"];\n\ta;\n}\ndigraph {\n\tnode [label=\"\\N\"];\n\tb;\n}\n"
    );
}

/// The scanner reads a token when the grammar asks for one, as bison calls `yylex`: once a
/// graph is open (`startgraph`), nothing after a syntax error is scanned (`endgraph` pushes
/// EOF), and each graph's tokens are scanned once. Before the header, bison's error recovery
/// discards tokens to the end of the input, so the scanner warns about all of them. C prints
/// those warnings after the error; the port prints the error last (#278).
#[test]
fn scanner_warnings_come_from_the_tokens_c_reads() {
    let (_, err, _) = run(&["-Tcanon"], "digraph { a -> ; 2x }\n3y\n");
    assert_eq!(err, "Error: <stdin>: syntax error in line 1 near ';'\n");
    let (_, err, ok) = run(&["-Tcanon"], "digraph { a }\ndigraph { 2x }\n");
    assert!(ok);
    assert_eq!(
        err,
        "Warning: syntax ambiguity - badly delimited number '2x' in line 1 of <stdin> splits \
         into two tokens\n"
    );
    // `@` is scan.l's end of input: recovery stops there.
    let (_, err, _) = run(&["-Tcanon"], "strict @ 2x\n");
    assert_eq!(err, "Error: <stdin>: syntax error in line 1 near '@'\n");
    let (_, err, _) = run(&["-Tcanon"], "; 2x\n3y\n");
    let mut lines: Vec<_> = err.lines().collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        [
            "Error: <stdin>: syntax error in line 1 near ';'",
            "Warning: syntax ambiguity - badly delimited number '2x' in line 1 of <stdin> splits \
             into two tokens",
            "Warning: syntax ambiguity - badly delimited number '3y' in line 2 of <stdin> splits \
             into two tokens",
        ]
    );
}

/// C: `Warning: syntax ambiguity - badly delimited number '1a' in line 2 of <stdin> splits
/// into two tokens` before the error (scan.l `chkNum`), which splits the token in two (#97).
#[test]
fn badly_delimited_number_warns() {
    let (_, err, _) = run(&["-Tsvg"], "digraph G {\n a -> b -> 1a ; c -> ;\n}\n");
    assert_eq!(
        err,
        "Warning: syntax ambiguity - badly delimited number '1a' in line 2 of <stdin> splits \
         into two tokens\nError: <stdin>: syntax error in line 2 near ';'\n"
    );
    // A line directive's file name, and a second `.`, as `twoDots` sees it.
    let (out, err, ok) = run(&["-Tcanon"], "# 7 \"foo.gv\"\ndigraph { 1.2.3 }\n");
    assert!(ok, "{err}");
    assert_eq!(
        err,
        "Warning: syntax ambiguity - badly delimited number '1.2.' in line 7 of foo.gv splits \
         into two tokens\n"
    );
    assert!(out.contains("\t1.2;\n\t.3;\n"), "{out}");
}

/// C: a backslash before CR LF in a quoted string is kept with the CR and LF (scan.l:140
/// ignores only a backslash before LF; rule 142 keeps the backslash) (#112).
#[test]
fn backslash_crlf_in_quoted_string_is_kept() {
    let (out, err, ok) = run(&["-Tdot"], "digraph G { a [label=\"x\\\r\ny\"] }");
    assert!(ok, "{err}");
    assert!(out.contains("label=\"x\\\r\ny\""), "{out}");
}

// Fuzz case for issue #171, verbatim. osage puts `i1ortho` at x=inf (C does too: the
// repeated `cluster_inner4`, see #173). C overruns `vpos[4]` in
// `lib/ortho/partition.c:make_new_monotone_poly` and then fails `assert(np->cells[1])` in
// `lib/ortho/maze.c:chkSgraph` (exit 134); the port panicked with an index out of bounds in
// the partition. Now the maze fails at `chkSgraph`, and the edges fall back to straight lines
// (`lib/neatogen/neatosplines.c:spline_edges_`).
#[test]
fn ortho_partition_with_non_finite_node_does_not_panic() {
    let src = "digraph{layout=osage
splines=ortho{label=\"inner\"{o2{i1ortho
cluster_outer{o1}subgraph cluster_inner4{j1 j2}}subgrapa}subgraph cluster_inner4{j2}}x i2 i3}
";
    let (_, err, ok) = run(&["-Tsvg"], src);
    assert!(ok && !err.contains("panicked"), "{err}");

    let src = src.replace("x i2 i3}", "x->i2 i3}");
    let (out, err, ok) = run(&["-Tdot"], &src);
    assert!(ok && !err.contains("panicked"), "{err}");
    assert!(
        out.contains("x -> i2\t[pos=\"e,153.78,58 85.298,58 102.1,58 123.89,58 142.44,58\"]"),
        "the edge falls back to a straight line:\n{out}"
    );
}

// Issue #202, verbatim. Tiny touching dot nodes make `lib/ortho/maze.c:markSmall` follow a
// missing cell or read past a node cell's side list; C segfaults (exit 139). The port
// panicked on an `unwrap` in `mark_small`; now the maze fails like the other cases C crashes
// on (see #171 above), and the ortho router gives up. What dot does after a failed maze
// (exit status, nodes-only output) is #200's, so this checks only that nothing panics, and
// that a successful run draws the nodes.
#[test]
fn ortho_tiny_touching_nodes_do_not_panic_mark_small() {
    let src = "digraph{splines=ortho nodesep=0.02 ranksep=0.02 node[shape=circle width=0.01 height=0.02 fixedsize=true label=\"\"] n0->n3 n1->n1 n2->n3 n1->n1 n3->n3 n0->n0}
";
    let out = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
        .arg("-Tsvg")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().unwrap().write_all(src.as_bytes())?;
            child.wait_with_output()
        })
        .unwrap();
    let (svg, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert_ne!(out.status.code(), Some(101), "{err}");
    assert!(!err.contains("panicked"), "{err}");
    if out.status.success() {
        for n in ["n0", "n1", "n2", "n3"] {
            assert!(svg.contains(&format!("<title>{n}</title>")), "{n}:\n{svg}");
        }
    }
}

// The ortho failure boundary (#228, #229): every failure inside `lib/ortho` ends at
// `orthoEdges`, which handles it the way C handles the failure it detects at that stage.
// #229: neato pins 1e12 inches apart make the maze's cells 7.2e13 points wide, so the
// distances in `shortPath` (`lib/ortho/sgraph.c`) do not fit an `int`. C fails
// `PQcheck`'s assertion (exit 134); the port overflowed `N_VAL(n) *= -1`. Now the
// routing fails like C's `goto orthofinish`: a warning, and neato draws the edge straight.
#[test]
fn ortho_distances_beyond_int_fall_back_to_straight_edges() {
    for (engine, far) in [("neato", "1e12"), ("neato", "1e8"), ("fdp", "1e12")] {
        let src = format!(
            "graph {{ layout={engine}; pack=false; splines=ortho; a [pos=\"0,0!\"]; x [pos=\"{far},0!\"]; a -- x }}"
        );
        let (out, err, ok) = run(&["-Tdot"], &src);
        assert!(ok && !err.contains("panicked"), "{engine} {far}: {err}");
        assert!(
            err.contains("orthogonal routing failed - edges not routed orthogonally"),
            "{engine} {far}: {err}"
        );
        let pos =
            edge_pos(&out, "a -- x").unwrap_or_else(|| panic!("{engine} {far}: no edge:\n{out}"));
        assert_eq!(
            pos.split_whitespace().count(),
            4,
            "a straight 4-point edge: {pos}"
        );
    }
}

/// The `pos` of the edge `name` in `-Tdot` output, whitespace-normalised (the writer wraps
/// long attribute lists).
fn edge_pos(dot: &str, name: &str) -> Option<String> {
    let rest = &dot[dot.find(&format!("{name}\t"))?..];
    let rest = &rest[rest.find("pos=\"")? + 5..];
    let pos = &rest[..rest.find('"')?];
    Some(pos.split_whitespace().collect::<Vec<_>>().join(" "))
}

// #228: a NaN or infinite pin. In C, `monotonate_trapezoids` (`lib/ortho/partition.c`)
// finds no trapezoid inside the polygon and fails `LIST_GET`'s bounds assertion (exit
// 134). The port fails the maze there, as C's `mkMaze` returning NULL.
#[test]
fn ortho_non_finite_pins_do_not_panic() {
    for pin in ["nan,0", "inf,0", "0,-inf"] {
        let src = format!(
            "graph {{ layout=neato; pack=false; splines=ortho; a [pos=\"{pin}!\"]; x [pos=\"1,0!\"]; a -- x; b }}"
        );
        let (out, err, ok) = run(&["-Tdot"], &src);
        assert!(ok && !err.contains("panicked"), "{pin}: {err}");
        // The maze fails, so neato draws the edge without ortho routing.
        assert!(edge_pos(&out, "a -- x").is_some(), "{pin}: no edge:\n{out}");
    }
}

// Degenerate inputs through every engine that routes ortho edges, plus pack: zero-size and
// point nodes, self-loops, coincident pins, clusters, touching tiny nodes and huge pins.
// Whatever C does (it aborts or segfaults on several of these), the port must not panic.
#[test]
fn ortho_degenerate_inputs_do_not_panic_in_any_engine() {
    let bodies = [
        "node[shape=box width=0 height=0 fixedsize=true label=\"\"] a->b b->c c->a a->c",
        "node[shape=point width=0 height=0] a->b a->b b->c c->c d->a",
        "a->a a->a a->a b->b a->b b->a c->c",
        "a [pos=\"0,0!\"] b [pos=\"0,0!\"] c [pos=\"0,0!\"] a->b b->c c->a a->a",
        "subgraph cluster_x { a b c } subgraph cluster_y { d e } a->d b->e c->c e->e a->b",
        "nodesep=0.02 ranksep=0.02 node[shape=circle width=0.01 height=0.01 fixedsize=true label=\"\"] a->b b->c c->a a->a b->b d->d d->a",
        "a [pos=\"0,0!\"] b [pos=\"1e12,0!\"] c [pos=\"0,1e12!\"] a->b b->c c->a",
    ];
    for engine in ["dot", "neato", "fdp", "twopi", "circo", "osage"] {
        for pack in ["false", "true"] {
            for body in bodies {
                let src =
                    format!("digraph {{ layout={engine} splines=ortho pack={pack}; {body} }}");
                let (_, err, _) = run(&["-Tsvg"], &src);
                assert!(!err.contains("panicked"), "{src}\n{err}");
            }
        }
    }
}

// Issue #171: tiny touching dot nodes also make a vertex need a fifth chain slot in
// `lib/ortho/partition.c:make_new_monotone_poly`. C writes past `vpos[4]`, but only into
// bookkeeping, so its partition and routes are still right (exit 0). The port must route the
// same, not fail the maze.
#[test]
fn ortho_partition_chain_slot_overflow_routes_like_c() {
    let src = "digraph{splines=ortho nodesep=0 ranksep=0.02 node[shape=point width=0.02 height=0.01 fixedsize=true label=\"\"]
n7->n7 n4->n0 n11->n1 n9->n4 n10->n7 n11->n1 n11->n4 n0->n4 n3->n11 n4->n11
n10->n9 n0->n2 n5->n10 n8->n4 n1->n11 n10->n8 n3->n10 n4->n1 n1->n6 n3->n0}
";
    let (out, err, ok) = run(&["-Tdot"], src);
    assert!(ok && !err.contains("panicked"), "{err}");
    assert!(
        out.contains("n4 -> n0\t[pos=\"e,39.816,20 36.103,52 38.215,52 72,52 72,52 72,52 72,20 72,20 72,20 51.329,20 51.329,20\"]"),
        "n4 -> n0 must be routed orthogonally, as C does:\n{out}"
    );
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

// Issue #203, reduced. With tiny touching point nodes `partition` keeps no free rectangle,
// so C's mkMaze fails and `orthoEdges` returns -1 (lib/ortho/partition.c, maze.c). dot_layout
// then gives up: no edge is drawn and dotneato_postprocess is skipped, so the drawing keeps
// its negative x. The port panicked in `convert_sp_to_route`. C: exit 0, 19x94pt, 12 nodes.
#[test]
fn ortho_maze_without_free_cells_abandons_the_dot_layout_like_c() {
    let src = "digraph{splines=ortho nodesep=0 ranksep=0.02
node[shape=point width=0.05 height=0.01 fixedsize=true label=\"\"]
n3->n2 n10->n7 n1->n11 n10->n4 n8->n2 n5->n3 n6->n4 n3->n10 n6->n3 n6->n0 n3->n6
n0->n8 n11->n10 n2->n10 n3->n2 n6->n10 n0->n9 n11->n5}
";
    let (out, err, ok) = run(&["-Tsvg"], src);
    assert!(ok, "{err}");
    assert_eq!(err, "");
    assert!(out.contains("<svg width=\"19pt\" height=\"94pt\""), "{out}");
    assert_eq!(out.matches("class=\"node\"").count(), 12, "{out}");
    assert!(!out.contains("class=\"edge\""), "{out}");
}

// Issue #200: zero-size point nodes make partition's trapezoidation fail, so mkMaze fails
// as in #203 and C's dot abandons the layout and exits 0 with the nodes only. The port exited 1
// with a layout error. C also prints two trapezoid failure messages, and so does the port
// (#127).
#[test]
fn ortho_trapezoid_failure_abandons_the_dot_layout_like_c() {
    let src = "digraph{splines=ortho nodesep=0 ranksep=0
node[shape=point width=0 height=0 fixedsize=true]
a->b a->c a->d b->e c->e d->e b->f c->f e->g f->g a->g}
";
    let (out, err, ok) = run(&["-Tsvg"], src);
    assert!(ok);
    assert_eq!(
        err,
        "trapezoid segment construction failed\nhorizontal trapezoid construction failed\n"
    );
    assert!(out.contains("<svg width=\"13pt\" height=\"35pt\""), "{out}");
    assert_eq!(out.matches("class=\"node\"").count(), 7, "{out}");
    assert!(!out.contains("class=\"edge\""), "{out}");
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

/// Issue #173: `cluster_inner4` is repeated under three parents and each copy holds j1 and
/// j2. osage's `layout` (`lib/osage/osageinit.c`) places them in the first copy, packs
/// nothing for the others and leaves them a NaN box, so `j1 -> o1` and `j2 -> o1` get NaN
/// control points. `update_bb_bz` (`lib/common/emit.c`) splits such a segment forever
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
            // The edges leave the root's 18x18 empty-graph box as it was.
            assert!(out.contains("bb=\"0,0,18,18\""), "{out}");
            assert!(out.contains("j1 -> o1\t[pos="), "{out}");
        }
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

// Issue #186: twopi stacks the nodes in a column whose sides differ by float noise, and the
// ortho search graph does not connect the two sides of t2's self-loop. shortPath never
// reaches the loop's start, and C's convertSPtoRoute (lib/ortho/ortho.c) dereferences its
// null N_DAD and segfaults (exit 139, no output). The port used to panic on the same
// parent ("called `Option::unwrap()` on a `None` value", exit 101); it now abandons ortho
// routing, as C's orthoEdges does when shortPath fails, and the edges are not routed
// orthogonally.
const ISSUE186_GRAPH: &str = "graph{layout=twopi splines=ortho oneblock=true rankdir=LR
node[shape=record]
t2--t2 t1--t1 t0--t1 t0--t1 t1--t2 t1--t2 t1--t1 t1--t0}
";

#[test]
fn twopi_ortho_self_loop_without_a_path_does_not_panic_convert_sp_to_route() {
    let (out, err, ok) = run(&["-Tsvg"], ISSUE186_GRAPH);
    assert!(ok, "C crashes here, the port abandons ortho routing: {err}");
    assert_eq!(
        err,
        "Warning: orthogonal routing found no path for an edge - \
         edges not routed orthogonally\n"
    );
    assert_eq!(out.matches("class=\"edge\"").count(), 8, "{out}");
}

// Issue #186, the case C survives: the same unreachable self-loop, routed after edges that
// reached the search graph's start node. C's reset (lib/ortho/sgraph.c) does not clear
// N_DAD, so convertSPtoRoute follows the stale parent and C routes every edge (exit 0).
// The port must not take the #186 fallback here: no warning, and t2--t2 gets the ortho route
// C draws through the stale parent.
const ISSUE186_STALE_PARENT_GRAPH: &str =
    "graph{layout=twopi splines=ortho oneblock=true rankdir=LR
node[shape=record]
a--t2 t0--t2 t2--t2 t1--t1 t0--t1 t0--t1 t1--t2 t1--t2 t1--t1 t1--t0}
";

#[test]
fn twopi_ortho_unreachable_self_loop_follows_the_stale_parent_like_c() {
    let (out, err, ok) = run(&["-Tsvg"], ISSUE186_STALE_PARENT_GRAPH);
    assert!(ok, "{err}");
    assert_eq!(err, "");
    assert!(
        out.contains(
            "d=\"M70.29,-168.25C70.29,-139.53 70.29,-87 70.29,-87 70.29,-87 96,-87 96,-87 \
             96,-87 96,-183.41 96,-186.43\""
        ),
        "{out}"
    );
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

/// Issue #174 (found by cargo-fuzz): a component taller than INT_MAX polyomino steps.
/// C's `computeStep` (`lib/pack/pack.c:86`) saturates a step of ~1e29 to INT_MAX, and
/// `genPoly`/`genBox` then fill ~1e22 cells per side: C Graphviz 16.1.0 never finishes on
/// these inputs. The port gives a saturated step a cell budget, warns when it runs out and
/// leaves the components unpacked. A node pinned near 1e30 (C hangs too) packs. Each run gets
/// 30 s (it takes well under a second) so a regression fails instead of hanging CI.
#[test]
fn oversized_component_does_not_hang_the_packer() {
    let warning = "Warning: components too large to pack; packing skipped\n";
    let cases = [
        ("graph { layout=fdp; a [height=\"1e30\"]; b }", warning),
        (
            "graph { layout=fdp; quantum=\"1e40\"; a -- b; c -- d }",
            warning,
        ),
        (
            "graph { layout=neato; pack=false; a [height=\"1e30\"]; b }",
            warning,
        ),
        ("graph { layout=fdp; a [height=\"1e300\"]; b }", warning),
        ("graph { layout=fdp; a [pos=\"1e30,1e30!\"]; b; c }", ""),
        // A saturated step within the budget packs as C does.
        (
            "graph { layout=neato; pack=true; a [pos=\"1e9,1e9!\"]; b }",
            "",
        ),
    ];
    for (src, expected) in cases {
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() > deadline {
                child.kill().unwrap();
                panic!("packing must terminate: {src}");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let out = child.wait_with_output().unwrap();
        let (svg, err) = (
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(out.status.success(), "{src}: {err}");
        assert_eq!(err, expected, "{src}");
        assert!(svg.contains("</svg>"), "{src}: {svg}");
    }
}

/// Issue #174: C's `fits` (`lib/pack/pack.c:437`) computes `step * x` in int, which wraps
/// once the step is large. With a saturated step (a 1e11-inch box) or a huge `pack` margin,
/// C places `b` near the origin; the expected output is C 16.1.0's `-Tplain`.
#[test]
fn large_pack_step_wraps_placement_like_c() {
    let cases = [
        (
            "graph { layout=fdp; node[shape=box fixedsize=true]; \
             a [width=\"1e11\" height=\"1e11\"]; b }",
            "graph 1 1e+11 1e+11
node a 5e+10 5e+10 1e+11 1e+11 a solid box black lightgrey
node b 0.375 0.27777 0.75 0.5 b solid box black lightgrey
stop
",
        ),
        (
            "graph { layout=neato; pack=2000000000; a; b }",
            "graph 1 4.5436e+07 1.2312e+07
node a 4.5436e+07 1.2312e+07 0.75 0.5 a solid ellipse black lightgrey
node b 0.375 0.25 0.75 0.5 b solid ellipse black lightgrey
stop
",
        ),
    ];
    for (src, expected) in cases {
        let (out, err, ok) = run(&["-Tplain"], src);
        assert!(ok && err.is_empty(), "{src}: {err}");
        assert_eq!(out, expected, "{src}");
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

// Issue #185. Under overlap=vpsc, stress_majorization_cola (lib/neatogen/
// constrained_majorization_ipsep.c) ends with removeoverlaps(orig_n, ...), and initCMajVPSC
// treats the last 2 * nclusters real nodes as the cluster boundary variables. C sizes
// generateNonoverlapConstraints' boxes before dropping the dummies, so cluster members among
// those nodes read zeroed boxes; the port sized them after and panicked. With fewer real nodes
// than 2 * nclusters, C segfaults (the first two graphs) and the port passes the dummies too.
#[test]
fn neato_ipsep_vpsc_with_clusters_does_not_panic() {
    for (src, nodes) in [
        (
            "digraph { layout=neato; mode=ipsep; overlap=vpsc; subgraph cluster_a { a } subgraph cluster_b { b } a -> b; b -> c }",
            3,
        ),
        (
            "digraph { layout=neato; mode=ipsep; overlap=vpsc; subgraph cluster_a { a } subgraph cluster_b { b } a -> b; b -> c; c -> d; d -> e }",
            5,
        ),
    ] {
        let (out, err, ok) = run(&["-Tsvg"], src);
        assert!(ok, "{src}: {err}");
        assert_eq!(err, "", "{src}");
        assert_eq!(out.matches("class=\"node\"").count(), nodes, "{src}: {out}");
        assert_eq!(out.matches("class=\"cluster\"").count(), 2, "{src}: {out}");
    }
    // C renders this one; n4 and n5 are the cluster's boundary variables in removeoverlaps.
    let src = "digraph { layout=neato; mode=ipsep; overlap=vpsc; subgraph cluster_0 { n0 } \
               n0 -> n1; n1 -> n2; n2 -> n3; n3 -> n4; n4 -> n5; }";
    let (out, err, ok) = run(&["-Tplain"], src);
    assert!(ok, "{err}");
    // C Graphviz 16.1.0.
    for line in [
        "graph 1 3.0059 1.7638",
        "node n0 1.5883 0.2916 0.75 0.5 n0 solid ellipse black lightgrey",
        "node n1 2.6309 0.80008 0.75 0.5 n1 solid ellipse black lightgrey",
        "node n2 1.3891 1.5138 0.75 0.5 n2 solid ellipse black lightgrey",
        "node n3 0.375 1.4439 0.75 0.5 n3 solid ellipse black lightgrey",
        "node n4 0.7271 0.25 0.75 0.5 n4 solid ellipse black lightgrey",
        "node n5 1.7698 0.90271 0.75 0.5 n5 solid ellipse black lightgrey",
    ] {
        assert!(out.contains(line), "{line}\n{out}");
    }
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

/// Runs the CLI on raw-byte arguments; returns stdout, stderr and the exit status.
#[cfg(unix)]
fn run_bytes(args: &[&[u8]], stdin: &str) -> (Vec<u8>, Vec<u8>, bool) {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let mut child = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
        .args(args.iter().map(|a| OsStr::from_bytes(a)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(e) = child.stdin.take().unwrap().write_all(stdin.as_bytes()) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "{e}");
    }
    let out = child.wait_with_output().unwrap();
    (out.stdout, out.stderr, out.status.success())
}

/// Issue #197: `x` is in two clusters (#33), so `overlap=ipsep` gives VPSC containment
/// constraints it cannot satisfy. C's `satisfyVPSC` (`lib/vpsc/csolve_VPSC.cpp`) lets the
/// "Cycle Error!" `std::runtime_error` escape and aborts. The port went on projecting, each
/// cycle error splitting 10000 blocks that it never freed, without bound in time or memory.
/// It now reuses unused blocks and ends the layout after `MAX_CYCLE_ERRORS` cycle errors, with
/// a warning, and every node keeps a position of its own.
#[test]
fn ipsep_vpsc_cycle_error_ends_the_layout() {
    let src = "digraph { layout=neato; mode=ipsep; overlap=ipsep; \
               subgraph cluster_a { a; x; c } subgraph cluster_b { b; x; d } \
               a -> b; x -> a; c -> d; h -> {a b c d} }";
    let mut child = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
        .arg("-Tplain")
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
    // The output is under 1 KB, well under the pipe buffer, so polling cannot block. The capped
    // layout takes about 2 s; the deadline is short because the runaway also grows memory.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            panic!("the ipsep layout must terminate");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    let (plain, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{err}");
    assert_eq!(
        err,
        "Warning: VPSC: too many cycle errors; ending the ipsep layout early\n"
    );
    let positions: std::collections::BTreeSet<(&str, &str)> = plain
        .lines()
        .filter_map(|l| l.strip_prefix("node "))
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            (f[1], f[2])
        })
        .collect();
    assert_eq!(positions.len(), 6, "{plain}");
}

/// Issue #152: C's `main` takes its arguments as bytes; a file name that is not valid UTF-8
/// (here Latin-1 `né.gv`) is opened, and named in errors with its own bytes.
#[cfg(unix)]
#[test]
fn non_utf8_file_name_argument_is_opened() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tmp("issue152");
    std::fs::create_dir_all(&dir).unwrap();
    let name = std::ffi::OsStr::from_bytes(b"n\xe9.gv");
    let path = dir.join(name);
    let path_bytes = path.as_os_str().as_bytes();

    // macOS (APFS) refuses names that are not UTF-8; Linux keeps them.
    if std::fs::write(&path, "digraph{a->b}").is_ok() {
        let (out, err, ok) = run_bytes(&[b"-Tsvg", path_bytes], "");
        assert!(ok, "{}", String::from_utf8_lossy(&err));
        assert_eq!(String::from_utf8_lossy(&out).matches("<svg").count(), 1);

        // -O writes `<input>.svg` next to the input, with the input's bytes.
        let (_, err, ok) = run_bytes(&[b"-Tsvg", b"-O", path_bytes], "");
        assert!(ok, "{}", String::from_utf8_lossy(&err));
        let mut auto = path.clone().into_os_string();
        auto.push(".svg");
        assert!(std::fs::read(&auto).unwrap().starts_with(b"<?xml"));

        // -o, separate and attached.
        let target = dir.join(std::ffi::OsStr::from_bytes(b"out\xe9.svg"));
        let target_bytes = target.as_os_str().as_bytes();
        let (_, err, ok) = run_bytes(&[b"-Tsvg", b"-o", target_bytes, path_bytes], "");
        assert!(ok, "{}", String::from_utf8_lossy(&err));
        assert!(std::fs::read(&target).unwrap().starts_with(b"<?xml"));
        std::fs::remove_file(&target).unwrap();
        let attached = [b"-o", target_bytes].concat();
        let (_, err, ok) = run_bytes(&[b"-Tsvg", &attached, path_bytes], "");
        assert!(ok, "{}", String::from_utf8_lossy(&err));
        assert!(std::fs::read(&target).unwrap().starts_with(b"<?xml"));
    }

    // A missing file is an error naming its bytes, not a panic.
    let missing = dir.join(std::ffi::OsStr::from_bytes(b"missing\xe9.gv"));
    let missing = missing.as_os_str().as_bytes();
    let (_, err, ok) = run_bytes(&[b"-Tsvg", missing], "");
    assert!(!ok);
    let expected = [b"can't open ", missing, b": "].concat();
    assert!(
        err.windows(expected.len()).any(|w| w == expected),
        "{}",
        String::from_utf8_lossy(&err)
    );
}

/// Issue #152: other flags with bytes that are not UTF-8 behave as C's do
/// (`lib/common/input.c:dotneato_args_initialize`): the values keep their bytes.
#[cfg(unix)]
#[test]
fn non_utf8_flag_values_keep_their_bytes() {
    let (out, _, ok) = run_bytes(&[b"-Tdot", b"-Gx\xe9=1"], "digraph{a}");
    assert!(ok);
    assert!(out.windows(5).any(|w| w == b"x\xe9=1\n"), "{out:?}");

    let (_, err, ok) = run_bytes(&[b"-T\xe9"], "digraph{a}");
    assert!(!ok);
    assert!(
        err.starts_with(b"Format: \"\xe9\" not recognized."),
        "{err:?}"
    );

    // `fprintf`, not `agerrorf`: control bytes are written as they are. A char of the
    // range the port uses for raw bytes (U+10FF54) keeps its four bytes.
    let (_, err, ok) = run_bytes(&[b"-T\x01\xf4\x8f\xbd\x94"], "digraph{a}");
    assert!(!ok);
    assert!(
        err.starts_with(b"Format: \"\x01\xf4\x8f\xbd\x94\" not recognized."),
        "{err:?}"
    );

    // The same as a separate argument, and as a -G value.
    let (_, err, ok) = run_bytes(&[b"-T", b"\xf4\x8f\xbd\x94"], "digraph{a}");
    assert!(!ok);
    assert!(err.starts_with(b"Format: \"\xf4\x8f\xbd\x94\" not recognized."));
    let (out, _, ok) = run_bytes(&[b"-Tdot", b"-Gx=\xf4\x8f\xbd\x94"], "digraph{a}");
    assert!(ok);
    assert!(
        out.windows(7).any(|w| w == b"x=\xf4\x8f\xbd\x94\n"),
        "{out:?}"
    );

    // atoi skips only C's white space, not U+00A0.
    let (_, err, ok) = run_bytes(&[b"-n\xc2\xa01"], "digraph{a}");
    assert!(!ok);
    assert!(err.starts_with(b"Error: Invalid parameter \"\xc2\xa01\" for -n flag\n"));

    let (_, err, ok) = run_bytes(&[b"-K\xe9"], "digraph{a}");
    assert!(!ok);
    assert!(err.starts_with(b"There is no layout engine support for \"\xe9\""));

    let (_, err, ok) = run_bytes(&[b"-n\xe9"], "digraph{a}");
    assert!(!ok);
    assert!(err.starts_with(b"Error: Invalid parameter \"\xe9\" for -n flag\n"));

    // C switches on the byte after `-`, even inside a multi-byte UTF-8 char (`-é`).
    // A char of the range the port uses for raw bytes (U+10FF54 here) is split the same way.
    for (arg, flag) in [
        (&b"-\xe9"[..], &b"-\xe9 "[..]),
        (b"-\xc3\xa9", b"-\xc3 "),
        (b"-\xf4\x8f\xbd\x94svg", b"-\xf4 "),
        (b"-\xf4\x8f\xbd\x87x=1", b"-\xf4 "),
    ] {
        let (_, err, ok) = run_bytes(&[arg], "digraph{a}");
        assert!(!ok);
        let expected = [b"option ", flag, b"unrecognized"].concat();
        assert!(
            err.windows(expected.len()).any(|w| w == expected),
            "{}",
            String::from_utf8_lossy(&err)
        );
    }
}

// Issue #153: `-q` (`-q1`) sets cgraph's message level to errors (`agseterr(AGERR)`, args.c),
// `-q2` and up turn every message off, and `-q0` is reported and ignored. An error that is not
// written still fails the run (`agerrors()`). stderr and exit status are C 16.1.0's.
#[test]
fn q_drops_warnings_and_q2_drops_errors_like_c() {
    let src = "digraph { a [shape=foo]; b [shape=record,label=\"{a|\"] }";
    let both = "Warning: using box for unknown shape foo\nError: bad label format {a|\n";
    for (flag, want) in [
        (None, both.to_string()),
        (Some("-q"), "Error: bad label format {a|\n".to_string()),
        (Some("-q1"), "Error: bad label format {a|\n".to_string()),
        (Some("-q2"), String::new()),
        (Some("-q5"), String::new()),
        (
            Some("-q0"),
            format!("Invalid parameter \"0\" for -q flag - ignored\n{both}"),
        ),
    ] {
        let mut args = vec!["-Tsvg"];
        args.extend(flag);
        let (out, err, ok) = run(&args, src);
        assert_eq!(err, want, "{flag:?}");
        assert!(!ok, "{flag:?}: the error still sets the exit status");
        assert!(out.contains("</svg>"), "{flag:?}");
    }
}

// `-q` applies from where it is read (dotneato_args_initialize), so it drops the error of an
// unrecognized option after it; `-n` is read before (`neato_extra_args`), so its error is
// written whatever `-q` says. C 16.1.0 writes the usage after both.
#[test]
fn q2_drops_later_option_errors_but_not_the_n_flag_error() {
    let (_, err, ok) = run(&["-q2", "-Z"], "graph{}");
    assert!(!ok);
    assert!(err.starts_with("Usage: "), "{err}");
    let (_, err, ok) = run(&["-Z", "-q2"], "graph{}");
    assert!(!ok);
    // The program name is `graphviz-pure.exe` on Windows.
    assert!(
        err.starts_with("Error: graphviz-pure")
            && err.contains(": option -Z unrecognized\n\nUsage: "),
        "{err}"
    );
    let (_, err, ok) = run(&["-q2", "-n0"], "graph{}");
    assert!(!ok);
    assert!(
        err.starts_with("Error: Invalid parameter \"0\" for -n flag\nUsage: "),
        "{err}"
    );
}

// Issue #121: cgraph's `default_usererrf` (agerror.c) writes control characters other than
// white space as `\ooo`, in every message: the color warning of `gvrender_resolve_color`, the
// style warning of `gvrender_set_style` and the arrow warning of `arrow_match_name`. stderr is
// C 16.1.0's.
#[test]
fn control_characters_in_warnings_are_escaped_like_c() {
    let src = "digraph { a [color=\"x\u{1}y\", style=\"foo\u{2}\"]; \
               a -> b [arrowhead=\"zz\u{3}\"] }";
    let (_, err, ok) = run(&["-Tsvg"], src);
    assert!(ok, "{err}");
    assert_eq!(
        err,
        "Warning: Arrow type \"zz\\003\" unknown - ignoring\n\
         Warning: gvrender_set_style: unsupported style foo\\002 - ignoring\n\
         Warning: x\\001y is not a known color.\n"
    );
}

// Render warnings worded as C 16.1.0's: `firstlayer` names the output format, the
// `layersep`/`layerlistsep` warning names the character, and `parseSegs`' "Total size > 1"
// has no newline, so the next message continues its line.
#[test]
fn render_warnings_are_worded_like_c() {
    let (_, err, _) = run(
        &["-Txdot"],
        "digraph { layers=\"a:b\"; layersep=\":\"; layerlistsep=\":\"; a }",
    );
    assert_eq!(
        err,
        "Warning: The character ':' appears in both the layersep and layerlistsep attributes \
         - layerlistsep ignored.\n\
         Warning: layers not supported in xdot output\n"
    );
    let (_, err, _) = run(
        &["-Tsvg"],
        "digraph { a [style=filled fillcolor=\"red;0.8:blue;0.7\"] }",
    );
    assert!(
        err.starts_with(
            "Warning: Total size > 1 in \"red;0.8:blue;0.7\" color spec Warning: red;0.8 is \
             not a known color.\n"
        ),
        "{err}"
    );
    // `parseSegs` reports only the first bad color spec of the process (`static atomic_flag
    // warned`): the later ones say nothing.
    let (_, err, _) = run(
        &["-Tsvg"],
        "digraph { a [style=filled fillcolor=\"red;0.8:blue;0.7\"]; \
         b [style=filled fillcolor=\"red;x:blue\"]; c [style=filled fillcolor=\"red;x:blue\"] }",
    );
    assert_eq!(
        err,
        "Warning: Total size > 1 in \"red;0.8:blue;0.7\" color spec Warning: red;0.8 is not a \
         known color.\nWarning: red;x is not a known color.\n"
    );
}

// The callers of `parseSegs` continue its message with `agerr(AGPREV, ...)`: `in node`
// (`poly_gencode` for wedged and striped), `in cluster` (`emit_clusters`) and `in edge`
// (`multicolor`). One run each: C reports only the first bad spec of a process.
#[test]
fn color_spec_messages_name_the_object_like_c() {
    for (src, want) in [
        (
            "digraph { a [shape=box style=striped fillcolor=\"red;0.8:blue;0.7\"] }",
            "Warning: Total size > 1 in \"red;0.8:blue;0.7\" color spec in node a\n",
        ),
        (
            "digraph { a [style=wedged fillcolor=\"red;x:blue\"] }",
            "Error: Illegal value in \"red;x:blue\" color attribute; float expected after ';'\n\
             in node a\n",
        ),
        (
            "digraph { subgraph cluster_c { style=striped fillcolor=\"red;0.8:blue;0.7\"; a } }",
            "Warning: Total size > 1 in \"red;0.8:blue;0.7\" color spec in cluster cluster_c\n",
        ),
        (
            "digraph { a -> b [color=\"red;0.8:blue;0.7\"] }",
            "Warning: Total size > 1 in \"red;0.8:blue;0.7\" color spec in edge a -> b\n",
        ),
        (
            "graph { a -- b [color=\"red;x:blue\"] }",
            "Error: Illegal value in \"red;x:blue\" color attribute; float expected after ';'\n\
             in edge a -- b\n",
        ),
    ] {
        let (_, err, _) = run(&["-Tsvg"], src);
        assert_eq!(err, want, "{src}");
    }
}

// `neato_extra_args` takes `-n` out of the arguments before the others are read, so the file
// after `-o -n2` is the output file, as in C 16.1.0.
#[test]
fn n_flag_is_taken_out_before_the_other_options_like_c() {
    let path = tmp("n_flag_taken_out.svg");
    let _ = std::fs::remove_file(&path);
    let (_, err, ok) = run(
        &["-o", "-n2", path.to_str().unwrap(), "-Tsvg"],
        "digraph { a [pos=\"0,0\"] }",
    );
    assert!(ok, "{err}");
    assert!(std::fs::read_to_string(&path).unwrap().contains("</svg>"));
}

// Issue #150 (its message only): `init_nop` reports a node with no position and
// `neato_layout` continues the message with `agerr(AGPREV, ...)`, on a second line, as
// C 16.1.0 does. `-q2` drops both lines, the continuation at the level of the first. The port's
// `Layout computation error` line after them goes with the fatal `LayoutError`, which the
// `-n` rework (C8, the rest of #150) replaces.
#[test]
fn neato_n_missing_pos_is_reported_on_two_lines_like_c() {
    let src = "graph A { a; b [pos=\"0,0\"] }";
    let (_, err, ok) = run(&["-Kneato", "-n", "-Tsvg"], src);
    assert!(!ok);
    assert!(
        err.starts_with("Error: node a in graph A has no position\nas required by the -n flag\n"),
        "{err}"
    );
    let (_, err, _) = run(&["-q2", "-Kneato", "-n", "-Tsvg"], src);
    assert!(!err.contains("as required"), "{err}");
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

/// Issue #198: after `overlap=ipsep`, m1 cannot be reached from m3 among the obstacles
/// fdp's `compoundEdges` (`lib/fdpgen/clusteredges.c`) builds, so `shortestPath` leaves
/// `dad[N] == -1`. C's `Pobspath` (`lib/pathplan/cvt.c`) then reads `dad[-1]`: it hangs on
/// `tests/fdp/hw_compound_mid.gv` and routes through garbage on this reduction. The port
/// panicked; now it warns and routes the edges without compound edges.
#[test]
fn compound_edge_without_a_path_falls_back_instead_of_panicking() {
    let src = "graph { layout=fdp; splines=compound; overlap=ipsep;
subgraph cluster_mid { m1 -- m2; m3 -- m1; m4 -- m1 }
a -- m1; b -- m2; c -- m3; d -- c }
";
    let (out, err, ok) = run(&["-Tdot"], src);
    assert!(ok && !err.contains("panicked"), "{err}");
    // C's own warnings, plus the port's for the missing path.
    assert_eq!(
        err,
        "Warning: compoundEdges: could not find a path - falling back to straight line edges\n\
         Warning: compoundEdges: nodes touch - falling back to straight line edges\n\
         Warning: some nodes with margin (3.20,3.20) touch - falling back to straight line edges\n"
    );
    assert!(
        out.contains("m3 -- m1\t[pos=\"83.797,51.424 80.429,48.095 76.858,44.566 73.484,41.231\"]"),
        "m3 -- m1 falls back to a straight line:\n{out}"
    );
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

// Issue #269: at 1e300 points, `update_bb_bz`'s pieces stop shrinking (one recurs two splits
// down) and `ptToLine2` overflows to NaN, so C recurses until its stack overflows (SIGSEGV).
// The port refines on an explicit stack and takes the pieces whole past C's stack depth.
#[test]
fn issue269_update_bb_bz_at_1e300_does_not_overflow_the_stack() {
    for (engine, src) in [
        ("dot", "digraph{ratio=\"1e300\"; f:n -> b}"),
        (
            "twopi",
            "digraph G { a -> b; a -> c; subgraph cluster_x { } c -> d; d -> e; { } \
             f:p -> b; ranksep=\"1e300\"; }",
        ),
    ] {
        let (out, err, ok) = run(&["-K", engine, "-Tplain"], src);
        assert!(ok, "{src}: {err}");
        assert!(out.ends_with("stop\n"), "{src}: {out}");
    }
}

// Issue #270: huge scales make some path boxes empty, `add_box` drops them, and
// `adjustregularpath`'s first loop runs past the boxes kept. C touches only stale slots there,
// so its output is the port's (Graphviz 16.1.0). Where every box is dropped, C segfaults in
// `Pshortestpath`; the port loses the edge.
#[test]
fn issue270_dropped_path_boxes_lay_out_like_c() {
    let (out, err, ok) = run(
        &["-Tplain"],
        "digraph{quantum=\"1e300\"; a -> b [label=\"ab\"]}",
    );
    assert!(ok, "{err}");
    assert_eq!(
        out,
        "graph 1 1.4142e+300 2.8284e+300
node a 7.0711e+299 2.1213e+300 1.4142e+300 1.4142e+300 a solid ellipse black lightgrey
node b 7.0711e+299 7.0711e+299 1.4142e+300 1.4142e+300 b solid ellipse black lightgrey
edge a b 7 3.9088e+299 1.4889e+300 2.0258e+299 1.1123e+300 0 7.0711e+299 0 7.0711e+299 0 \
7.0711e+299 0 7.0711e+299 1.3218e+284 7.0711e+299 ab 7.0711e+299 1.4142e+300 solid black
stop
"
    );
    let (out, err, ok) = run(
        &["-Tplain"],
        "digraph{ratio=\"9223372036854775808\"; a -> b [label=\"ab\"]; b -> c; a -> c}",
    );
    assert!(ok, "{err}");
    assert!(
        out.contains(
            "edge a b 4 0.93056 1.0946e+19 0.93056 1.0946e+19 0.375 5.5399e+18 \
                      0.375 5.5399e+18 ab 1.0223 8.2429e+18 solid black\n"
        ),
        "{out}"
    );
    let (_, err, ok) = run(
        &["-Tplain"],
        "digraph{ratio=\"inf\"; a -> c; b -> c; {rank=same; f [shape=record]; a} f:p -> b}",
    );
    assert!(!ok);
    assert!(
        err.contains("Error: lost a c edge\n") && !err.contains("panicked"),
        "{err}"
    );
}

// Issue #301. An infinite (or 400-digit) edge label font size gives the label an infinite
// size. C's `xlhdxload` (lib/label/xlabels.c:513) asserts that the label area's centre fits
// an `int` and aborts; past that point, `adjust`'s slides stay at `-inf` forever. The
// port takes `xlhdxload`'s own failure path there: no exterior label is placed.
#[test]
fn infinite_edge_label_size_does_not_hang_xlabel_placement() {
    let huge = format!("1{}", "0".repeat(400));
    for (engine, src) in [
        (
            "dot",
            "digraph G { edge [labelfontsize=inf]; a -> b [headlabel=h]; }".to_string(),
        ),
        (
            "neato",
            "graph G { edge [labelfontsize=inf]; a -- b [headlabel=h]; }".to_string(),
        ),
        (
            "dot",
            format!("digraph G {{ edge [labelfontsize={huge}]; a -> b [headlabel=h]; }}"),
        ),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
            .args(["-Tplain", &format!("-K{engine}")])
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
        // The output is a few lines, well under the pipe buffer, so polling cannot block.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() > deadline {
                child.kill().unwrap();
                panic!("xlabel placement must terminate: {src}");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let out = child.wait_with_output().unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{src}: {err}");
        assert!(
            err.contains("0 out of 1 exterior labels positioned"),
            "{src}: {err}"
        );
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

/// Issue #127: when the ortho maze fails, C prints `add_segment`'s and `partition`'s bare
/// `fprintf` messages (`lib/ortho/trapezoid.c:623`, `partition.c:694/711`), and before its
/// `chkSgraph` assertion, the node it fails at (`maze.c:315/317`). No prefix, no error status.
#[test]
fn issue127_ortho_maze_failures_print_cs_messages() {
    let src = "graph G { layout=neato; splines=ortho;
a [shape=circle, fixedsize=shape, width=0.3, label=\"a very long label here\", pos=\"0,0!\"];
b [pos=\"1.0,0!\"]; c [pos=\"0,1.0!\"]; a -- b; b -- c; a -- c; }";
    let (err, ok) = warnings("neato", src);
    assert!(ok);
    assert_eq!(
        err,
        "trapezoid segment construction failed\nvertical trapezoid construction failed\n"
    );
    let src = "graph { splines=ortho; a [pos=\"1e300,0!\"]; b [pos=\"1.125,0!\"]; \
               c [pos=\"0,1!\"]; a -- b; b -- c }";
    assert_eq!(
        warnings("neato", src).0,
        "trapezoid segment construction failed\nhorizontal trapezoid construction failed\n"
    );
    // C fails its assertion after the message (exit 134); the port draws straight edges.
    let src = "digraph{layout=osage
splines=ortho{label=\"inner\"{o2{i1ortho
cluster_outer{o1}subgraph cluster_inner4{j1 j2}}subgrapa}subgraph cluster_inner4{j2}}x i2 i3}
";
    assert_eq!(warnings("osage", src).0, "failed at node 1[1]\n");
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
