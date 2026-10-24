//! High-level Graphviz orchestration engine (`graphviz-pure`).
//!
//! DOT text taken and returned by the functions here is ordinary text. The port reads every
//! string in a decoded form ([`charset`]), where U+10FF00..=U+10FFFF stands for an input byte
//! that is not UTF-8; these functions convert at the boundary, so a real U+10FFxx character in
//! the source is that character, as it is to C. A parsed [`Graph`] holds its names and values
//! in the decoded form. For text already decoded, such as a file's bytes read with
//! [`charset::decode`], use the same entry points in [`decoded`].

pub mod decoded;
pub mod plugins;

use graphviz_pure_core::AttrMap;
use std::borrow::Cow;

pub use graphviz_pure_core::{Graph, GraphvizError, Result, charset};
pub use graphviz_pure_layout::cutil::{RandState, in_rand_run, with_rand_state};
pub use graphviz_pure_layout::fdpgen::tlayout::FdpParms;
pub use graphviz_pure_layout::{
    LayoutEngine, LayoutOptions, layout_graph, layout_graph_in_run, layout_graph_with_options,
};
pub use graphviz_pure_parser::GlobalAttrs;
pub use graphviz_pure_render::{OutputFormat, loadimage_installed, set_y_invert};

/// End-to-end DOT rendering pipeline for any supported engine and output format. Renders the
/// first graph in `dot_source`.
pub fn render_dot(dot_source: &str, engine: &str, format: &str) -> Result<String> {
    decoded::render_dot(
        &from_text(dot_source),
        &from_text(engine),
        &from_text(format),
    )
    .map(text)
    .map_err(error_text)
}

/// Renders every graph in `dot_source`, applying `globals` (`-G`/`-N`/`-E`) before each graph body.
pub fn render_dot_all(
    dot_source: &str,
    engine: &str,
    format: &str,
    globals: &GlobalAttrs,
) -> Result<Vec<String>> {
    render_dot_all_with_options(
        dot_source,
        engine,
        format,
        globals,
        LayoutOptions::default(),
    )
}

/// [`render_dot_all`] with engine options such as `-n` (keep input positions).
pub fn render_dot_all_with_options(
    dot_source: &str,
    engine: &str,
    format: &str,
    globals: &GlobalAttrs,
    options: LayoutOptions,
) -> Result<Vec<String>> {
    decoded::render_dot_all_with_options(
        &from_text(dot_source),
        &from_text(engine),
        &from_text(format),
        &globals_from_text(globals),
        options,
    )
    .map(|outs| outs.into_iter().map(text).collect())
    .map_err(error_text)
}

/// [`render_dot_all_with_options`] that renders the graphs read before a syntax error
/// and returns that error alongside, as `dot` does.
pub fn render_dot_all_partial(
    dot_source: &str,
    engine: &str,
    format: &str,
    globals: &GlobalAttrs,
    options: LayoutOptions,
) -> (Vec<String>, Option<GraphvizError>) {
    let (outs, err) = decoded::render_dot_all_partial(
        &from_text(dot_source),
        &from_text(engine),
        &from_text(format),
        &globals_from_text(globals),
        options,
    );
    (outs.into_iter().map(text).collect(), err.map(error_text))
}

/// Lays out and renders every graph in `dot_source` as SVG, for raster output: each SVG comes
/// with whether its graph sets `dpi` (or `resolution`), since Graphviz then scales the drawing
/// to that resolution instead of drawing raster formats at 96 dpi. Like
/// [`render_dot_all_partial`], the graphs read before a syntax error are returned with it.
pub fn render_svg_all_partial(
    dot_source: &str,
    engine: &str,
    globals: &GlobalAttrs,
    options: LayoutOptions,
) -> (Vec<(String, bool)>, Option<GraphvizError>) {
    let (outs, err) = decoded::render_svg_all_partial(
        &from_text(dot_source),
        &from_text(engine),
        &globals_from_text(globals),
        options,
    );
    let outs = outs
        .into_iter()
        .map(|(svg, dpi)| (text(svg), dpi))
        .collect();
    (outs, err.map(error_text))
}

/// Parses the first graph in `dot_source`. The graph holds its names and values in decoded
/// form ([`charset`]).
pub fn parse_dot(dot_source: &str) -> Result<Graph> {
    decoded::parse_dot(&from_text(dot_source)).map_err(error_text)
}

/// Parses every graph in `dot_source`, applying `globals` before each graph body. The graphs
/// hold their names and values in decoded form ([`charset`]).
pub fn parse_dot_all(dot_source: &str, globals: &GlobalAttrs) -> Result<Vec<Graph>> {
    decoded::parse_dot_all(&from_text(dot_source), &globals_from_text(globals)).map_err(error_text)
}

/// Renders a laid-out graph in `format`.
pub fn render_graph(graph: &Graph, format: OutputFormat) -> Result<String> {
    decoded::render_graph(graph, format)
        .map(text)
        .map_err(error_text)
}

/// [`render_graph`] for the format named `name` (an alias such as `gv` for `dot`), which the
/// render diagnostics print.
pub fn render_graph_named(graph: &Graph, format: OutputFormat, name: &str) -> Result<String> {
    decoded::render_graph_named(graph, format, &from_text(name))
        .map(text)
        .map_err(error_text)
}

/// Text from the caller in decoded form: a real U+10FF00..=U+10FFFF is not a raw byte.
fn from_text(s: &str) -> Cow<'_, str> {
    charset::from_text(s)
}

/// `globals` in decoded form.
fn globals_from_text(globals: &GlobalAttrs) -> Cow<'_, GlobalAttrs> {
    let raw = |m: &AttrMap| {
        m.iter()
            .any(|(k, v)| charset::has_raw(k) || charset::has_raw(v))
    };
    if !(raw(&globals.graph) || raw(&globals.node) || raw(&globals.edge)) {
        return Cow::Borrowed(globals);
    }
    let map = |m: &AttrMap| {
        let mut out = AttrMap::new();
        for (k, v) in m {
            out.set(&from_text(k), from_text(v).into_owned(), m.is_html(k));
        }
        out
    };
    Cow::Owned(GlobalAttrs {
        graph: map(&globals.graph),
        node: map(&globals.node),
        edge: map(&globals.edge),
    })
}

/// Decoded output as text for the caller.
fn text(s: String) -> String {
    match charset::to_text(&s) {
        Cow::Borrowed(_) => s,
        Cow::Owned(t) => t,
    }
}

/// An error whose messages are text for the caller.
fn error_text(e: GraphvizError) -> GraphvizError {
    use GraphvizError::*;
    match e {
        SyntaxError {
            line,
            col,
            message,
            file,
        } => SyntaxError {
            line,
            col,
            message: text(message),
            file: file.map(text),
        },
        UnsupportedEngine(s) => UnsupportedEngine(text(s)),
        UnsupportedFormat(s) => UnsupportedFormat(text(s)),
        LayoutError { stage, message } => LayoutError {
            stage,
            message: text(message),
        },
        AttributeError { attr, message } => AttributeError {
            attr: text(attr),
            message: text(message),
        },
        RenderError(s) => RenderError(text(s)),
    }
}

/// The layout plugins, as `gvplugin_list(gvc, API_layout, "")` lists them, with their
/// packages.
pub const LAYOUT_PLUGINS: [(&str, &str); 11] = [
    ("circo", "neato_layout"),
    ("dot", "dot_layout"),
    ("fdp", "neato_layout"),
    ("neato", "neato_layout"),
    ("nop", "neato_layout"),
    ("nop1", "neato_layout"),
    ("nop2", "neato_layout"),
    ("osage", "neato_layout"),
    ("patchwork", "neato_layout"),
    ("sfdp", "neato_layout"),
    ("twopi", "neato_layout"),
];

/// The output device plugins: type (`format:render`) and package, in `gvplugin_list` order.
pub const DEVICE_PLUGINS: [(&str, &str); 17] = [
    ("canon:dot", "core"),
    ("dot:dot", "core"),
    ("dot_json:json", "core"),
    ("gv:dot", "core"),
    ("jpe:cairo", "cairo"),
    ("jpeg:cairo", "cairo"),
    ("jpg:cairo", "cairo"),
    ("json:json", "core"),
    ("json0:json", "core"),
    ("plain:dot", "core"),
    ("plain-ext:dot", "core"),
    ("png:cairo", "cairo"),
    ("svg:svg", "core"),
    ("xdot:xdot", "core"),
    ("xdot1.2:xdot", "core"),
    ("xdot1.4:xdot", "core"),
    ("xdot_json:json", "core"),
];

/// `gvplugin_load`'s match of a requested `type[:dependency[:package]]` against `plugins`:
/// the index of the first match. Names compare as C's `strcmp` does, case and all.
pub fn plugin_load(plugins: &[(&str, &str)], req: &str) -> Option<usize> {
    let mut parts = req.splitn(3, ':');
    let reqtyp = parts.next().unwrap_or("");
    let reqdep = parts.next();
    let reqpkg = parts.next();
    plugins.iter().position(|&(typestr, package)| {
        let (typ, dep) = match typestr.split_once(':') {
            Some((t, d)) => (t, Some(d)),
            None => (typestr, None),
        };
        typ == reqtyp
            && !matches!((dep, reqdep), (Some(d), Some(r)) if d != r)
            && reqpkg.is_none_or(|p| p == package)
    })
}

/// `gvplugin_list(gvc, api, str)`: `" type:package"` for each plugin of the requested type
/// when `str` names one with a `:`, else `" type"` for each type.
pub fn plugin_list(plugins: &[(&str, &str)], req: &str) -> String {
    let mut out = String::new();
    if let Some((typ, _)) = req.split_once(':') {
        for &(typestr, package) in plugins {
            let t = typestr.split(':').next().unwrap_or("");
            if typ.is_empty() || typ.eq_ignore_ascii_case(t) {
                out.push_str(&format!(" {typestr}:{package}"));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    let mut last: Option<&str> = None;
    for &(typestr, _) in plugins {
        let t = typestr.split(':').next().unwrap_or("");
        if last.is_none_or(|l| !l.eq_ignore_ascii_case(t)) {
            out.push(' ');
            out.push_str(t);
        }
        last = Some(t);
    }
    out
}

/// `gvlayout_select`: the engine a layout name (`-K`, the `layout` attribute) selects.
pub fn layout_select(name: &str) -> Option<LayoutEngine> {
    let i = plugins::load_layout(name)?;
    LayoutEngine::parse(LAYOUT_PLUGINS[i].0).ok()
}

/// `agget(g, name)` on the root graph: its value, the declared default, or `None` when the
/// attribute is not declared.
fn root_attr<'g>(graph: &'g Graph, name: &str) -> Option<&'g str> {
    graph.graph_attrs.get(name).map(String::as_str).or_else(|| {
        graph
            .dicts
            .first()
            .and_then(|d| d.graph.get(name))
            .map(|sym| sym.defval.as_str())
    })
}

/// gvc's layout state for a run of `dot`: the engine selected (`gvc->layout`), which a
/// graph's `layout` attribute replaces for that graph and the ones after it, and the C
/// library's generator states, which carry over from one graph's layout to the next as they
/// do in the C process ([`graphviz_pure_layout::cutil`]).
#[derive(Debug, Clone)]
pub struct LayoutJobs {
    engine: Option<LayoutEngine>,
    options: LayoutOptions,
    rand: RandState,
}

impl LayoutJobs {
    /// The state after `-K` (or the default engine) selected `engine`.
    pub fn new(engine: LayoutEngine, options: LayoutOptions) -> Self {
        LayoutJobs {
            engine: Some(engine),
            options,
            rand: RandState::default(),
        }
    }

    /// [`LayoutJobs::new`] whose layouts continue from the generator state `rand`.
    pub fn continuing(engine: LayoutEngine, options: LayoutOptions, rand: RandState) -> Self {
        LayoutJobs {
            rand,
            ..Self::new(engine, options)
        }
    }

    /// The generator state the layouts so far left.
    pub fn rand_state(&self) -> RandState {
        self.rand
    }

    /// `gvLayoutJobs`: lays `graph` out. `Ok(false)` when no layout was done (an unknown
    /// `layout` reported here, or one reported for an earlier graph), which `gvRenderJobs`
    /// then reports as `Layout was not done`.
    pub fn layout(&mut self, graph: &mut Graph) -> Result<bool> {
        if let Some(name) = root_attr(graph, "layout") {
            self.engine = layout_select(name);
            if self.engine.is_none() {
                graphviz_pure_core::diag::agerrorf(&format!(
                    "Layout type: \"{name}\" not recognized. Use one of:{}\n",
                    plugin_list(&LAYOUT_PLUGINS, name)
                ));
                return Ok(false);
            }
        }
        let Some(engine) = self.engine else {
            return Ok(false);
        };
        // `Nop` is the run's, as C's global: a `nop` engine or fdp's splines clear it.
        let options = &mut self.options;
        with_rand_state(&mut self.rand, || {
            layout_graph_in_run(graph, engine, options)
        })?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn repeated_renders_are_identical() {
        // Gradient and HTML anchor ids are per-process statics in C; a library call must
        // not see the counts left by an earlier call.
        let src = r#"digraph { a [style=filled, fillcolor="red:blue"];
            b [label=<<TABLE><TR><TD HREF="x">c</TD></TR></TABLE>>]; a -> b }"#;
        let first = render_dot(src, "dot", "svg").unwrap();
        assert_eq!(first, render_dot(src, "dot", "svg").unwrap());
        assert!(first.contains("_l_0"));
    }

    use super::*;
    use crate::decoded::GraphJobs;
    use std::fs;
    use std::path::PathBuf;

    // Issue #290: the graphs of one call share the C library's generator state, as in one C
    // process; each call (and each layout on its own) starts fresh, so results don't depend
    // on earlier calls; a caller can carry a state across calls through `LayoutJobs`.
    #[test]
    fn one_call_is_one_process_for_the_random_state() {
        let g = "graph { dim=3; a [pos=\"10,20,5\" z=\"3\"]; b [pos=\"30,40\"]; a -- b }\n";
        let opts = LayoutOptions {
            nop: 1,
            ..LayoutOptions::default()
        };
        let both = render_dot_all_with_options(
            &g.repeat(2),
            "neato",
            "dot",
            &GlobalAttrs::default(),
            opts,
        )
        .unwrap();
        assert!(both[0].contains("z=57.090927") && both[1].contains("z=121.029893"));
        let again = render_dot_all_with_options(
            &g.repeat(2),
            "neato",
            "dot",
            &GlobalAttrs::default(),
            opts,
        )
        .unwrap();
        assert_eq!(both, again);
        // Inside a caller's run, every entry point continues the caller's state and leaves it
        // advanced.
        let mut state = RandState::default();
        let outer = with_rand_state(&mut state, || {
            render_dot_all_with_options(&g.repeat(2), "neato", "dot", &GlobalAttrs::default(), opts)
        })
        .unwrap();
        assert_eq!(outer, both);
        assert_ne!(state, RandState::default());
        let (third, _) = with_rand_state(&mut state, || {
            render_dot_all_partial(g, "neato", "dot", &GlobalAttrs::default(), opts)
        });
        assert!(!third[0].contains("z=57.090927") && !third[0].contains("z=121.029893"));
        let (svgs, _) = with_rand_state(&mut RandState::default(), || {
            render_svg_all_partial(&g.repeat(2), "neato", &GlobalAttrs::default(), opts)
        });
        assert_eq!(svgs.len(), 2);

        let mut jobs = LayoutJobs::new(LayoutEngine::Neato, opts);
        let mut first = parse_dot(g).unwrap();
        jobs.layout(&mut first).unwrap();
        let mut graph = parse_dot(g).unwrap();
        let mut carried = LayoutJobs::continuing(LayoutEngine::Neato, opts, jobs.rand_state());
        carried.layout(&mut graph).unwrap();
        let z = |g: &Graph| g.nodes[1].attrs["z"].clone();
        assert_eq!(z(&graph), "121.029893");
        let mut alone = parse_dot(g).unwrap();
        layout_graph_with_options(&mut alone, LayoutEngine::Neato, opts).unwrap();
        assert_eq!(z(&alone), "57.090927");
    }

    // Issue #332: `Nop` belongs to the run, as C's global: after a `nop` engine
    // (`gvlayout_neato_layout.c:48-57`) or fdp with splines (`fdpgen/layout.c:1057`), `-n2`
    // no longer applies to the call's later graphs, nor to a `LayoutJobs`' later layouts.
    #[test]
    fn nop_engines_clear_the_runs_nop() {
        let opts = LayoutOptions {
            nop: 2,
            ..LayoutOptions::default()
        };
        let b = "graph B { layout=neato; b; c; b -- c }\n";
        let laid_out = "node b 0.375 0.39777 0.75 0.5 b solid ellipse black lightgrey\n";
        for a in [
            "graph A { layout=nop2; a [pos=\"0,0\"] }\n",
            "graph A { layout=fdp; splines=true; a -- x }\n",
        ] {
            let src = format!("{a}{b}");
            let all =
                render_dot_all_with_options(&src, "neato", "plain", &GlobalAttrs::default(), opts)
                    .unwrap();
            assert!(all[1].contains(laid_out), "{}", all[1]);
            let (all, _) =
                render_dot_all_partial(&src, "neato", "plain", &GlobalAttrs::default(), opts);
            assert!(all[1].contains(laid_out), "{}", all[1]);
            let (svgs, _) = render_svg_all_partial(&src, "neato", &GlobalAttrs::default(), opts);
            let (alone, _) = render_svg_all_partial(
                b,
                "neato",
                &GlobalAttrs::default(),
                LayoutOptions::default(),
            );
            assert_eq!(svgs[1], alone[0]);

            let mut jobs = LayoutJobs::new(LayoutEngine::Neato, opts);
            jobs.layout(&mut parse_dot(a).unwrap()).unwrap();
            let mut graph = parse_dot(b).unwrap();
            jobs.layout(&mut graph).unwrap();
            assert!(graph.nodes[0].pos != graph.nodes[1].pos);
        }
        // fdp with line edges leaves `Nop`.
        let src = format!("graph A {{ layout=fdp; a -- x }}\n{b}");
        let (all, _) = graphviz_pure_core::diag::quietly(|| {
            render_dot_all_partial(&src, "neato", "plain", &GlobalAttrs::default(), opts)
        });
        assert!(all[1].contains("node b 0 0 "), "{}", all[1]);
    }

    #[test]
    fn smoke_render_svg() {
        let svg = render_dot("digraph G { a -> b; }", "dot", "svg").unwrap();
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn render_dot_all_renders_every_graph_with_globals() {
        let mut globals = GlobalAttrs::default();
        globals.node.insert("shape".into(), "box".into());
        let outs = render_dot_all(
            "digraph G { a -> b }\ndigraph H { c -> d }",
            "dot",
            "svg",
            &globals,
        )
        .unwrap();
        assert_eq!(outs.len(), 2);
        assert!(outs[0].contains("<title>a</title>") && !outs[0].contains("<ellipse"));
        assert!(outs[1].contains("<title>c</title>"));
    }

    // Issue #378: the root functions take and return text. A real U+10FF41 is the four bytes
    // F4 8F BD 81 to C, which measures each and writes them unchanged (C 16.1.0, -Tplain). The
    // decoded entry points read the same string as the raw byte 0x41, `A`.
    #[test]
    fn text_entry_points_keep_a_real_raw_byte_range_char() {
        let src = "digraph { \"x\u{10FF41}y\" -> b [label=\"\u{10FF41}\"] }";
        let want = "graph 1 0.84307 1.7333\n\
             node x\u{10FF41}y 0.42153 1.4833 0.84307 0.5 x\u{10FF41}y solid ellipse black lightgrey\n\
             node b 0.42153 0.25 0.75 0.5 b solid ellipse black lightgrey\n\
             edge x\u{10FF41}y b 4 0.42153 1.2313 0.42153 1.0689 0.42153 0.84989 0.42153 0.66136 \
             \u{10FF41} 0.51876 0.86667 solid black\n\
             stop\n";
        assert_eq!(render_dot(src, "dot", "plain").unwrap(), want);
        // the command line's path: the file's bytes, decoded, rendered and encoded
        let bytes = decoded::render_dot(&charset::decode_slice(src.as_bytes()), "dot", "plain");
        assert_eq!(charset::encode(&bytes.unwrap()), want.as_bytes());
        let raw = decoded::render_dot(src, "dot", "plain").unwrap();
        let raw = String::from_utf8_lossy(&charset::encode(&raw)).into_owned();
        assert!(raw.contains("\nnode xAy "), "{raw}");
        let opts = LayoutOptions::default();
        let all = render_dot_all_with_options(src, "dot", "plain", &GlobalAttrs::default(), opts);
        assert_eq!(all.unwrap(), [want]);
        let (all, err) = render_dot_all_partial(src, "dot", "plain", &GlobalAttrs::default(), opts);
        assert!(err.is_none());
        assert_eq!(all, [want]);
        // globals are text too, and so is a parsed graph's source
        let mut globals = GlobalAttrs::default();
        globals.node.insert("label".into(), "\u{10FF41}".into());
        let out = render_dot_all("digraph { a }", "dot", "plain", &globals).unwrap();
        assert!(out[0].contains(" 0.5 \u{10FF41} solid "), "{}", out[0]);
        let g = parse_dot(src).unwrap();
        let name = g.nodes[0].name.as_str();
        assert_eq!(charset::encode(name), "x\u{10FF41}y".as_bytes());
        assert_eq!(
            render_graph(&g, OutputFormat::Canon)
                .unwrap()
                .matches('\u{10FF41}')
                .count(),
            2
        );
        // an error message holds the text it quotes
        let err = render_dot("digraph { a -> \u{10FF41} ; ]", "dot", "plain").unwrap_err();
        assert!(!charset::has_raw(&err.to_string()), "{err}");
    }

    #[test]
    fn test_multigraph_and_duplicate_cluster_scopes() {
        // 1. Multi-graph DOT stream (`multi.gv`)
        let multi = "digraph G { a -> b; }\ndigraph H { c -> d; }\n";
        let svg = render_dot(multi, "dot", "svg").unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<title>a</title>"));

        // 2. Same cluster name in different parent scopes (`b7.gv`)
        let b7 = r#"
            digraph G {
                subgraph cluster_0 {
                    subgraph cluster_1 {}
                    a -> c;
                }
                subgraph cluster_1 {
                    y -> z;
                    y -> q;
                }
                y -> b;
            }
        "#;
        let g = parse_dot(b7).unwrap();
        assert_eq!(g.subgraphs.len(), 3);
        let j0 = render_dot(b7, "dot", "json0").unwrap();
        assert!(j0.contains("\"_subgraph_cnt\": 3"));
    }

    #[test]
    fn test_wild_corpus_renders_without_error() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let wild_dir = manifest_dir.join("../../tests/wild");
        let mut entries: Vec<PathBuf> = fs::read_dir(&wild_dir)
            .expect("tests/wild directory must exist")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "gv"))
            .collect();
        entries.sort();
        assert!(
            entries.len() >= 30,
            "expected at least 30 wild corpus fixtures"
        );

        for path in entries {
            let src = graphviz_pure_core::charset::decode(fs::read(&path).unwrap());
            let svg = render_dot(&src, "dot", "svg")
                .unwrap_or_else(|e| panic!("Failed rendering SVG for {}: {e}", path.display()));
            assert!(svg.contains("<svg"), "Missing <svg in {}", path.display());
            assert!(
                !svg.contains("NaN") && !svg.contains("Infinity"),
                "Non-finite coordinate in {}",
                path.display()
            );

            let j0 = render_dot(&src, "dot", "json0")
                .unwrap_or_else(|e| panic!("Failed rendering json0 for {}: {e}", path.display()));
            assert!(j0.contains("\"bb\":"), "Missing bb in {}", path.display());
        }
    }

    /// A scratch directory with a 40x20 PNG header and an EPS file whose `%%BoundingBox` is
    /// 100x50, and the paths of the two.
    fn image_dir(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("gvp-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        write_png(&dir.join("a.png"), 40, 20);
        fs::write(dir.join("box.eps"), "%%BoundingBox: 10 20 110 70\n").unwrap();
        (dir.join("a.png"), dir.join("box.eps"))
    }

    fn write_png(path: &std::path::Path, w: u32, h: u32) {
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend(w.to_be_bytes());
        png.extend(h.to_be_bytes());
        png.extend([8, 2, 0, 0, 0, 0, 0, 0, 0]);
        fs::write(path, png).unwrap();
    }

    /// `-Tdot` of the epsf graph `src` as the CLI writes it, with C's PostScript call, in a
    /// run of its own.
    fn epsf_run(src: &str) -> String {
        graphviz_pure_render::reset_render_state();
        let mut g = parse_dot(src).unwrap();
        layout_graph(&mut g, LayoutEngine::Dot).unwrap();
        let mut jobs = GraphJobs::new(&g);
        jobs.write_output_file_text(true);
        jobs.render(OutputFormat::Dot, "dot").unwrap()
    }

    // #377: where C writes an xdot key with no value (invalid JSON), the library keeps one
    // JSON document: the value is `""`. Only the CLI writes C's bytes.
    #[test]
    fn json_with_an_unparsable_xdot_value_parses() {
        let src = "digraph { a [_tldraw_=\"zz\"]; a -> b [_hdraw_=\"zz\"] }";
        for format in ["json", "xdot_json", "json0", "dot_json"] {
            let out = render_dot(src, "dot", format).unwrap();
            let doc: serde_json::Value =
                serde_json::from_str(&out).unwrap_or_else(|e| panic!("{format}: {e}\n{out}"));
            let nodes = doc["objects"].as_array().unwrap();
            let a = nodes.iter().find(|o| o["name"] == "a").unwrap();
            let want = if format.contains("json0") || format == "dot_json" {
                "zz"
            } else {
                ""
            };
            assert_eq!(a["_tldraw_"], want, "{format}");
        }
    }

    // #347 review: a call is one C process. The files it read and the epsf files it
    // numbered are its own, and another thread has its own runs.
    #[test]
    fn usershape_state_is_per_run() {
        let (png, eps) = image_dir("lib-runs");
        let src = format!("digraph {{ a [image=\"{}\" label=\"\"] }}", png.display());
        let first = render_dot(&src, "dot", "dot").unwrap();
        assert!(first.contains("height=0.5,"), "{first}");
        write_png(&png, 400, 200);
        let second = render_dot(&src, "dot", "dot").unwrap();
        assert!(second.contains("height=2.9856,"), "{second}");
        let src = format!(
            "digraph {{ e [shape=epsf shapefile=\"{}\"] }}",
            eps.display()
        );
        for _ in 0..2 {
            render_dot(&src, "dot", "dot").unwrap();
        }
        let call = "-10 -20 translate newpath user_shape_0\n";
        assert!(epsf_run(&src).starts_with(call));
        let other = std::thread::spawn(move || epsf_run(&src)).join().unwrap();
        assert!(other.starts_with(call), "{other}");
    }

    // #347 review: the renderer draws the images the layout found, from the graph, whatever
    // was laid out since and on whichever thread it draws.
    #[test]
    fn images_are_drawn_from_the_layout_result() {
        let (png, _) = image_dir("lib-render");
        let src = format!("digraph {{ a [image=\"{}\"] }}", png.display());
        let mut a = parse_dot(&src).unwrap();
        layout_graph(&mut a, LayoutEngine::Dot).unwrap();
        let mut b = parse_dot("digraph { imagepath=\"nowhere\"; x [image=\"none.png\"] }").unwrap();
        layout_graph(&mut b, LayoutEngine::Dot).unwrap();
        let image = format!("<image xlink:href=\"{}\" width=\"40px\"", png.display());
        let svg = render_graph(&a, OutputFormat::Svg).unwrap();
        assert!(svg.contains(&image), "{svg}");
        let svg = std::thread::spawn(move || render_graph(&a, OutputFormat::Svg).unwrap())
            .join()
            .unwrap();
        assert!(svg.contains(&image), "{svg}");
    }

    // #347 review: each job keeps its own loadimage plugin. The PostScript image comes first,
    // so neither svg job has a plugin for it yet, and neither draws it.
    #[test]
    fn jobs_keep_their_own_loadimage_plugin() {
        let (png, eps) = image_dir("lib-jobs");
        let src = format!(
            "digraph {{ a [image=\"{}\"]; b [image=\"{}\"] }}",
            eps.display(),
            png.display()
        );
        graphviz_pure_render::reset_render_state();
        let mut g = parse_dot(&src).unwrap();
        layout_graph(&mut g, LayoutEngine::Dot).unwrap();
        let mut jobs = GraphJobs::new(&g);
        let first = jobs.render_as(0, OutputFormat::Svg, "svg").unwrap();
        let second = jobs.render_as(1, OutputFormat::Svg, "svg").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.matches("<image ").count(), 1, "{first}");
    }

    // #347 review: the library's output is the writer's document: no PostScript call from
    // `epsf_gencode`, which C writes into its output file.
    #[test]
    fn epsf_output_is_a_document() {
        let (_, eps) = image_dir("lib-doc");
        let src = format!(
            "digraph {{ e [shape=epsf shapefile=\"{}\"] }}",
            eps.display()
        );
        let json = render_dot(&src, "dot", "json").unwrap();
        serde_json::from_str::<serde_json::Value>(&json).unwrap();
        assert!(
            render_dot(&src, "dot", "dot")
                .unwrap()
                .starts_with("digraph {")
        );
        assert!(!render_dot(&src, "dot", "svg").unwrap().contains("newpath"));
    }
}
