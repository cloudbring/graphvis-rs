//! Node shape classification, text/record/HTML-table sizing, polygon vertex generation,
//! and ray-to-boundary intersection ported from Graphviz 16.1.0 (`lib/common/shapes.c`
//! and `lib/common/htmltable.c`).

use gv_core::font::{
    DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, LINESPACING, LabelSubstContext, TextJustify,
    estimate_text_size_styled, measure_multiline_label_with_context, substitute_object_escapes,
};
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

/// Default horizontal node margin in inches (`0.11`, i.e., `7.92` points per side).
pub const DEFAULT_MARGIN_X: f64 = 0.11;

/// Default vertical node margin in inches (`0.055`, i.e., `3.96` points per side).
pub const DEFAULT_MARGIN_Y: f64 = 0.055;

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
    /// Unrecognized shapes fall back to [`ShapeKind::Ellipse`] (Graphviz default node shape).
    pub fn parse(name: &str) -> Self {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Self::Ellipse;
        }
        match trimmed {
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
            other => match other.to_ascii_lowercase().as_str() {
                "box" | "rect" | "rectangle" => Self::Box,
                "polygon" => Self::Polygon,
                "ellipse" | "oval" => Self::Ellipse,
                "circle" => Self::Circle,
                "doublecircle" => Self::DoubleCircle,
                "mcircle" => Self::Mcircle,
                "point" => Self::Point,
                "egg" => Self::Egg,
                "triangle" => Self::Triangle,
                "invtriangle" => Self::InvTriangle,
                "plaintext" => Self::Plaintext,
                "plain" => Self::Plain,
                "none" => Self::None,
                "diamond" => Self::Diamond,
                "mdiamond" => Self::Mdiamond,
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
                "msquare" => Self::Msquare,
                "record" => Self::Record,
                "mrecord" => Self::Mrecord,
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
                _ => Self::Ellipse,
            },
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
    pub cells: Vec<HtmlCell>,
}

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

/// Parses the `(margin_x, margin_y)` attribute in inches, defaulting to `(0.11, 0.055)`.
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
    let shape = ShapeKind::from_node(node);

    // 1. Special case: `shape=point` (`point_init` in `shapes.c`).
    if shape == ShapeKind::Point {
        let w_attr = node
            .attrs
            .get("width")
            .and_then(|s| s.trim().parse::<f64>().ok());
        let h_attr = node
            .attrs
            .get("height")
            .and_then(|s| s.trim().parse::<f64>().ok());
        let base_in = match (w_attr, h_attr) {
            (Some(w), Some(h)) => w.min(h),
            (Some(w), None) => w,
            (None, Some(h)) => h,
            (None, None) => DEF_POINT_SIZE,
        };
        let base_in = if base_in > 0.0 {
            base_in.max(MIN_POINT_SIZE)
        } else {
            0.0
        };
        let peripheries = node
            .attrs
            .get("peripheries")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(1);
        let mut sz_pts = base_in * POINTS_PER_INCH;
        if peripheries > 1 {
            sz_pts += 2.0 * ((peripheries - 1) as f64) * PERIPHERY_GAP_PTS;
        }
        let final_in = (sz_pts / POINTS_PER_INCH).max(MIN_POINT_SIZE);
        node.width = final_in;
        node.height = final_in;
        return;
    }

    // 2. Parse common font, margin, and size attributes.
    let font_name = node
        .attrs
        .get("fontname")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_FONT_NAME)
        .to_string();
    let font_size = node
        .attrs
        .get("fontsize")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(DEFAULT_FONT_SIZE)
        .max(1.0);
    let (margin_x, margin_y) = parse_node_margin(node.attrs.get("margin").map(String::as_str));

    let explicit_w = node
        .attrs
        .get("width")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|w| w.max(MIN_NODE_WIDTH));
    let explicit_h = node
        .attrs
        .get("height")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|h| h.max(MIN_NODE_HEIGHT));

    let default_w = if node.width > 0.0 {
        node.width
    } else {
        DEFAULT_NODE_WIDTH
    };
    let default_h = if node.height > 0.0 {
        node.height
    } else {
        DEFAULT_NODE_HEIGHT
    };

    let regular = shape.is_regular_by_default()
        || parse_graphviz_bool(node.attrs.get("regular").map(String::as_str), false);

    let (mut min_w_pts, mut min_h_pts) = if shape == ShapeKind::Plain {
        (
            explicit_w.unwrap_or(0.0) * POINTS_PER_INCH,
            explicit_h.unwrap_or(0.0) * POINTS_PER_INCH,
        )
    } else if regular {
        let user_max = explicit_w.unwrap_or(0.0).max(explicit_h.unwrap_or(0.0));
        if user_max > 0.0 {
            let pts = user_max * POINTS_PER_INCH;
            (pts, pts)
        } else {
            let pts = default_w.min(default_h) * POINTS_PER_INCH;
            (pts, pts)
        }
    } else {
        (
            explicit_w.unwrap_or(default_w) * POINTS_PER_INCH,
            explicit_h.unwrap_or(default_h) * POINTS_PER_INCH,
        )
    };

    let fixed_str = node.attrs.get("fixedsize").map_or("false", |s| s.trim());
    let is_fixed_shape = fixed_str.eq_ignore_ascii_case("shape");
    let is_fixed = is_fixed_shape || parse_graphviz_bool(Some(fixed_str), false);

    let raw_label = node
        .attrs
        .get("label")
        .map_or("\\N", |s| s.as_str())
        .to_string();

    // 3. Record / Mrecord shapes (`record_init` in `shapes.c`).
    if shape.is_record() && !is_html_label(&raw_label, shape) {
        let rec = layout_record_internal(
            &raw_label, &node.name, graph_name, &font_name, font_size, margin_x, margin_y,
            min_w_pts, min_h_pts, is_fixed, rankdir,
        );
        let w_pts = rec.bbox.width();
        let h_pts = rec.bbox.height();
        node.width = (w_pts / POINTS_PER_INCH).max(MIN_NODE_WIDTH);
        node.height = (h_pts / POINTS_PER_INCH).max(MIN_NODE_HEIGHT);
        return;
    }

    // 4. Measure label dimensions (HTML table/text vs plain multiline text).
    let mut dimen = if is_html_label(&raw_label, shape) {
        measure_html_label_dimen(&raw_label, &node.name, graph_name, &font_name, font_size)
    } else {
        let ctx = LabelSubstContext {
            node_name: &node.name,
            graph_name,
            tail_name: "",
            head_name: "",
            is_edge: false,
            is_directed: true,
            tail_port: None,
            head_port: None,
            label_text: "",
        };
        let layout = measure_multiline_label_with_context(&raw_label, &ctx, &font_name, font_size);
        Point::new(layout.width, layout.height)
    };

    // Add padding around non-empty label (`poly_init` in `shapes.c`).
    if (dimen.x > 0.0 || dimen.y > 0.0) && shape != ShapeKind::Plain {
        dimen.x += 2.0 * margin_x * POINTS_PER_INCH;
        dimen.y += 2.0 * margin_y * POINTS_PER_INCH;
    }

    // Quantization if `quantum` is set on the node.
    if let Some(q) = node
        .attrs
        .get("quantum")
        .and_then(|s| s.trim().parse::<f64>().ok())
        && q > 0.0
    {
        let q_pts = q * POINTS_PER_INCH;
        dimen.x = (dimen.x / q_pts).ceil() * q_pts;
        dimen.y = (dimen.y / q_pts).ceil() * q_pts;
    }

    let mut sides = shape.default_sides();
    let mut orientation = shape.default_orientation()
        + node
            .attrs
            .get("orientation")
            .and_then(|s| s.trim().parse::<f64>().ok())
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
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        distortion = node
            .attrs
            .get("distortion")
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
    }
    if sides <= 2 && (distortion.abs() > 1e-9 || skew.abs() > 1e-9) {
        sides = 120;
    }

    let peripheries = node
        .attrs
        .get("peripheries")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or_else(|| shape.default_peripheries());

    let valign = node
        .attrs
        .get("labelloc")
        .and_then(|s| s.trim().chars().next())
        .map(|c| c.to_ascii_lowercase())
        .filter(|&c| c == 't' || c == 'b')
        .unwrap_or('c');

    let is_box = sides == 4
        && (orientation.rem_euclid(90.0)).abs() < 0.5
        && distortion.abs() <= 1e-9
        && skew.abs() <= 1e-9
        && !matches!(shape, ShapeKind::Cylinder | ShapeKind::Star);

    let mut bb = dimen;
    if is_box {
        // Exact fit for axis-aligned rectangular shapes.
    } else if shape == ShapeKind::Cylinder {
        bb.y *= 1.375;
    } else if shape == ShapeKind::Star {
        bb = star_size(bb);
    } else {
        let temp = bb.y * SQRT_2;
        if min_h_pts > temp && valign == 'c' && min_h_pts > 0.0 {
            let ratio = bb.y / min_h_pts;
            bb.x *= (1.0 / (1.0 - ratio * ratio)).sqrt();
        } else {
            bb.x *= SQRT_2;
            bb.y = temp;
        }
        if sides > 2 {
            let cos_factor = (PI / (sides as f64)).cos();
            if cos_factor.abs() > 1e-9 {
                bb.x /= cos_factor;
                bb.y /= cos_factor;
            }
        }
    }

    if is_fixed {
        bb.x = min_w_pts;
        bb.y = min_h_pts;
    } else {
        bb.x = bb.x.max(min_w_pts);
        bb.y = bb.y.max(min_h_pts);
        min_w_pts = bb.x;
        min_h_pts = bb.y;
    }

    if regular {
        let m = bb.x.max(bb.y);
        bb.x = m;
        bb.y = m;
        min_w_pts = m;
        min_h_pts = m;
    }

    if sides < 3 {
        if peripheries > 1 {
            let extra = 2.0 * ((peripheries - 1) as f64) * PERIPHERY_GAP_PTS;
            bb.x += extra;
            bb.y += extra;
        }
    } else {
        // Polygon vertex generation & periphery expansion matching `poly_init`.
        let (mut xmax, mut ymax) = if shape == ShapeKind::Star {
            let sz = star_adjusted_bb(bb);
            (sz.x / 2.0, sz.y / 2.0)
        } else if shape == ShapeKind::Cylinder || is_box {
            (bb.x / 2.0, bb.y / 2.0)
        } else {
            orientation %= 360.0;
            let (_, xm, ym) =
                compute_raw_polygon_vertices(sides, orientation, distortion, skew, bb.x, bb.y);
            (xm, ym)
        };
        xmax *= 2.0;
        ymax *= 2.0;
        bb.x = min_w_pts.max(xmax);
        bb.y = min_h_pts.max(ymax);

        if peripheries > 1 {
            let cos_pi_n = (PI / (sides.max(3) as f64)).cos().max(0.2);
            let extra = 2.0 * ((peripheries - 1) as f64) * PERIPHERY_GAP_PTS / cos_pi_n;
            bb.x += extra;
            bb.y += extra;
        }
    }

    if is_fixed_shape {
        bb.x = bb.x.max(dimen.x);
        bb.y = bb.y.max(dimen.y);
    }

    node.width = (bb.x / POINTS_PER_INCH).max(if shape == ShapeKind::Plain {
        0.001
    } else {
        MIN_NODE_WIDTH
    });
    node.height = (bb.y / POINTS_PER_INCH).max(if shape == ShapeKind::Plain {
        0.001
    } else {
        MIN_NODE_HEIGHT
    });
}

fn star_size(sz0: Point) -> Point {
    let alpha = PI / 10.0;
    let alpha2 = 2.0 * alpha;
    let alpha3 = 3.0 * alpha;
    let alpha4 = 4.0 * alpha;
    let rx = sz0.x / (2.0 * alpha.cos());
    let ry = sz0.y / (alpha.sin() + alpha3.sin());
    let r0 = rx.max(ry);
    let r = r0 * alpha4.sin() * alpha2.cos() / (alpha.cos() * alpha4.cos());
    Point::new(2.0 * r * alpha.cos(), r * (1.0 + alpha3.sin()))
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
            .and_then(|s| s.trim().parse::<f64>().ok())
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
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        distortion = node
            .attrs
            .get("distortion")
            .and_then(|s| s.trim().parse::<f64>().ok())
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

/// Parses and lays out a `record` / `Mrecord` label for `node`, returning the root [`RecordField`]
/// with all nested field bounding boxes in points relative to `node.pos`.
pub fn record_layout(node: &NodeData, graph_name: &str, rankdir: &str) -> Option<RecordField> {
    let shape = ShapeKind::from_node(node);
    let raw_label = node.attrs.get("label").map_or("\\N", |s| s.as_str());
    if !shape.is_record() || is_html_label(raw_label, shape) {
        return None;
    }
    let font_name = node
        .attrs
        .get("fontname")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_FONT_NAME);
    let font_size = node
        .attrs
        .get("fontsize")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(DEFAULT_FONT_SIZE)
        .max(1.0);
    let (margin_x, margin_y) = parse_node_margin(node.attrs.get("margin").map(String::as_str));
    let w_pts = (node.width * POINTS_PER_INCH).max(MIN_NODE_WIDTH * POINTS_PER_INCH);
    let h_pts = (node.height * POINTS_PER_INCH).max(MIN_NODE_HEIGHT * POINTS_PER_INCH);
    let is_fixed = parse_graphviz_bool(node.attrs.get("fixedsize").map(String::as_str), false);

    Some(layout_record_internal(
        raw_label, &node.name, graph_name, font_name, font_size, margin_x, margin_y, w_pts, h_pts,
        is_fixed, rankdir,
    ))
}

#[allow(clippy::too_many_arguments)]
fn layout_record_internal(
    raw_label: &str,
    node_name: &str,
    graph_name: &str,
    font_name: &str,
    font_size: f64,
    margin_x: f64,
    margin_y: f64,
    min_w_pts: f64,
    min_h_pts: f64,
    is_fixed: bool,
    rankdir: &str,
) -> RecordField {
    // In Graphviz `record_init`: `flip = !GD_realflip(agraphof(n))`.
    // For TB / BT, top-level record fields run horizontally (LR = true).
    // For LR / RL, top-level record fields run vertically (LR = false).
    let top_lr = !matches!(rankdir.trim().to_ascii_uppercase().as_str(), "LR" | "RL");
    let chars: Vec<char> = raw_label.chars().collect();
    let mut pos = 0usize;

    let mut root = parse_record_fields(&chars, &mut pos, top_lr, true).unwrap_or_else(|| {
        let fallback: Vec<char> = "\\N".chars().collect();
        let mut p = 0;
        parse_record_fields(&fallback, &mut p, top_lr, true).unwrap_or(RecordField {
            port: None,
            text: node_name.to_string(),
            bbox: BoundingBox::default(),
            children: Vec::new(),
            horizontal: top_lr,
        })
    });

    let ctx = LabelSubstContext {
        node_name,
        graph_name,
        tail_name: "",
        head_name: "",
        is_edge: false,
        is_directed: true,
        tail_port: None,
        head_port: None,
        label_text: "",
    };

    let natural_size = size_record_field(&mut root, &ctx, font_name, font_size, margin_x, margin_y);
    let target_w = if is_fixed {
        min_w_pts
    } else {
        natural_size.x.max(min_w_pts)
    };
    let target_h = if is_fixed {
        min_h_pts
    } else {
        natural_size.y.max(min_h_pts)
    };

    resize_record_field(&mut root, Point::new(target_w, target_h));
    position_record_field(&mut root, Point::new(-target_w / 2.0, target_h / 2.0));
    root
}

fn is_record_ctrl(c: char) -> bool {
    matches!(c, '{' | '}' | '|' | '<' | '>')
}

fn parse_record_fields(
    chars: &[char],
    pos: &mut usize,
    lr: bool,
    is_root: bool,
) -> Option<RecordField> {
    let mut children = Vec::new();
    let mut has_text = false;
    let mut has_port = false;
    let mut has_table = false;
    let mut in_text = false;
    let mut in_port = false;
    let mut text_buf = String::new();
    let mut port_buf = String::new();
    let mut tmp_port: Option<String> = None;
    let mut nested_field: Option<RecordField> = None;

    loop {
        if *pos < chars.len() && (chars[*pos] as u32) < 32 && chars[*pos] != '\n' {
            *pos += 1;
            continue;
        }
        let ch = if *pos < chars.len() {
            Some(chars[*pos])
        } else {
            None
        };

        match ch {
            Some('<') => {
                if has_table || has_port {
                    return None;
                }
                has_port = true;
                in_port = true;
                port_buf.clear();
                *pos += 1;
            }
            Some('>') => {
                if !in_port {
                    return None;
                }
                let trimmed_port = port_buf.trim_end_matches(' ').to_string();
                tmp_port = Some(trimmed_port);
                in_port = false;
                *pos += 1;
            }
            Some('{') => {
                *pos += 1;
                if in_port || in_text || has_table || has_port || has_text || *pos >= chars.len() {
                    return None;
                }
                has_table = true;
                let sub = parse_record_fields(chars, pos, !lr, false)?;
                nested_field = Some(sub);
            }
            Some('}') | Some('|') | None => {
                if (ch.is_none() && !is_root) || in_port {
                    return None;
                }
                if let Some(mut sub) = nested_field.take() {
                    if tmp_port.is_some() {
                        sub.port = tmp_port.take();
                    }
                    children.push(sub);
                } else {
                    if !has_text {
                        text_buf.push(' ');
                    } else if text_buf.len() > 1 && text_buf.ends_with(' ') {
                        text_buf.pop();
                    }
                    children.push(RecordField {
                        port: tmp_port.take(),
                        text: std::mem::take(&mut text_buf),
                        bbox: BoundingBox::default(),
                        children: Vec::new(),
                        horizontal: true,
                    });
                }

                if let Some(c) = ch {
                    *pos += 1;
                    if c == '}' {
                        return Some(RecordField {
                            port: None,
                            text: String::new(),
                            bbox: BoundingBox::default(),
                            children,
                            horizontal: lr,
                        });
                    }
                    has_text = false;
                    has_port = false;
                    has_table = false;
                    in_text = false;
                    in_port = false;
                    text_buf.clear();
                } else {
                    break;
                }
            }
            Some('\\') => {
                let mut is_hard_space = false;
                if *pos + 1 < chars.len() {
                    let next = chars[*pos + 1];
                    if is_record_ctrl(next) {
                        // Escaped control character: consume `\` and emit `next` literally.
                    } else if next == ' ' {
                        is_hard_space = true;
                    } else {
                        text_buf.push('\\');
                        in_text = true;
                        has_text = true;
                    }
                    *pos += 1;
                }
                if *pos < chars.len() {
                    let cur = chars[*pos];
                    if has_table && cur != ' ' {
                        return None;
                    }
                    if !in_text && !in_port && (cur != ' ' || is_hard_space) {
                        in_text = true;
                        has_text = true;
                    }
                    if in_text {
                        text_buf.push(cur);
                    } else if in_port {
                        port_buf.push(cur);
                    }
                    *pos += 1;
                }
            }
            Some(cur) => {
                if has_table && cur != ' ' {
                    return None;
                }
                if !in_text && !in_port && cur != ' ' {
                    in_text = true;
                    has_text = true;
                }
                if in_text {
                    if !(cur == ' ' && text_buf.ends_with(' ')) {
                        text_buf.push(cur);
                    }
                } else if in_port
                    && !(cur == ' ' && (port_buf.is_empty() || port_buf.ends_with(' ')))
                {
                    port_buf.push(cur);
                }
                *pos += 1;
            }
        }
    }

    Some(RecordField {
        port: None,
        text: String::new(),
        bbox: BoundingBox::default(),
        children,
        horizontal: lr,
    })
}

fn size_record_field(
    field: &mut RecordField,
    ctx: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
    margin_x: f64,
    margin_y: f64,
) -> Point {
    let size = if field.children.is_empty() {
        let layout = measure_multiline_label_with_context(&field.text, ctx, font_name, font_size);
        let mut w = layout.width;
        let mut h = layout.height;
        if w > 0.0 || h > 0.0 {
            w += 2.0 * margin_x * POINTS_PER_INCH;
            h += 2.0 * margin_y * POINTS_PER_INCH;
        }
        Point::new(w, h)
    } else {
        let mut w = 0.0_f64;
        let mut h = 0.0_f64;
        for child in &mut field.children {
            let cs = size_record_field(child, ctx, font_name, font_size, margin_x, margin_y);
            if field.horizontal {
                w += cs.x;
                h = h.max(cs.y);
            } else {
                h += cs.y;
                w = w.max(cs.x);
            }
        }
        Point::new(w, h)
    };
    field.bbox = BoundingBox::new(0.0, 0.0, size.x, size.y);
    size
}

fn resize_record_field(field: &mut RecordField, new_sz: Point) {
    let cur_w = field.bbox.width();
    let cur_h = field.bbox.height();
    let dx = new_sz.x - cur_w;
    let dy = new_sz.y - cur_h;
    field.bbox = BoundingBox::new(0.0, 0.0, new_sz.x, new_sz.y);

    let n = field.children.len();
    if n > 0 {
        let inc = if field.horizontal {
            dx / (n as f64)
        } else {
            dy / (n as f64)
        };
        let is_horiz = field.horizontal;
        for (i, child) in field.children.iter_mut().enumerate() {
            let amt = (((i + 1) as f64) * inc).trunc() - ((i as f64) * inc).trunc();
            let child_sz = if is_horiz {
                Point::new((child.bbox.width() + amt).max(0.0), new_sz.y)
            } else {
                Point::new(new_sz.x, (child.bbox.height() + amt).max(0.0))
            };
            resize_record_field(child, child_sz);
        }
    }
}

fn position_record_field(field: &mut RecordField, mut ul: Point) {
    let w = field.bbox.width();
    let h = field.bbox.height();
    field.bbox = BoundingBox::new(ul.x, ul.y - h, ul.x + w, ul.y);
    let is_horiz = field.horizontal;
    for child in &mut field.children {
        let cw = child.bbox.width();
        let ch = child.bbox.height();
        position_record_field(child, ul);
        if is_horiz {
            ul.x += cw;
        } else {
            ul.y -= ch;
        }
    }
}

// ============================================================================
// HTML-like label & table parser and layout (`lib/common/htmltable.c`)
// ============================================================================

/// Parses and lays out an HTML-like `<TABLE>` label for `node` if present.
pub fn html_table_layout(node: &NodeData, graph_name: &str) -> Option<HtmlTable> {
    let shape = ShapeKind::from_node(node);
    let raw_label = node.attrs.get("label")?;
    if !is_html_label(raw_label, shape) {
        return None;
    }
    let font_name = node
        .attrs
        .get("fontname")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_FONT_NAME);
    let font_size = node
        .attrs
        .get("fontsize")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(DEFAULT_FONT_SIZE)
        .max(1.0);
    parse_html_table(raw_label, &node.name, graph_name, font_name, font_size)
}

fn measure_html_label_dimen(
    raw_label: &str,
    node_name: &str,
    graph_name: &str,
    font_name: &str,
    font_size: f64,
) -> Point {
    if let Some(tbl) = parse_html_table(raw_label, node_name, graph_name, font_name, font_size) {
        return Point::new(tbl.bbox.width(), tbl.bbox.height());
    }
    let trimmed = raw_label.trim();
    let inner = if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    let ctx = LabelSubstContext {
        node_name,
        graph_name,
        tail_name: "",
        head_name: "",
        is_edge: false,
        is_directed: true,
        tail_port: None,
        head_port: None,
        label_text: "",
    };
    measure_html_rich_text(inner, &ctx, font_name, font_size).0
}

/// Parses and lays out an HTML-like `<TABLE>` label string, returning [`HtmlTable`] with cell boxes
/// centered around `(0, 0)`.
pub fn parse_html_table(
    raw_html: &str,
    node_name: &str,
    graph_name: &str,
    font_name: &str,
    font_size: f64,
) -> Option<HtmlTable> {
    let trimmed = raw_html.trim();
    let inner = if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
        trimmed[1..trimmed.len() - 1].trim()
    } else {
        trimmed
    };
    let ctx = LabelSubstContext {
        node_name,
        graph_name,
        tail_name: "",
        head_name: "",
        is_edge: false,
        is_directed: true,
        tail_port: None,
        head_port: None,
        label_text: "",
    };
    let mut tbl = parse_single_table(inner, &ctx, font_name, font_size)?;
    let w = tbl.bbox.width();
    let h = tbl.bbox.height();
    let root_box = BoundingBox::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
    position_html_table(&mut tbl, root_box);
    Some(tbl)
}

fn find_tag_ci(haystack: &str, tag_prefix: &str) -> Option<usize> {
    let hb = haystack.as_bytes();
    let tb = tag_prefix.as_bytes();
    if tb.len() > hb.len() {
        return None;
    }
    for i in 0..=(hb.len() - tb.len()) {
        if hb[i..i + tb.len()].eq_ignore_ascii_case(tb) {
            let next_b = hb.get(i + tb.len()).copied();
            if matches!(
                next_b,
                None | Some(b'>')
                    | Some(b' ')
                    | Some(b'\t')
                    | Some(b'\n')
                    | Some(b'\r')
                    | Some(b'/')
            ) {
                return Some(i);
            }
        }
    }
    None
}

fn find_matching_close_tag(s: &str, open_tag: &str, close_tag: &str) -> Option<(usize, usize)> {
    let open_prefix = format!("<{open_tag}");
    let close_prefix = format!("</{close_tag}");
    let start_idx = find_tag_ci(s, &open_prefix)?;
    let tag_end_rel = s[start_idx..].find('>')?;
    let content_start = start_idx + tag_end_rel + 1;

    let mut depth = 1usize;
    let mut cursor = content_start;
    while cursor < s.len() {
        let rem = &s[cursor..];
        let next_open = find_tag_ci(rem, &open_prefix);
        let next_close = find_tag_ci(rem, &close_prefix);
        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                let after = cursor + o + open_prefix.len();
                cursor = s[after..].find('>').map_or(s.len(), |p| after + p + 1);
            }
            (_, Some(c)) => {
                depth -= 1;
                let close_pos = cursor + c;
                if depth == 0 {
                    return Some((content_start, close_pos));
                }
                let after = close_pos + close_prefix.len();
                cursor = s[after..].find('>').map_or(s.len(), |p| after + p + 1);
            }
            _ => break,
        }
    }
    None
}

fn parse_tag_attrs(open_tag_header: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let bytes = open_tag_header.as_bytes();
    let mut i = 0;
    // Skip `<TAGNAME`
    while i < bytes.len() && (bytes[i] == b'<' || bytes[i].is_ascii_whitespace()) {
        i += 1;
    }
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/'
    {
        i += 1;
    }

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'>' || bytes[i] == b'/' {
            break;
        }
        let key_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
        {
            i += 1;
        }
        let key = open_tag_header[key_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let val_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                let val = open_tag_header[val_start..i].to_string();
                if i < bytes.len() {
                    i += 1;
                }
                attrs.push((key, val));
            } else {
                let val_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
                let val = open_tag_header[val_start..i].to_string();
                attrs.push((key, val));
            }
        }
    }
    attrs
}

fn parse_single_table(
    s: &str,
    ctx: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
) -> Option<HtmlTable> {
    let table_start = find_tag_ci(s, "<table")?;
    let header_end = table_start + s[table_start..].find('>')?;
    let table_header = &s[table_start..=header_end];
    let (body_start, body_end) = find_matching_close_tag(&s[table_start..], "table", "table")?;
    let table_body = &s[table_start + body_start..table_start + body_end];

    let t_attrs = parse_tag_attrs(table_header);
    let mut border = 1.0_f64;
    let mut cellborder: Option<f64> = None;
    let mut cellspacing = 2.0_f64;
    let mut cellpadding = 2.0_f64;
    let mut cellpadding_set = false;
    let mut port: Option<String> = None;
    let mut bgcolor: Option<String> = None;
    let mut rounded = false;
    let mut min_width = 0.0_f64;
    let mut min_height = 0.0_f64;

    for (k, v) in t_attrs {
        match k.as_str() {
            "border" => {
                if let Ok(b) = v.trim().parse::<f64>() {
                    border = b.max(0.0);
                }
            }
            "cellborder" => {
                if let Ok(cb) = v.trim().parse::<f64>() {
                    cellborder = Some(cb.max(0.0));
                }
            }
            "cellspacing" => {
                if let Ok(cs) = v.trim().parse::<f64>() {
                    cellspacing = cs.max(0.0);
                }
            }
            "cellpadding" => {
                if let Ok(cp) = v.trim().parse::<f64>() {
                    cellpadding = cp.max(0.0);
                    cellpadding_set = true;
                }
            }
            "port" => port = Some(v),
            "bgcolor" => bgcolor = Some(v),
            "style" => {
                if v.to_ascii_lowercase().contains("rounded") {
                    rounded = true;
                }
            }
            "width" => {
                if let Ok(w) = v.trim().parse::<f64>() {
                    min_width = w.max(0.0);
                }
            }
            "height" => {
                if let Ok(h) = v.trim().parse::<f64>() {
                    min_height = h.max(0.0);
                }
            }
            _ => {}
        }
    }

    // Extract rows `<TR>...</TR>` at the current table level.
    let mut raw_rows: Vec<&str> = Vec::new();
    let mut cursor = 0usize;
    while cursor < table_body.len() {
        let rem = &table_body[cursor..];
        let Some(tr_rel) = find_tag_ci(rem, "<tr") else {
            break;
        };
        let tr_abs = cursor + tr_rel;
        let Some((c_start, c_end)) = find_matching_close_tag(&table_body[tr_abs..], "tr", "tr")
        else {
            break;
        };
        raw_rows.push(&table_body[tr_abs + c_start..tr_abs + c_end]);
        let after = tr_abs + c_end + "</tr".len();
        cursor = table_body[after..]
            .find('>')
            .map_or(table_body.len(), |p| after + p + 1);
    }

    // Track occupied `(row, col)` grid slots for `ROWSPAN` and `COLSPAN`.
    let mut occupied: std::collections::BTreeSet<(usize, usize)> =
        std::collections::BTreeSet::new();
    let mut cells: Vec<HtmlCell> = Vec::new();
    let mut n_rows = 0usize;
    let mut n_cols = 0usize;

    for (r_idx, row_str) in raw_rows.iter().enumerate() {
        let mut c_idx = 0usize;
        let mut td_cursor = 0usize;
        while td_cursor < row_str.len() {
            let rem = &row_str[td_cursor..];
            let Some(td_rel) = find_tag_ci(rem, "<td") else {
                break;
            };
            let td_abs = td_cursor + td_rel;
            let Some(td_hdr_end) = row_str[td_abs..].find('>') else {
                break;
            };
            let td_header = &row_str[td_abs..=td_abs + td_hdr_end];
            let Some((c_start, c_end)) = find_matching_close_tag(&row_str[td_abs..], "td", "td")
            else {
                break;
            };
            let td_content = &row_str[td_abs + c_start..td_abs + c_end];

            let c_attrs = parse_tag_attrs(td_header);
            let mut rowspan = 1usize;
            let mut colspan = 1usize;
            let mut cell_port: Option<String> = None;
            let mut cell_bgcolor: Option<String> = None;
            let mut cell_border = cellborder.unwrap_or(border);
            let mut cell_pad = if cellpadding_set { cellpadding } else { 2.0 };
            let mut cell_min_w = 0.0_f64;
            let mut cell_min_h = 0.0_f64;
            let mut align = TextJustify::Center;

            for (k, v) in c_attrs {
                match k.as_str() {
                    "rowspan" => {
                        if let Ok(rs) = v.trim().parse::<usize>() {
                            rowspan = rs.max(1);
                        }
                    }
                    "colspan" => {
                        if let Ok(cs) = v.trim().parse::<usize>() {
                            colspan = cs.max(1);
                        }
                    }
                    "port" => cell_port = Some(v),
                    "bgcolor" => cell_bgcolor = Some(v),
                    "border" => {
                        if let Ok(b) = v.trim().parse::<f64>() {
                            cell_border = b.max(0.0);
                        }
                    }
                    "cellpadding" => {
                        if let Ok(cp) = v.trim().parse::<f64>() {
                            cell_pad = cp.max(0.0);
                        }
                    }
                    "width" => {
                        if let Ok(w) = v.trim().parse::<f64>() {
                            cell_min_w = w.max(0.0);
                        }
                    }
                    "height" => {
                        if let Ok(h) = v.trim().parse::<f64>() {
                            cell_min_h = h.max(0.0);
                        }
                    }
                    "align" | "balign" => {
                        if v.eq_ignore_ascii_case("left") {
                            align = TextJustify::Left;
                        } else if v.eq_ignore_ascii_case("right") {
                            align = TextJustify::Right;
                        }
                    }
                    _ => {}
                }
            }

            // Find first available column `c_idx` where `c_idx..c_idx + colspan` is free on `r_idx`.
            loop {
                let conflict = (c_idx..c_idx + colspan)
                    .rev()
                    .find(|&c| occupied.contains(&(r_idx, c)));
                if let Some(conf_c) = conflict {
                    c_idx = conf_c + 1;
                } else {
                    break;
                }
            }
            for rr in r_idx..r_idx + rowspan {
                for cc in c_idx..c_idx + colspan {
                    occupied.insert((rr, cc));
                }
            }

            let nested_table = if find_tag_ci(td_content, "<table").is_some() {
                parse_single_table(td_content, ctx, font_name, font_size)
            } else {
                None
            };

            let (child_dim, plain_txt) = if let Some(ref nt) = nested_table {
                (Point::new(nt.bbox.width(), nt.bbox.height()), String::new())
            } else {
                measure_html_rich_text(td_content, ctx, font_name, font_size)
            };

            let margin = 2.0 * (cell_pad + cell_border);
            let cw = (child_dim.x + margin).max(cell_min_w);
            let ch = (child_dim.y + margin).max(cell_min_h);

            cells.push(HtmlCell {
                port: cell_port,
                text: plain_txt,
                row: r_idx,
                col: c_idx,
                rowspan,
                colspan,
                border: cell_border,
                cellpadding: cell_pad,
                bgcolor: cell_bgcolor,
                align,
                bbox: BoundingBox::new(0.0, 0.0, cw, ch),
                nested_table,
            });

            c_idx += colspan;
            n_cols = n_cols.max(c_idx);
            n_rows = n_rows.max(r_idx + rowspan);

            let after = td_abs + c_end + "</td".len();
            td_cursor = row_str[after..]
                .find('>')
                .map_or(row_str.len(), |p| after + p + 1);
        }
    }

    n_rows = n_rows.max(1);
    n_cols = n_cols.max(1);

    // Compute column widths (`set_cell_widths` in `htmltable.c`).
    let mut col_widths = vec![0.0_f64; n_cols];
    for cell in &cells {
        if cell.colspan == 1 {
            col_widths[cell.col] = col_widths[cell.col].max(cell.bbox.width());
        }
    }
    for cell in &cells {
        if cell.colspan > 1 {
            let span_w: f64 = col_widths[cell.col..cell.col + cell.colspan].iter().sum();
            let spacing = ((cell.colspan - 1) as f64) * cellspacing;
            if span_w + spacing < cell.bbox.width() {
                let widen_by = (cell.bbox.width() - spacing - span_w) / (cell.colspan as f64);
                for w in &mut col_widths[cell.col..cell.col + cell.colspan] {
                    *w += widen_by;
                }
            }
        }
    }

    // Compute row heights (`set_cell_heights` in `htmltable.c`).
    let mut row_heights = vec![0.0_f64; n_rows];
    for cell in &cells {
        if cell.rowspan == 1 {
            row_heights[cell.row] = row_heights[cell.row].max(cell.bbox.height());
        }
    }
    for cell in &cells {
        if cell.rowspan > 1 {
            let span_h: f64 = row_heights[cell.row..cell.row + cell.rowspan].iter().sum();
            let spacing = ((cell.rowspan - 1) as f64) * cellspacing;
            if span_h + spacing < cell.bbox.height() {
                let heighten_by = (cell.bbox.height() - spacing - span_h) / (cell.rowspan as f64);
                for h in &mut row_heights[cell.row..cell.row + cell.rowspan] {
                    *h += heighten_by;
                }
            }
        }
    }

    // Store column widths and row heights into cell bboxes prior to positioning.
    for cell in &mut cells {
        let min_w: f64 = col_widths[cell.col..cell.col + cell.colspan].iter().sum();
        let sp_x = ((cell.colspan - 1) as f64) * cellspacing;
        let w = cell.bbox.width().max(min_w + sp_x);

        let min_h: f64 = row_heights[cell.row..cell.row + cell.rowspan].iter().sum();
        let sp_y = ((cell.rowspan - 1) as f64) * cellspacing;
        let h = cell.bbox.height().max(min_h + sp_y);
        cell.bbox = BoundingBox::new(0.0, 0.0, w, h);
    }

    let total_w =
        (((n_cols + 1) as f64) * cellspacing + 2.0 * border + col_widths.iter().sum::<f64>())
            .max(min_width);
    let total_h =
        (((n_rows + 1) as f64) * cellspacing + 2.0 * border + row_heights.iter().sum::<f64>())
            .max(min_height);

    Some(HtmlTable {
        port,
        border,
        cellborder,
        cellspacing,
        cellpadding,
        bgcolor,
        rounded,
        row_count: n_rows,
        col_count: n_cols,
        bbox: BoundingBox::new(0.0, 0.0, total_w, total_h),
        cells,
    })
}

fn position_html_table(tbl: &mut HtmlTable, pos: BoundingBox) {
    let n_cols = tbl.col_count.max(1);
    let n_rows = tbl.row_count.max(1);

    let mut col_widths = vec![0.0_f64; n_cols];
    let mut row_heights = vec![0.0_f64; n_rows];

    for cell in &tbl.cells {
        if cell.colspan == 1 {
            col_widths[cell.col] = col_widths[cell.col].max(cell.bbox.width());
        }
        if cell.rowspan == 1 {
            row_heights[cell.row] = row_heights[cell.row].max(cell.bbox.height());
        }
    }
    for cell in &tbl.cells {
        if cell.colspan > 1 {
            let span_w: f64 = col_widths[cell.col..cell.col + cell.colspan].iter().sum();
            let sp = ((cell.colspan - 1) as f64) * tbl.cellspacing;
            if span_w + sp < cell.bbox.width() {
                let add = (cell.bbox.width() - sp - span_w) / (cell.colspan as f64);
                for w in &mut col_widths[cell.col..cell.col + cell.colspan] {
                    *w += add;
                }
            }
        }
        if cell.rowspan > 1 {
            let span_h: f64 = row_heights[cell.row..cell.row + cell.rowspan].iter().sum();
            let sp = ((cell.rowspan - 1) as f64) * tbl.cellspacing;
            if span_h + sp < cell.bbox.height() {
                let add = (cell.bbox.height() - sp - span_h) / (cell.rowspan as f64);
                for h in &mut row_heights[cell.row..cell.row + cell.rowspan] {
                    *h += add;
                }
            }
        }
    }

    let natural_w =
        ((n_cols + 1) as f64) * tbl.cellspacing + 2.0 * tbl.border + col_widths.iter().sum::<f64>();
    let natural_h = ((n_rows + 1) as f64) * tbl.cellspacing
        + 2.0 * tbl.border
        + row_heights.iter().sum::<f64>();

    let extra_x = (pos.width() - natural_w).max(0.0) / (n_cols as f64);
    let extra_y = (pos.height() - natural_h).max(0.0) / (n_rows as f64);

    let mut x_offsets = Vec::with_capacity(n_cols + 1);
    let mut x = pos.ll.x + tbl.border + tbl.cellspacing;
    for &cw in &col_widths {
        x_offsets.push(x);
        x += cw + extra_x + tbl.cellspacing;
    }
    x_offsets.push(x);

    let mut y_offsets = Vec::with_capacity(n_rows + 1);
    let mut y = pos.ur.y - tbl.border - tbl.cellspacing;
    for &rh in &row_heights {
        y_offsets.push(y);
        y -= rh + extra_y + tbl.cellspacing;
    }
    y_offsets.push(y);

    for cell in &mut tbl.cells {
        let ll_x = x_offsets[cell.col];
        let ur_x = x_offsets[cell.col + cell.colspan] - tbl.cellspacing;
        let ur_y = y_offsets[cell.row];
        let ll_y = y_offsets[cell.row + cell.rowspan] + tbl.cellspacing;
        let cbox = BoundingBox::new(ll_x, ll_y, ur_x, ur_y);
        cell.bbox = cbox;

        if let Some(ref mut nested) = cell.nested_table {
            let inset = cell.border + cell.cellpadding;
            let inner_box = BoundingBox::new(
                cbox.ll.x + inset,
                cbox.ll.y + inset,
                (cbox.ur.x - inset).max(cbox.ll.x + inset),
                (cbox.ur.y - inset).max(cbox.ll.y + inset),
            );
            position_html_table(nested, inner_box);
        }
    }

    tbl.bbox = pos;
}

fn measure_html_rich_text(
    html: &str,
    ctx: &LabelSubstContext<'_>,
    base_font_name: &str,
    base_font_size: f64,
) -> (Point, String) {
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut bold_depth = 0usize;
    let mut italic_depth = 0usize;
    let mut font_stack: Vec<(String, f64)> = vec![(base_font_name.to_string(), base_font_size)];

    let mut max_w = 0.0_f64;
    let mut total_h = 0.0_f64;
    let mut line_w = 0.0_f64;
    let mut line_max_fs = base_font_size;

    let flush_span = |text: &str,
                      font_stack: &[(String, f64)],
                      bold: bool,
                      italic: bool,
                      line_w: &mut f64,
                      line_max_fs: &mut f64| {
        if text.is_empty() {
            return;
        }
        let (fn_name, fs) = font_stack.last().unwrap();
        let fs = *fs;
        let subst = substitute_object_escapes(text, ctx);
        let (w, _) = estimate_text_size_styled(&subst, fn_name, fs, bold, italic);
        *line_w += w;
        *line_max_fs = line_max_fs.max(fs);
    };

    let mut span_buf = String::new();
    let bytes = html.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(end_rel) = html[i..].find('>')
        {
            flush_span(
                &span_buf,
                &font_stack,
                bold_depth > 0,
                italic_depth > 0,
                &mut line_w,
                &mut line_max_fs,
            );
            span_buf.clear();

            let tag = html[i + 1..i + end_rel].trim();
            let tag_lower = tag.to_ascii_lowercase();
            if tag_lower.starts_with("br") {
                max_w = max_w.max(line_w);
                total_h += line_max_fs * LINESPACING;
                line_w = 0.0;
                line_max_fs = font_stack.last().map_or(base_font_size, |f| f.1);
                lines.push(std::mem::take(&mut current_line));
            } else if tag_lower == "b" || tag_lower.starts_with("b ") {
                bold_depth += 1;
            } else if tag_lower == "/b" {
                bold_depth = bold_depth.saturating_sub(1);
            } else if tag_lower == "i" || tag_lower.starts_with("i ") {
                italic_depth += 1;
            } else if tag_lower == "/i" {
                italic_depth = italic_depth.saturating_sub(1);
            } else if tag_lower.starts_with("font") {
                let attrs = parse_tag_attrs(&html[i..=i + end_rel]);
                let (mut fn_name, mut fs) = font_stack.last().cloned().unwrap();
                for (k, v) in attrs {
                    if k == "face" && !v.trim().is_empty() {
                        fn_name = v.trim().to_string();
                    } else if k == "point-size"
                        && let Ok(parsed_fs) = v.trim().parse::<f64>()
                    {
                        fs = parsed_fs.max(1.0);
                    }
                }
                font_stack.push((fn_name, fs));
            } else if tag_lower.starts_with("/font") && font_stack.len() > 1 {
                font_stack.pop();
            }
            i += end_rel + 1;
            continue;
        }
        let ch = html[i..].chars().next().unwrap();
        if ch == '\n' || ch == '\r' {
            if !span_buf.ends_with(' ') && !span_buf.is_empty() {
                span_buf.push(' ');
                current_line.push(' ');
            }
        } else {
            span_buf.push(ch);
            current_line.push(ch);
        }
        i += ch.len_utf8();
    }

    flush_span(
        &span_buf,
        &font_stack,
        bold_depth > 0,
        italic_depth > 0,
        &mut line_w,
        &mut line_max_fs,
    );
    if !current_line.is_empty() || lines.is_empty() {
        max_w = max_w.max(line_w);
        if max_w > 0.0 || !lines.is_empty() {
            total_h += line_max_fs * LINESPACING;
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }

    let plain = decode_basic_html_entities(&lines.join("\n"));
    (Point::new(max_w, total_h), plain)
}

fn decode_basic_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
}

// ============================================================================
// Port resolution & Ray-to-boundary clipping (`poly_port`, `record_port`, `bezier_clip`)
// ============================================================================

fn parse_compass_direction(s: &str) -> Option<&'static str> {
    match s.trim().to_ascii_lowercase().as_str() {
        "n" => Some("n"),
        "ne" => Some("ne"),
        "e" => Some("e"),
        "se" => Some("se"),
        "s" => Some("s"),
        "sw" => Some("sw"),
        "w" => Some("w"),
        "nw" => Some("nw"),
        "c" => Some("c"),
        "_" => Some("_"),
        _ => None,
    }
}

/// Resolves the sub-box (in absolute coordinates centered at `node.pos`) targeted by `port`,
/// plus an optional compass direction (`"n"`, `"ne"`, `"e"`, `"se"`, `"s"`, `"sw"`, `"w"`, `"nw"`, `"c"`).
pub fn resolve_port_box_and_compass(
    node: &NodeData,
    port: &Port,
) -> (BoundingBox, Option<&'static str>, bool) {
    let rx = (node.width * POINTS_PER_INCH / 2.0).max(0.5);
    let ry = (node.height * POINTS_PER_INCH / 2.0).max(0.5);
    let node_box = BoundingBox::new(
        node.pos.x - rx,
        node.pos.y - ry,
        node.pos.x + rx,
        node.pos.y + ry,
    );

    let explicit_compass = port.compass.as_deref().and_then(parse_compass_direction);
    let rankdir = node.attrs.get("_rankdir").map_or("TB", String::as_str);

    if let Some(ref port_name) = port.name {
        // 1. Check record sub-field port.
        if let Some(rec) = record_layout(node, "", rankdir)
            && let Some(sub) = rec.find_port(port_name)
        {
            let sub_box = BoundingBox::new(
                node.pos.x + sub.bbox.ll.x,
                node.pos.y + sub.bbox.ll.y,
                node.pos.x + sub.bbox.ur.x,
                node.pos.y + sub.bbox.ur.y,
            );
            return (sub_box, explicit_compass, true);
        }

        // 2. Check HTML table cell port.
        if let Some(tbl) = html_table_layout(node, "")
            && let Some(sub_bb) = tbl.find_port_box(port_name)
        {
            let sub_box = BoundingBox::new(
                node.pos.x + sub_bb.ll.x,
                node.pos.y + sub_bb.ll.y,
                node.pos.x + sub_bb.ur.x,
                node.pos.y + sub_bb.ur.y,
            );
            return (sub_box, explicit_compass, true);
        }

        // 3. If `port.compass` is None and `port.name` is itself a compass token (`a:n -> b:s`).
        if explicit_compass.is_none()
            && let Some(comp) = parse_compass_direction(port_name)
        {
            return (node_box, Some(comp), false);
        }
    }

    (node_box, explicit_compass, false)
}

/// Resolves the center offset of `port` relative to `node.pos` in points.
pub fn resolve_port_offset(node: &NodeData, port: &Port) -> Point {
    let (bbox, compass, is_subbox) = resolve_port_box_and_compass(node, port);
    let cx = (bbox.ll.x + bbox.ur.x) / 2.0;
    let cy = (bbox.ll.y + bbox.ur.y) / 2.0;
    if is_subbox {
        if let Some(comp) = compass {
            let pt = compass_point_on_box(bbox, comp);
            return Point::new(pt.x - node.pos.x, pt.y - node.pos.y);
        }
        return Point::new(cx - node.pos.x, cy - node.pos.y);
    }
    if let Some(comp) = compass
        && comp != "c"
        && comp != "_"
    {
        let dir = compass_unit_vector(comp);
        let far = Point::new(
            node.pos.x + dir.x * (bbox.width() + bbox.height()) * 4.0,
            node.pos.y + dir.y * (bbox.width() + bbox.height()) * 4.0,
        );
        let boundary = clip_ray_from_origin(node, node.pos, far);
        return Point::new(boundary.x - node.pos.x, boundary.y - node.pos.y);
    }
    Point::new(0.0, 0.0)
}

fn compass_point_on_box(b: BoundingBox, compass: &str) -> Point {
    let cx = (b.ll.x + b.ur.x) / 2.0;
    let cy = (b.ll.y + b.ur.y) / 2.0;
    match compass {
        "n" => Point::new(cx, b.ur.y),
        "ne" => Point::new(b.ur.x, b.ur.y),
        "e" => Point::new(b.ur.x, cy),
        "se" => Point::new(b.ur.x, b.ll.y),
        "s" => Point::new(cx, b.ll.y),
        "sw" => Point::new(b.ll.x, b.ll.y),
        "w" => Point::new(b.ll.x, cy),
        "nw" => Point::new(b.ll.x, b.ur.y),
        _ => Point::new(cx, cy),
    }
}

fn compass_unit_vector(compass: &str) -> Point {
    let s = FRAC_PI_2.sin(); // 1.0
    let d = std::f64::consts::FRAC_1_SQRT_2;
    match compass {
        "n" => Point::new(0.0, s),
        "ne" => Point::new(d, d),
        "e" => Point::new(s, 0.0),
        "se" => Point::new(d, -d),
        "s" => Point::new(0.0, -s),
        "sw" => Point::new(-d, -d),
        "w" => Point::new(-s, 0.0),
        "nw" => Point::new(-d, d),
        _ => Point::new(0.0, 0.0),
    }
}

/// Computes the exact intersection point on the perimeter of `node` (ellipse, circle, box,
/// polygon, star, or record/HTML port sub-box) for a ray originating at `node`/`port` and
/// aiming toward `outside_pt`.
pub fn clip_ray_to_node_boundary(node: &NodeData, port: &Port, outside_pt: Point) -> Point {
    let (target_box, compass, is_subbox) = resolve_port_box_and_compass(node, port);

    if is_subbox {
        if let Some(comp) = compass
            && comp != "c"
            && comp != "_"
        {
            return compass_point_on_box(target_box, comp);
        }
        let origin = Point::new(
            (target_box.ll.x + target_box.ur.x) / 2.0,
            (target_box.ll.y + target_box.ur.y) / 2.0,
        );
        let shape = ShapeKind::from_node(node);
        if shape.is_record() {
            return intersect_ray_with_box(origin, outside_pt, target_box);
        }
        return clip_ray_from_origin(node, origin, outside_pt);
    }

    if let Some(comp) = compass {
        match comp {
            "c" => {
                if outside_pt.dist(node.pos) <= 1e-9 {
                    return node.pos;
                }
                return clip_ray_from_origin(node, node.pos, outside_pt);
            }
            "_" => return clip_ray_from_origin(node, node.pos, outside_pt),
            dir_str => {
                let shape = ShapeKind::from_node(node);
                let sides = shape.default_sides();
                let is_axis_box = sides == 4
                    && (shape.default_orientation().rem_euclid(90.0)).abs() < 0.5
                    && shape.default_distortion().abs() <= 1e-9
                    && shape.default_skew().abs() <= 1e-9
                    && !matches!(shape, ShapeKind::Star);
                if is_axis_box || shape.is_record() {
                    return compass_point_on_box(target_box, dir_str);
                }
                let dir = compass_unit_vector(dir_str);
                let maxv = target_box.width().max(target_box.height()) * 4.0;
                let far = Point::new(node.pos.x + dir.x * maxv, node.pos.y + dir.y * maxv);
                return clip_ray_from_origin(node, node.pos, far);
            }
        }
    }

    clip_ray_from_origin(node, node.pos, outside_pt)
}

fn clip_ray_from_origin(node: &NodeData, origin: Point, outside_pt: Point) -> Point {
    let rx = (node.width * POINTS_PER_INCH) / 2.0;
    let ry = (node.height * POINTS_PER_INCH) / 2.0;
    if rx <= 1e-9 || ry <= 1e-9 {
        return node.pos;
    }

    let dx = outside_pt.x - origin.x;
    let dy = outside_pt.y - origin.y;
    if dx.hypot(dy) <= 1e-12 {
        return Point::new(node.pos.x + rx, node.pos.y);
    }

    let shape = ShapeKind::from_node(node);
    let mut sides = shape.default_sides();
    let orientation = shape.default_orientation()
        + node
            .attrs
            .get("orientation")
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
    let mut distortion = shape.default_distortion();
    let mut skew = shape.default_skew();
    if sides == 0 {
        sides = node
            .attrs
            .get("sides")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(4);
        distortion = node
            .attrs
            .get("distortion")
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        skew = node
            .attrs
            .get("skew")
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
    }

    // 1. Ellipses and circles (`sides <= 2` and no distortion/skew).
    if sides <= 2 && distortion.abs() <= 1e-9 && skew.abs() <= 1e-9 && !shape.is_record() {
        return intersect_ray_with_ellipse(origin, outside_pt, node.pos, rx, ry);
    }

    // 2. Axis-aligned rectangular boxes and records.
    let is_box = (sides == 4
        && (orientation.rem_euclid(90.0)).abs() < 0.5
        && distortion.abs() <= 1e-9
        && skew.abs() <= 1e-9
        && !matches!(shape, ShapeKind::Star))
        || shape.is_record()
        || shape == ShapeKind::Cylinder;
    if is_box {
        let bbox = BoundingBox::new(
            node.pos.x - rx,
            node.pos.y - ry,
            node.pos.x + rx,
            node.pos.y + ry,
        );
        return intersect_ray_with_box(origin, outside_pt, bbox);
    }

    // 3. General convex/non-convex polygons (diamond, triangle, hexagon, octagon, trapezium, star, etc.).
    let verts = shape_polygon_vertices(node);
    if verts.len() >= 3
        && let Some(hit) = intersect_ray_with_polygon(origin, outside_pt, node.pos, &verts)
    {
        return hit;
    }

    intersect_ray_with_ellipse(origin, outside_pt, node.pos, rx, ry)
}

fn intersect_ray_with_ellipse(
    origin: Point,
    outside_pt: Point,
    center: Point,
    rx: f64,
    ry: f64,
) -> Point {
    let dx = outside_pt.x - origin.x;
    let dy = outside_pt.y - origin.y;
    let ox = origin.x - center.x;
    let oy = origin.y - center.y;

    let a = (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry);
    if a <= 1e-18 {
        return Point::new(center.x + rx, center.y);
    }
    let b = 2.0 * ((ox * dx) / (rx * rx) + (oy * dy) / (ry * ry));
    let c = (ox * ox) / (rx * rx) + (oy * oy) / (ry * ry) - 1.0;

    let disc = b * b - 4.0 * a * c;
    if disc >= 0.0 {
        let t = (-b + disc.sqrt()) / (2.0 * a);
        if t >= 0.0 {
            return Point::new(origin.x + t * dx, origin.y + t * dy);
        }
    }

    // Fallback: radial projection from ellipse center.
    let rdx = outside_pt.x - center.x;
    let rdy = outside_pt.y - center.y;
    let scale = ((rdx / rx).powi(2) + (rdy / ry).powi(2)).sqrt();
    if scale <= 1e-12 {
        Point::new(center.x + rx, center.y)
    } else {
        Point::new(center.x + rdx / scale, center.y + rdy / scale)
    }
}

fn intersect_ray_with_box(origin: Point, outside_pt: Point, bbox: BoundingBox) -> Point {
    let ox = origin.x.clamp(bbox.ll.x, bbox.ur.x);
    let oy = origin.y.clamp(bbox.ll.y, bbox.ur.y);
    let dx = outside_pt.x - ox;
    let dy = outside_pt.y - oy;

    if dx.hypot(dy) <= 1e-12 {
        return Point::new(bbox.ur.x, oy);
    }

    let tx = if dx > 1e-12 {
        (bbox.ur.x - ox) / dx
    } else if dx < -1e-12 {
        (bbox.ll.x - ox) / dx
    } else {
        f64::INFINITY
    };

    let ty = if dy > 1e-12 {
        (bbox.ur.y - oy) / dy
    } else if dy < -1e-12 {
        (bbox.ll.y - oy) / dy
    } else {
        f64::INFINITY
    };

    let t = tx.min(ty);
    if t.is_finite() && t >= 0.0 {
        Point::new(ox + t * dx, oy + t * dy)
    } else {
        Point::new(bbox.ur.x, oy)
    }
}

fn intersect_ray_with_polygon(
    origin: Point,
    outside_pt: Point,
    center: Point,
    local_verts: &[Point],
) -> Option<Point> {
    let dx = outside_pt.x - origin.x;
    let dy = outside_pt.y - origin.y;
    if dx.hypot(dy) <= 1e-12 {
        return None;
    }

    let mut best_t: Option<f64> = None;
    let n = local_verts.len();
    for i in 0..n {
        let a = Point::new(center.x + local_verts[i].x, center.y + local_verts[i].y);
        let b = Point::new(
            center.x + local_verts[(i + 1) % n].x,
            center.y + local_verts[(i + 1) % n].y,
        );
        let sx = b.x - a.x;
        let sy = b.y - a.y;

        // Solve: origin + t * d = a + u * s
        let denom = dx * sy - dy * sx;
        if denom.abs() <= 1e-12 {
            continue;
        }
        let qpx = a.x - origin.x;
        let qpy = a.y - origin.y;
        let t = (qpx * sy - qpy * sx) / denom;
        let u = (qpx * dy - qpy * dx) / denom;

        if t >= -1e-9 && (-1e-9..=1.0 + 1e-9).contains(&u) {
            let t_pos = t.max(0.0);
            best_t = Some(match best_t {
                Some(prev) => prev.max(t_pos),
                None => t_pos,
            });
        }
    }

    best_t.map(|t| Point::new(origin.x + t * dx, origin.y + t * dy))
}
