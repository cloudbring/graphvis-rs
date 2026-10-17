//! The render job and the `gvrender_*` layer: port of `lib/gvc/gvrender.c`, the object state
//! stack of `lib/common/emit.c` (`push_obj_state`, `pop_obj_state`) and the job geometry set
//! up by `gvRenderJobs` (`init_job_pad`, `init_job_margin`, `init_job_dpi`,
//! `init_job_viewport`, `init_job_pagination`, `setup_page`, `emit_graph`).

use super::attrs::{StyleItem, strtod};
use super::model::{Model, TextSpan};
use gv_core::{BoundingBox, Point};

pub(crate) const EMIT_SORTED: u32 = 1 << 0;
pub(crate) const EMIT_CLUSTERS_LAST: u32 = 1 << 2;
pub(crate) const EMIT_EDGE_SORTED: u32 = 1 << 4;
pub(crate) const GVDEVICE_DOES_LAYERS: u32 = 1 << 6;
pub(crate) const GVDEVICE_DOES_TRUECOLOR: u32 = 1 << 8;
pub(crate) const GVRENDER_Y_GOES_DOWN: u32 = 1 << 12;
pub(crate) const GVRENDER_DOES_TRANSFORM: u32 = 1 << 13;
pub(crate) const GVRENDER_DOES_LABELS: u32 = 1 << 15;
pub(crate) const GVRENDER_DOES_MAPS: u32 = 1 << 16;
pub(crate) const GVRENDER_DOES_TOOLTIPS: u32 = 1 << 22;
pub(crate) const GVRENDER_DOES_TARGETS: u32 = 1 << 23;
pub(crate) const GVRENDER_NO_WHITE_BG: u32 = 1 << 25;

pub(crate) const FILL: i32 = 1;
pub(crate) const GRADIENT: i32 = 2;
pub(crate) const RGRADIENT: i32 = 3;
pub(crate) const NO_POLY: i32 = 4;

pub(crate) const PENWIDTH_NORMAL: f64 = 1.0;
pub(crate) const PENWIDTH_BOLD: f64 = 2.0;

/// `emit_state_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmitState {
    GDraw = 0,
    CDraw,
    TDraw,
    HDraw,
    GLabel,
    CLabel,
    TLabel,
    HLabel,
    NDraw,
    EDraw,
    NLabel,
    ELabel,
}

/// `gvcolor_t` as resolved for the renderer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GvColor {
    Str(String),
    Rgba([u8; 4]),
}

impl Default for GvColor {
    fn default() -> Self {
        GvColor::Rgba([0, 0, 0, 0])
    }
}

impl GvColor {
    pub fn rgba(&self) -> [u8; 4] {
        match self {
            GvColor::Rgba(c) => *c,
            GvColor::Str(_) => [0, 0, 0, 255],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pen {
    None,
    Dashed,
    Dotted,
    Solid,
}

/// The object an `obj_state_t` describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjKind {
    Root,
    Cluster(usize),
    Node(usize),
    Edge(usize),
}

/// `obj_state_t` (the parts the supported renderers use).
#[derive(Debug, Clone)]
pub(crate) struct ObjState {
    pub kind: ObjKind,
    pub emit_state: EmitState,
    pub pencolor: GvColor,
    pub fillcolor: GvColor,
    pub stopcolor: GvColor,
    pub gradient_angle: i64,
    pub gradient_frac: f64,
    pub pen: Pen,
    pub penwidth: f64,
    pub rawstyle: Option<Vec<StyleItem>>,
    pub id: Option<String>,
    pub url: Option<String>,
    pub tooltip: Option<String>,
    pub target: Option<String>,
    pub labelurl: Option<String>,
    pub tailurl: Option<String>,
    pub headurl: Option<String>,
    pub labeltooltip: Option<String>,
    pub tailtooltip: Option<String>,
    pub headtooltip: Option<String>,
    pub labeltarget: Option<String>,
    pub tailtarget: Option<String>,
    pub headtarget: Option<String>,
    pub explicit_tooltip: bool,
    pub explicit_tailtooltip: bool,
    pub explicit_headtooltip: bool,
    pub explicit_labeltooltip: bool,
    pub explicit_tailurl: bool,
    pub explicit_headurl: bool,
    pub label: Option<String>,
    pub taillabel: Option<String>,
    pub headlabel: Option<String>,
    pub labeledgealigned: bool,
}

impl ObjState {
    fn new(kind: ObjKind, parent: Option<&ObjState>) -> Self {
        let mut o = ObjState {
            kind,
            emit_state: EmitState::GDraw,
            pencolor: GvColor::default(),
            fillcolor: GvColor::default(),
            stopcolor: GvColor::default(),
            gradient_angle: 0,
            gradient_frac: 0.0,
            pen: Pen::Solid,
            penwidth: PENWIDTH_NORMAL,
            rawstyle: None,
            id: None,
            url: None,
            tooltip: None,
            target: None,
            labelurl: None,
            tailurl: None,
            headurl: None,
            labeltooltip: None,
            tailtooltip: None,
            headtooltip: None,
            labeltarget: None,
            tailtarget: None,
            headtarget: None,
            explicit_tooltip: false,
            explicit_tailtooltip: false,
            explicit_headtooltip: false,
            explicit_labeltooltip: false,
            explicit_tailurl: false,
            explicit_headurl: false,
            label: None,
            taillabel: None,
            headlabel: None,
            labeledgealigned: false,
        };
        if let Some(p) = parent {
            o.pencolor = p.pencolor.clone();
            o.fillcolor = p.fillcolor.clone();
            o.pen = p.pen;
            o.penwidth = p.penwidth;
            o.gradient_angle = p.gradient_angle;
            o.stopcolor = p.stopcolor.clone();
        }
        o
    }
}

/// `gvrender_features_t` + `gvdevice_features_t`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Features {
    pub flags: u32,
    pub default_pad: f64,
    /// Sorted color names the renderer passes through as strings.
    pub knowncolors: &'static [&'static str],
    /// `RGBA_BYTE` (true) or `COLOR_STRING` (false).
    pub rgba: bool,
}

/// Render plugin callbacks (`gvrender_engine_t`).
#[allow(unused_variables)]
pub(crate) trait Engine {
    fn features(&self) -> Features;
    fn begin_job(&mut self, j: &Job) {}
    fn begin_graph(&mut self, j: &Job) {}
    fn end_graph(&mut self, j: &Job) {}
    fn begin_layer(&mut self, j: &Job, name: &str) {}
    fn end_layer(&mut self, j: &Job) {}
    fn begin_page(&mut self, j: &Job) {}
    fn end_page(&mut self, j: &Job) {}
    fn begin_cluster(&mut self, j: &Job) {}
    fn end_cluster(&mut self, j: &Job) {}
    fn begin_node(&mut self, j: &Job) {}
    fn end_node(&mut self, j: &Job) {}
    fn begin_edge(&mut self, j: &Job) {}
    fn end_edge(&mut self, j: &Job) {}
    fn begin_anchor(
        &mut self,
        j: &Job,
        href: Option<&str>,
        tooltip: Option<&str>,
        target: Option<&str>,
        id: Option<&str>,
    ) {
    }
    fn end_anchor(&mut self, j: &Job) {}
    fn textspan(&mut self, j: &Job, p: Point, span: &TextSpan) {}
    fn ellipse(&mut self, j: &Job, a: [Point; 2], filled: i32) {}
    fn polygon(&mut self, j: &Job, a: &[Point], filled: i32) {}
    fn bezier(&mut self, j: &Job, a: &[Point], filled: i32) {}
    fn polyline(&mut self, j: &Job, a: &[Point]) {}
    fn comment(&mut self, j: &Job, s: &str) {}
}

/// `GVJ_t`: render state shared with the engine callbacks.
pub(crate) struct Job<'a> {
    pub m: &'a Model<'a>,
    objs: Vec<ObjState>,
    pub flags: u32,
    pub features: Features,
    #[allow(dead_code)]
    pub pad: Point,
    pub zoom: f64,
    pub rotation: i32,
    pub width: i32,
    pub height: i32,
    pub page_bounding_box: [i32; 4],
    pub clip: BoundingBox,
    pub translation: Point,
    pub scale: Point,
    pub pages: (i32, i32),
    /// The active `colorscheme` (`setColorScheme`).
    pub color_scheme: Option<String>,
    /// `job->layerNum` and `job->numLayers`.
    pub layer_num: i32,
    pub num_layers: i32,
    /// `gvc->layerIDs` (index 0 unused).
    pub layer_ids: Vec<String>,
    pub layer_delims: String,
    pub layer_list_delims: String,
    /// `gvc->layerlist`: the selected layers.
    pub layer_list: Option<Vec<i32>>,
}

fn round(v: f64) -> i32 {
    // ROUND(f) ((f>=0)?(int)(f + .5):(int)(f - .5))
    if v >= 0.0 {
        (v + 0.5) as i32
    } else {
        (v - 0.5) as i32
    }
}

impl<'a> Job<'a> {
    /// Sets up the job geometry as `gvRenderJobs` does for a single-page device.
    pub fn new(m: &'a Model<'a>, features: Features, flags: u32) -> Self {
        let attrs = m.root_attrs();
        let scan2 = |k: &str| -> Option<(f64, f64)> {
            let s = attrs.get(k)?;
            let (a, rest) = strtod(s)?;
            let b = rest.strip_prefix(',').and_then(strtod).map_or(a, |x| x.0);
            Some((a * 72.0, b * 72.0))
        };
        // init_job_pad / init_job_margin / init_job_dpi
        let pad = scan2("pad").map_or(
            Point::new(features.default_pad, features.default_pad),
            |(x, y)| Point::new(x, y),
        );
        let margin = scan2("margin").map_or(Point::default(), |(x, y)| Point::new(x, y));
        let dpi = {
            let d = attrs
                .get("dpi")
                .or_else(|| attrs.get("resolution"))
                .and_then(strtod)
                .map_or(0.0, |x| x.0);
            if d > 0.0 { d } else { 72.0 }
        };
        // init_job_viewport
        let bb = m.bb;
        let job_bb_ll = Point::new(bb.ll.x - pad.x, bb.ll.y - pad.y);
        let job_bb_ur = Point::new(bb.ur.x + pad.x, bb.ur.y + pad.y);
        let mut sz = Point::new(job_bb_ur.x - job_bb_ll.x, job_bb_ur.y - job_bb_ll.y);
        let mut z = 1.0;
        if let Some((sx, sy, filled)) = m.drawing_size()
            && sx > 0.001
            && sy > 0.001
        {
            if sz.x <= 0.001 {
                sz.x = sx;
            }
            if sz.y <= 0.001 {
                sz.y = sy;
            }
            if sx < sz.x || sy < sz.y || (filled && sx > sz.x && sy > sz.y) {
                z = (sx / sz.x).min(sy / sz.y);
            }
        }
        let mut focus = Point::new((bb.ll.x + bb.ur.x) * 0.5, (bb.ll.y + bb.ur.y) * 0.5);
        let rotation = if m.landscape() { 90 } else { 0 };
        let mut view = Point::new(z * sz.x, z * sz.y);
        if let Some(vp) = attrs.get("viewport") {
            let mut parts = vp.split(',');
            let mut nums = [view.x, view.y, z, focus.x, focus.y];
            for slot in nums.iter_mut() {
                match parts.next().and_then(strtod) {
                    Some((v, _)) => *slot = v,
                    None => break,
                }
            }
            view = Point::new(nums[0], nums[1]);
            z = nums[2];
            focus = Point::new(nums[3], nums[4]);
        }
        // init_job_pagination (no page attribute, no paging device)
        let mut image_size = view;
        if rotation != 0 {
            image_size = Point::new(image_size.y, image_size.x);
        }
        let mut page_size = Point::new(
            (0.0 - 2.0 * margin.x).max(0.0),
            (0.0 - 2.0 * margin.y).max(0.0),
        );
        page_size.x = page_size.x.max(image_size.x);
        page_size.y = page_size.y.max(image_size.y);
        let width = round((page_size.x + 2.0 * margin.x) * dpi / 72.0);
        let height = round((page_size.y + 2.0 * margin.y) * dpi / 72.0);
        let mut centering = Point::default();
        if m.centered() {
            if page_size.x > image_size.x {
                centering.x = (page_size.x - image_size.x) / 2.0;
            }
            if page_size.y > image_size.y {
                centering.y = (page_size.y - image_size.y) / 2.0;
            }
        }
        let mut margin_g = margin;
        if rotation != 0 {
            image_size = Point::new(image_size.y, image_size.x);
            margin_g = Point::new(margin.y, margin.x);
            centering = Point::new(centering.y, centering.x);
        }
        let canvas_ll = Point::new(margin_g.x + centering.x, margin_g.y + centering.y);
        let canvas_ur = Point::new(canvas_ll.x + image_size.x, canvas_ll.y + image_size.y);
        let job_page_size = Point::new(image_size.x / z, image_size.y / z);
        let mut pbb = [
            round(canvas_ll.x * dpi / 72.0),
            round(canvas_ll.y * dpi / 72.0),
            round(canvas_ur.x * dpi / 72.0),
            round(canvas_ur.y * dpi / 72.0),
        ];
        let (mut cll, mut _cur) = (canvas_ll, canvas_ur);
        if rotation != 0 {
            pbb = [pbb[1], pbb[0], pbb[3], pbb[2]];
            cll = Point::new(canvas_ll.y, canvas_ll.x);
            _cur = Point::new(canvas_ur.y, canvas_ur.x);
        }
        // emit_graph: scale and view
        let scale = Point::new(z * dpi / 72.0, z * dpi / 72.0);
        let y_down = features.flags & GVRENDER_Y_GOES_DOWN != 0;
        let view = if rotation != 0 {
            Point::new(f64::from(height) / scale.x, f64::from(width) / scale.y)
        } else {
            Point::new(f64::from(width) / scale.x, f64::from(height) / scale.y)
        };
        let _ = view;
        // setup_page (single page)
        let (pe, ps) = ((0.0, 0.0), (1.0, 1.0));
        let clip_ll = Point::new(
            focus.x + job_page_size.x * (pe.0 - ps.0 / 2.0),
            focus.y + job_page_size.y * (pe.1 - ps.1 / 2.0),
        );
        let clip = BoundingBox {
            ll: clip_ll,
            ur: Point::new(clip_ll.x + job_page_size.x, clip_ll.y + job_page_size.y),
        };
        let translation = if rotation != 0 {
            Point::new(
                if y_down {
                    -clip.ur.x - cll.x / z
                } else {
                    -clip.ll.x + cll.x / z
                },
                -clip.ur.y - cll.y / z,
            )
        } else {
            Point::new(
                -clip.ll.x + cll.x / z,
                if y_down {
                    -clip.ur.y - cll.y / z
                } else {
                    -clip.ll.y + cll.y / z
                },
            )
        };
        Job {
            m,
            objs: Vec::new(),
            flags: flags | features.flags,
            features,
            pad,
            zoom: z,
            rotation,
            width,
            height,
            page_bounding_box: pbb,
            clip,
            translation,
            scale,
            pages: (1, 1),
            color_scheme: None,
            layer_num: 0,
            num_layers: 1,
            layer_ids: Vec::new(),
            layer_delims: ":\t ".to_string(),
            layer_list_delims: ",".to_string(),
            layer_list: None,
        }
    }

    pub fn obj(&self) -> &ObjState {
        self.objs.last().expect("object state")
    }

    pub fn obj_mut(&mut self) -> &mut ObjState {
        self.objs.last_mut().expect("object state")
    }

    pub fn push_obj(&mut self, kind: ObjKind) {
        let o = ObjState::new(kind, self.objs.last());
        self.objs.push(o);
    }

    pub fn pop_obj(&mut self) {
        self.objs.pop();
    }

    pub fn has_obj(&self) -> bool {
        !self.objs.is_empty()
    }

    /// `gvrender_resolve_color`.
    pub fn resolve_color(&self, name: &str) -> GvColor {
        let known = self
            .features
            .knowncolors
            .binary_search_by(|k| cmp_ignore_case(k, name))
            .is_ok();
        if known || !self.features.rgba {
            return GvColor::Str(name.to_string());
        }
        match gv_core::resolve_color_with_scheme(name, self.color_scheme.as_deref()) {
            Some(c) => GvColor::Rgba([c.r, c.g, c.b, c.a]),
            None => {
                gv_core::warn_unknown_color(name);
                GvColor::Rgba([0, 0, 0, 255])
            }
        }
    }
}

fn cmp_ignore_case(a: &str, b: &str) -> std::cmp::Ordering {
    let la = a.bytes().map(|c| c.to_ascii_lowercase());
    let lb = b.bytes().map(|c| c.to_ascii_lowercase());
    la.cmp(lb)
}

/// The job together with its renderer: the `gvrender_*` functions.
pub(crate) struct Emitter<'a, E: Engine> {
    pub job: Job<'a>,
    pub eng: E,
}

impl<'a, E: Engine> Emitter<'a, E> {
    pub fn set_pencolor(&mut self, name: &str) {
        let first = name.split(':').next().unwrap_or("");
        let c = self.job.resolve_color(first);
        self.job.obj_mut().pencolor = c;
    }

    pub fn set_fillcolor(&mut self, name: &str) {
        let first = name.split(':').next().unwrap_or("");
        let c = self.job.resolve_color(first);
        self.job.obj_mut().fillcolor = c;
    }

    pub fn set_gradient_vals(&mut self, stopcolor: &str, angle: i64, frac: f64) {
        let c = self.job.resolve_color(stopcolor);
        let o = self.job.obj_mut();
        o.stopcolor = c;
        o.gradient_angle = angle;
        o.gradient_frac = frac;
    }

    pub fn set_style(&mut self, s: &[StyleItem]) {
        let o = self.job.obj_mut();
        o.rawstyle = Some(s.to_vec());
        for item in s {
            match item.name.as_str() {
                "solid" => o.pen = Pen::Solid,
                "dashed" => o.pen = Pen::Dashed,
                "dotted" => o.pen = Pen::Dotted,
                "invis" | "invisible" => o.pen = Pen::None,
                "bold" => o.penwidth = PENWIDTH_BOLD,
                "setlinewidth" => {
                    o.penwidth = item.args.first().map_or(0.0, |a| super::attrs::atof(a));
                }
                "filled" | "unfilled" | "tapered" => {}
                other => {
                    eprintln!("Warning: gvrender_set_style: unsupported style {other} - ignoring")
                }
            }
        }
    }

    pub fn set_penwidth(&mut self, w: f64) {
        self.job.obj_mut().penwidth = w;
    }

    pub fn textspan(&mut self, p: Point, span: &TextSpan) {
        if !span.str.is_empty() && (!self.job.has_obj() || self.job.obj().pen != Pen::None) {
            self.eng.textspan(&self.job, p, span);
        }
    }

    pub fn ellipse(&mut self, pf: [Point; 2], filled: i32) {
        if self.job.obj().pen != Pen::None {
            let center = Point::new((pf[0].x + pf[1].x) / 2.0, (pf[0].y + pf[1].y) / 2.0);
            self.eng.ellipse(&self.job, [center, pf[1]], filled);
        }
    }

    pub fn polygon(&mut self, a: &[Point], filled: i32) {
        if self.job.obj().pen == Pen::None {
            return;
        }
        if filled & NO_POLY != 0 {
            let save = self.job.obj().pencolor.clone();
            let fill = self.job.obj().fillcolor.clone();
            self.job.obj_mut().pencolor = fill;
            self.eng.polygon(&self.job, a, filled & !NO_POLY);
            self.job.obj_mut().pencolor = save;
        } else {
            self.eng.polygon(&self.job, a, filled);
        }
    }

    pub fn boxf(&mut self, b: BoundingBox, filled: i32) {
        let a = [
            b.ll,
            Point::new(b.ll.x, b.ur.y),
            b.ur,
            Point::new(b.ur.x, b.ll.y),
        ];
        self.polygon(&a, filled);
    }

    pub fn bezier(&mut self, a: &[Point], filled: i32) {
        if self.job.obj().pen != Pen::None {
            self.eng.bezier(&self.job, a, filled);
        }
    }

    pub fn polyline(&mut self, a: &[Point]) {
        if self.job.obj().pen != Pen::None {
            self.eng.polyline(&self.job, a);
        }
    }

    pub fn comment(&mut self, s: &str) {
        if !s.is_empty() {
            self.eng.comment(&self.job, s);
        }
    }

    pub fn begin_anchor(
        &mut self,
        href: Option<&str>,
        tooltip: Option<&str>,
        target: Option<&str>,
        id: Option<&str>,
    ) {
        self.eng.begin_anchor(&self.job, href, tooltip, target, id);
    }

    pub fn end_anchor(&mut self) {
        self.eng.end_anchor(&self.job);
    }

    /// Calls `begin_anchor` with the current object's url, tooltip, target and id.
    pub fn begin_obj_anchor(&mut self) {
        let o = self.job.obj();
        let (u, t, g, i) = (
            o.url.clone(),
            o.tooltip.clone(),
            o.target.clone(),
            o.id.clone(),
        );
        self.begin_anchor(u.as_deref(), t.as_deref(), g.as_deref(), i.as_deref());
    }
}
