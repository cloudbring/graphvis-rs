//! Node shape classification, text/record/HTML-table sizing, polygon vertex generation,
//! and ray-to-boundary intersection ported from Graphviz 16.1.0 (`lib/common/shapes.c`
//! and `lib/common/htmltable.c`).

use crate::html_label::{
    HtmlContent, HtmlLabel, HtmlLabelLayout, HtmlTbl, HtmlText, make_html_label,
};
use crate::node_size::{LabelGeometry, NodeSizeEnv, node_font, node_geometry};
use gv_core::font::{LabelSubstContext, TextJustify};
use gv_core::{BoundingBox, NodeData, Point, Port};
use std::f64::consts::{FRAC_PI_2, PI, SQRT_2};

/// Points per inch in Graphviz (`72.0`).
pub const POINTS_PER_INCH: f64 = 72.0;

/// Default node width in inches (`0.75`).
pub const DEFAULT_NODE_WIDTH: f64 = 0.75;

/// Default node height in inches (`0.5`).
pub const DEFAULT_NODE_HEIGHT: f64 = 0.5;

/// Minimum node width in inches (`0.01`).
pub const MIN_NODE_WIDTH: f64 = 0.01;

/// Minimum node height in inches (`0.02`).
pub const MIN_NODE_HEIGHT: f64 = 0.02;

/// Default `point` shape diameter in inches (`0.05`).
pub const DEF_POINT_SIZE: f64 = 0.05;

/// Minimum `point` shape diameter in inches (`0.0003`).
pub const MIN_POINT_SIZE: f64 = 0.0003;

/// Default horizontal node margin in inches (`2 * GAP = 8.0 pt = 8.0 / 72.0 in` per side, matching `PAD(d)` in `lib/common/macros.h`).
pub const DEFAULT_MARGIN_X: f64 = 8.0 / 72.0;

/// Default vertical node margin in inches (`GAP = 4.0 pt = 4.0 / 72.0 in` per side, matching `PAD(d)` in `lib/common/macros.h`).
pub const DEFAULT_MARGIN_Y: f64 = 4.0 / 72.0;

/// Default periphery gap in points (`4.0`).
pub const PERIPHERY_GAP_PTS: f64 = 4.0;

/// Classification of Graphviz node shapes ported from `Shapes[]` in `lib/common/shapes.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShapeKind {
    #[default]
    Ellipse,
    Circle,
    DoubleCircle,
    Mcircle,
    Box,
    Square,
    Msquare,
    Polygon,
    Diamond,
    Mdiamond,
    Trapezium,
    InvTrapezium,
    Parallelogram,
    House,
    InvHouse,
    Pentagon,
    Hexagon,
    Septagon,
    Octagon,
    DoubleOctagon,
    TripleOctagon,
    Record,
    Mrecord,
    Plaintext,
    Plain,
    None,
    Point,
    Egg,
    Triangle,
    InvTriangle,
    Cylinder,
    Note,
    Tab,
    Folder,
    Box3d,
    Component,
    Underline,
    Cds,
    Star,
    Promoter,
    Terminator,
    Utr,
    Insulator,
    Ribosite,
    Rnastab,
    Proteasesite,
    Proteinstab,
    Primersite,
    Restrictionsite,
    Fivepoverhang,
    Threepoverhang,
    Noverhang,
    Assembly,
    Signature,
    Rpromoter,
    Lpromoter,
    Rarrow,
    Larrow,
}

impl ShapeKind {
    /// Parses a Graphviz shape name into a [`ShapeKind`].
    ///
    /// Matches `bind_shape` in `shapes.c`: names are case-sensitive, an empty name is the
    /// default `ellipse`, and unrecognized names fall back to `Shapes[0]` (`box`).
    pub fn parse(name: &str) -> Self {
        if name.is_empty() {
            // `late_nnstring(n, N_shape, DEFAULT_NODESHAPE)`
            return Self::Ellipse;
        }
        match name {
            "box" | "rect" | "rectangle" => Self::Box,
            "polygon" => Self::Polygon,
            "ellipse" | "oval" => Self::Ellipse,
            "circle" => Self::Circle,
            "doublecircle" => Self::DoubleCircle,
            "Mcircle" => Self::Mcircle,
            "point" => Self::Point,
            "egg" => Self::Egg,
            "triangle" => Self::Triangle,
            "invtriangle" => Self::InvTriangle,
            "plaintext" => Self::Plaintext,
            "plain" => Self::Plain,
            "none" => Self::None,
            "diamond" => Self::Diamond,
            "Mdiamond" => Self::Mdiamond,
            "trapezium" => Self::Trapezium,
            "invtrapezium" => Self::InvTrapezium,
            "parallelogram" => Self::Parallelogram,
            "house" => Self::House,
            "invhouse" => Self::InvHouse,
            "pentagon" => Self::Pentagon,
            "hexagon" => Self::Hexagon,
            "septagon" => Self::Septagon,
            "octagon" => Self::Octagon,
            "doubleoctagon" => Self::DoubleOctagon,
            "tripleoctagon" => Self::TripleOctagon,
            "square" => Self::Square,
            "Msquare" => Self::Msquare,
            "record" => Self::Record,
            "Mrecord" => Self::Mrecord,
            "cylinder" => Self::Cylinder,
            "note" => Self::Note,
            "tab" => Self::Tab,
            "folder" => Self::Folder,
            "box3d" => Self::Box3d,
            "component" => Self::Component,
            "underline" => Self::Underline,
            "cds" => Self::Cds,
            "star" => Self::Star,
            "promoter" => Self::Promoter,
            "terminator" => Self::Terminator,
            "utr" => Self::Utr,
            "insulator" => Self::Insulator,
            "ribosite" => Self::Ribosite,
            "rnastab" => Self::Rnastab,
            "proteasesite" => Self::Proteasesite,
            "proteinstab" => Self::Proteinstab,
            "primersite" => Self::Primersite,
            "restrictionsite" => Self::Restrictionsite,
            "fivepoverhang" => Self::Fivepoverhang,
            "threepoverhang" => Self::Threepoverhang,
            "noverhang" => Self::Noverhang,
            "assembly" => Self::Assembly,
            "signature" => Self::Signature,
            "rpromoter" => Self::Rpromoter,
            "lpromoter" => Self::Lpromoter,
            "rarrow" => Self::Rarrow,
            "larrow" => Self::Larrow,
            // `bind_shape`: unknown names (and wrong case) get `Shapes[0]`, i.e. `box`.
            _ => Self::Box,
        }
    }

    /// Resolves the [`ShapeKind`] of a [`NodeData`] from its `"shape"` attribute.
    #[inline]
    pub fn from_node(node: &NodeData) -> Self {
        node.attrs
            .get("shape")
            .map_or(Self::Ellipse, |s| Self::parse(s))
    }

    /// Returns `true` if this shape is `record` or `Mrecord`.
    #[inline]
    pub const fn is_record(self) -> bool {
        matches!(self, Self::Record | Self::Mrecord)
    }

    /// Returns `true` if this shape has no visible outline (`plaintext`, `plain`, `none`).
    #[inline]
    pub const fn is_borderless(self) -> bool {
        matches!(self, Self::Plaintext | Self::Plain | Self::None)
    }

    /// Returns `true` if the shape enforces `width == height` by default.
    #[inline]
    pub const fn is_regular_by_default(self) -> bool {
        matches!(
            self,
            Self::Circle
                | Self::DoubleCircle
                | Self::Mcircle
                | Self::Square
                | Self::Msquare
                | Self::Point
        )
    }

    /// Returns the default `peripheries` count for this shape.
    #[inline]
    pub const fn default_peripheries(self) -> usize {
        match self {
            Self::Plaintext | Self::Plain | Self::None => 0,
            Self::DoubleCircle | Self::DoubleOctagon => 2,
            Self::TripleOctagon => 3,
            _ => 1,
        }
    }

    /// Returns the default number of polygon sides (`1` or `2` for ellipses/circles).
    #[inline]
    pub const fn default_sides(self) -> usize {
        match self {
            Self::Ellipse
            | Self::Circle
            | Self::DoubleCircle
            | Self::Mcircle
            | Self::Point
            | Self::Egg => 1,
            Self::Polygon => 0,
            Self::Triangle | Self::InvTriangle => 3,
            Self::Pentagon | Self::House | Self::InvHouse => 5,
            Self::Hexagon => 6,
            Self::Septagon => 7,
            Self::Octagon | Self::DoubleOctagon | Self::TripleOctagon => 8,
            Self::Star => 10,
            Self::Cylinder => 19,
            _ => 4,
        }
    }

    /// Returns the built-in orientation angle in degrees (`shapes.c`).
    #[inline]
    pub const fn default_orientation(self) -> f64 {
        match self {
            Self::Diamond | Self::Mdiamond => 45.0,
            Self::InvTriangle | Self::InvTrapezium | Self::InvHouse => 180.0,
            _ => 0.0,
        }
    }

    /// Returns the built-in distortion factor (`shapes.c`).
    #[inline]
    pub const fn default_distortion(self) -> f64 {
        match self {
            Self::Egg => -0.3,
            Self::Trapezium | Self::InvTrapezium => -0.4,
            Self::House | Self::InvHouse => -0.64,
            _ => 0.0,
        }
    }

    /// Returns the built-in skew factor (`shapes.c`).
    #[inline]
    pub const fn default_skew(self) -> f64 {
        match self {
            Self::Parallelogram => 0.6,
            _ => 0.0,
        }
    }
}

/// A parsed and laid-out field in a `record` or `Mrecord` node.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordField {
    /// Optional port identifier from `<port>` syntax.
    pub port: Option<String>,
    /// Unescaped text content if this is a leaf field.
    pub text: String,
    /// Bounding box in points relative to the node center `(0, 0)`.
    pub bbox: BoundingBox,
    /// Nested sub-fields if this is a `{ ... }` group or the root record.
    pub children: Vec<RecordField>,
    /// `true` if `children` are arranged horizontally (`left-to-right`), `false` if vertically (`top-to-bottom`).
    pub horizontal: bool,
    /// Label of a leaf field (`fp->lp`): `dimen` and the justification `space`.
    pub label: Option<LabelGeometry>,
}

impl RecordField {
    /// Searches recursively for a sub-field whose `port` matches `port_name`.
    pub fn find_port(&self, port_name: &str) -> Option<&RecordField> {
        if self.port.as_deref() == Some(port_name) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_port(port_name) {
                return Some(found);
            }
        }
        None
    }
}

/// A laid-out cell (`<TD>`) within an HTML-like table label.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlCell {
    /// Optional port name (`PORT="..."`).
    pub port: Option<String>,
    /// Plain/multiline text content inside the cell.
    pub text: String,
    /// Row index (`0`-based).
    pub row: usize,
    /// Column index (`0`-based).
    pub col: usize,
    /// Row span (`>= 1`).
    pub rowspan: usize,
    /// Column span (`>= 1`).
    pub colspan: usize,
    /// Cell border thickness in points.
    pub border: f64,
    /// Cell padding in points.
    pub cellpadding: f64,
    /// Optional background color (`BGCOLOR="..."`).
    pub bgcolor: Option<String>,
    /// Horizontal alignment (`Center`, `Left`, `Right`).
    pub align: TextJustify,
    /// Bounding box in points relative to the node center `(0, 0)`.
    pub bbox: BoundingBox,
    /// Optional nested `<TABLE>` inside this cell.
    pub nested_table: Option<HtmlTable>,
    /// Sized and positioned rich text of a text cell (box in label coordinates); draw it with
    /// [`crate::html_label::emit_htextspans`].
    pub rich_text: Option<HtmlText>,
}

/// A parsed and laid-out HTML-like `<TABLE>` label.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlTable {
    /// Optional table-level port (`PORT="..."`).
    pub port: Option<String>,
    /// Outer table border thickness in points (`BORDER`, default `1.0`).
    pub border: f64,
    /// Default cell border thickness (`CELLBORDER`, if specified).
    pub cellborder: Option<f64>,
    /// Spacing between cells in points (`CELLSPACING`, default `2.0`).
    pub cellspacing: f64,
    /// Default padding inside cells in points (`CELLPADDING`, default `2.0`).
    pub cellpadding: f64,
    /// Optional table background color (`BGCOLOR="..."`).
    pub bgcolor: Option<String>,
    /// Rounded style flag (`STYLE="ROUNDED"`).
    pub rounded: bool,
    /// Number of rows in the table.
    pub row_count: usize,
    /// Number of columns in the table.
    pub col_count: usize,
    /// Bounding box in points relative to the node center `(0, 0)`.
    pub bbox: BoundingBox,
    /// Laid-out cells in reading order.
    pub cells: Vec<HtmlTableCellOrCell>,
}

type HtmlTableCellOrCell = HtmlCell;

impl HtmlTable {
    /// Finds the bounding box (relative to node center) of a port by name (case-insensitive, matching `htmltable.c`).
    pub fn find_port_box(&self, port_name: &str) -> Option<BoundingBox> {
        if self
            .port
            .as_deref()
            .is_some_and(|p| p.eq_ignore_ascii_case(port_name))
        {
            return Some(self.bbox);
        }
        for cell in &self.cells {
            if cell
                .port
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(port_name))
            {
                return Some(cell.bbox);
            }
            if let Some(nested) = &cell.nested_table
                && let Some(bb) = nested.find_port_box(port_name)
            {
                return Some(bb);
            }
        }
        None
    }
}

/// Parses a boolean attribute following Graphviz `mapbool` semantics.
pub fn parse_graphviz_bool(val: Option<&str>, default: bool) -> bool {
    let Some(s) = val else {
        return default;
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return default;
    }
    if trimmed.eq_ignore_ascii_case("true")
        || trimmed.eq_ignore_ascii_case("yes")
        || trimmed.eq_ignore_ascii_case("on")
    {
        return true;
    }
    if trimmed.eq_ignore_ascii_case("false")
        || trimmed.eq_ignore_ascii_case("no")
        || trimmed.eq_ignore_ascii_case("off")
    {
        return false;
    }
    if let Ok(n) = trimmed.parse::<f64>() {
        return n != 0.0;
    }
    default
}

/// Parses the `(margin_x, margin_y)` attribute in inches, defaulting to `(8.0 / 72.0, 4.0 / 72.0)`.
pub fn parse_node_margin(margin_attr: Option<&str>) -> (f64, f64) {
    let Some(raw) = margin_attr else {
        return (DEFAULT_MARGIN_X, DEFAULT_MARGIN_Y);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (DEFAULT_MARGIN_X, DEFAULT_MARGIN_Y);
    }
    let mut parts = trimmed.split([' ', ',']).filter(|p| !p.is_empty());
    let Some(first_str) = parts.next() else {
        return (DEFAULT_MARGIN_X, DEFAULT_MARGIN_Y);
    };
    let Ok(mx) = first_str.parse::<f64>() else {
        return (DEFAULT_MARGIN_X, DEFAULT_MARGIN_Y);
    };
    let mx = mx.max(0.0);
    if let Some(second_str) = parts.next()
        && let Ok(my) = second_str.parse::<f64>()
    {
        return (mx, my.max(0.0));
    }
    (mx, mx)
}

/// Returns `true` if `label` is an HTML-like label (`<...>`), accounting for `record` port syntax.
pub fn is_html_label(label: &str, shape: ShapeKind) -> bool {
    let trimmed = label.trim();
    if !(trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2) {
        return false;
    }
    // Verify balanced angle brackets where outer `<` closes only at the end.
    let mut depth = 0usize;
    for (idx, ch) in trimmed.char_indices() {
        if ch == '<' {
            depth += 1;
        } else if ch == '>' {
            if depth == 0 {
                return false;
            }
            depth -= 1;
            if depth == 0 && idx + ch.len_utf8() < trimmed.len() {
                return false;
            }
        }
    }
    if depth != 0 {
        return false;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    if shape.is_record() {
        // In record shapes, `<p1>` is a port name unless inner contains HTML tags.
        let lower = inner.to_ascii_lowercase();
        lower.contains("<table")
            || lower.contains("<tr")
            || lower.contains("<td")
            || lower.contains("<br")
            || lower.contains("<font")
            || lower.contains("<b>")
            || lower.contains("<i>")
            || lower.contains("<u>")
    } else {
        true
    }
}

pub fn parse_f64_strip_units(raw: &str) -> Option<f64> {
    let t = raw.trim();
    let stripped = t
        .strip_suffix("pt")
        .or_else(|| t.strip_suffix("PT"))
        .or_else(|| t.strip_suffix("px"))
        .or_else(|| t.strip_suffix("PX"))
        .or_else(|| t.strip_suffix("in"))
        .or_else(|| t.strip_suffix("IN"))
        .unwrap_or(t)
        .trim();
    stripped.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Computes `node.width` and `node.height` (in inches) from the node's label, font, margin,
/// shape, `fixedsize`, `regular`, and `peripheries` attributes, matching `lib/common/shapes.c`.
pub fn compute_node_dimensions(node: &mut NodeData, graph_name: &str) {
    let rankdir = node
        .attrs
        .get("_rankdir")
        .map_or("TB", |s| s.as_str())
        .to_string();
    compute_node_dimensions_with_rankdir(node, graph_name, &rankdir);
}

/// Computes `node.width` and `node.height` (in inches) with an explicit `rankdir` (`"TB"`, `"LR"`, `"BT"`, `"RL"`).
pub fn compute_node_dimensions_with_rankdir(node: &mut NodeData, graph_name: &str, rankdir: &str) {
    let env = NodeSizeEnv::with_rankdir(graph_name, rankdir);
    let geom = node_geometry(node, &env);
    node.width = geom.width;
    node.height = geom.height;
}

fn star_adjusted_bb(mut sz: Point) -> Point {
    if sz.x <= 1e-9 || sz.y <= 1e-9 {
        return sz;
    }
    let alpha = PI / 10.0;
    let alpha3 = 3.0 * alpha;
    let aspect = (1.0 + alpha3.sin()) / (2.0 * alpha.cos());
    let a = sz.y / sz.x;
    if a > aspect {
        sz.x = sz.y / aspect;
    } else if a < aspect {
        sz.y = sz.x * aspect;
    }
    sz
}

fn star_vertices_local(bb: Point) -> Vec<Point> {
    let sz = star_adjusted_bb(bb);
    let alpha = PI / 10.0;
    let alpha2 = 2.0 * alpha;
    let alpha3 = 3.0 * alpha;
    let alpha4 = 4.0 * alpha;
    let r = sz.x / (2.0 * alpha.cos());
    let r0 = r * alpha.cos() * alpha4.cos() / (alpha4.sin() * alpha2.cos());
    let offset = (r * (1.0 - alpha3.sin())) / 2.0;

    let mut verts = Vec::with_capacity(10);
    let mut theta = alpha;
    for _ in 0..5 {
        verts.push(Point::new(r * theta.cos(), r * theta.sin() - offset));
        theta += alpha2;
        verts.push(Point::new(r0 * theta.cos(), r0 * theta.sin() - offset));
        theta += alpha2;
    }
    let scale_x = if sz.x > 1e-9 { bb.x / sz.x } else { 1.0 };
    let scale_y = if sz.y > 1e-9 { bb.y / sz.y } else { 1.0 };
    for v in &mut verts {
        v.x *= scale_x;
        v.y *= scale_y;
    }
    verts
}

fn compute_raw_polygon_vertices(
    sides: usize,
    orientation_deg: f64,
    distortion: f64,
    skew: f64,
    bb_x: f64,
    bb_y: f64,
) -> (Vec<Point>, f64, f64) {
    let sides = sides.max(3);
    let sectorangle = 2.0 * PI / (sides as f64);
    let sidelength = (sectorangle / 2.0).sin();
    let skewdist = (distortion.abs() + skew.abs()).hypot(1.0);
    let gdistortion = distortion * SQRT_2 / (sectorangle / 2.0).cos();
    let gskew = skew / 2.0;
    let mut angle = (sectorangle - PI) / 2.0;
    let mut r_pt = Point::new(0.5 * angle.cos(), 0.5 * angle.sin());
    let mut xmax = 0.0_f64;
    let mut ymax = 0.0_f64;
    angle += (PI - sectorangle) / 2.0;

    let mut vertices = Vec::with_capacity(sides);
    let orient_rad = orientation_deg.to_radians();

    for _ in 0..sides {
        angle += sectorangle;
        r_pt.x += sidelength * angle.cos();
        r_pt.y += sidelength * angle.sin();

        let px = r_pt.x * (skewdist + r_pt.y * gdistortion) + r_pt.y * gskew;
        let py = r_pt.y;

        let alpha = orient_rad + py.atan2(px);
        let rad = px.hypot(py);
        let sx = rad * alpha.cos() * bb_x;
        let sy = rad * alpha.sin() * bb_y;

        xmax = xmax.max(sx.abs());
        ymax = ymax.max(sy.abs());
        vertices.push(Point::new(sx, sy));
    }

    (vertices, xmax, ymax)
}

/// Computes the polygon vertices in node-local coordinates (centered at `(0, 0)`)
/// for polygonal node shapes, scaled to the node's `(width * 72, height * 72)` bounding box.
pub fn shape_polygon_vertices(node: &NodeData) -> Vec<Point> {
    let shape = ShapeKind::from_node(node);
    let w_pts = (node.width * POINTS_PER_INCH).max(1.0);
    let h_pts = (node.height * POINTS_PER_INCH).max(1.0);
    let rx = w_pts / 2.0;
    let ry = h_pts / 2.0;

    if shape == ShapeKind::Star {
        return star_vertices_local(Point::new(w_pts, h_pts));
    }

    let mut sides = shape.default_sides();
    let orientation = shape.default_orientation()
        + node
            .attrs
            .get("orientation")
            .and_then(|s| parse_f64_strip_units(s))
            .unwrap_or(0.0);
    let mut skew = shape.default_skew();
    let mut distortion = shape.default_distortion();
    if sides == 0 {
        sides = node
            .attrs
            .get("sides")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(4);
        skew = node
            .attrs
            .get("skew")
            .and_then(|s| parse_f64_strip_units(s))
            .unwrap_or(0.0);
        distortion = node
            .attrs
            .get("distortion")
            .and_then(|s| parse_f64_strip_units(s))
            .unwrap_or(0.0);
    }
    if sides <= 2 && (distortion.abs() > 1e-9 || skew.abs() > 1e-9) {
        sides = 120;
    }

    let is_box = sides == 4
        && (orientation.rem_euclid(90.0)).abs() < 0.5
        && distortion.abs() <= 1e-9
        && skew.abs() <= 1e-9;

    if is_box || shape.is_record() || shape == ShapeKind::Cylinder {
        return vec![
            Point::new(rx, ry),
            Point::new(-rx, ry),
            Point::new(-rx, -ry),
            Point::new(rx, -ry),
        ];
    }

    if sides < 3 {
        return Vec::new();
    }

    let (mut verts, xmax, ymax) =
        compute_raw_polygon_vertices(sides, orientation, distortion, skew, w_pts, h_pts);
    let scale_x = if xmax > 1e-9 { rx / xmax } else { 1.0 };
    let scale_y = if ymax > 1e-9 { ry / ymax } else { 1.0 };
    for v in &mut verts {
        v.x *= scale_x;
        v.y *= scale_y;
    }
    verts
}

/// Parses and lays out a `record` / `Mrecord` label for `node` (`record_init` in `shapes.c`),
/// returning the root [`RecordField`] with all field boxes in points relative to the node centre.
pub fn record_layout(node: &NodeData, graph_name: &str, rankdir: &str) -> Option<RecordField> {
    let env = NodeSizeEnv::with_rankdir(graph_name, rankdir);
    node_geometry(node, &env).record
}

// ============================================================================
// HTML-like label & table layout (`lib/common/htmltable.c`, see `html_label.rs`)
// ============================================================================

/// Parses and lays out an HTML-like `<TABLE>` label for `node` if present.
pub fn html_table_layout(node: &NodeData, graph_name: &str) -> Option<HtmlTable> {
    let shape = ShapeKind::from_node(node);
    let raw_label = node.attrs.get("label")?;
    if !is_html_label(raw_label, shape) {
        return None;
    }
    let (font_name, font_size) = node_font(&node.attrs);
    parse_html_table(raw_label, &node.name, graph_name, font_name, font_size)
}

/// Strips the `<`...`>` delimiters the parser keeps around HTML strings.
pub(crate) fn html_inner(raw: &str) -> &str {
    if raw.len() >= 2 && raw.starts_with('<') && raw.ends_with('>') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

/// `make_html_label` for an HTML-like label value `<...>` (with delimiters).
pub(crate) fn html_label_layout(
    raw_label: &str,
    obj: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
) -> HtmlLabelLayout {
    make_html_label(html_inner(raw_label), obj, font_name, font_size, None, None)
}

/// Parses and lays out an HTML-like `<TABLE>` label string, returning [`HtmlTable`] with cell
/// boxes centered around `(0, 0)` (y up), as `pos_html_tbl` places them.
pub fn parse_html_table(
    raw_html: &str,
    node_name: &str,
    graph_name: &str,
    font_name: &str,
    font_size: f64,
) -> Option<HtmlTable> {
    let ctx = LabelSubstContext {
        graph_name,
        ..LabelSubstContext::for_node(node_name)
    };
    match html_label_layout(raw_html, &ctx, font_name, font_size).label? {
        HtmlLabel::Table(t) => Some(convert_html_tbl(&t)),
        HtmlLabel::Text(_) => None,
    }
}

fn html_text_string(t: &HtmlText) -> String {
    t.spans
        .iter()
        .map(|s| s.items.iter().map(|i| i.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn convert_html_tbl(t: &HtmlTbl) -> HtmlTable {
    HtmlTable {
        port: t.data.port.clone(),
        border: f64::from(t.data.border),
        cellborder: (t.cellborder >= 0).then(|| f64::from(t.cellborder)),
        cellspacing: f64::from(t.data.space),
        cellpadding: if t.data.flags & crate::html_label::PAD_SET != 0 {
            f64::from(t.data.pad)
        } else {
            2.0
        },
        bgcolor: t.data.bgcolor.clone(),
        rounded: t.data.style.rounded,
        row_count: t.row_count,
        col_count: t.column_count,
        bbox: t.data.bx,
        cells: t
            .cells
            .iter()
            .map(|c| HtmlCell {
                port: c.data.port.clone(),
                text: match &c.child {
                    HtmlContent::Text(tx) => html_text_string(tx),
                    _ => String::new(),
                },
                row: usize::from(c.row),
                col: usize::from(c.col),
                rowspan: usize::from(c.rowspan),
                colspan: usize::from(c.colspan),
                border: f64::from(c.data.border),
                cellpadding: f64::from(c.data.pad),
                bgcolor: c.data.bgcolor.clone(),
                align: match c.data.flags & crate::html_label::HALIGN_MASK {
                    crate::html_label::HALIGN_LEFT => TextJustify::Left,
                    crate::html_label::HALIGN_RIGHT => TextJustify::Right,
                    _ => TextJustify::Center,
                },
                bbox: c.data.bx,
                nested_table: match &c.child {
                    HtmlContent::Table(n) => Some(convert_html_tbl(n)),
                    _ => None,
                },
                rich_text: match &c.child {
                    HtmlContent::Text(tx) => Some(tx.clone()),
                    _ => None,
                },
            })
            .collect(),
    }
}

// ============================================================================
// Ray-to-boundary clipping (`clip_ray_to_node_boundary`)
// ============================================================================

/// Returns the `(dx, dy)` offset in points from `node.pos` to the center of `port`,
/// if `port.name` refers to a valid `record` field or HTML table `<TD PORT="...">` cell.
pub fn resolve_port_offset(node: &NodeData, port: &Port, graph_name: &str, rankdir: &str) -> Point {
    let Some(ref port_name) = port.name else {
        return Point::new(0.0, 0.0);
    };
    let shape = ShapeKind::from_node(node);
    let raw_label = node.attrs.get("label").map_or("\\N", |s| s.as_str());

    if shape.is_record()
        && !is_html_label(raw_label, shape)
        && let Some(rec) = record_layout(node, graph_name, rankdir)
        && let Some(field) = rec.find_port(port_name)
    {
        return Point::new(
            (field.bbox.ll.x + field.bbox.ur.x) / 2.0,
            (field.bbox.ll.y + field.bbox.ur.y) / 2.0,
        );
    }

    if is_html_label(raw_label, shape)
        && let Some(tbl) = html_table_layout(node, graph_name)
        && let Some(pbox) = tbl.find_port_box(port_name)
    {
        return Point::new((pbox.ll.x + pbox.ur.x) / 2.0, (pbox.ll.y + pbox.ur.y) / 2.0);
    }

    Point::new(0.0, 0.0)
}

/// Computes the exact intersection point on a node's boundary (`ellipse`, `box`, `polygon`,
/// `record`, or HTML table cell) along the ray from the node/port center (or compass direction)
/// toward `outside_pt`, matching `clip_and_install` / `poly_inside` / `record_inside` in C Graphviz.
pub fn clip_ray_to_node_boundary(node: &NodeData, port: &Port, outside_pt: Point) -> Point {
    clip_ray_to_node_boundary_with_context(node, port, outside_pt, "", "TB")
}

/// Computes the boundary intersection point with explicit `graph_name` and `rankdir` context
/// for resolving `record` and HTML-table ports.
pub fn clip_ray_to_node_boundary_with_context(
    node: &NodeData,
    port: &Port,
    outside_pt: Point,
    graph_name: &str,
    rankdir: &str,
) -> Point {
    let shape = ShapeKind::from_node(node);
    let w_pts = (node.width * POINTS_PER_INCH).max(MIN_POINT_SIZE * POINTS_PER_INCH);
    let h_pts = (node.height * POINTS_PER_INCH).max(MIN_POINT_SIZE * POINTS_PER_INCH);
    let rx = w_pts / 2.0;
    let ry = h_pts / 2.0;

    // 1. Check if `port.name` resolves to a sub-box inside a `record` or HTML `<TABLE>` node.
    let raw_label = node.attrs.get("label").map_or("\\N", |s| s.as_str());
    let sub_box: Option<BoundingBox> = if let Some(ref pname) = port.name {
        if shape.is_record() && !is_html_label(raw_label, shape) {
            record_layout(node, graph_name, rankdir)
                .and_then(|rec| rec.find_port(pname).map(|f| f.bbox))
        } else if is_html_label(raw_label, shape) {
            html_table_layout(node, graph_name).and_then(|tbl| tbl.find_port_box(pname))
        } else {
            None
        }
    } else {
        None
    };

    if let Some(local_box) = sub_box {
        let world_box = BoundingBox::new(
            node.pos.x + local_box.ll.x,
            node.pos.y + local_box.ll.y,
            node.pos.x + local_box.ur.x,
            node.pos.y + local_box.ur.y,
        );
        if let Some(compass_pt) = compass_point_on_box(&world_box, port.compass.as_deref()) {
            return compass_pt;
        }
        return clip_ray_to_box(
            &world_box,
            Point::new(
                (world_box.ll.x + world_box.ur.x) / 2.0,
                (world_box.ll.y + world_box.ur.y) / 2.0,
            ),
            outside_pt,
        );
    }

    // 2. If a compass point (`n`, `ne`, `e`, `se`, `s`, `sw`, `w`, `nw`, `c`) is specified on the node:
    if let Some(ref comp) = port.compass {
        let c = comp.trim().to_ascii_lowercase();
        if c == "c" || c == "_" {
            return node.pos;
        }
        if matches!(
            shape,
            ShapeKind::Box
                | ShapeKind::Record
                | ShapeKind::Mrecord
                | ShapeKind::Plain
                | ShapeKind::Note
                | ShapeKind::Tab
                | ShapeKind::Folder
                | ShapeKind::Box3d
                | ShapeKind::Component
                | ShapeKind::Cylinder
        ) {
            let nbox = BoundingBox::new(
                node.pos.x - rx,
                node.pos.y - ry,
                node.pos.x + rx,
                node.pos.y + ry,
            );
            if let Some(pt) = compass_point_on_box(&nbox, Some(&c)) {
                return pt;
            }
        }
        if let Some((dx, dy)) = compass_unit_vector(&c) {
            let far_pt = Point::new(
                node.pos.x + dx * (rx + ry + 1024.0),
                node.pos.y + dy * (rx + ry + 1024.0),
            );
            return clip_center_ray_to_shape(node, shape, rx, ry, far_pt);
        }
    }

    // 3. Default ray from `node.pos` toward `outside_pt`.
    clip_center_ray_to_shape(node, shape, rx, ry, outside_pt)
}

fn compass_unit_vector(compass: &str) -> Option<(f64, f64)> {
    let s = FRAC_PI_2.sin(); // 1.0
    let d = std::f64::consts::FRAC_1_SQRT_2;
    match compass {
        "n" => Some((0.0, s)),
        "ne" => Some((d, d)),
        "e" => Some((s, 0.0)),
        "se" => Some((d, -d)),
        "s" => Some((0.0, -s)),
        "sw" => Some((-d, -d)),
        "w" => Some((-s, 0.0)),
        "nw" => Some((-d, d)),
        _ => None,
    }
}

fn compass_point_on_box(b: &BoundingBox, compass: Option<&str>) -> Option<Point> {
    let c = compass?.trim().to_ascii_lowercase();
    let cx = (b.ll.x + b.ur.x) / 2.0;
    let cy = (b.ll.y + b.ur.y) / 2.0;
    match c.as_str() {
        "n" => Some(Point::new(cx, b.ur.y)),
        "ne" => Some(Point::new(b.ur.x, b.ur.y)),
        "e" => Some(Point::new(b.ur.x, cy)),
        "se" => Some(Point::new(b.ur.x, b.ll.y)),
        "s" => Some(Point::new(cx, b.ll.y)),
        "sw" => Some(Point::new(b.ll.x, b.ll.y)),
        "w" => Some(Point::new(b.ll.x, cy)),
        "nw" => Some(Point::new(b.ll.x, b.ur.y)),
        "c" | "_" => Some(Point::new(cx, cy)),
        _ => None,
    }
}

fn clip_center_ray_to_shape(
    node: &NodeData,
    shape: ShapeKind,
    rx: f64,
    ry: f64,
    outside_pt: Point,
) -> Point {
    let dx = outside_pt.x - node.pos.x;
    let dy = outside_pt.y - node.pos.y;
    if dx.abs() <= 1e-9 && dy.abs() <= 1e-9 {
        return Point::new(node.pos.x, node.pos.y + ry);
    }

    let distortion = shape.default_distortion()
        + node
            .attrs
            .get("distortion")
            .and_then(|s| parse_f64_strip_units(s))
            .unwrap_or(0.0);
    let skew = shape.default_skew()
        + node
            .attrs
            .get("skew")
            .and_then(|s| parse_f64_strip_units(s))
            .unwrap_or(0.0);

    // Elliptical / circular shapes without distortion/skew: exact analytical ellipse intersection.
    if matches!(
        shape,
        ShapeKind::Ellipse
            | ShapeKind::Circle
            | ShapeKind::DoubleCircle
            | ShapeKind::Mcircle
            | ShapeKind::Point
    ) && distortion.abs() <= 1e-9
        && skew.abs() <= 1e-9
    {
        let denom = ((dx / rx.max(1e-6)).powi(2) + (dy / ry.max(1e-6)).powi(2)).sqrt();
        if denom <= 1e-9 {
            return node.pos;
        }
        return Point::new(node.pos.x + dx / denom, node.pos.y + dy / denom);
    }

    // Polygonal shapes: intersect ray `(0, 0) -> (dx, dy)` with the polygon's edges.
    let poly_verts = shape_polygon_vertices(node);
    if poly_verts.len() >= 3
        && let Some(hit) = clip_ray_to_local_polygon(&poly_verts, Point::new(dx, dy))
    {
        return Point::new(node.pos.x + hit.x, node.pos.y + hit.y);
    }

    // Fallback to axis-aligned bounding box intersection (`box`, `record`, `plaintext`, `cylinder`, etc.).
    let world_box = BoundingBox::new(
        node.pos.x - rx,
        node.pos.y - ry,
        node.pos.x + rx,
        node.pos.y + ry,
    );
    clip_ray_to_box(&world_box, node.pos, outside_pt)
}

fn clip_ray_to_box(b: &BoundingBox, origin: Point, target: Point) -> Point {
    let dx = target.x - origin.x;
    let dy = target.y - origin.y;
    if dx.abs() <= 1e-9 && dy.abs() <= 1e-9 {
        return origin;
    }
    let mut t_min = f64::INFINITY;
    if dx > 1e-9 {
        t_min = t_min.min((b.ur.x - origin.x) / dx);
    } else if dx < -1e-9 {
        t_min = t_min.min((b.ll.x - origin.x) / dx);
    }
    if dy > 1e-9 {
        t_min = t_min.min((b.ur.y - origin.y) / dy);
    } else if dy < -1e-9 {
        t_min = t_min.min((b.ll.y - origin.y) / dy);
    }
    if !t_min.is_finite() || t_min < 0.0 {
        return origin;
    }
    Point::new(origin.x + dx * t_min, origin.y + dy * t_min)
}

fn clip_ray_to_local_polygon(verts: &[Point], dir: Point) -> Option<Point> {
    let n = verts.len();
    let mut best_t = f64::INFINITY;
    for i in 0..n {
        let a = verts[i];
        let b = verts[(i + 1) % n];
        let edge = Point::new(b.x - a.x, b.y - a.y);
        // Solve `t * dir = a + u * edge` for `t > 0` and `u in [0, 1]`.
        let det = dir.x * (-edge.y) - dir.y * (-edge.x);
        if det.abs() <= 1e-9 {
            continue;
        }
        let t = (a.x * (-edge.y) - a.y * (-edge.x)) / det;
        let u = (dir.x * a.y - dir.y * a.x) / det;
        if t >= 0.0 && (-1e-7..=1.0 + 1e-7).contains(&u) && t < best_t {
            best_t = t;
        }
    }
    if best_t.is_finite() {
        Some(Point::new(dir.x * best_t, dir.y * best_t))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gv_core::{AttrMap, NodeId};

    fn make_node(name: &str, attrs: &[(&str, &str)]) -> NodeData {
        let mut map = AttrMap::new();
        for &(k, v) in attrs {
            map.insert(k.to_string(), v.to_string());
        }
        NodeData {
            id: NodeId(0),
            name: name.to_string(),
            attrs: map,
            pos: Point::new(100.0, 200.0),
            width: DEFAULT_NODE_WIDTH,
            height: DEFAULT_NODE_HEIGHT,
            rank: 0,
            order: 0,
        }
    }

    #[test]
    fn test_default_ellipse_and_point_dimensions() {
        let mut n = make_node("A", &[]);
        compute_node_dimensions(&mut n, "G");
        assert!((n.width - 0.75).abs() < 1e-6);
        assert!((n.height - 0.50).abs() < 1e-6);

        let mut pt = make_node("P", &[("shape", "point")]);
        compute_node_dimensions(&mut pt, "G");
        assert!((pt.width - DEF_POINT_SIZE).abs() < 1e-6);
        assert!((pt.height - DEF_POINT_SIZE).abs() < 1e-6);
    }

    #[test]
    fn test_circle_and_doublecircle_dimensions() {
        let mut c = make_node("C", &[("shape", "circle")]);
        compute_node_dimensions(&mut c, "G");
        assert!((c.width - c.height).abs() < 1e-6);

        let mut dc = make_node("C", &[("shape", "doublecircle")]);
        compute_node_dimensions(&mut dc, "G");
        assert!((dc.width - dc.height).abs() < 1e-6);
        // Doublecircle adds 2 * 4pt = 8pt (8/72 in) over circle.
        assert!(((dc.width - c.width) * POINTS_PER_INCH - 8.0).abs() < 1e-4);
    }

    fn bbox_center(bb: &BoundingBox) -> Point {
        Point::new((bb.ll.x + bb.ur.x) / 2.0, (bb.ll.y + bb.ur.y) / 2.0)
    }

    #[test]
    fn test_record_nested_fields_and_ports() {
        let mut rec = make_node(
            "struct1",
            &[
                ("shape", "record"),
                ("label", "<f0> left|{<f1> mid|<f2> bottom}|<f3> right"),
            ],
        );
        compute_node_dimensions(&mut rec, "G");
        assert!(rec.width > 1.5);
        assert!(rec.height > 0.6);

        let layout = record_layout(&rec, "G", "TB").expect("record layout");
        assert_eq!(layout.children.len(), 3);
        let f0 = layout.find_port("f0").expect("port f0");
        let f1 = layout.find_port("f1").expect("port f1");
        let f2 = layout.find_port("f2").expect("port f2");
        let f3 = layout.find_port("f3").expect("port f3");

        assert!(bbox_center(&f0.bbox).x < bbox_center(&f1.bbox).x);
        assert!(bbox_center(&f1.bbox).x < bbox_center(&f3.bbox).x);
        assert!(bbox_center(&f1.bbox).y > bbox_center(&f2.bbox).y);
    }

    #[test]
    fn test_html_table_layout_and_port_lookup() {
        let html = r#"<<TABLE BORDER="1" CELLBORDER="1" CELLSPACING="2">
            <TR><TD PORT="p1">Header</TD><TD COLSPAN="2">Wide Cell</TD></TR>
            <TR><TD ROWSPAN="2">Tall</TD><TD>A</TD><TD PORT="p2">B</TD></TR>
            <TR><TD COLSPAN="2">Footer</TD></TR>
        </TABLE>>"#;
        let mut n = make_node("html_node", &[("shape", "plaintext"), ("label", html)]);
        compute_node_dimensions(&mut n, "G");
        assert!(n.width > 0.8);
        assert!(n.height > 0.6);

        let tbl = html_table_layout(&n, "G").expect("html table layout");
        assert_eq!(tbl.row_count, 3);
        assert_eq!(tbl.col_count, 3);
        let p1 = tbl.find_port_box("p1").expect("port p1");
        let p2 = tbl.find_port_box("p2").expect("port p2");
        assert!(bbox_center(&p1).x < bbox_center(&p2).x);
        assert!(bbox_center(&p1).y > bbox_center(&p2).y);
    }

    #[test]
    fn test_clip_ray_to_ellipse_and_box_and_compass() {
        let mut ell = make_node("E", &[("shape", "ellipse")]);
        compute_node_dimensions(&mut ell, "G");
        let rx = ell.width * POINTS_PER_INCH / 2.0;
        let ry = ell.height * POINTS_PER_INCH / 2.0;

        let east_hit = clip_ray_to_node_boundary(
            &ell,
            &Port::default(),
            Point::new(ell.pos.x + 200.0, ell.pos.y),
        );
        assert!((east_hit.x - (ell.pos.x + rx)).abs() < 1e-4);
        assert!((east_hit.y - ell.pos.y).abs() < 1e-4);

        let north_port = Port {
            name: None,
            compass: Some("n".to_string()),
        };
        let north_hit =
            clip_ray_to_node_boundary(&ell, &north_port, Point::new(ell.pos.x + 200.0, ell.pos.y));
        assert!((north_hit.x - ell.pos.x).abs() < 1e-4);
        assert!((north_hit.y - (ell.pos.y + ry)).abs() < 1e-4);
    }
}
