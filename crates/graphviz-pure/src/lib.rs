//! High-level Graphviz orchestration engine (`graphviz-pure`).
//!
//! DOT text taken and returned here is in the port's decoded form ([`charset`]).

pub use graphviz_pure_core::{Graph, GraphvizError, Result, charset};
pub use graphviz_pure_layout::cutil::{RandState, in_rand_run, with_rand_state};
pub use graphviz_pure_layout::fdpgen::tlayout::FdpParms;
pub use graphviz_pure_layout::{
    LayoutEngine, LayoutOptions, layout_graph, layout_graph_with_options,
};
pub use graphviz_pure_parser::{GlobalAttrs, GraphStream, parse_dot, parse_dot_all};
pub use graphviz_pure_render::{
    GraphJobs, OutputFormat, render_graph, render_graph_named, set_y_invert,
};

/// End-to-end DOT rendering pipeline for any supported engine and output format. Renders the
/// first graph in `dot_source`.
///
/// `dot_source` and the output are in the port's decoded form ([`charset`]), as for every
/// function in this crate that takes or returns DOT text. For ordinary text that may hold
/// U+10FF00..=U+10FFFF, pass [`charset::from_text`]`(text)` and read the output with
/// [`charset::to_text`].
pub fn render_dot(dot_source: &str, engine: &str, format: &str) -> Result<String> {
    let output_format = OutputFormat::parse(format)?;
    // Each call is one C process: per-process counters (gradient and anchor ids) restart.
    graphviz_pure_render::reset_render_state();
    let graph = parse_dot(dot_source)?;
    render_parsed(
        graph,
        engine,
        output_format,
        format,
        LayoutOptions::default(),
    )
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
    let output_format = OutputFormat::parse(format)?;
    graphviz_pure_render::reset_render_state();
    let graphs = parse_dot_all(dot_source, globals)?;
    // One call is one C process: its graphs share the C library's generator states (the
    // caller's, inside a `with_rand_state`).
    in_rand_run(|| {
        graphs
            .into_iter()
            .map(|graph| render_parsed(graph, engine, output_format, format, options))
            .collect()
    })
}

/// [`render_dot_all_with_options`] that renders the graphs read before a syntax error
/// and returns that error alongside, as `dot` does.
pub fn render_dot_all_partial(
    dot_source: &str,
    engine: &str,
    format: &str,
    globals: &GlobalAttrs,
    options: LayoutOptions,
) -> (Vec<String>, Option<graphviz_pure_core::GraphvizError>) {
    let output_format = match OutputFormat::parse(format) {
        Ok(f) => f,
        Err(e) => return (Vec::new(), Some(e)),
    };
    graphviz_pure_render::reset_render_state();
    let (graphs, parse_err) = graphviz_pure_parser::parse_dot_all_partial(dot_source, globals);
    let mut out = Vec::with_capacity(graphs.len());
    // One call is one C process: its graphs share the C library's generator states (the
    // caller's, inside a `with_rand_state`).
    let err = in_rand_run(|| {
        for graph in graphs {
            match render_parsed(graph, engine, output_format, format, options) {
                Ok(s) => out.push(s),
                Err(e) => return Some(e),
            }
        }
        None
    });
    (out, err.or(parse_err))
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
) -> (
    Vec<(String, bool)>,
    Option<graphviz_pure_core::GraphvizError>,
) {
    graphviz_pure_render::reset_render_state();
    let (graphs, parse_err) = graphviz_pure_parser::parse_dot_all_partial(dot_source, globals);
    let mut out = Vec::with_capacity(graphs.len());
    // One call is one C process: its graphs share the C library's generator states (the
    // caller's, inside a `with_rand_state`).
    let err = in_rand_run(|| {
        for graph in graphs {
            let explicit_dpi = graphviz_pure_core::cnum::drawing_dpi(&graph.graph_attrs) > 0.0;
            match render_parsed(graph, engine, OutputFormat::Svg, "svg", options) {
                Ok(s) => out.push((s, explicit_dpi)),
                Err(e) => return Some(e),
            }
        }
        None
    });
    (out, err.or(parse_err))
}

fn render_parsed(
    mut graph: Graph,
    engine: &str,
    output_format: OutputFormat,
    format_name: &str,
    options: LayoutOptions,
) -> Result<String> {
    let engine = LayoutEngine::parse(engine)?;
    // As in C (`gvLayoutJobs`), a graph's `layout` attribute overrides the requested engine.
    let layout_engine = match graph.graph_attrs.get("layout") {
        Some(name) if !name.trim().is_empty() => LayoutEngine::parse(name.trim())?,
        _ => engine,
    };
    // `dot` lays the graph out for every format, `canon` and the json formats without
    // layout attributes included: they write the graph as the layout left it.
    layout_graph_with_options(&mut graph, layout_engine, options)?;
    render_graph_named(&graph, output_format, format_name)
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
    let i = plugin_load(&LAYOUT_PLUGINS, name)?;
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
        let options = self.options;
        with_rand_state(&mut self.rand, || {
            layout_graph_with_options(graph, engine, options)
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
