//! HTML-like labels, ported from Graphviz 16.1.0 `lib/common/htmllex.c` (the expat-driven
//! lexer, including `findNext`, `scanEntity`, `characterData`, `startElement`, `endElement` and
//! the attribute tables), `lib/common/htmlparse.y` (grammar and its actions) and
//! `lib/common/htmltable.c` (`make_html_label`, `size_html_txt`, `size_html_cell`,
//! `processTbl`, `findCol`, `set_cell_widths`, `set_cell_heights`, `size_html_tbl`,
//! `pushFontInfo`/`popFontInfo`, `pos_html_tbl`, `pos_html_cell`, `pos_html_txt`).
//!
//! The XML well-formedness rules expat enforces (exactly matching end tags, quoted attribute
//! values, known entities) are reproduced, because any error makes C drop the label entirely
//! (`make_html_label` returns 3 and the label keeps `dimen = (0, 0)`).

use graphviz_pure_core::charset::{self, Charset};
use graphviz_pure_core::font::{
    LINESPACING, LabelSubstContext, estimate_text_size_styled, strdup_and_subst_obj,
};
use graphviz_pure_core::{BoundingBox, Point};
use std::collections::HashSet;

// Font flags (`lib/common/textspan.h`).
pub const HTML_BF: u8 = 1 << 0;
pub const HTML_IF: u8 = 1 << 1;
pub const HTML_UL: u8 = 1 << 2;
pub const HTML_SUP: u8 = 1 << 3;
pub const HTML_SUB: u8 = 1 << 4;
pub const HTML_S: u8 = 1 << 5;
pub const HTML_OL: u8 = 1 << 6;

// `htmldata_t.flags` (`lib/common/htmltable.h`).
pub const FIXED_FLAG: u16 = 1;
pub const HALIGN_RIGHT: u16 = 1 << 1;
pub const HALIGN_LEFT: u16 = 1 << 2;
pub const HALIGN_MASK: u16 = HALIGN_RIGHT | HALIGN_LEFT;
pub const HALIGN_TEXT: u16 = HALIGN_MASK;
pub const VALIGN_TOP: u16 = 1 << 3;
pub const VALIGN_BOTTOM: u16 = 1 << 4;
pub const VALIGN_MASK: u16 = VALIGN_TOP | VALIGN_BOTTOM;
pub const BORDER_SET: u16 = 1 << 5;
pub const PAD_SET: u16 = 1 << 6;
pub const SPACE_SET: u16 = 1 << 7;
pub const BALIGN_RIGHT: u16 = 1 << 8;
pub const BALIGN_LEFT: u16 = 1 << 9;
pub const BALIGN_MASK: u16 = BALIGN_RIGHT | BALIGN_LEFT;
pub const BORDER_LEFT: u16 = 1 << 10;
pub const BORDER_TOP: u16 = 1 << 11;
pub const BORDER_RIGHT: u16 = 1 << 12;
pub const BORDER_BOTTOM: u16 = 1 << 13;
pub const BORDER_MASK: u16 = BORDER_LEFT | BORDER_TOP | BORDER_RIGHT | BORDER_BOTTOM;

const DEFAULT_BORDER: u8 = 1;
const DEFAULT_CELLPADDING: u8 = 2;
const DEFAULT_CELLSPACING: i8 = 2;

// Sides (`BOTTOM`, `RIGHT`, `TOP`, `LEFT` in `const.h`).
pub const SIDE_BOTTOM: u8 = 1 << 0;
pub const SIDE_RIGHT: u8 = 1 << 1;
pub const SIDE_TOP: u8 = 1 << 2;
pub const SIDE_LEFT: u8 = 1 << 3;

/// `textfont_t` as used by HTML labels (`size < 0` means unset).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlFont {
    pub name: Option<String>,
    pub color: Option<String>,
    pub size: f64,
    pub flags: u8,
}

/// `htmlstyle_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HtmlStyle {
    pub rounded: bool,
    pub radial: bool,
    pub invisible: bool,
    pub dotted: bool,
    pub dashed: bool,
}

/// `htmldata_t`: attributes shared by tables and cells, plus the computed box.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlData {
    pub href: Option<String>,
    pub port: Option<String>,
    pub target: Option<String>,
    pub title: Option<String>,
    pub id: Option<String>,
    pub bgcolor: Option<String>,
    pub pencolor: Option<String>,
    pub gradientangle: u16,
    pub space: i8,
    pub border: u8,
    pub pad: u8,
    pub sides: u8,
    pub flags: u16,
    pub width: u16,
    pub height: u16,
    pub style: HtmlStyle,
    /// Size after sizing (`LL = (0, 0)`), position (label coordinates, y up) after positioning.
    pub bx: BoundingBox,
}

/// One measured text item (`textspan_t` inside an `htextspan_t`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlTextItem {
    /// Text after `strdup_and_subst_obj`.
    pub text: String,
    /// The font in effect when the item was parsed (`None`: label font).
    pub font: Option<HtmlFont>,
    /// Resolved font used for measuring.
    pub font_name: String,
    pub font_size: f64,
    pub flags: u8,
    pub color: Option<String>,
    /// `size.x`.
    pub width: f64,
    pub yoffset_layout: f64,
    pub yoffset_centerline: f64,
}

/// A line of text items (`htextspan_t`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlTextLine {
    pub items: Vec<HtmlTextItem>,
    /// `'l'`, `'r'`, `'n'` or `'\0'` (unset).
    pub just: char,
    /// Width of the line.
    pub size: f64,
    /// Offset from the previous baseline to this one.
    pub lfsize: f64,
}

/// `htmltxt_t`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlText {
    pub spans: Vec<HtmlTextLine>,
    pub simple: bool,
    pub bx: BoundingBox,
}

/// `htmlimg_t`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlImage {
    pub src: Option<String>,
    pub scale: Option<String>,
    pub bx: BoundingBox,
}

/// Cell content (`htmllabel_t` child).
#[derive(Debug, Clone, PartialEq)]
pub enum HtmlContent {
    Text(HtmlText),
    Table(Box<HtmlTbl>),
    Image(HtmlImage),
}

/// `htmlcell_t`.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlCellData {
    pub data: HtmlData,
    pub colspan: u16,
    pub rowspan: u16,
    pub col: u16,
    pub row: u16,
    pub child: HtmlContent,
    pub vruled: bool,
    pub hruled: bool,
}

/// `htmltbl_t`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlTbl {
    pub data: HtmlData,
    /// Cells in row-major parse order (`tbl->cells`).
    pub cells: Vec<HtmlCellData>,
    /// Row index of every cell in `cells` at parse time and the row `ruled` flags.
    row_of_cell: Vec<usize>,
    row_ruled: Vec<bool>,
    /// `tbl->font`: font in effect around the `<TABLE>`.
    pub font: Option<HtmlFont>,
    pub cellborder: i8,
    pub row_count: usize,
    pub column_count: usize,
    /// Column widths during sizing, column start x positions after positioning.
    pub widths: Vec<f64>,
    /// Row heights during sizing, row start y positions after positioning.
    pub heights: Vec<f64>,
    pub hrule: bool,
    pub vrule: bool,
}

/// `htmllabel_t`.
#[derive(Debug, Clone, PartialEq)]
pub enum HtmlLabel {
    Text(HtmlText),
    Table(Box<HtmlTbl>),
}

/// Result of [`make_html_label`].
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlLabelLayout {
    /// `lp->dimen` in points.
    pub dimen: Point,
    /// Positioned label (centered on `(0, 0)`, y up), `None` when C rejected the label.
    pub label: Option<HtmlLabel>,
}

// ---------------------------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Html,
    EndHtml,
    Table(Box<HtmlTbl>),
    EndTable,
    Row,
    EndRow,
    Cell(Box<HtmlCellData>),
    EndCell,
    Font(HtmlFont),
    EndFont,
    Italic(HtmlFont),
    NItalic,
    Bold(HtmlFont),
    NBold,
    Underline(HtmlFont),
    NUnderline,
    Overline(HtmlFont),
    NOverline,
    Sup(HtmlFont),
    NSup,
    Sub(HtmlFont),
    NSub,
    S(HtmlFont),
    NS,
    Br(char),
    BrFull(char),
    EndBr,
    Hr,
    HrFull,
    EndHr,
    Vr,
    VrFull,
    EndVr,
    Img(HtmlImage),
    ImgFull(HtmlImage),
    EndImg,
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    Br,
    Hr,
    Vr,
    Img,
}

struct Lexer {
    toks: Vec<Tok>,
    in_cell: bool,
    stack: Vec<String>,
    /// The token produced by the current `htmllex` unit (C's `ctx->tok`).
    cur: Option<Tok>,
    pending: Pending,
    /// `doAttrs` warnings: the index of the token whose unit printed them, the message and
    /// whether it sets `ctx->warn`.
    warnings: Vec<(usize, String, bool)>,
}

/// Lexer errors carry expat's message (`XML_ErrorString`) or `htmllex.c`'s own.
type LexResult<T> = Result<T, String>;

const NOT_WELL_FORMED: &str = "not well-formed (invalid token)";

fn nwf<T>() -> LexResult<T> {
    Err(NOT_WELL_FORMED.to_string())
}

/// `scanEntity` from `lib/common/utils.c`: `t` starts after `&`; appends `&` (and `#N;` for a
/// known name) to `out`, returning the number of input bytes consumed after the `&`.
fn scan_entity(t: &[u8], out: &mut Vec<u8>) -> usize {
    out.push(b'&');
    let Some(semi) = t.iter().position(|&c| c == b';') else {
        return 0;
    };
    // strview(t, ';') stops at ';' or NUL; NUL cannot occur in Rust strings.
    let key = &t[..semi];
    if key.len() > 8 || key.len() < 2 {
        return 0;
    }
    let Ok(name) = std::str::from_utf8(key) else {
        return 0;
    };
    match html_entity_value(name) {
        Some(v) => {
            out.extend_from_slice(format!("#{v};").as_bytes());
            semi + 1
        }
        None => 0,
    }
}

fn html_entity_value(name: &str) -> Option<u32> {
    // Same table as `htmlEntityUTF8` (entities.h).
    let probe = format!("&{name};");
    let decoded = graphviz_pure_core::font::html_entity_utf8(&probe);
    if decoded == probe {
        return None;
    }
    decoded.chars().next().map(u32::from)
}

/// Decodes XML character/entity references as expat does; errors on anything malformed.
fn xml_decode(s: &[u8], in_attr: bool) -> LexResult<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c == b'&' {
            let semi = s[i + 1..]
                .iter()
                .position(|&b| b == b';')
                .ok_or_else(|| NOT_WELL_FORMED.to_string())?;
            let name = std::str::from_utf8(&s[i + 1..i + 1 + semi])
                .map_err(|_| NOT_WELL_FORMED.to_string())?;
            let ch = if let Some(num) = name.strip_prefix('#') {
                let v = if let Some(hex) = num.strip_prefix('x') {
                    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return nwf();
                    }
                    u32::from_str_radix(hex, 16).map_err(|_| NOT_WELL_FORMED.to_string())?
                } else {
                    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
                        return nwf();
                    }
                    num.parse::<u32>()
                        .map_err(|_| NOT_WELL_FORMED.to_string())?
                };
                let bad = || "reference to invalid character number".to_string();
                let ch = char::from_u32(v).ok_or_else(bad)?;
                // XML Char production
                if !(ch == '\t' || ch == '\n' || ch == '\r' || ch >= ' ') {
                    return Err(bad());
                }
                ch
            } else {
                match name {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ => return Err("undefined entity".to_string()),
                }
            };
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            i += semi + 2;
            continue;
        }
        if c == b'<' && in_attr {
            return nwf();
        }
        if c < b' ' && c != b'\t' && c != b'\n' && c != b'\r' {
            return nwf();
        }
        if c == b'\r' {
            // line-end normalisation
            out.push(if in_attr { b' ' } else { b'\n' });
            if s.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            i += 1;
            continue;
        }
        if in_attr && (c == b'\t' || c == b'\n') {
            out.push(b' ');
        } else {
            out.push(c);
        }
        i += 1;
    }
    Ok(out)
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80
}

fn is_name_char(c: u8) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == b'-' || c == b'.'
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// The `agwarningf` messages of `doAttrs` and its attribute actions for one element, each
/// with whether it sets `ctx->warn` (which makes `make_label` add `in label of ...`).
#[derive(Debug, Default)]
struct AttrWarnings(Vec<(String, bool)>);

impl AttrWarnings {
    /// A warning whose action returns 1.
    fn warn(&mut self, msg: String) {
        self.0.push((msg, true));
    }
    /// A warning that leaves `ctx->warn` alone (`sidesfn`).
    fn note(&mut self, msg: String) {
        self.0.push((msg, false));
    }
}

/// `doInt(v, s, min, max, &ul)` from `htmllex.c`: `strtol` base 10, then the range check.
/// The messages have no trailing newline, as in C.
fn do_int(v: &str, s: &str, min: i64, max: i64, w: &mut AttrWarnings) -> Option<i64> {
    let b = v.as_bytes();
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let ds = i;
    let mut val: i64 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        val = val
            .saturating_mul(10)
            .saturating_add(i64::from(b[i] - b'0'));
        i += 1;
    }
    if i == ds {
        w.warn(format!("Improper {s} value {v} - ignored"));
        return None;
    }
    let val = if neg { -val } else { val };
    if val > max {
        w.warn(format!("{s} value {v} > {max} - too large - ignored"));
        return None;
    }
    if val < min {
        w.warn(format!("{s} value {v} < {min} - too small - ignored"));
        return None;
    }
    Some(val)
}

/// `tok(v, " ,")` from `lib/util/tokenize.h`: unlike `split` it yields an empty first token
/// for an empty or separator-led string and an empty last token after trailing separators.
fn style_tokens(v: &str) -> Vec<&str> {
    let is_sep = |c: char| c == ' ' || c == ',';
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let size = v[start..].find(is_sep).unwrap_or(v.len() - start);
        out.push(&v[start..start + size]);
        let mut next = start + size;
        if next == v.len() {
            return out;
        }
        next += v[next..].find(|c| !is_sep(c)).unwrap_or(v.len() - next);
        start = next;
    }
}

/// The actions of `tbl_items` (`cell = false`) and `cell_items` (`cell = true`) that act on
/// the shared `htmldata_t`. Returns `false` for a name neither table has.
fn data_attr(p: &mut HtmlData, name: &str, v: &str, cell: bool, w: &mut AttrWarnings) -> bool {
    let n = name.to_ascii_lowercase();
    match n.as_str() {
        "align" if cell => {
            // cell_halignfn
            if v.eq_ignore_ascii_case("LEFT") {
                p.flags |= HALIGN_LEFT;
            } else if v.eq_ignore_ascii_case("RIGHT") {
                p.flags |= HALIGN_RIGHT;
            } else if v.eq_ignore_ascii_case("TEXT") {
                p.flags |= HALIGN_TEXT;
            } else if !v.eq_ignore_ascii_case("CENTER") {
                w.warn(format!("Illegal value {v} for ALIGN in TD - ignored\n"));
            }
        }
        "align" => {
            // halignfn
            if v.eq_ignore_ascii_case("LEFT") {
                p.flags |= HALIGN_LEFT;
            } else if v.eq_ignore_ascii_case("RIGHT") {
                p.flags |= HALIGN_RIGHT;
            } else if !v.eq_ignore_ascii_case("CENTER") {
                w.warn(format!("Illegal value {v} for ALIGN - ignored\n"));
            }
        }
        "balign" if cell => {
            if v.eq_ignore_ascii_case("LEFT") {
                p.flags |= BALIGN_LEFT;
            } else if v.eq_ignore_ascii_case("RIGHT") {
                p.flags |= BALIGN_RIGHT;
            } else if !v.eq_ignore_ascii_case("CENTER") {
                w.warn(format!("Illegal value {v} for BALIGN in TD - ignored\n"));
            }
        }
        "bgcolor" => p.bgcolor = Some(v.to_string()),
        "border" => {
            if let Some(u) = do_int(v, "BORDER", 0, 255, w) {
                p.border = u as u8;
                p.flags |= BORDER_SET;
            }
        }
        "cellpadding" => {
            if let Some(u) = do_int(v, "CELLPADDING", 0, 255, w) {
                p.pad = u as u8;
                p.flags |= PAD_SET;
            }
        }
        "cellspacing" => {
            if let Some(u) = do_int(v, "CELLSPACING", -128, 127, w) {
                p.space = u as i8;
                p.flags |= SPACE_SET;
            }
        }
        "color" => p.pencolor = Some(v.to_string()),
        "fixedsize" => {
            if v.eq_ignore_ascii_case("TRUE") {
                p.flags |= FIXED_FLAG;
            } else if !v.eq_ignore_ascii_case("FALSE") {
                w.warn(format!("Illegal value {v} for FIXEDSIZE - ignored\n"));
            }
        }
        "gradientangle" => {
            if let Some(u) = do_int(v, "GRADIENTANGLE", 0, 360, w) {
                p.gradientangle = u as u16;
            }
        }
        "height" => {
            if let Some(u) = do_int(v, "HEIGHT", 0, 65535, w) {
                p.height = u as u16;
            }
        }
        "width" => {
            if let Some(u) = do_int(v, "WIDTH", 0, 65535, w) {
                p.width = u as u16;
            }
        }
        "href" => p.href = Some(v.to_string()),
        "id" => p.id = Some(v.to_string()),
        "port" => p.port = Some(v.to_string()),
        "target" => p.target = Some(v.to_string()),
        "title" | "tooltip" => p.title = Some(v.to_string()),
        "sides" => {
            let mut flags = 0;
            for c in graphviz_pure_core::charset::encode(v).iter().copied() {
                match c.to_ascii_lowercase() {
                    b'l' => flags |= BORDER_LEFT,
                    b't' => flags |= BORDER_TOP,
                    b'r' => flags |= BORDER_RIGHT,
                    b'b' => flags |= BORDER_BOTTOM,
                    _ => {
                        // `%c` of the byte and `%d` of a signed `char` (the oracle's ABI).
                        let shown = if c.is_ascii() {
                            char::from(c)
                        } else {
                            graphviz_pure_core::charset::raw_char(c)
                        };
                        w.note(format!(
                            "Unrecognized character '{shown}' ({}) in sides attribute\n",
                            c as i8
                        ));
                    }
                }
            }
            if flags != BORDER_MASK {
                p.flags |= flags;
            }
        }
        "style" => {
            for tk in style_tokens(v) {
                if tk.eq_ignore_ascii_case("ROUNDED") {
                    p.style.rounded = true;
                } else if tk.eq_ignore_ascii_case("RADIAL") {
                    p.style.radial = true;
                } else if tk.eq_ignore_ascii_case("SOLID") {
                    p.style.dotted = false;
                    p.style.dashed = false;
                } else if tk.eq_ignore_ascii_case("INVISIBLE") || tk.eq_ignore_ascii_case("INVIS") {
                    p.style.invisible = true;
                } else if tk.eq_ignore_ascii_case("DOTTED") {
                    p.style.dotted = true;
                } else if tk.eq_ignore_ascii_case("DASHED") {
                    p.style.dashed = true;
                } else {
                    w.warn(format!("Illegal value {tk} for STYLE - ignored\n"));
                }
            }
        }
        "valign" => {
            if v.eq_ignore_ascii_case("BOTTOM") {
                p.flags |= VALIGN_BOTTOM;
            } else if v.eq_ignore_ascii_case("TOP") {
                p.flags |= VALIGN_TOP;
            } else if !v.eq_ignore_ascii_case("MIDDLE") {
                w.warn(format!("Illegal value {v} for VALIGN - ignored\n"));
            }
        }
        _ => return false,
    }
    true
}

/// `doAttrs`' message for a name the element's item table lacks.
fn illegal_attr(w: &mut AttrWarnings, name: &str, elem: &str) {
    w.warn(format!("Illegal attribute {name} in {elem} - ignored\n"));
}

fn mk_tbl(atts: &[(String, String)], w: &mut AttrWarnings) -> HtmlTbl {
    let mut tbl = HtmlTbl {
        cellborder: -1,
        ..HtmlTbl::default()
    };
    for (name, v) in atts {
        let n = name.to_ascii_lowercase();
        match n.as_str() {
            "cellborder" => {
                if let Some(u) = do_int(v, "CELLBORDER", 0, 127, w) {
                    tbl.cellborder = u as i8;
                }
            }
            "columns" => {
                if v.starts_with('*') {
                    tbl.vrule = true;
                } else {
                    w.warn(format!("Unknown value {v} for COLUMNS - ignored\n"));
                }
            }
            "rows" => {
                if v.starts_with('*') {
                    tbl.hrule = true;
                } else {
                    w.warn(format!("Unknown value {v} for ROWS - ignored\n"));
                }
            }
            _ => {
                if !data_attr(&mut tbl.data, name, v, false, w) {
                    illegal_attr(w, name, "<TABLE>");
                }
            }
        }
    }
    tbl
}

fn mk_cell(atts: &[(String, String)], w: &mut AttrWarnings) -> HtmlCellData {
    let mut cell = HtmlCellData {
        data: HtmlData::default(),
        colspan: 1,
        rowspan: 1,
        col: 0,
        row: 0,
        child: HtmlContent::Text(HtmlText::default()),
        vruled: false,
        hruled: false,
    };
    for (name, v) in atts {
        let n = name.to_ascii_lowercase();
        match n.as_str() {
            "colspan" => match do_int(v, "COLSPAN", 0, 65535, w) {
                Some(0) => w.warn("COLSPAN value cannot be 0 - ignored\n".to_string()),
                Some(u) => cell.colspan = u as u16,
                None => {}
            },
            "rowspan" => match do_int(v, "ROWSPAN", 0, 65535, w) {
                Some(0) => w.warn("ROWSPAN value cannot be 0 - ignored\n".to_string()),
                Some(u) => cell.rowspan = u as u16,
                None => {}
            },
            _ => {
                if !data_attr(&mut cell.data, name, v, true, w) {
                    illegal_attr(w, name, "<TD>");
                }
            }
        }
    }
    cell
}

fn mk_font(atts: &[(String, String)], flags: u8, w: &mut AttrWarnings) -> HtmlFont {
    let mut f = HtmlFont {
        size: -1.0,
        flags,
        ..HtmlFont::default()
    };
    for (name, v) in atts {
        match name.to_ascii_lowercase().as_str() {
            "color" => f.color = Some(v.clone()),
            "face" => f.name = Some(v.clone()),
            "point-size" => {
                if let Some(u) = do_int(v, "POINT-SIZE", 0, 255, w) {
                    f.size = u as f64;
                }
            }
            _ => illegal_attr(w, name, "<FONT>"),
        }
    }
    f
}

fn mk_img(atts: &[(String, String)], w: &mut AttrWarnings) -> HtmlImage {
    let mut img = HtmlImage::default();
    for (name, v) in atts {
        match name.to_ascii_lowercase().as_str() {
            "scale" => img.scale = Some(v.clone()),
            "src" => img.src = Some(v.clone()),
            _ => illegal_attr(w, name, "<IMG>"),
        }
    }
    img
}

fn mk_br(atts: &[(String, String)], w: &mut AttrWarnings) -> char {
    let mut just = '\0';
    for (name, v) in atts {
        if name.eq_ignore_ascii_case("align") {
            // alignfn
            if v.eq_ignore_ascii_case("RIGHT") {
                just = 'r';
            } else if v.eq_ignore_ascii_case("LEFT") {
                just = 'l';
            } else if v.eq_ignore_ascii_case("CENTER") {
                just = 'n';
            } else {
                w.warn(format!("Illegal value {v} for ALIGN - ignored\n"));
            }
        } else {
            illegal_attr(w, name, "<BR>");
        }
    }
    just
}

impl Lexer {
    fn start_element(&mut self, name: &str, atts: &[(String, String)]) -> LexResult<()> {
        let n = name.to_ascii_uppercase();
        self.pending = Pending::None;
        let mut w = AttrWarnings::default();
        let w = &mut w;
        let tok = match n.as_str() {
            "TABLE" => {
                self.in_cell = false;
                Tok::Table(Box::new(mk_tbl(atts, w)))
            }
            "TR" | "TH" => {
                self.in_cell = false;
                Tok::Row
            }
            "TD" => {
                self.in_cell = true;
                Tok::Cell(Box::new(mk_cell(atts, w)))
            }
            "FONT" => Tok::Font(mk_font(atts, 0, w)),
            "B" => Tok::Bold(mk_font(&[], HTML_BF, w)),
            "S" => Tok::S(mk_font(&[], HTML_S, w)),
            "U" => Tok::Underline(mk_font(&[], HTML_UL, w)),
            "O" => Tok::Overline(mk_font(&[], HTML_OL, w)),
            "I" => Tok::Italic(mk_font(&[], HTML_IF, w)),
            "SUP" => Tok::Sup(mk_font(&[], HTML_SUP, w)),
            "SUB" => Tok::Sub(mk_font(&[], HTML_SUB, w)),
            "BR" => {
                self.pending = Pending::Br;
                Tok::Br(mk_br(atts, w))
            }
            "HR" => {
                self.pending = Pending::Hr;
                Tok::Hr
            }
            "VR" => {
                self.pending = Pending::Vr;
                Tok::Vr
            }
            "IMG" => {
                self.pending = Pending::Img;
                Tok::Img(mk_img(atts, w))
            }
            "HTML" => Tok::Html,
            // lexerror
            _ => return Err(format!("\0Unknown HTML element <{name}>")),
        };
        let at = self.toks.len();
        self.warnings
            .extend(w.0.drain(..).map(|(m, set)| (at, m, set)));
        self.cur = Some(tok);
        Ok(())
    }

    fn end_element(&mut self, name: &str) -> LexResult<()> {
        let n = name.to_ascii_uppercase();
        let same_unit = |l: &Lexer, p: Pending| l.cur.is_some() && l.pending == p;
        let tok = match n.as_str() {
            "TABLE" => {
                self.in_cell = true;
                Tok::EndTable
            }
            "TR" | "TH" => Tok::EndRow,
            "TD" => {
                self.in_cell = false;
                Tok::EndCell
            }
            "HTML" => Tok::EndHtml,
            "FONT" => Tok::EndFont,
            "B" => Tok::NBold,
            "U" => Tok::NUnderline,
            "O" => Tok::NOverline,
            "I" => Tok::NItalic,
            "SUP" => Tok::NSup,
            "SUB" => Tok::NSub,
            "S" => Tok::NS,
            "BR" => {
                if same_unit(self, Pending::Br) {
                    match self.cur.take() {
                        Some(Tok::Br(j)) => Tok::BrFull(j),
                        _ => Tok::EndBr,
                    }
                } else {
                    Tok::EndBr
                }
            }
            "HR" => {
                if same_unit(self, Pending::Hr) {
                    Tok::HrFull
                } else {
                    Tok::EndHr
                }
            }
            "VR" => {
                if same_unit(self, Pending::Vr) {
                    Tok::VrFull
                } else {
                    Tok::EndVr
                }
            }
            "IMG" => {
                if same_unit(self, Pending::Img) {
                    match self.cur.take() {
                        Some(Tok::Img(i)) => Tok::ImgFull(i),
                        _ => Tok::EndImg,
                    }
                } else {
                    Tok::EndImg
                }
            }
            _ => return Err(format!("\0Unknown HTML element <{name}>")),
        };
        self.pending = Pending::None;
        self.cur = Some(tok);
        Ok(())
    }

    /// Feeds one `findNext` unit through the XML parser.
    fn unit(&mut self, u: &[u8], is_tag: bool) -> LexResult<()> {
        self.cur = None;
        self.pending = Pending::None;
        if is_tag {
            self.tag(u)?;
        } else {
            let text = xml_decode(u, false)?;
            if text.windows(3).any(|w| w == b"]]>") {
                return nwf();
            }
            // characterData
            if self.in_cell {
                let kept: Vec<u8> = text.into_iter().filter(|&c| c >= b' ').collect();
                if !kept.is_empty() {
                    let s = String::from_utf8_lossy(&kept).into_owned();
                    self.cur = Some(Tok::String(s));
                }
            }
        }
        if let Some(t) = self.cur.take() {
            self.toks.push(t);
        }
        Ok(())
    }

    fn tag(&mut self, u: &[u8]) -> LexResult<()> {
        if u.starts_with(b"<!--") {
            if u.len() < 7 || !u.ends_with(b"-->") {
                return nwf();
            }
            return Ok(());
        }
        if u.starts_with(b"<?") || u.starts_with(b"<!") {
            return nwf();
        }
        if !u.ends_with(b">") {
            return nwf();
        }
        let body = &u[1..u.len() - 1];
        if let Some(rest) = body.strip_prefix(b"/") {
            let mut e = rest.len();
            while e > 0 && is_ws(rest[e - 1]) {
                e -= 1;
            }
            let name = &rest[..e];
            if name.is_empty() || !is_name_start(name[0]) || !name.iter().all(|&c| is_name_char(c))
            {
                return nwf();
            }
            let name = String::from_utf8_lossy(name).into_owned();
            if self.stack.pop().as_deref() != Some(name.as_str()) {
                return Err("mismatched tag".to_string());
            }
            return self.end_element(&name);
        }
        // start tag
        let mut i = 0;
        if body.is_empty() || !is_name_start(body[0]) {
            return nwf();
        }
        while i < body.len() && is_name_char(body[i]) {
            i += 1;
        }
        let name = String::from_utf8_lossy(&body[..i]).into_owned();
        let mut atts: Vec<(String, String)> = Vec::new();
        let mut empty = false;
        loop {
            let ws_start = i;
            while i < body.len() && is_ws(body[i]) {
                i += 1;
            }
            if i >= body.len() {
                break;
            }
            if body[i] == b'/' {
                if i + 1 != body.len() {
                    return nwf();
                }
                empty = true;
                break;
            }
            if i == ws_start || !is_name_start(body[i]) {
                return nwf();
            }
            let ns = i;
            while i < body.len() && is_name_char(body[i]) {
                i += 1;
            }
            let an = String::from_utf8_lossy(&body[ns..i]).into_owned();
            while i < body.len() && is_ws(body[i]) {
                i += 1;
            }
            if i >= body.len() || body[i] != b'=' {
                return nwf();
            }
            i += 1;
            while i < body.len() && is_ws(body[i]) {
                i += 1;
            }
            if i >= body.len() || (body[i] != b'"' && body[i] != b'\'') {
                return nwf();
            }
            let q = body[i];
            i += 1;
            let vs = i;
            while i < body.len() && body[i] != q {
                i += 1;
            }
            if i >= body.len() {
                return nwf();
            }
            let v = xml_decode(&body[vs..i], true)?;
            i += 1;
            if atts.iter().any(|(n, _)| *n == an) {
                return Err("duplicate attribute".to_string());
            }
            atts.push((an, String::from_utf8_lossy(&v).into_owned()));
        }
        self.stack.push(name.clone());
        if let Err(e) = self.start_element(&name, &atts) {
            // lexerror does not stop expat: for an empty element it goes on to endElement,
            // which reports the unknown element a second time.
            return Err(if empty && e.starts_with('\0') {
                format!("\0{e}")
            } else {
                e
            });
        }
        if empty {
            self.stack.pop();
            self.end_element(&name)?;
        }
        Ok(())
    }
}

/// `eatComment` from `htmllex.c`: returns the index of the closing `>` (or `len`).
fn eat_comment(s: &[u8], p: usize) -> usize {
    let mut depth = 1;
    let mut i = p;
    while depth > 0 && i < s.len() {
        let c = s[i];
        i += 1;
        if c == b'<' {
            depth += 1;
        } else if c == b'>' {
            depth -= 1;
        }
    }
    if depth == 0 { i - 1 } else { s.len() }
}

/// An HTML label error as `htmllex.c` reports it: the message and the index of the
/// `findNext` unit being read (`ctx->currtok`; the unit before it is `prevtok`).
#[derive(Debug, Clone)]
pub struct HtmlError {
    pub msg: String,
    pub unit: usize,
    /// `findNext` units (raw source, plus the `<HTML>`/`</HTML>` wrappers).
    pub units: Vec<String>,
}

impl HtmlError {
    /// The `agerrorf` texts and their `AGPREV` context line (`error_context`), or only the
    /// first for `lexerror`'s unknown elements (twice for an empty one).
    pub fn report(&self) -> (Vec<String>, Option<String>) {
        let line = 1 + self.units[..self.unit]
            .iter()
            .map(|u| u.bytes().filter(|&c| c == b'\n').count())
            .sum::<usize>();
        if let Some(m) = self.msg.strip_prefix('\0') {
            let (m, n) = m.strip_prefix('\0').map_or((m, 1), |m| (m, 2));
            return (vec![format!("{m} on line {line} \n"); n], None);
        }
        let prev = self
            .unit
            .checked_sub(1)
            .map_or("", |i| self.units[i].as_str());
        let cur = self.units.get(self.unit).map_or("", String::as_str);
        (
            vec![format!("{} in line {line} \n", self.msg)],
            Some(format!("... {prev}{cur} ...\n")),
        )
    }
}

/// Lexed tokens, the unit each came from, and the units.
struct Lexed {
    toks: Vec<Tok>,
    tok_unit: Vec<usize>,
    units: Vec<String>,
    /// [`Lexer::warnings`].
    warnings: Vec<(usize, String, bool)>,
}

/// Splits a label into `findNext` units and lexes them.
/// The tokens, and the lexer error that stopped them, if any.
fn lex(src: &str) -> (Lexed, Option<HtmlError>) {
    let s = src.as_bytes();
    let mut lx = Lexer {
        toks: Vec::new(),
        in_cell: true,
        stack: Vec::new(),
        cur: None,
        pending: Pending::None,
        warnings: Vec::new(),
    };
    let mut units: Vec<String> = Vec::new();
    let mut tok_unit: Vec<usize> = Vec::new();
    let mut feed = |lx: &mut Lexer, raw: &[u8], data: &[u8], is_tag: bool| {
        units.push(String::from_utf8_lossy(raw).into_owned());
        let r = lx.unit(data, is_tag);
        let u = units.len() - 1;
        tok_unit.resize(lx.toks.len(), u);
        r.map(|()| u).map_err(|msg| (msg, u))
    };
    let res = (|| {
        feed(&mut lx, b"<HTML>", b"<HTML>", true)?;
        let mut pos = 0;
        while pos < s.len() {
            if s[pos] == b'<' {
                let mut t = pos + 1;
                if s[t..].starts_with(b"!--") {
                    t = eat_comment(s, t + 3);
                } else {
                    while t < s.len() && s[t] != b'>' {
                        t += 1;
                    }
                }
                if t >= s.len() {
                    // findNext warns, and expat sees an unterminated tag.
                    graphviz_pure_core::diag::agwarningf(
                        "Label closed before end of HTML element\n",
                    );
                    let raw = &s[pos..];
                    let u = feed(&mut lx, raw, raw, true)?;
                    return Err(("unclosed token".to_string(), u));
                }
                t += 1;
                feed(&mut lx, &s[pos..t], &s[pos..t], true)?;
                pos = t;
            } else {
                let mut buf = Vec::new();
                let mut t = pos;
                while t < s.len() && s[t] != b'<' {
                    if s[t] == b'&' && s.get(t + 1) != Some(&b'#') {
                        t += 1 + scan_entity(&s[t + 1..], &mut buf);
                    } else {
                        buf.push(s[t]);
                        t += 1;
                    }
                }
                // protect_rsqb only affects expat buffering, not the result
                feed(&mut lx, &s[pos..t], &buf, false)?;
                pos = t;
            }
        }
        let u = feed(&mut lx, b"</HTML>", b"</HTML>", true)?;
        if !lx.stack.is_empty() {
            return Err(("mismatched tag".to_string(), u));
        }
        Ok(())
    })();
    match res {
        Ok(()) => (
            Lexed {
                toks: lx.toks,
                tok_unit,
                units,
                warnings: lx.warnings,
            },
            None,
        ),
        Err((msg, unit)) => (
            Lexed {
                toks: lx.toks,
                tok_unit,
                units: units.clone(),
                warnings: lx.warnings,
            },
            Some(HtmlError { msg, unit, units }),
        ),
    }
}

// ---------------------------------------------------------------------------------------------
// Parser (htmlparse.y)
// ---------------------------------------------------------------------------------------------

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    /// Furthest token read: yacc's lookahead when it reports an error.
    furthest: std::cell::Cell<usize>,
    /// `htmlerror` message of a semantic action (`syntax error` otherwise).
    err_msg: Option<&'static str>,
    /// `fontstack`: bottom entry is `None`.
    fontstack: Vec<Option<HtmlFont>>,
    fitem_list: Vec<HtmlTextItem>,
    fspan_list: Vec<HtmlTextLine>,
}

type PResult<T> = Result<T, ()>;

fn is_font_start(t: &Tok) -> bool {
    matches!(
        t,
        Tok::Font(_)
            | Tok::Italic(_)
            | Tok::Bold(_)
            | Tok::Underline(_)
            | Tok::Overline(_)
            | Tok::Sup(_)
            | Tok::Sub(_)
            | Tok::S(_)
    )
}

/// The wrappers `fonttable` accepts around a table (no `sup`, `sub`, `s`).
fn is_table_wrapper(t: &Tok) -> bool {
    matches!(
        t,
        Tok::Font(_) | Tok::Italic(_) | Tok::Bold(_) | Tok::Underline(_) | Tok::Overline(_)
    )
}

fn font_of(t: &Tok) -> Option<&HtmlFont> {
    match t {
        Tok::Font(f)
        | Tok::Italic(f)
        | Tok::Bold(f)
        | Tok::Underline(f)
        | Tok::Overline(f)
        | Tok::Sup(f)
        | Tok::Sub(f)
        | Tok::S(f) => Some(f),
        _ => None,
    }
}

fn closes(open: &Tok, close: &Tok) -> bool {
    matches!(
        (open, close),
        (Tok::Font(_), Tok::EndFont)
            | (Tok::Italic(_), Tok::NItalic)
            | (Tok::Bold(_), Tok::NBold)
            | (Tok::Underline(_), Tok::NUnderline)
            | (Tok::Overline(_), Tok::NOverline)
            | (Tok::Sup(_), Tok::NSup)
            | (Tok::Sub(_), Tok::NSub)
            | (Tok::S(_), Tok::NS)
    )
}

fn non_space(s: &str) -> bool {
    s.bytes().any(|c| c != b' ')
}

impl Parser {
    fn touch(&self, i: usize) {
        self.furthest.set(self.furthest.get().max(i));
    }

    fn peek(&self) -> Option<&Tok> {
        self.touch(self.pos);
        self.toks.get(self.pos)
    }

    fn peek_at(&self, i: usize) -> Option<&Tok> {
        self.touch(self.pos + i);
        self.toks.get(self.pos + i)
    }

    fn next(&mut self) -> PResult<Tok> {
        self.touch(self.pos);
        let t = self.toks.get(self.pos).cloned().ok_or(())?;
        self.pos += 1;
        Ok(t)
    }

    fn push_font(&mut self, f: &HtmlFont) {
        let mut f = f.clone();
        if let Some(Some(cur)) = self.fontstack.last() {
            if f.color.is_none() && cur.color.is_some() {
                f.color.clone_from(&cur.color);
            }
            if f.size < 0.0 && cur.size >= 0.0 {
                f.size = cur.size;
            }
            if f.name.is_none() && cur.name.is_some() {
                f.name.clone_from(&cur.name);
            }
            if cur.flags != 0 {
                f.flags |= cur.flags;
            }
        }
        self.fontstack.push(Some(f));
    }

    fn top_font(&self) -> Option<HtmlFont> {
        self.fontstack.last().cloned().flatten()
    }

    /// Consumes consecutive `T_string` tokens (`string : string T_string`).
    fn strings(&mut self) -> Option<String> {
        let mut s: Option<String> = None;
        while let Some(Tok::String(t)) = self.peek() {
            s.get_or_insert_with(String::new).push_str(t);
            self.pos += 1;
        }
        s
    }

    /// Is a `fonttable` starting at the current position?
    fn at_fonttable(&self) -> bool {
        let mut i = 0;
        if self.peek().is_some_and(is_table_wrapper) {
            i += 1;
        }
        while let Some(Tok::String(_)) = self.peek_at(i) {
            i += 1;
        }
        matches!(self.peek_at(i), Some(Tok::Table(_)))
    }

    fn append_fitem(&mut self, s: String) {
        self.fitem_list.push(HtmlTextItem {
            text: s,
            font: self.top_font(),
            ..HtmlTextItem::default()
        });
    }

    fn append_fline(&mut self, just: char) {
        let items = if self.fitem_list.is_empty() {
            vec![HtmlTextItem {
                text: String::new(),
                font: self.top_font(),
                ..HtmlTextItem::default()
            }]
        } else {
            std::mem::take(&mut self.fitem_list)
        };
        self.fspan_list.push(HtmlTextLine {
            items,
            just,
            ..HtmlTextLine::default()
        });
    }

    fn mk_text(&mut self) -> HtmlText {
        if !self.fitem_list.is_empty() {
            self.append_fline('\0');
        }
        HtmlText {
            spans: std::mem::take(&mut self.fspan_list),
            ..HtmlText::default()
        }
    }

    /// `text : text textitem | textitem` — at least one item.
    fn text(&mut self) -> PResult<()> {
        let mut n = 0;
        loop {
            match self.peek() {
                Some(Tok::String(_)) => {
                    let s = self.strings().unwrap_or_default();
                    self.append_fitem(s);
                }
                Some(Tok::BrFull(_)) => {
                    let Tok::BrFull(j) = self.next()? else {
                        unreachable!()
                    };
                    self.append_fline(j);
                }
                Some(Tok::Br(_)) => {
                    let Tok::Br(j) = self.next()? else {
                        unreachable!()
                    };
                    if !matches!(self.next()?, Tok::EndBr) {
                        return Err(());
                    }
                    self.append_fline(j);
                }
                Some(t) if is_font_start(t) => {
                    let open = self.next()?;
                    self.push_font(font_of(&open).ok_or(())?);
                    self.text()?;
                    let close = self.next()?;
                    if !closes(&open, &close) {
                        return Err(());
                    }
                    self.fontstack.pop();
                }
                _ => break,
            }
            n += 1;
        }
        if n == 0 { Err(()) } else { Ok(()) }
    }

    fn fonttext(&mut self) -> PResult<HtmlText> {
        self.text()?;
        Ok(self.mk_text())
    }

    /// `table : opt_space T_table rows T_end_table opt_space`.
    fn table(&mut self) -> PResult<HtmlTbl> {
        if let Some(s) = self.strings()
            && non_space(&s)
        {
            self.err_msg = Some("Syntax error: non-space string used before <TABLE>");
            return Err(());
        }
        let Tok::Table(tbl) = self.next()? else {
            return Err(());
        };
        let mut tbl = *tbl;
        tbl.font = self.top_font();
        // rows
        let mut nrows = 0;
        loop {
            match self.peek() {
                Some(Tok::Row) => {
                    self.row(&mut tbl)?;
                    nrows += 1;
                }
                Some(Tok::HrFull | Tok::Hr) if nrows > 0 => {
                    if matches!(self.next()?, Tok::Hr) && !matches!(self.next()?, Tok::EndHr) {
                        return Err(());
                    }
                    if let Some(r) = tbl.row_ruled.last_mut() {
                        *r = true;
                    }
                    if !matches!(self.peek(), Some(Tok::Row)) {
                        return Err(());
                    }
                }
                _ => break,
            }
        }
        if nrows == 0 {
            return Err(());
        }
        if !matches!(self.next()?, Tok::EndTable) {
            return Err(());
        }
        if let Some(s) = self.strings()
            && non_space(&s)
        {
            self.err_msg = Some("Syntax error: non-space string used after </TABLE>");
            return Err(());
        }
        Ok(tbl)
    }

    fn row(&mut self, tbl: &mut HtmlTbl) -> PResult<()> {
        self.next()?; // T_row
        let r = tbl.row_ruled.len();
        tbl.row_ruled.push(tbl.hrule);
        let mut ncells = 0;
        loop {
            match self.peek() {
                Some(Tok::Cell(_)) => {
                    let cell = self.cell(tbl)?;
                    tbl.cells.push(cell);
                    tbl.row_of_cell.push(r);
                    ncells += 1;
                }
                Some(Tok::VrFull | Tok::Vr) if ncells > 0 => {
                    if matches!(self.next()?, Tok::Vr) && !matches!(self.next()?, Tok::EndVr) {
                        return Err(());
                    }
                    if let Some(c) = tbl.cells.last_mut() {
                        c.vruled = true;
                    }
                    if !matches!(self.peek(), Some(Tok::Cell(_))) {
                        return Err(());
                    }
                }
                _ => break,
            }
        }
        if ncells == 0 || !matches!(self.next()?, Tok::EndRow) {
            return Err(());
        }
        Ok(())
    }

    fn cell(&mut self, tbl: &HtmlTbl) -> PResult<HtmlCellData> {
        let Tok::Cell(cell) = self.next()? else {
            return Err(());
        };
        let mut cell = *cell;
        let child = if self.at_fonttable() {
            HtmlContent::Table(Box::new(self.fonttable()?))
        } else if matches!(self.peek(), Some(Tok::ImgFull(_))) {
            let Tok::ImgFull(img) = self.next()? else {
                unreachable!()
            };
            HtmlContent::Image(img)
        } else if matches!(self.peek(), Some(Tok::Img(_))) {
            let Tok::Img(img) = self.next()? else {
                unreachable!()
            };
            if !matches!(self.next()?, Tok::EndImg) {
                return Err(());
            }
            HtmlContent::Image(img)
        } else if matches!(self.peek(), Some(Tok::EndCell)) {
            HtmlContent::Text(self.mk_text())
        } else {
            HtmlContent::Text(self.fonttext()?)
        };
        cell.child = child;
        if tbl.vrule {
            cell.vruled = true;
            cell.hruled = false;
        }
        if !matches!(self.next()?, Tok::EndCell) {
            return Err(());
        }
        Ok(cell)
    }

    fn fonttable(&mut self) -> PResult<HtmlTbl> {
        if self.peek().is_some_and(is_table_wrapper) {
            let open = self.next()?;
            self.push_font(font_of(&open).ok_or(())?);
            let t = self.table()?;
            let close = self.next()?;
            if !closes(&open, &close) {
                return Err(());
            }
            self.fontstack.pop();
            Ok(t)
        } else {
            self.table()
        }
    }

    fn html(&mut self) -> PResult<HtmlLabel> {
        if !matches!(self.next()?, Tok::Html) {
            return Err(());
        }
        let lbl = if self.at_fonttable() {
            HtmlLabel::Table(Box::new(self.fonttable()?))
        } else {
            HtmlLabel::Text(self.fonttext()?)
        };
        if !matches!(self.next()?, Tok::EndHtml) || self.pos != self.toks.len() {
            return Err(());
        }
        Ok(lbl)
    }
}

/// `parseHTML`: returns `None` when C would report an error (rv 3).
pub fn parse_html(text: &str) -> Option<HtmlLabel> {
    parse_html_cs(text, Charset::Utf8)
}

/// [`parse_html`] for a graph of charset `cs`. C hands the label's bytes to expat with the
/// graph's encoding (`charsetToStr`): Latin-1 bytes are decoded to UTF-8, input that is not
/// valid UTF-8 is a fatal "not well-formed" error, and `BIG-5` is an unknown encoding.
pub fn parse_html_cs(text: &str, cs: Charset) -> Option<HtmlLabel> {
    parse_html_report(text, cs).ok()
}

/// [`parse_html_cs`] with the error C reports; `Err(None)` where the port does not
/// reproduce the message (encodings).
pub fn parse_html_report(text: &str, cs: Charset) -> Result<HtmlLabel, Option<HtmlError>> {
    parse_html_warn(text, cs).0
}

/// The attribute warnings `parseHTML` prints: each message (in order) and whether it sets
/// `ctx->warn`, making `parseHTML` return 1.
pub type HtmlWarnings = Vec<(String, bool)>;

/// [`parse_html_report`] plus the `doAttrs` warnings of the tokens C's on-demand lexer
/// reads before it succeeds or fails.
pub fn parse_html_warn(
    text: &str,
    cs: Charset,
) -> (Result<HtmlLabel, Option<HtmlError>>, HtmlWarnings) {
    let decoded;
    let text = match cs {
        Charset::Big5 => return (Err(None), Vec::new()),
        Charset::Utf8 if charset::has_raw(text) => return (Err(None), Vec::new()),
        Charset::Utf8 => text,
        Charset::Latin1 => {
            decoded = charset::encode(text)
                .iter()
                .map(|&b| char::from(b))
                .collect::<String>();
            &decoded
        }
    };
    // C lexes on demand: yacc can reject a token before the lexer reaches a later error.
    let (lexed, lex_err) = lex(text);
    let Lexed {
        toks,
        tok_unit,
        units,
        warnings,
    } = lexed;
    let ntoks = toks.len();
    let mut p = Parser {
        toks,
        pos: 0,
        furthest: std::cell::Cell::new(0),
        err_msg: None,
        fontstack: vec![None],
        fitem_list: Vec::new(),
        fspan_list: Vec::new(),
    };
    let res = p.html();
    // A parse error leaves the tokens after yacc's lookahead unread.
    let read = if res.is_err() && p.furthest.get() < ntoks {
        p.furthest.get()
    } else {
        usize::MAX
    };
    let warnings = warnings
        .into_iter()
        .filter(|&(at, ..)| at <= read)
        .map(|(_, m, set)| (m, set))
        .collect();
    if let Some(e) = lex_err {
        if res.is_err() && p.furthest.get() < ntoks {
            // the parser failed on a token it was given
        } else {
            return (Err(Some(e)), warnings);
        }
    }
    let res = res.map_err(|()| {
        // At end of input yacc's context is the last unit read (`</HTML>`).
        let unit = tok_unit
            .get(p.furthest.get())
            .copied()
            .unwrap_or(units.len() - 1);
        Some(HtmlError {
            msg: p.err_msg.unwrap_or("syntax error").to_string(),
            unit,
            units,
        })
    });
    (res, warnings)
}

// ---------------------------------------------------------------------------------------------
// Sizing (htmltable.c)
// ---------------------------------------------------------------------------------------------

/// `htmlenv_t.finfo` plus the substitution object.
struct Env<'a> {
    name: String,
    color: Option<String>,
    size: f64,
    flags: u8,
    obj: &'a LabelSubstContext<'a>,
    /// `static textfont_t savef` of `size_html_tbl`, shared by every nesting level.
    savef: SaveFont,
    /// Whether this sizing prints its warnings (the first for the object's label).
    report: bool,
    /// `make_html_label`'s non-zero `rv`: a warning was given.
    warn: bool,
}

impl Env<'_> {
    /// A sizing `agwarningf` that makes the size function return 1.
    fn warning(&mut self, msg: &str) {
        if self.report {
            graphviz_pure_core::diag::agwarningf(msg);
        }
        self.warn = true;
    }
}

#[derive(Debug, Clone, Default)]
struct SaveFont {
    name: Option<String>,
    color: Option<String>,
    size: f64,
}

/// `pushFontInfo`.
fn push_font_info(env: &mut Env<'_>, fp: &HtmlFont) {
    // env->finfo.name is always set for labels
    if let Some(n) = &fp.name {
        env.savef.name = Some(std::mem::replace(&mut env.name, n.clone()));
    } else {
        env.savef.name = None;
    }
    if env.color.is_some() {
        if let Some(c) = &fp.color {
            env.savef.color = env.color.replace(c.clone());
        } else {
            env.savef.color = None;
        }
    }
    if env.size >= 0.0 {
        if fp.size >= 0.0 {
            env.savef.size = env.size;
            env.size = fp.size;
        } else {
            env.savef.size = -1.0;
        }
    }
}

/// `popFontInfo`.
fn pop_font_info(env: &mut Env<'_>) {
    if let Some(n) = env.savef.name.clone() {
        env.name = n;
    }
    if let Some(c) = env.savef.color.clone() {
        env.color = Some(c);
    }
    if env.savef.size >= 0.0 {
        env.size = env.savef.size;
    }
}

/// `size_html_txt`.
fn size_html_txt(ftxt: &mut HtmlText, env: &Env<'_>) {
    let mut simple = true;
    let mut prev_fsize = -1.0;
    let mut prev_fname: Option<String> = None;
    for (i, span) in ftxt.spans.iter().enumerate() {
        if span.items.len() > 1 {
            simple = false;
            break;
        }
        let (size, name) = match &span.items[0].font {
            Some(f) => {
                if f.flags != 0 {
                    simple = false;
                    break;
                }
                (
                    if f.size > 0.0 { f.size } else { env.size },
                    f.name.clone().unwrap_or_else(|| env.name.clone()),
                )
            }
            None => (env.size, env.name.clone()),
        };
        if i == 0 {
            prev_fsize = size;
        } else if size != prev_fsize {
            simple = false;
            break;
        }
        match &prev_fname {
            None => prev_fname = Some(name),
            Some(p) if *p != name => {
                simple = false;
                break;
            }
            _ => {}
        }
    }
    ftxt.simple = simple;

    let mut xsize = 0.0_f64;
    let mut ysize = 0.0_f64;
    let mut curbline = 0.0_f64;
    let mut mxysize = 0.0_f64;
    let nspans = ftxt.spans.len();
    for (i, span) in ftxt.spans.iter_mut().enumerate() {
        let mut width = 0.0;
        mxysize = 0.0;
        let mut maxoffset = 0.0_f64;
        let mut maxlayout = 0.0_f64;
        let mut mxfsize = 0.0_f64;
        for item in &mut span.items {
            let s = strdup_and_subst_obj(&item.text, env.obj);
            let (flags, size, name, color) = match &item.font {
                Some(f) => (
                    if f.flags != 0 { f.flags } else { env.flags },
                    if f.size > 0.0 { f.size } else { env.size },
                    f.name.clone().unwrap_or_else(|| env.name.clone()),
                    f.color.clone().or_else(|| env.color.clone()),
                ),
                None => (env.flags, env.size, env.name.clone(), env.color.clone()),
            };
            // estimate_textspan_size
            let (sx, sy) = estimate_text_size_styled(
                &s,
                &name,
                size,
                flags & HTML_BF != 0,
                flags & HTML_IF != 0,
            );
            debug_assert!((sy - size * LINESPACING).abs() < 1e-12);
            item.text = s;
            item.width = sx;
            item.yoffset_layout = size;
            item.yoffset_centerline = 0.1 * size;
            item.font_name = name;
            item.font_size = size;
            item.flags = flags;
            item.color = color;
            width += sx;
            mxfsize = mxfsize.max(size);
            mxysize = mxysize.max(sy);
            maxoffset = maxoffset.max(item.yoffset_centerline);
            maxlayout = maxlayout.max(item.yoffset_layout);
        }
        span.size = width;
        let lsize;
        if simple {
            lsize = mxysize;
            span.lfsize = if i == 0 {
                maxlayout + maxoffset
            } else {
                mxysize
            };
        } else {
            lsize = mxfsize;
            span.lfsize = if i == 0 {
                mxfsize - maxoffset
            } else {
                mxfsize + ysize - curbline - maxoffset
            };
        }
        curbline += span.lfsize;
        xsize = xsize.max(width);
        ysize += lsize;
    }
    ftxt.bx = BoundingBox::new(0.0, 0.0, xsize, if nspans == 1 { mxysize } else { ysize });
}

/// `size_html_cell`.
fn size_html_cell(cp: &mut HtmlCellData, parent: &HtmlTbl, env: &mut Env<'_>) {
    if cp.data.flags & PAD_SET == 0 {
        cp.data.pad = if parent.data.flags & PAD_SET != 0 {
            parent.data.pad
        } else {
            DEFAULT_CELLPADDING
        };
    }
    if cp.data.flags & BORDER_SET == 0 {
        cp.data.border = if parent.cellborder >= 0 {
            parent.cellborder as u8
        } else if parent.data.flags & BORDER_SET != 0 {
            parent.data.border
        } else {
            DEFAULT_BORDER
        };
    }
    let child_sz = match &mut cp.child {
        HtmlContent::Table(t) => {
            size_html_tbl(t, Some(&cp.data), env);
            Point::new(t.data.bx.ur.x, t.data.bx.ur.y)
        }
        HtmlContent::Image(img) => {
            // gvusershape_size cannot load images here: C reports an error and uses 0x0.
            img.bx = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
            Point::new(0.0, 0.0)
        }
        HtmlContent::Text(t) => {
            size_html_txt(t, env);
            Point::new(t.bx.ur.x, t.bx.ur.y)
        }
    };
    let margin = 2.0 * (f64::from(cp.data.pad) + f64::from(cp.data.border));
    let mut sz = Point::new(child_sz.x + margin, child_sz.y + margin);
    if cp.data.flags & FIXED_FLAG != 0 {
        if cp.data.width != 0 && cp.data.height != 0 {
            if (f64::from(cp.data.width) < sz.x || f64::from(cp.data.height) < sz.y)
                && !matches!(cp.child, HtmlContent::Image(_))
            {
                env.warning("cell size too small for content\n");
            }
            sz = Point::new(0.0, 0.0);
        } else {
            env.warning("fixed cell size with unspecified width or height\n");
        }
    }
    cp.data.bx = BoundingBox::new(
        0.0,
        0.0,
        sz.x.max(f64::from(cp.data.width)),
        sz.y.max(f64::from(cp.data.height)),
    );
}

/// `findCol`.
fn find_col(ps: &mut HashSet<(i64, i64)>, row: i64, mut col: i64, cell: &HtmlCellData) -> u16 {
    let end = i64::from(cell.colspan) - 1;
    loop {
        let lastc = col + end;
        let mut c = lastc;
        while c >= col {
            if ps.contains(&(c, row)) {
                break;
            }
            c -= 1;
        }
        if c >= col {
            col = c + 1;
        } else {
            break;
        }
    }
    for j in col..col + i64::from(cell.colspan) {
        for i in row..row + i64::from(cell.rowspan) {
            ps.insert((j, i));
        }
    }
    col as u16
}

/// `processTbl`.
fn process_tbl(tbl: &mut HtmlTbl, env: &mut Env<'_>) {
    let mut ps = HashSet::new();
    let nrows_parsed = tbl.row_ruled.len();
    let mut is_ruled = vec![false; nrows_parsed + 2];
    for (r, &ruled) in tbl.row_ruled.iter().enumerate() {
        if ruled {
            is_ruled[r + 1] = true;
        }
    }
    let mut n_rows = 0usize;
    let mut n_cols = 0usize;
    let mut cells = std::mem::take(&mut tbl.cells);
    let mut c: i64 = 0;
    let mut last_row = usize::MAX;
    for (idx, cell) in cells.iter_mut().enumerate() {
        let r = tbl.row_of_cell[idx];
        if r != last_row {
            c = 0;
            last_row = r;
        }
        size_html_cell(cell, tbl, env);
        let col = find_col(&mut ps, r as i64, c, cell);
        cell.row = r as u16;
        cell.col = col;
        c = i64::from(col) + i64::from(cell.colspan);
        n_cols = n_cols.max(c as usize);
        n_rows = n_rows.max(r + usize::from(cell.rowspan));
        if is_ruled
            .get(r + usize::from(cell.rowspan))
            .copied()
            .unwrap_or(false)
        {
            cell.hruled = true;
        }
    }
    tbl.cells = cells;
    tbl.row_count = n_rows;
    tbl.column_count = n_cols;
}

/// `set_cell_widths`.
fn set_cell_widths(t: &mut HtmlTbl) {
    t.widths = vec![0.0; t.column_count + 1];
    for cell in &t.cells {
        if cell.colspan > 1 {
            continue;
        }
        let c = usize::from(cell.col);
        t.widths[c] = t.widths[c].max(cell.data.bx.ur.x);
    }
    let space = f64::from(t.data.space);
    for cell in &t.cells {
        if cell.colspan == 1 {
            continue;
        }
        let (c0, n) = (usize::from(cell.col), usize::from(cell.colspan));
        let span_width: f64 = t.widths[c0..c0 + n].iter().sum();
        let spacing = (n as f64 - 1.0) * space;
        if span_width + spacing < cell.data.bx.ur.x {
            let widen_by = (cell.data.bx.ur.x - spacing - span_width) / n as f64;
            for w in &mut t.widths[c0..c0 + n] {
                *w += widen_by;
            }
        }
    }
    for i in 0..t.cells.len() {
        let (c0, n) = (usize::from(t.cells[i].col), usize::from(t.cells[i].colspan));
        let min_width: f64 = t.widths[c0..c0 + n].iter().sum();
        let spacing = (n as f64 - 1.0) * space;
        let b = &mut t.cells[i].data.bx;
        b.ur.x = b.ur.x.max(min_width + spacing);
    }
}

/// `set_cell_heights`.
fn set_cell_heights(t: &mut HtmlTbl) {
    t.heights = vec![0.0; t.row_count + 1];
    for cell in &t.cells {
        if cell.rowspan > 1 {
            continue;
        }
        let r = usize::from(cell.row);
        t.heights[r] = t.heights[r].max(cell.data.bx.ur.y);
    }
    let space = f64::from(t.data.space);
    for cell in &t.cells {
        if cell.rowspan == 1 {
            continue;
        }
        let (r0, n) = (usize::from(cell.row), usize::from(cell.rowspan));
        let span_height: f64 = t.heights[r0..r0 + n].iter().sum();
        let spacing = (n as f64 - 1.0) * space;
        if span_height + spacing < cell.data.bx.ur.y {
            let by = (cell.data.bx.ur.y - spacing - span_height) / n as f64;
            for h in &mut t.heights[r0..r0 + n] {
                *h += by;
            }
        }
    }
    for i in 0..t.cells.len() {
        let (r0, n) = (usize::from(t.cells[i].row), usize::from(t.cells[i].rowspan));
        let min_height: f64 = t.heights[r0..r0 + n].iter().sum();
        let spacing = (n as f64 - 1.0) * space;
        let b = &mut t.cells[i].data.bx;
        b.ur.y = b.ur.y.max(min_height + spacing);
    }
}

/// `size_html_tbl`.
fn size_html_tbl(tbl: &mut HtmlTbl, _parent: Option<&HtmlData>, env: &mut Env<'_>) {
    let font = tbl.font.clone();
    if let Some(f) = &font {
        push_font_info(env, f);
    }
    process_tbl(tbl, env);
    if tbl.data.flags & SPACE_SET == 0 {
        tbl.data.space = DEFAULT_CELLSPACING;
    }
    if tbl.data.flags & BORDER_SET == 0 {
        tbl.data.border = DEFAULT_BORDER;
    }
    set_cell_widths(tbl);
    set_cell_heights(tbl);
    let space = f64::from(tbl.data.space);
    let border = f64::from(tbl.data.border);
    let mut wd = (tbl.column_count as f64 + 1.0) * space + 2.0 * border;
    let mut ht = (tbl.row_count as f64 + 1.0) * space + 2.0 * border;
    for w in &tbl.widths[..tbl.column_count] {
        wd += w;
    }
    for h in &tbl.heights[..tbl.row_count] {
        ht += h;
    }
    if tbl.data.flags & FIXED_FLAG != 0 {
        if tbl.data.width != 0 && tbl.data.height != 0 {
            if f64::from(tbl.data.width) < wd || f64::from(tbl.data.height) < ht {
                env.warning("table size too small for content\n");
            }
            wd = 0.0;
            ht = 0.0;
        } else {
            env.warning("fixed table size with unspecified width or height\n");
        }
    }
    tbl.data.bx = BoundingBox::new(
        0.0,
        0.0,
        wd.max(f64::from(tbl.data.width)),
        ht.max(f64::from(tbl.data.height)),
    );
    if font.is_some() {
        pop_font_info(env);
    }
}

// ---------------------------------------------------------------------------------------------
// Positioning
// ---------------------------------------------------------------------------------------------

#[allow(clippy::cast_possible_truncation)]
fn c_round(f: f64) -> i64 {
    if f >= 0.0 {
        (f + 0.5) as i64
    } else {
        (f - 0.5) as i64
    }
}

/// `pos_html_txt`.
fn pos_html_txt(ftxt: &mut HtmlText, c: char) {
    for s in &mut ftxt.spans {
        if s.just == '\0' {
            s.just = c;
        }
    }
}

/// `pos_html_cell`.
fn pos_html_cell(cp: &mut HtmlCellData, mut pos: BoundingBox, sides: u8, parent_pen: Option<&str>) {
    if cp.data.pencolor.is_none()
        && let Some(p) = parent_pen
    {
        cp.data.pencolor = Some(p.to_string());
    }
    if cp.data.flags & FIXED_FLAG != 0 {
        let oldsz = cp.data.bx.ur;
        let delx = pos.ur.x - pos.ll.x - oldsz.x;
        if delx > 0.0 {
            match cp.data.flags & HALIGN_MASK {
                HALIGN_LEFT => pos.ur.x = pos.ll.x + oldsz.x,
                HALIGN_RIGHT => {
                    pos.ur.x += delx;
                    pos.ll.x += delx;
                }
                _ => {
                    pos.ll.x += delx / 2.0;
                    pos.ur.x -= delx / 2.0;
                }
            }
        }
        let dely = pos.ur.y - pos.ll.y - oldsz.y;
        if dely > 0.0 {
            match cp.data.flags & VALIGN_MASK {
                VALIGN_BOTTOM => pos.ur.y = pos.ll.y + oldsz.y,
                VALIGN_TOP => {
                    pos.ur.y += dely;
                    pos.ll.y += dely;
                }
                _ => {
                    pos.ll.y += dely / 2.0;
                    pos.ur.y -= dely / 2.0;
                }
            }
        }
    }
    cp.data.bx = pos;
    cp.data.sides = sides;
    let bp = f64::from(cp.data.border) + f64::from(cp.data.pad);
    let mut cbox = BoundingBox::new(pos.ll.x + bp, pos.ll.y + bp, pos.ur.x - bp, pos.ur.y - bp);
    let flags = cp.data.flags;
    let pen = cp.data.pencolor.clone();
    match &mut cp.child {
        HtmlContent::Table(t) => pos_html_tbl(t, cbox, sides, pen.as_deref()),
        HtmlContent::Image(img) => {
            let oldsz = img.bx.ur;
            let delx = cbox.ur.x - cbox.ll.x - oldsz.x;
            if delx > 0.0 {
                match flags & HALIGN_MASK {
                    HALIGN_LEFT => cbox.ur.x -= delx,
                    HALIGN_RIGHT => cbox.ll.x += delx,
                    _ => {}
                }
            }
            let dely = cbox.ur.y - cbox.ll.y - oldsz.y;
            if dely > 0.0 {
                match flags & VALIGN_MASK {
                    VALIGN_BOTTOM => cbox.ur.y -= dely,
                    VALIGN_TOP => cbox.ll.y += dely,
                    _ => {}
                }
            }
            img.bx = cbox;
        }
        HtmlContent::Text(t) => {
            let oldsz = t.bx.ur;
            let delx = cbox.ur.x - cbox.ll.x - oldsz.x;
            let af = flags & HALIGN_MASK;
            if delx > 0.0 && af != HALIGN_TEXT {
                match af {
                    HALIGN_LEFT => cbox.ur.x -= delx,
                    HALIGN_RIGHT => cbox.ll.x += delx,
                    _ => {
                        cbox.ll.x += delx / 2.0;
                        cbox.ur.x -= delx / 2.0;
                    }
                }
            }
            let dely = cbox.ur.y - cbox.ll.y - oldsz.y;
            if dely > 0.0 {
                match flags & VALIGN_MASK {
                    VALIGN_BOTTOM => cbox.ur.y -= dely,
                    VALIGN_TOP => cbox.ll.y += dely,
                    _ => {
                        cbox.ll.y += dely / 2.0;
                        cbox.ur.y -= dely / 2.0;
                    }
                }
            }
            t.bx = cbox;
            let dflt = match flags & BALIGN_MASK {
                BALIGN_LEFT => 'l',
                BALIGN_RIGHT => 'r',
                _ => 'n',
            };
            pos_html_txt(t, dflt);
        }
    }
}

/// `pos_html_tbl`.
fn pos_html_tbl(tbl: &mut HtmlTbl, mut pos: BoundingBox, sides: u8, parent_pen: Option<&str>) {
    if tbl.data.pencolor.is_none()
        && let Some(p) = parent_pen
    {
        tbl.data.pencolor = Some(p.to_string());
    }
    let oldsz_x = tbl.data.bx.ur.x;
    let mut delx = (pos.ur.x - pos.ll.x - oldsz_x).max(0.0);
    let oldsz = tbl.data.bx.ur.y;
    let mut dely = (pos.ur.y - pos.ll.y - oldsz).max(0.0);
    if tbl.data.flags & FIXED_FLAG != 0 {
        if delx > 0.0 {
            match tbl.data.flags & HALIGN_MASK {
                // C uses `oldsz`, which at this point holds the *height* (sic).
                HALIGN_LEFT => pos.ur.x = pos.ll.x + oldsz,
                HALIGN_RIGHT => {
                    pos.ur.x += delx;
                    pos.ll.x += delx;
                }
                _ => {
                    pos.ll.x += delx / 2.0;
                    pos.ur.x -= delx / 2.0;
                }
            }
            delx = 0.0;
        }
        if dely > 0.0 {
            match tbl.data.flags & VALIGN_MASK {
                VALIGN_BOTTOM => pos.ur.y = pos.ll.y + oldsz,
                VALIGN_TOP => {
                    pos.ll.y += dely;
                    pos.ur.y = pos.ll.y + oldsz;
                }
                _ => {
                    pos.ll.y += dely / 2.0;
                    pos.ur.y -= dely / 2.0;
                }
            }
            dely = 0.0;
        }
    }
    let space = f64::from(tbl.data.space);
    let border = f64::from(tbl.data.border);
    let mut x = pos.ll.x + border + space;
    let ncols = tbl.column_count as f64;
    let extra = delx / ncols;
    let plus = c_round(delx - extra * ncols);
    for i in 0..=tbl.column_count {
        let d = tbl.widths[i] + extra + if (i as i64) < plus { 1.0 } else { 0.0 };
        tbl.widths[i] = x;
        x += d + space;
    }
    let mut y = pos.ur.y - border - space;
    let nrows = tbl.row_count as f64;
    let extra = dely / nrows;
    let plus = c_round(dely - extra * nrows);
    for i in 0..=tbl.row_count {
        let d = tbl.heights[i] + extra + if (i as i64) < plus { 1.0 } else { 0.0 };
        tbl.heights[i] = y;
        y -= d + space;
    }
    let pen = tbl.data.pencolor.clone();
    let (ncol, nrow) = (tbl.column_count, tbl.row_count);
    for k in 0..tbl.cells.len() {
        let cp = &tbl.cells[k];
        let mut mask = 0u8;
        if sides != 0 {
            if cp.col == 0 {
                mask |= SIDE_LEFT;
            }
            if cp.row == 0 {
                mask |= SIDE_TOP;
            }
            if usize::from(cp.col) + usize::from(cp.colspan) == ncol {
                mask |= SIDE_RIGHT;
            }
            if usize::from(cp.row) + usize::from(cp.rowspan) == nrow {
                mask |= SIDE_BOTTOM;
            }
        }
        let (c0, c1) = (usize::from(cp.col), usize::from(cp.col + cp.colspan));
        let (r0, r1) = (usize::from(cp.row), usize::from(cp.row + cp.rowspan));
        let cbox = BoundingBox::new(
            tbl.widths[c0],
            tbl.heights[r1] + space,
            tbl.widths[c1] - space,
            tbl.heights[r0],
        );
        pos_html_cell(&mut tbl.cells[k], cbox, sides & mask, pen.as_deref());
    }
    tbl.data.sides = sides;
    tbl.data.bx = pos;
}

/// `agnameof` of the label's object as `make_label`'s `in label of ...` names it.
fn html_label_owner(obj: &LabelSubstContext<'_>) -> String {
    if obj.is_edge {
        let op = if obj.is_directed { "->" } else { "--" };
        format!("edge {} {op} {}", obj.tail_name, obj.head_name)
    } else if !obj.node_name.is_empty() {
        format!("node {}", obj.node_name)
    } else if obj.graph_name.is_empty() {
        // agnameof of the anonymous root graph (cgraph's first anonymous ID)
        "graph %1".to_string()
    } else {
        format!("graph {}", obj.graph_name)
    }
}

/// `make_html_label`: parses, sizes and positions an HTML label. `text` is the label without
/// its outer `<`...`>` delimiters. `pencolor` is the owning object's `pencolor`/`color`
/// (`getPenColor`), inherited by the outermost table.
///
/// Diagnostics, as C prints them: the lexer's warnings, the error of a rejected label, the
/// sizing warnings, then `make_label`'s `in label of ...` when `make_html_label` returns
/// non-zero. The port's repeat sizings run inside `diag::quietly`, which silences them.
pub fn make_html_label(
    text: &str,
    obj: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
    font_color: Option<&str>,
    pencolor: Option<&str>,
    cs: Charset,
) -> HtmlLabelLayout {
    let owner = html_label_owner(obj);
    let report = graphviz_pure_core::diag::reporting();
    let (res, warnings) = parse_html_warn(text, cs);
    if report {
        for (m, _) in &warnings {
            graphviz_pure_core::diag::agwarningf(m);
        }
    }
    let mut lbl = match res {
        Ok(l) => l,
        Err(e) => {
            if report && let Some(e) = e {
                let (msgs, context) = e.report();
                for msg in &msgs {
                    graphviz_pure_core::diag::agerrorf(msg);
                }
                if let Some(c) = context {
                    graphviz_pure_core::diag::agerr_prev(&c);
                }
                graphviz_pure_core::diag::agerr_prev(&format!("in label of {owner}\n"));
            }
            return HtmlLabelLayout {
                dimen: Point::new(0.0, 0.0),
                label: None,
            };
        }
    };
    let mut env = Env {
        name: font_name.to_string(),
        color: Some(font_color.unwrap_or("black").to_string()),
        size: font_size,
        flags: 0,
        obj,
        savef: SaveFont::default(),
        report,
        warn: warnings.iter().any(|&(_, set)| set),
    };
    let dimen = match &mut lbl {
        HtmlLabel::Table(tbl) => {
            if tbl.data.pencolor.is_none()
                && let Some(p) = pencolor
            {
                tbl.data.pencolor = Some(p.to_string());
            }
            size_html_tbl(tbl, None, &mut env);
            let wd2 = tbl.data.bx.ur.x / 2.0;
            let ht2 = tbl.data.bx.ur.y / 2.0;
            let b = BoundingBox::new(-wd2, -ht2, wd2, ht2);
            pos_html_tbl(
                tbl,
                b,
                SIDE_BOTTOM | SIDE_RIGHT | SIDE_TOP | SIDE_LEFT,
                None,
            );
            Point::new(b.ur.x - b.ll.x, b.ur.y - b.ll.y)
        }
        HtmlLabel::Text(t) => {
            size_html_txt(t, &env);
            let wd2 = t.bx.ur.x / 2.0;
            let ht2 = t.bx.ur.y / 2.0;
            let b = BoundingBox::new(-wd2, -ht2, wd2, ht2);
            t.bx = b;
            Point::new(b.ur.x - b.ll.x, b.ur.y - b.ll.y)
        }
    };
    if report && env.warn {
        graphviz_pure_core::diag::agerr_prev(&format!("in label of {owner}\n"));
    }
    HtmlLabelLayout {
        dimen,
        label: Some(lbl),
    }
}

/// One text span as `emit_htextspans` hands it to `gvrender_textspan` (always left-justified).
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlTextSpan {
    /// Left end of the span (graph coordinates).
    pub x: f64,
    /// Baseline in graph coordinates (y up), before `yoffset_centerline` is applied.
    pub y: f64,
    /// `tl.yoffset_centerline`: the item's value for simple text, else `1`.
    pub yoffset_centerline: f64,
    pub text: String,
    pub font_name: String,
    pub font_size: f64,
    pub flags: u8,
    pub color: String,
    pub width: f64,
}

/// `emit_html_txt` + `emit_htextspans`: positions the spans of a sized and placed text block.
/// `pos` is `env.pos` (the label position), `font_color` the label's `fontcolor`.
pub fn emit_htextspans(txt: &HtmlText, pos: Point, font_color: &str) -> Vec<HtmlTextSpan> {
    let mut out = Vec::new();
    if txt.spans.is_empty() {
        return out;
    }
    let b = txt.bx;
    let halfwidth_x = (b.ur.x - b.ll.x) / 2.0;
    let center_x = pos.x + (b.ur.x + b.ll.x) / 2.0;
    let py = pos.y + (b.ur.y + b.ll.y) / 2.0;
    let left_x = center_x - halfwidth_x;
    let right_x = center_x + halfwidth_x;
    let mut base_y = py + (b.ur.y - b.ll.y) / 2.0;
    for span in &txt.spans {
        let mut x = match span.just {
            'l' => left_x,
            'r' => right_x - span.size,
            _ => center_x - span.size / 2.0,
        };
        base_y -= span.lfsize;
        for item in &span.items {
            out.push(HtmlTextSpan {
                x,
                y: base_y,
                yoffset_centerline: if txt.simple {
                    item.yoffset_centerline
                } else {
                    1.0
                },
                text: item.text.clone(),
                font_name: item.font_name.clone(),
                font_size: item.font_size,
                flags: item.flags,
                color: item
                    .font
                    .as_ref()
                    .and_then(|f| f.color.clone())
                    .unwrap_or_else(|| font_color.to_string()),
                width: item.width,
            });
            x += item.width;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimen(s: &str) -> Point {
        let obj = LabelSubstContext::for_node("a");
        make_html_label(s, &obj, "Times-Roman", 14.0, None, None, Charset::Utf8).dimen
    }

    // Expected values from `dot -Txdot` (Graphviz 16.1.0, LUT estimator).
    #[test]
    fn errors_drop_the_label() {
        assert_eq!(dimen("<b>x</B>"), Point::new(0.0, 0.0));
        assert_eq!(dimen("<B></B>"), Point::new(0.0, 0.0));
        assert_eq!(dimen("a &foo; b"), Point::new(0.0, 0.0));
    }

    #[test]
    fn charset_decides_how_label_bytes_are_read() {
        let raw = graphviz_pure_core::charset::decode(b"<B>caf\xe9</B>".to_vec());
        // expat: invalid UTF-8 is not well-formed; BIG-5 is an unknown encoding.
        assert!(parse_html_cs(&raw, Charset::Utf8).is_none());
        assert!(parse_html_cs("<B>x</B>", Charset::Big5).is_none());
        let Some(HtmlLabel::Text(t)) = parse_html_cs(&raw, Charset::Latin1) else {
            panic!("latin1 label");
        };
        assert_eq!(t.spans[0].items[0].text, "caf\u{e9}");
    }

    #[test]
    fn text_keeps_spaces_and_mixes_fonts() {
        let d = dimen(" abc ");
        assert!((d.x - 26.43).abs() < 0.01, "{d:?}");
        let d = dimen("right <B>bold</B>");
        assert!((d.x - (29.94 + 26.46)).abs() < 0.02, "{d:?}");
        let d = dimen("a<BR/>b");
        assert!((d.y - 33.6).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn simple_table() {
        // node height 0.51111in = 36.8pt => dimen.y = 28.8; text " a " 13.21 wide
        let d = dimen("<TABLE><TR><TD> a </TD></TR></TABLE>");
        assert!((d.y - 28.8).abs() < 1e-9, "{d:?}");
        assert!((d.x - (13.21 + 12.0)).abs() < 0.01, "{d:?}");
    }

    #[test]
    fn right_bold_spans_match_c_svg() {
        // C: <text text-anchor="start" x="8" y="-14.8" ...>right </text>
        //    <text text-anchor="start" x="37.94" y="-14.8" ... font-weight="bold">bold</text>
        // for a box node at (36.2, 18) with width 72.4.
        let obj = LabelSubstContext::for_node("a");
        let lay = make_html_label(
            "right <B>bold</B>",
            &obj,
            "Times-Roman",
            14.0,
            None,
            None,
            Charset::Utf8,
        );
        let Some(HtmlLabel::Text(t)) = lay.label else {
            panic!()
        };
        let spans = emit_htextspans(&t, Point::new(36.2, 18.0), "black");
        assert_eq!(spans.len(), 2);
        assert!((spans[0].x - 8.0).abs() < 0.01);
        assert!((spans[0].y + spans[0].yoffset_centerline - 14.8).abs() < 0.01);
        assert!((spans[1].x - 37.94).abs() < 0.01);
        assert_eq!(spans[1].flags, HTML_BF);
        assert_eq!(spans[0].text, "right ");
    }

    #[test]
    fn right_bold_items() {
        let Some(HtmlLabel::Text(t)) = parse_html("right <B>bold</B>") else {
            panic!()
        };
        assert_eq!(t.spans.len(), 1);
        assert_eq!(t.spans[0].items[0].text, "right ");
        assert_eq!(t.spans[0].items[1].text, "bold");
        assert_eq!(t.spans[0].items[1].font.as_ref().unwrap().flags, HTML_BF);
    }
}
