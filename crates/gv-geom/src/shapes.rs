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
    let shape = ShapeKind::from_node(node);

    // 1. Special case: `shape=point` (`point_init` in `shapes.c`).
    if shape == ShapeKind::Point {
        let w_attr = node
            .attrs
            .get("width")
            .and_then(|s| parse_f64_strip_units(s));
        let h_attr = node
            .attrs
            .get("height")
            .and_then(|s| parse_f64_strip_units(s));
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
        let final_in = if sz_pts > 0.0 {
            (sz_pts / POINTS_PER_INCH).max(MIN_POINT_SIZE)
        } else {
            0.0
        };
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
        .and_then(|s| parse_f64_strip_units(s))
        .unwrap_or(DEFAULT_FONT_SIZE)
        .max(1.0);
    let (margin_x, margin_y) = parse_node_margin(node.attrs.get("margin").map(String::as_str));

    let min_allowed_w = if shape == ShapeKind::Plain {
        0.0
    } else {
        MIN_NODE_WIDTH
    };
    let min_allowed_h = if shape == ShapeKind::Plain {
        0.0
    } else {
        MIN_NODE_HEIGHT
    };
    let explicit_w = node
        .attrs
        .get("width")
        .and_then(|s| parse_f64_strip_units(s))
        .map(|w| w.max(min_allowed_w));
    let explicit_h = node
        .attrs
        .get("height")
        .and_then(|s| parse_f64_strip_units(s))
        .map(|h| h.max(min_allowed_h));

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
    // In C Graphviz (`make_label` / `make_simple_label`), plain text labels are stored in
    // integer points (`dimen.x = (int)w`, `dimen.y = (int)h`), and HTML table labels
    // with `shape=plaintext` have `PAD` skipped when `<TABLE>` is the root.
    let is_html = is_html_label(&raw_label, shape);
    let mut dimen = if is_html {
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
        Point::new(layout.width, layout.height.floor())
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
        .and_then(|s| parse_f64_strip_units(s))
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
        0.0
    } else {
        MIN_NODE_WIDTH
    });
    node.height = (bb.y / POINTS_PER_INCH).max(if shape == ShapeKind::Plain {
        0.0
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
        .and_then(|s| parse_f64_strip_units(s))
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
        // In C Graphviz (`record_size` / `make_simple_label`), plain text height is truncated
        // to integer points before adding `2 * margin_y`.
        let mut h = layout.height.floor();
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
        .and_then(|s| parse_f64_strip_units(s))
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
    let sz = measure_html_rich_text(inner, &ctx, font_name, font_size).0;
    Point::new(sz.x, sz.y.floor())
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
        let after_close = tr_abs + c_end;
        cursor = table_body[after_close..]
            .find('>')
            .map_or(table_body.len(), |p| after_close + p + 1);
    }

    let row_count = raw_rows.len();
    if row_count == 0 {
        let w = (2.0 * border).max(min_width);
        let h = (2.0 * border).max(min_height);
        return Some(HtmlTable {
            port,
            border,
            cellborder,
            cellspacing,
            cellpadding,
            bgcolor,
            rounded,
            row_count: 0,
            col_count: 0,
            bbox: BoundingBox::new(0.0, 0.0, w, h),
            cells: Vec::new(),
        });
    }

    // Parse cells and place them on a 2D occupancy grid (handling `rowspan` and `colspan`).
    let mut occupied: Vec<Vec<bool>> = vec![Vec::new(); row_count];
    let mut cells: Vec<HtmlCell> = Vec::new();
    let mut max_cols = 0usize;

    for (r_idx, &row_str) in raw_rows.iter().enumerate() {
        let mut td_cursor = 0usize;
        let mut c_idx = 0usize;
        while td_cursor < row_str.len() {
            let rem = &row_str[td_cursor..];
            let Some(td_rel) = find_tag_ci(rem, "<td") else {
                break;
            };
            let td_abs = td_cursor + td_rel;
            let Some(td_hdr_end_rel) = row_str[td_abs..].find('>') else {
                break;
            };
            let td_header = &row_str[td_abs..=td_abs + td_hdr_end_rel];
            let Some((c_start, c_end)) = find_matching_close_tag(&row_str[td_abs..], "td", "td")
            else {
                break;
            };
            let td_body = &row_str[td_abs + c_start..td_abs + c_end];

            // Advance `c_idx` past any columns occupied by prior rowspans.
            while c_idx < occupied[r_idx].len() && occupied[r_idx][c_idx] {
                c_idx += 1;
            }

            let td_attrs = parse_tag_attrs(td_header);
            let mut colspan = 1usize;
            let mut rowspan = 1usize;
            let mut cell_port: Option<String> = None;
            let mut cell_bg: Option<String> = None;
            let mut cell_border = cellborder.unwrap_or(1.0);
            let mut cell_pad = if cellpadding_set { cellpadding } else { 2.0 };
            let mut cell_align = TextJustify::Center;
            let mut cell_min_w = 0.0_f64;
            let mut cell_min_h = 0.0_f64;
            let mut cell_fixed = false;

            for (k, v) in td_attrs {
                match k.as_str() {
                    "colspan" => {
                        if let Ok(cs) = v.trim().parse::<usize>() {
                            colspan = cs.max(1);
                        }
                    }
                    "rowspan" => {
                        if let Ok(rs) = v.trim().parse::<usize>() {
                            rowspan = rs.max(1);
                        }
                    }
                    "port" => cell_port = Some(v),
                    "bgcolor" => cell_bg = Some(v),
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
                    "align" | "balign" => {
                        cell_align = match v.trim().to_ascii_lowercase().as_str() {
                            "left" => TextJustify::Left,
                            "right" => TextJustify::Right,
                            _ => TextJustify::Center,
                        };
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
                    "fixedsize" => {
                        cell_fixed = parse_graphviz_bool(Some(&v), false);
                    }
                    _ => {}
                }
            }

            // Clamp rowspan to `row_count - r_idx` (or expand if needed; in Graphviz `rowspan` exceeding table rows is clamped/extended).
            let eff_rowspan = rowspan.min(row_count - r_idx).max(1);
            for row_occ in occupied.iter_mut().skip(r_idx).take(eff_rowspan) {
                if row_occ.len() < c_idx + colspan {
                    row_occ.resize(c_idx + colspan, false);
                }
                for cell_occ in row_occ.iter_mut().skip(c_idx).take(colspan) {
                    *cell_occ = true;
                }
            }
            max_cols = max_cols.max(c_idx + colspan);

            // Check if `td_body` contains a nested `<TABLE>`.
            let nested_table = if find_tag_ci(td_body, "<table").is_some() {
                parse_single_table(td_body, ctx, font_name, font_size)
            } else {
                None
            };

            let (content_sz, plain_text) = if let Some(ref nt) = nested_table {
                (Point::new(nt.bbox.width(), nt.bbox.height()), String::new())
            } else {
                measure_html_rich_text(td_body, ctx, font_name, font_size)
            };

            let pad_delta = 2.0 * (cell_pad + cell_border);
            let nat_w = content_sz.x + pad_delta;
            let nat_h = content_sz.y + pad_delta;
            let cw = if cell_fixed && cell_min_w > 0.0 {
                cell_min_w
            } else {
                nat_w.max(cell_min_w)
            };
            let ch = if cell_fixed && cell_min_h > 0.0 {
                cell_min_h
            } else {
                nat_h.max(cell_min_h)
            };

            cells.push(HtmlCell {
                port: cell_port,
                text: plain_text,
                row: r_idx,
                col: c_idx,
                rowspan: eff_rowspan,
                colspan,
                border: cell_border,
                cellpadding: cell_pad,
                bgcolor: cell_bg,
                align: cell_align,
                bbox: BoundingBox::new(0.0, 0.0, cw, ch),
                nested_table,
            });

            c_idx += colspan;
            let after_close = td_abs + c_end;
            td_cursor = row_str[after_close..]
                .find('>')
                .map_or(row_str.len(), |p| after_close + p + 1);
        }
    }

    let col_count = max_cols.max(1);
    let mut col_widths = vec![0.0_f64; col_count];
    let mut row_heights = vec![0.0_f64; row_count];

    // Pass 1: Single-column / single-row cells establish baseline widths and heights.
    for cell in &cells {
        if cell.colspan == 1 {
            col_widths[cell.col] = col_widths[cell.col].max(cell.bbox.width());
        }
        if cell.rowspan == 1 {
            row_heights[cell.row] = row_heights[cell.row].max(cell.bbox.height());
        }
    }

    // Pass 2: Multi-column / multi-row cells distribute any excess across spanned columns/rows.
    for cell in &cells {
        if cell.colspan > 1 {
            let span_cols = &mut col_widths[cell.col..cell.col + cell.colspan];
            let current_sum: f64 =
                span_cols.iter().sum::<f64>() + ((cell.colspan - 1) as f64) * cellspacing;
            if cell.bbox.width() > current_sum {
                let extra = (cell.bbox.width() - current_sum) / (cell.colspan as f64);
                for w in span_cols.iter_mut() {
                    *w += extra;
                }
            }
        }
        if cell.rowspan > 1 {
            let span_rows = &mut row_heights[cell.row..cell.row + cell.rowspan];
            let current_sum: f64 =
                span_rows.iter().sum::<f64>() + ((cell.rowspan - 1) as f64) * cellspacing;
            if cell.bbox.height() > current_sum {
                let extra = (cell.bbox.height() - current_sum) / (cell.rowspan as f64);
                for h in span_rows.iter_mut() {
                    *h += extra;
                }
            }
        }
    }

    let total_cols_w: f64 = col_widths.iter().sum();
    let total_rows_h: f64 = row_heights.iter().sum();
    let total_w =
        (total_cols_w + ((col_count + 1) as f64) * cellspacing + 2.0 * border).max(min_width);
    let total_h =
        (total_rows_h + ((row_count + 1) as f64) * cellspacing + 2.0 * border).max(min_height);

    // Store column widths and row heights into cell bboxes relative to table `(0, 0)..(total_w, total_h)`.
    if total_w > total_cols_w + ((col_count + 1) as f64) * cellspacing + 2.0 * border {
        let extra = (total_w
            - (total_cols_w + ((col_count + 1) as f64) * cellspacing + 2.0 * border))
            / (col_count as f64);
        for w in &mut col_widths {
            *w += extra;
        }
    }
    if total_h > total_rows_h + ((row_count + 1) as f64) * cellspacing + 2.0 * border {
        let extra = (total_h
            - (total_rows_h + ((row_count + 1) as f64) * cellspacing + 2.0 * border))
            / (row_count as f64);
        for h in &mut row_heights {
            *h += extra;
        }
    }

    let mut col_x = vec![0.0_f64; col_count + 1];
    col_x[0] = border + cellspacing;
    for c in 0..col_count {
        col_x[c + 1] = col_x[c] + col_widths[c] + cellspacing;
    }

    let mut row_y_top = vec![0.0_f64; row_count + 1];
    row_y_top[0] = total_h - border - cellspacing;
    for r in 0..row_count {
        row_y_top[r + 1] = row_y_top[r] - row_heights[r] - cellspacing;
    }

    for cell in &mut cells {
        let x0 = col_x[cell.col];
        let x1 = col_x[cell.col + cell.colspan] - cellspacing;
        let y1 = row_y_top[cell.row];
        let y0 = row_y_top[cell.row + cell.rowspan] + cellspacing;
        cell.bbox = BoundingBox::new(x0, y0, x1, y1);
    }

    Some(HtmlTable {
        port,
        border,
        cellborder,
        cellspacing,
        cellpadding,
        bgcolor,
        rounded,
        row_count,
        col_count,
        bbox: BoundingBox::new(0.0, 0.0, total_w, total_h),
        cells,
    })
}

fn position_html_table(tbl: &mut HtmlTable, target_box: BoundingBox) {
    let cur_w = tbl.bbox.width().max(1e-9);
    let cur_h = tbl.bbox.height().max(1e-9);
    let sx = target_box.width() / cur_w;
    let sy = target_box.height() / cur_h;
    let ox = target_box.ll.x - tbl.bbox.ll.x * sx;
    let oy = target_box.ll.y - tbl.bbox.ll.y * sy;
    tbl.bbox = target_box;

    for cell in &mut tbl.cells {
        let c_ll = Point::new(ox + cell.bbox.ll.x * sx, oy + cell.bbox.ll.y * sy);
        let c_ur = Point::new(ox + cell.bbox.ur.x * sx, oy + cell.bbox.ur.y * sy);
        cell.bbox = BoundingBox::new(c_ll.x, c_ll.y, c_ur.x, c_ur.y);
        if let Some(ref mut nested) = cell.nested_table {
            let inset = cell.border + cell.cellpadding;
            let inner_box = BoundingBox::new(
                (c_ll.x + inset).min(c_ur.x),
                (c_ll.y + inset).min(c_ur.y),
                (c_ur.x - inset).max(c_ll.x),
                (c_ur.y - inset).max(c_ll.y),
            );
            position_html_table(nested, inner_box);
        }
    }
}

/// Measures HTML rich text (supporting `<BR/>`, `<B>`, `<I>`, `<FONT FACE="..." POINT-SIZE="...">`),
/// returning `(Point(width, height), plain_text)`.
fn measure_html_rich_text(
    html_fragment: &str,
    ctx: &LabelSubstContext<'_>,
    base_font: &str,
    base_size: f64,
) -> (Point, String) {
    #[derive(Clone)]
    struct StyleState {
        font: String,
        size: f64,
        bold: bool,
        italic: bool,
    }

    let mut stack = vec![StyleState {
        font: base_font.to_string(),
        size: base_size,
        bold: false,
        italic: false,
    }];

    let mut line_widths: Vec<f64> = Vec::new();
    let mut line_heights: Vec<f64> = Vec::new();
    let mut cur_line_w = 0.0_f64;
    let mut cur_line_h = 0.0_f64;
    let mut cur_text = String::new();
    let mut all_text = String::new();

    let flush_segment =
        |seg: &str, st: &StyleState, cur_w: &mut f64, cur_h: &mut f64, plain_out: &mut String| {
            if seg.is_empty() {
                return;
            }
            let collapsed = collapse_html_whitespace(seg);
            if collapsed.is_empty() {
                return;
            }
            let sub = substitute_object_escapes(&collapsed, ctx);
            let (w, h) = estimate_text_size_styled(&sub, &st.font, st.size, st.bold, st.italic);
            *cur_w += w;
            *cur_h = cur_h.max(h.floor());
            plain_out.push_str(&sub);
        };

    let bytes = html_fragment.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(rel_end) = html_fragment[i..].find('>')
        {
            let cur_style = stack.last().unwrap().clone();
            flush_segment(
                &cur_text,
                &cur_style,
                &mut cur_line_w,
                &mut cur_line_h,
                &mut all_text,
            );
            cur_text.clear();

            let tag_str = html_fragment[i + 1..i + rel_end].trim();
            let tag_lower = tag_str.to_ascii_lowercase();
            let tag_name = tag_lower
                .split(|c: char| c.is_ascii_whitespace() || c == '/')
                .next()
                .unwrap_or("");

            match tag_name {
                "br" => {
                    let h = if cur_line_h > 0.0 {
                        cur_line_h
                    } else {
                        (cur_style.size * LINESPACING).floor()
                    };
                    line_widths.push(cur_line_w);
                    line_heights.push(h);
                    cur_line_w = 0.0;
                    cur_line_h = 0.0;
                    all_text.push('\n');
                }
                "b" | "strong" => {
                    let mut next = cur_style;
                    next.bold = true;
                    stack.push(next);
                }
                "/b" | "/strong" | "/i" | "/em" | "/font" => {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                }
                "i" | "em" => {
                    let mut next = cur_style;
                    next.italic = true;
                    stack.push(next);
                }
                "font" => {
                    let mut next = cur_style;
                    for (k, v) in parse_tag_attrs(&html_fragment[i..=i + rel_end]) {
                        if k == "face" && !v.trim().is_empty() {
                            next.font = v.trim().to_string();
                        } else if k == "point-size"
                            && let Ok(sz) = v.trim().parse::<f64>()
                        {
                            next.size = sz.max(1.0);
                        }
                    }
                    stack.push(next);
                }
                _ => {}
            }
            i += rel_end + 1;
            continue;
        }
        let ch = html_fragment[i..].chars().next().unwrap();
        cur_text.push(ch);
        i += ch.len_utf8();
    }

    let cur_style = stack.last().unwrap().clone();
    flush_segment(
        &cur_text,
        &cur_style,
        &mut cur_line_w,
        &mut cur_line_h,
        &mut all_text,
    );
    if cur_line_w > 0.0 || line_widths.is_empty() {
        let h = if cur_line_h > 0.0 {
            cur_line_h
        } else if !all_text.trim().is_empty() {
            (cur_style.size * LINESPACING).floor()
        } else {
            0.0
        };
        line_widths.push(cur_line_w);
        line_heights.push(h);
    }

    let trimmed_lines: Vec<String> = all_text.lines().map(|l| l.trim().to_string()).collect();
    let final_plain = trimmed_lines.join("\n");

    let max_w = line_widths.into_iter().fold(0.0_f64, f64::max);
    let total_h: f64 = line_heights.into_iter().map(f64::floor).sum();
    (Point::new(max_w.ceil(), total_h.floor()), final_plain)
}

fn collapse_html_whitespace(s: &str) -> String {
    let decoded = gv_core::font::decode_html_entities(s);
    let mut out = String::with_capacity(decoded.len());
    let mut prev_space = false;
    for ch in decoded.chars() {
        if ch.is_ascii_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
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
