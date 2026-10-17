//! Built-in font metrics lookup tables and text/label sizing ported from Graphviz 16.1.0
//! (`lib/common/textspan_lut.c`, `lib/common/textspan.c`, `lib/common/ps_font_equiv.h`,
//! and `lib/common/labels.c`).

use serde::{Deserialize, Serialize};

/// Default font family name in Graphviz (`Times-Roman`).
pub const DEFAULT_FONT_NAME: &str = "Times-Roman";

/// Default font size in points (`14.0`).
pub const DEFAULT_FONT_SIZE: f64 = 14.0;

/// Default head/tail label font size in points (`11.0`).
pub const DEFAULT_LABEL_FONT_SIZE: f64 = 11.0;

/// Minimum allowed font size in points (`1.0`).
pub const MIN_FONT_SIZE: f64 = 1.0;

/// Line spacing multiplier (`1.20`) from `lib/common/const.h`.
pub const LINESPACING: f64 = 1.20;

/// Horizontal justification of a single line in a Graphviz label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextJustify {
    /// Centered line (`\n`, default).
    #[default]
    Center,
    /// Left-justified line (`\l`).
    Left,
    /// Right-justified line (`\r`).
    Right,
}

impl TextJustify {
    /// Returns the Graphviz justification character (`'n'`, `'l'`, `'r'`).
    #[inline]
    pub const fn as_char(self) -> char {
        match self {
            Self::Center => 'n',
            Self::Left => 'l',
            Self::Right => 'r',
        }
    }

    /// Parses a Graphviz justification character (`'n'`, `'l'`, `'r'`).
    #[inline]
    pub const fn from_char(c: char) -> Self {
        match c {
            'l' => Self::Left,
            'r' => Self::Right,
            _ => Self::Center,
        }
    }
}

/// Measured layout of a single line within a multiline text label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextLineLayout {
    /// Unescaped text content of the line.
    pub text: String,
    /// Width of the line in points.
    pub width: f64,
    /// Height of the line in points (`font_size * LINESPACING`).
    pub height: f64,
    /// Horizontal justification of this line (`\n`, `\l`, `\r`).
    pub justify: TextJustify,
}

/// Measured layout of a multiline Graphviz label in points.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TextLabelLayout {
    /// Measured lines in top-to-bottom order.
    pub lines: Vec<TextLineLayout>,
    /// Individual line widths in points (mirrors `lines.iter().map(|l| l.width)`).
    pub line_widths: Vec<f64>,
    /// Total bounding width in points (maximum line width).
    pub width: f64,
    /// Total bounding height in points (sum of line heights).
    pub height: f64,
    /// Font family name used for measurement.
    pub font_name: String,
    /// Font size in points used for measurement.
    pub font_size: f64,
}

/// Context for Graphviz object-based escape sequence substitution (`\N`, `\G`, `\T`, `\H`, `\E`, `\L`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabelSubstContext<'a> {
    pub node_name: &'a str,
    pub graph_name: &'a str,
    pub tail_name: &'a str,
    pub head_name: &'a str,
    pub is_edge: bool,
    pub is_directed: bool,
    pub tail_port: Option<&'a str>,
    pub head_port: Option<&'a str>,
    pub label_text: &'a str,
}

impl<'a> LabelSubstContext<'a> {
    /// Creates a substitution context for a node with the given name.
    #[inline]
    pub const fn for_node(node_name: &'a str) -> Self {
        Self {
            node_name,
            graph_name: "",
            tail_name: "",
            head_name: "",
            is_edge: false,
            is_directed: true,
            tail_port: None,
            head_port: None,
            label_text: "",
        }
    }
}

/// Font family character width lookup table (`struct FontFamilyMetrics` in
/// `lib/common/textspan_lut.c`).
pub(crate) struct FontFamilyMetrics {
    /// Case-insensitive list of metrically-equivalent family names.
    pub(crate) font_names: &'static [&'static str],
    /// Widths are stored in `(units_per_em * 1)` pt.
    pub(crate) units_per_em: f64,
    pub(crate) widths_regular: [i16; 128],
    pub(crate) widths_bold: [i16; 128],
    pub(crate) widths_italic: [i16; 128],
    pub(crate) widths_bold_italic: [i16; 128],
}

/// PostScript font alias entry (`PostscriptAlias` in `lib/common/textspan.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostscriptFontAlias {
    pub name: &'static str,
    pub family: &'static str,
    pub weight: Option<&'static str>,
    pub stretch: Option<&'static str>,
    pub style: Option<&'static str>,
    pub xfig_code: i32,
    pub svg_font_family: &'static str,
    pub svg_font_weight: Option<&'static str>,
    pub svg_font_style: Option<&'static str>,
}

#[rustfmt::skip]
#[path = "font_tables.rs"]
mod font_tables;

/// Standard PostScript font equivalence table (`postscript_alias[]` from `lib/common/textspan.c`,
/// generated from `ps_font_equiv.h` by `scripts/gen-font-tables.py`).
pub const POSTSCRIPT_FONT_ALIASES: &[PostscriptFontAlias] = &font_tables::POSTSCRIPT_ALIASES;

/// C `strcasecmp` ordering (ASCII case folding, byte comparison).
fn strcasecmp(a: &str, b: &str) -> std::cmp::Ordering {
    let la = a.bytes().map(|c| c.to_ascii_lowercase());
    let lb = b.bytes().map(|c| c.to_ascii_lowercase());
    la.cmp(lb)
}

/// `translate_postscript_fontname` from `lib/common/textspan.c`: `bsearch` of the alias table
/// with `strcasecmp`. The name is used verbatim (no trimming), as in C.
pub fn translate_postscript_fontname(font_name: &str) -> Option<&'static PostscriptFontAlias> {
    POSTSCRIPT_FONT_ALIASES
        .binary_search_by(|alias| strcasecmp(alias.name, font_name))
        .ok()
        .map(|i| &POSTSCRIPT_FONT_ALIASES[i])
}

/// `font_name_equal_permissive` from `lib/common/textspan_lut.c`: compares two font names
/// ignoring case and every character that is not an ASCII letter.
fn font_name_equal_permissive(a: &str, b: &str) -> bool {
    let a_iter = a
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .map(|c| c.to_ascii_lowercase());
    let b_iter = b
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .map(|c| c.to_ascii_lowercase());
    a_iter.eq(b_iter)
}

/// `font_in_list_permissive` from `lib/common/textspan_lut.c`.
fn font_in_list_permissive(value: &str, list: &[&str]) -> bool {
    list.iter()
        .any(|entry| font_name_equal_permissive(value, entry))
}

/// `get_metrics_for_font_family` from `lib/common/textspan_lut.c`: the first family whose
/// name list matches, else the `Times` metrics (C emits a once-only warning).
fn get_metrics_for_font_family(font_name: &str) -> &'static FontFamilyMetrics {
    font_tables::FONT_METRICS
        .iter()
        .find(|m| font_in_list_permissive(font_name, m.font_names))
        .unwrap_or(&font_tables::FONT_METRICS[0])
}

/// `get_metrics_for_font_variant` from `lib/common/textspan_lut.c`.
fn get_metrics_for_font_variant(
    family_metrics: &'static FontFamilyMetrics,
    bold: bool,
    italic: bool,
) -> &'static [i16; 128] {
    if bold && italic {
        &family_metrics.widths_bold_italic
    } else if bold {
        &family_metrics.widths_bold
    } else if italic {
        &family_metrics.widths_italic
    } else {
        &family_metrics.widths_regular
    }
}

/// `estimate_character_width_canonical` from `lib/common/textspan_lut.c`: width of one byte in
/// `(units_per_em * 1)` points. Bytes `>= 128` (every byte of a UTF-8 multi-byte sequence) use
/// the width of `' '`; unknown ASCII widths (`-1`) count as `0`.
#[inline]
fn estimate_character_width_canonical(variant_metrics: &[i16; 128], byte: u8) -> u16 {
    let character = if byte >= 128 { b' ' } else { byte };
    let width = variant_metrics[usize::from(character)];
    if width == -1 { 0 } else { width as u16 }
}

/// `estimate_text_width_1pt` from `lib/common/textspan_lut.c`: width of `text` at 1pt.
///
/// Matches C exactly: the font name is looked up directly in the LUT families (PostScript
/// aliases such as `Times-Bold` are *not* consulted, so they measure as regular Times), and
/// the text is measured byte by byte, so each byte of a UTF-8 multi-byte character counts as
/// one space (see `docs/c-bugs-ledger.md`, GV-BUG-011/013).
pub fn estimate_text_width_1pt(font_name: &str, text: &str, bold: bool, italic: bool) -> f64 {
    let family_metrics = get_metrics_for_font_family(font_name);
    let variant_metrics = get_metrics_for_font_variant(family_metrics, bold, italic);
    let text_width_canonical: u32 = text
        .bytes()
        .map(|b| u32::from(estimate_character_width_canonical(variant_metrics, b)))
        .sum();
    f64::from(text_width_canonical) / family_metrics.units_per_em
}

/// `htmlEntity` from `lib/common/utils.c`. `s` starts at the byte after `&`. On success returns
/// `(value, consumed)`. Like C, a numeric reference terminated by `;` is consumed even when
/// its value is `0` (so `&#0;` collapses to `&`); unknown names consume nothing.
fn html_entity(s: &[u8]) -> (u32, usize) {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    if at(0) == b'#' {
        let mut n: u32 = 0;
        let mut byte;
        let mut i;
        if at(1) == b'x' || at(1) == b'X' {
            i = 2;
            loop {
                byte = at(i);
                if i >= 8 {
                    break;
                }
                let d = match byte {
                    b'A'..=b'F' => byte - b'A' + 10,
                    b'a'..=b'f' => byte - b'a' + 10,
                    b'0'..=b'9' => byte - b'0',
                    _ => break,
                };
                n = n.wrapping_mul(16).wrapping_add(u32::from(d));
                i += 1;
            }
        } else {
            i = 1;
            loop {
                byte = at(i);
                if i >= 8 {
                    break;
                }
                if byte.is_ascii_digit() {
                    n = n.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'));
                } else {
                    break;
                }
                i += 1;
            }
        }
        // C checks the last byte it examined; when the loop ran to completion `byte` is
        // the final digit, which can never be ';'.
        if i < 8 && byte == b';' {
            (n, i + 1)
        } else {
            (0, 0)
        }
    } else {
        for i in 0..font_tables::ENTITY_NAME_LENGTH_MAX {
            let byte = at(i);
            if byte == 0 {
                break;
            }
            if byte == b';' {
                let key = &s[..i];
                if let Ok(idx) = font_tables::HTML_ENTITIES
                    .binary_search_by(|(name, _)| name.as_bytes().cmp(key))
                {
                    return (font_tables::HTML_ENTITIES[idx].1, i + 1);
                }
                break;
            }
        }
        (0, 0)
    }
}

/// Appends the C encoding of entity value `v` (`htmlEntityUTF8`/`latin1ToUTF8`: 1, 2 or 3
/// bytes, never 4).
fn push_entity_bytes(out: &mut Vec<u8>, v: u32) {
    if v < 0x7F {
        out.push(v as u8);
    } else if v < 0x07FF {
        out.push(((v >> 6) | 0xC0) as u8);
        out.push(((v & 0x3F) | 0x80) as u8);
    } else {
        out.push(((v >> 12) | 0xE0) as u8);
        out.push((((v >> 6) & 0x3F) | 0x80) as u8);
        out.push(((v & 0x3F) | 0x80) as u8);
    }
}

/// `htmlEntityUTF8` from `lib/common/utils.c`: replaces HTML entities (`&amp;`, `&#123;`,
/// `&#x7B;`) by their UTF-8 encoding. Input is already valid UTF-8 in Rust, so C's
/// invalid-UTF-8-as-Latin-1 recovery never triggers.
pub fn html_entity_utf8(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let s = text.as_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        i += 1;
        if c == b'&' {
            let (v, used) = html_entity(&s[i..]);
            i += used;
            if v != 0 {
                push_entity_bytes(&mut out, v);
                continue;
            }
        }
        out.push(c);
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// `latin1ToUTF8` from `lib/common/utils.c` (used when `charset=latin1`): translates HTML
/// entities and re-encodes every byte `>= 0x7F` as if it were a Latin-1 code point.
pub fn latin1_to_utf8(text: &str) -> String {
    let s = text.as_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let mut v = u32::from(s[i]);
        i += 1;
        if v == u32::from(b'&') {
            let (ev, used) = html_entity(&s[i..]);
            i += used;
            v = if ev == 0 { u32::from(b'&') } else { ev };
        }
        push_entity_bytes(&mut out, v);
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Decodes HTML character entity references, matching `htmlEntityUTF8` in
/// `lib/common/utils.c` (kept under its historical name for existing callers).
pub fn decode_html_entities(text: &str) -> String {
    html_entity_utf8(text)
}

/// Estimates the `(width, height)` in points of a single-line text span for a given
/// `font_name` and `font_size`, matching `estimate_textspan_size` in `lib/common/textspan.c`.
pub fn estimate_text_size(text: &str, font_name: &str, font_size: f64) -> (f64, f64) {
    estimate_text_size_styled(text, font_name, font_size, false, false)
}

/// `estimate_textspan_size` from `lib/common/textspan.c` with explicit `bold`/`italic` flags
/// (`HTML_BF`/`HTML_IF`): `(fontsize * width_1pt, fontsize * LINESPACING)`.
pub fn estimate_text_size_styled(
    text: &str,
    font_name: &str,
    font_size: f64,
    bold: bool,
    italic: bool,
) -> (f64, f64) {
    let width = font_size * estimate_text_width_1pt(font_name, text, bold, italic);
    let height = font_size * LINESPACING;
    (width, height)
}

/// Kind of graph object a label belongs to, deciding which `strdup_and_subst_obj0`
/// substitutions are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubstObjKind {
    Graph,
    Node,
    Edge,
}

impl LabelSubstContext<'_> {
    fn obj_kind(&self) -> SubstObjKind {
        if self.is_edge {
            SubstObjKind::Edge
        } else if !self.node_name.is_empty() {
            SubstObjKind::Node
        } else {
            SubstObjKind::Graph
        }
    }
}

/// `strdup_and_subst_obj0` from `lib/common/labels.c`.
///
/// Replaces `\G`, `\N`, `\E`, `\H`, `\T`, `\L` with the names of the associated objects.
/// Escapes that do not apply to the object kind stay literal (e.g. `\T` in a node label stays
/// `\T`, which `make_simple_label` later reduces to `T`), and `\L` stays literal when
/// `label_text` is empty (no label exists yet). When `esc_backslash` is set `\\` becomes `\`;
/// every other `\c` dyad is passed through unchanged.
fn strdup_and_subst_obj0(s: &str, ctx: &LabelSubstContext<'_>, esc_backslash: bool) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let kind = ctx.obj_kind();
    let g_str = ctx.graph_name;
    let n_str = if kind == SubstObjKind::Node {
        ctx.node_name
    } else {
        "\\N"
    };
    let (t_str, h_str) = if kind == SubstObjKind::Edge {
        (ctx.tail_name, ctx.head_name)
    } else {
        ("\\T", "\\H")
    };
    let l_str = if ctx.label_text.is_empty() {
        "\\L"
    } else {
        ctx.label_text
    };
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(next) = chars.clone().next() else {
            out.push('\\');
            continue;
        };
        chars.next();
        match next {
            'G' => out.push_str(g_str),
            'N' => out.push_str(n_str),
            'E' => {
                if kind == SubstObjKind::Edge {
                    out.push_str(t_str);
                    if let Some(tp) = ctx.tail_port.filter(|p| !p.is_empty()) {
                        out.push(':');
                        out.push_str(tp);
                    }
                    out.push_str(if ctx.is_directed { "->" } else { "--" });
                    out.push_str(h_str);
                    if let Some(hp) = ctx.head_port.filter(|p| !p.is_empty()) {
                        out.push(':');
                        out.push_str(hp);
                    }
                }
            }
            'T' => out.push_str(t_str),
            'H' => out.push_str(h_str),
            'L' => out.push_str(l_str),
            '\\' if esc_backslash => out.push('\\'),
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

/// Substitutes Graphviz object escapes (`\N`, `\G`, `\T`, `\H`, `\E`, `\L`) while preserving
/// line-formatting escapes (`\n`, `\l`, `\r`, `\\`) for `make_simple_label`; this is
/// `strdup_and_subst_obj0(str, obj, 0)` from `lib/common/labels.c`.
pub fn substitute_object_escapes(label: &str, ctx: &LabelSubstContext<'_>) -> String {
    strdup_and_subst_obj0(label, ctx, false)
}

/// `strdup_and_subst_obj` from `lib/common/labels.c`: object escapes plus `\\` -> `\`
/// (used for `URL`, `tooltip`, `id`, ... attributes).
pub fn strdup_and_subst_obj(s: &str, ctx: &LabelSubstContext<'_>) -> String {
    strdup_and_subst_obj0(s, ctx, true)
}

/// `storeline` from `lib/common/labels.c`: measures one line and accumulates it into the
/// label. Empty lines get height `(int)(fontsize * LINESPACING)` (C truncates; non-empty
/// lines do not), see GV-BUG-012 in `docs/c-bugs-ledger.md`.
fn storeline(layout: &mut TextLabelLayout, line: String, terminator: TextJustify) {
    let (width, height) = if line.is_empty() {
        (0.0, (layout.font_size * LINESPACING).trunc())
    } else {
        estimate_text_size(&line, &layout.font_name, layout.font_size)
    };
    layout.width = layout.width.max(width);
    layout.height += height;
    layout.line_widths.push(width);
    layout.lines.push(TextLineLayout {
        text: line,
        width,
        height,
        justify: terminator,
    });
}

/// `make_simple_label` from `lib/common/labels.c`: splits `text` (already object- and
/// entity-substituted) into lines on `\n`, `\l`, `\r` and literal newlines, reducing every
/// other `\c` to `c`. `width`/`height` are C's `lp->dimen`.
pub fn make_simple_label(text: &str, font_name: &str, font_size: f64) -> TextLabelLayout {
    let mut layout = TextLabelLayout {
        font_name: font_name.to_string(),
        font_size,
        ..TextLabelLayout::default()
    };
    if text.is_empty() {
        return layout;
    }
    let mut line = String::new();
    // C appends the NUL after a trailing backslash, which makes the buffer non-empty without
    // adding visible text.
    let mut pending_nul = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.clone().next() {
                Some(j @ ('n' | 'l' | 'r')) => {
                    chars.next();
                    storeline(
                        &mut layout,
                        std::mem::take(&mut line),
                        TextJustify::from_char(j),
                    );
                    pending_nul = false;
                }
                Some(other) => {
                    chars.next();
                    if !pending_nul {
                        line.push(other);
                    }
                }
                None => pending_nul = true,
            }
        } else if c == '\n' {
            storeline(&mut layout, std::mem::take(&mut line), TextJustify::Center);
            pending_nul = false;
        } else if !pending_nul {
            line.push(c);
        }
    }
    if !line.is_empty() || pending_nul {
        storeline(&mut layout, line, TextJustify::Center);
    }
    layout
}

/// Plain-text branch of `make_label` from `lib/common/labels.c`: object escape substitution,
/// charset conversion (`latin1ToUTF8` for `charset=latin1`, else `htmlEntityUTF8`) and
/// `make_simple_label`.
pub fn make_text_label(
    label: &str,
    ctx: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
    charset_latin1: bool,
) -> TextLabelLayout {
    // While a label is being made, the object has no label yet, so `\L` stays literal.
    let ctx = LabelSubstContext {
        label_text: "",
        ..ctx.clone()
    };
    let substituted = strdup_and_subst_obj0(label, &ctx, false);
    let converted = if charset_latin1 {
        latin1_to_utf8(&substituted)
    } else {
        html_entity_utf8(&substituted)
    };
    make_simple_label(&converted, font_name, font_size)
}

/// Returns `true` if a graph's `charset` attribute selects Latin-1 (`findCharset` in
/// `lib/common/input.c`: `latin-1`, `latin1`, `l1`, `ISO-8859-1`, `ISO_8859-1`, `ISO8859-1`,
/// `ISO-IR-100`, case-insensitively).
pub fn charset_is_latin1(charset: Option<&str>) -> bool {
    charset.is_some_and(|c| {
        [
            "latin-1",
            "latin1",
            "l1",
            "ISO-8859-1",
            "ISO_8859-1",
            "ISO8859-1",
            "ISO-IR-100",
        ]
        .iter()
        .any(|n| n.eq_ignore_ascii_case(c))
    })
}

/// Measures a multiline Graphviz label for a node named `node_name`.
pub fn measure_multiline_label(
    label: &str,
    node_name: &str,
    font_name: &str,
    font_size: f64,
) -> TextLabelLayout {
    let ctx = LabelSubstContext::for_node(node_name);
    measure_multiline_label_with_context(label, &ctx, font_name, font_size)
}

/// Measures a multiline plain-text Graphviz label (UTF-8 charset); see [`make_text_label`].
pub fn measure_multiline_label_with_context(
    label: &str,
    ctx: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
) -> TextLabelLayout {
    make_text_label(label, ctx, font_name, font_size, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_character_widths_times_helvetica_courier() {
        // Times-Roman: 'A' is index 65 -> width 1479 / 2048
        let w_times_a = estimate_text_width_1pt("Times-Roman", "A", false, false);
        assert!((w_times_a - 1479.0 / 2048.0).abs() < 1e-12);

        // C looks the name up directly in the LUT families: "Times-Bold" and "Times-Italic"
        // are not family names, so they fall back to regular Times (verified against
        // `dot -Txdot` 16.1.0: `AAAA` in Times-Bold 14pt is 40.44pt wide, as in Times-Roman).
        let w_times_bold_a = estimate_text_width_1pt("Times-Bold", "A", false, false);
        assert!((w_times_bold_a - w_times_a).abs() < 1e-12);
        let w_times_italic_a = estimate_text_width_1pt("Times", "A", false, true);
        assert!((w_times_italic_a - 1251.0 / 2048.0).abs() < 1e-12);

        // Helvetica: 'A' is index 65 -> width 1366 / 2048
        let w_helv_a = estimate_text_width_1pt("Helvetica", "A", false, false);
        assert!((w_helv_a - 1366.0 / 2048.0).abs() < 1e-12);

        // "Helvetica-Bold" is not a family name either: Times fallback.
        let w_helv_bold_a = estimate_text_width_1pt("Helvetica-Bold", "A", false, false);
        assert!((w_helv_bold_a - w_times_a).abs() < 1e-12);
        let w_helv_bold_a = estimate_text_width_1pt("Helvetica", "A", true, false);
        assert!((w_helv_bold_a - 1479.0 / 2048.0).abs() < 1e-12);

        // Courier and Courier-Bold: monospace 1229 / 2048 per printable ASCII char
        let w_cour = estimate_text_width_1pt("Courier", "Hello", false, false);
        assert!((w_cour - (5.0 * 1229.0 / 2048.0)).abs() < 1e-12);
        let w_cour_bold = estimate_text_width_1pt("Courier New", "Hello", true, false);
        assert!((w_cour_bold - (5.0 * 1229.0 / 2048.0)).abs() < 1e-12);

        // Symbol falls back to Times metrics
        let w_symbol_a = estimate_text_width_1pt("Symbol", "A", false, false);
        assert!((w_symbol_a - w_times_a).abs() < 1e-12);
    }

    #[test]
    fn test_estimate_text_size_points() {
        let (w, h) = estimate_text_size("abc", "Courier", 14.0);
        let expected_w = 14.0 * (3.0 * 1229.0 / 2048.0);
        let expected_h = 14.0 * 1.20;
        assert!((w - expected_w).abs() < 1e-9);
        assert!((h - expected_h).abs() < 1e-9);
    }

    #[test]
    fn test_measure_multiline_label_escapes_and_justifications() {
        let layout =
            measure_multiline_label("\\N\\nline two\\lright\\r", "node_1", "Courier", 10.0);
        assert_eq!(layout.lines.len(), 3);
        assert_eq!(layout.line_widths.len(), 3);

        assert_eq!(layout.lines[0].text, "node_1");
        assert_eq!(layout.lines[0].justify, TextJustify::Center);

        assert_eq!(layout.lines[1].text, "line two");
        assert_eq!(layout.lines[1].justify, TextJustify::Left);

        assert_eq!(layout.lines[2].text, "right");
        assert_eq!(layout.lines[2].justify, TextJustify::Right);

        // Max width is "line two" (8 chars)
        let char_w = 10.0 * (1229.0 / 2048.0);
        assert!((layout.line_widths[0] - 6.0 * char_w).abs() < 1e-9);
        assert!((layout.line_widths[1] - 8.0 * char_w).abs() < 1e-9);
        assert!((layout.line_widths[2] - 5.0 * char_w).abs() < 1e-9);
        assert!((layout.width - 8.0 * char_w).abs() < 1e-9);
        assert!((layout.height - 3.0 * 10.0 * LINESPACING).abs() < 1e-9);
    }

    #[test]
    fn test_measure_multiline_label_context_escapes() {
        let ctx = LabelSubstContext {
            node_name: "n1",
            graph_name: "G_main",
            tail_name: "src",
            head_name: "dst",
            is_edge: true,
            is_directed: true,
            tail_port: None,
            head_port: None,
            label_text: "",
        };
        let layout = measure_multiline_label_with_context(
            "\\G: \\T -> \\H (\\E)",
            &ctx,
            "Times-Roman",
            14.0,
        );
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].text, "G_main: src -> dst (src->dst)");
    }

    #[test]
    fn utf8_is_measured_per_byte_like_c() {
        // `dot -Txdot`: "é€x" in Times-Roman 14pt is 24.5pt (5 non-ASCII bytes as spaces + x).
        let (w, _) = estimate_text_size("é€x", "Times-Roman", 14.0);
        assert!((w - 24.5).abs() < 1e-9);
    }

    #[test]
    fn empty_line_height_is_truncated_like_c() {
        let l = make_simple_label("a\\n\\nb", "Times-Roman", 14.0);
        assert_eq!(l.lines.len(), 3);
        assert!((l.lines[1].height - 16.0).abs() < 1e-12);
        assert!((l.height - (16.8 + 16.0 + 16.8)).abs() < 1e-9);
    }

    #[test]
    fn html_entities_follow_c_rules() {
        assert_eq!(html_entity_utf8("a&amp;b"), "a&b");
        assert_eq!(html_entity_utf8("&#65;&#x42;"), "AB");
        assert_eq!(html_entity_utf8("&bogus;"), "&bogus;");
        assert_eq!(html_entity_utf8("&#0;x"), "&x");
        assert_eq!(html_entity_utf8("&alpha;"), "\u{3b1}");
    }

    #[test]
    fn subst_leaves_inapplicable_escapes() {
        let ctx = LabelSubstContext::for_node("n");
        let l = make_text_label("\\N \\T \\L", &ctx, "Times-Roman", 14.0, false);
        assert_eq!(l.lines[0].text, "n T L");
    }
}
