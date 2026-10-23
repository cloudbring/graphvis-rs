//! Node shape classification (`shapes.c`'s shape table) and the HTML-label
//! entry points used by node sizing (`lib/common/shapes.c`, `htmltable.c`).

use crate::html_label::{HtmlLabelLayout, make_html_label};
use crate::node_size::LabelGeometry;
use graphviz_pure_core::font::LabelSubstContext;
use graphviz_pure_core::{BoundingBox, NodeData};

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
    /// `epsf`: a PostScript user shape (`epsf_init`, `epsf_gencode`).
    Epsf,
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
            "epsf" => Self::Epsf,
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
    /// `f->sides` (`BOTTOM` = 1, `RIGHT` = 2, `TOP` = 4, `LEFT` = 8).
    pub sides: u8,
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

/// `make_html_label` for an HTML label (the inner text of the HTML string).
pub(crate) fn html_label_layout(
    label: &str,
    obj: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
    cs: graphviz_pure_core::charset::Charset,
) -> HtmlLabelLayout {
    make_html_label(
        label,
        obj,
        font_name,
        font_size,
        obj.font_color,
        obj.pen_color,
        cs,
    )
}
