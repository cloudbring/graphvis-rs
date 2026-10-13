//! Color translation tables, hex/HSV parsing, and SVG color formatting ported from
//! Graphviz 16.1.0 (`lib/common/colxlate.c`, `lib/common/colortbl.h`, `lib/common/color.h`).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Resolved 8-bit RGBA color (`r`, `g`, `b`, `a`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for RgbaColor {
    #[inline]
    fn default() -> Self {
        Self::BLACK
    }
}

impl RgbaColor {
    /// Opaque black (`#000000`).
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Opaque white (`#ffffff`).
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    /// Transparent color (`invis` / `none` / `transparent` in Graphviz `colortbl.h`).
    pub const TRANSPARENT: Self = Self::rgba(255, 255, 254, 0);

    /// Creates a new `RgbaColor` from 8-bit RGBA components.
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Creates an opaque `RgbaColor` (`a = 255`) from 8-bit RGB components.
    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates an `RgbaColor` from 8-bit RGBA components.
    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Returns `true` if the color is completely transparent (`a == 0`).
    #[inline]
    pub const fn is_transparent(&self) -> bool {
        self.a == 0
    }

    /// Returns `true` if the color is completely opaque (`a == 255`).
    #[inline]
    pub const fn is_opaque(&self) -> bool {
        self.a == 255
    }

    /// Returns normalized alpha opacity in `[0.0, 1.0]`.
    #[inline]
    pub fn opacity(&self) -> f64 {
        f64::from(self.a) / 255.0
    }

    /// Formats the color as a 6-digit lowercase hex string (`#rrggbb`).
    pub fn to_hex_rgb(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Formats the color as an 8-digit lowercase hex string (`#rrggbbaa`).
    pub fn to_hex_rgba(&self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }

    /// Formats the color for SVG attributes:
    /// - `"none"` when `a == 0` (`transparent` / `invis` / `none`)
    /// - `"#rrggbb"` when `a == 255`
    /// - `"#rrggbbaa"` when `0 < a < 255`
    pub fn to_svg_hex(&self) -> String {
        if self.a == 0 {
            "none".to_string()
        } else if self.a == 255 {
            self.to_hex_rgb()
        } else {
            self.to_hex_rgba()
        }
    }

    /// Formats the color as a CSS/SVG `rgb(...)` or `rgba(...)` string.
    pub fn to_css_rgba(&self) -> String {
        if self.a == 0 {
            "rgba(0,0,0,0)".to_string()
        } else if self.a == 255 {
            format!("rgb({},{},{})", self.r, self.g, self.b)
        } else {
            let alpha = (f64::from(self.a) / 255.0 * 1000.0).round() / 1000.0;
            format!("rgba({},{},{},{})", self.r, self.g, self.b, alpha)
        }
    }

    /// Alias for [`Self::to_svg_hex`].
    #[inline]
    pub fn to_svg_string(&self) -> String {
        self.to_svg_hex()
    }

    /// Converts normalized HSV (`h, s, v` in `[0.0, 1.0]`) to an opaque `RgbaColor`,
    /// matching `hsv2rgb` in `lib/common/colxlate.c`.
    pub fn from_hsv(h: f64, s: f64, v: f64) -> Self {
        Self::from_hsva(h, s, v, 1.0)
    }

    /// Converts normalized HSVA (`h, s, v, a` in `[0.0, 1.0]`) to an `RgbaColor`,
    /// matching `hsv2rgb` and `RGBA_BYTE` conversion in `lib/common/colxlate.c`.
    pub fn from_hsva(h: f64, s: f64, v: f64, a: f64) -> Self {
        let h = h.clamp(0.0, 1.0);
        let s = s.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);
        let a = a.clamp(0.0, 1.0);
        let (r, g, b) = hsv2rgb(h, s, v);
        Self {
            r: (r * 255.0) as u8,
            g: (g * 255.0) as u8,
            b: (b * 255.0) as u8,
            a: (a * 255.0) as u8,
        }
    }

    /// Converts this `RgbaColor` into normalized `(h, s, v, a)` in `[0.0, 1.0]`,
    /// matching `rgb2hsv` in `lib/common/colxlate.c`.
    pub fn to_hsva(&self) -> (f64, f64, f64, f64) {
        let r = f64::from(self.r) / 255.0;
        let g = f64::from(self.g) / 255.0;
        let b = f64::from(self.b) / 255.0;
        let a = f64::from(self.a) / 255.0;
        let (h, s, v) = rgb2hsv(r, g, b);
        (h, s, v, a)
    }
}

impl fmt::Display for RgbaColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_svg_hex())
    }
}

/// Port of `hsv2rgb` from `lib/common/colxlate.c`.
pub fn hsv2rgb(mut h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s <= 0.0 {
        (v, v, v)
    } else {
        if h >= 1.0 {
            h = 0.0;
        }
        h *= 6.0;
        let i = h as i32;
        let f = h - f64::from(i);
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        match i {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        }
    }
}

/// Port of `rgb2hsv` from `lib/common/colxlate.c`.
pub fn rgb2hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let rgbmin = r.min(g.min(b));
    let rgbmax = r.max(g.max(b));
    let st = if rgbmax > 0.0 {
        (rgbmax - rgbmin) / rgbmax
    } else {
        0.0
    };
    let mut ht = 0.0;
    if st > 0.0 {
        let denom = rgbmax - rgbmin;
        let rc = (rgbmax - r) / denom;
        let gc = (rgbmax - g) / denom;
        let bc = (rgbmax - b) / denom;
        if (r - rgbmax).abs() == 0.0 {
            ht = bc - gc;
        } else if (g - rgbmax).abs() == 0.0 {
            ht = 2.0 + rc - bc;
        } else if (b - rgbmax).abs() == 0.0 {
            ht = 4.0 + gc - rc;
        }
        ht *= 60.0;
        if ht < 0.0 {
            ht += 360.0;
        }
    }
    (ht / 360.0, st, rgbmax)
}

/// Resolves a Graphviz color specification (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`,
/// `"h,s,v"`, `"h s v"`, or X11/SVG/Brewer named color) to an [`RgbaColor`].
pub fn resolve_color(spec: &str) -> Option<RgbaColor> {
    resolve_color_with_scheme(spec, None)
}

/// Resolves a Graphviz color specification with an optional active `colorscheme`.
pub fn resolve_color_with_scheme(spec: &str, colorscheme: Option<&str>) -> Option<RgbaColor> {
    let p = spec.trim();
    if p.is_empty() {
        return None;
    }

    // 1. Hex color: #rgb, #rgba, #rrggbb, #rrggbbaa
    if let Some(hex) = p.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    // 2. HSV / HSVA numeric specification: starts with '.' or ASCII digit
    let first = p.as_bytes()[0];
    if (first == b'.' || first.is_ascii_digit())
        && let Some(hsv_color) = parse_hsv_color(p)
    {
        return Some(hsv_color);
    }

    // 3. Named color lookup (X11 / SVG / Brewer color schemes)
    let resolved_name = normalize_color_scheme_name(p, colorscheme);
    lookup_named_color(&resolved_name).or_else(|| {
        // Also support `/svg/<name>` or `<name>` fallback if a scheme prefix was supplied
        if let Some(stripped) = resolved_name.strip_prefix("/svg/") {
            lookup_named_color(stripped)
        } else {
            None
        }
    })
}

/// Formats a Graphviz color spec into an SVG color string (`#rrggbb`, `#rrggbbaa`, or `"none"`),
/// falling back to `"black"` (`#000000`) if unknown.
pub fn format_svg_color(spec: &str) -> String {
    resolve_color(spec).unwrap_or(RgbaColor::BLACK).to_svg_hex()
}

fn parse_hex_color(hex: &str) -> Option<RgbaColor> {
    let hex = hex.trim();
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some(RgbaColor::rgb(r | (r << 4), g | (g << 4), b | (b << 4)))
        }
        4 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            let a = u8::from_str_radix(&hex[3..4], 16).ok()?;
            Some(RgbaColor::rgba(
                r | (r << 4),
                g | (g << 4),
                b | (b << 4),
                a | (a << 4),
            ))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(RgbaColor::rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(RgbaColor::rgba(r, g, b, a))
        }
        _ => None,
    }
}

fn parse_hsv_color(p: &str) -> Option<RgbaColor> {
    let mut nums = [0.0_f64; 4];
    let mut count = 0;
    for token in p.split(|c: char| c == ',' || c.is_ascii_whitespace()) {
        if token.is_empty() {
            continue;
        }
        if count >= 4 {
            return None;
        }
        nums[count] = token.parse::<f64>().ok()?;
        count += 1;
    }
    if count == 3 {
        Some(RgbaColor::from_hsva(nums[0], nums[1], nums[2], 1.0))
    } else if count == 4 {
        Some(RgbaColor::from_hsva(nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

/// Port of `resolveColor` from `lib/common/colxlate.c`.
fn normalize_color_scheme_name(str_in: &str, colorscheme: Option<&str>) -> String {
    let lower = str_in.to_ascii_lowercase();
    if lower == "black" || lower == "white" || lower == "lightgrey" {
        return lower;
    }

    let is_non_dflt = |scheme: Option<&str>| -> Option<String> {
        let s = scheme?.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("x11") {
            None
        } else {
            Some(s.to_ascii_lowercase())
        }
    };

    if let Some(c2) = lower.strip_prefix('/') {
        if let Some(slash_idx) = c2.find('/') {
            if let Some(after_double) = c2.strip_prefix('/') {
                if let Some(scheme) = is_non_dflt(colorscheme) {
                    format!("/{scheme}/{after_double}")
                } else {
                    after_double.to_string()
                }
            } else if c2.starts_with("x11/") {
                c2[slash_idx + 1..].to_string()
            } else {
                lower
            }
        } else {
            c2.to_string()
        }
    } else if let Some(scheme) = is_non_dflt(colorscheme) {
        format!("/{scheme}/{lower}")
    } else {
        lower
    }
}

fn lookup_named_color(name: &str) -> Option<RgbaColor> {
    let key = name.trim().to_ascii_lowercase();
    if let Ok(idx) = NAMED_COLORS.binary_search_by(|(n, _)| (*n).cmp(key.as_str())) {
        return Some(NAMED_COLORS[idx].1);
    }
    // Check gray0..gray100 / grey0..grey100 dynamically if not in main table
    if let Some(num_str) = key
        .strip_prefix("gray")
        .or_else(|| key.strip_prefix("grey"))
        && let Ok(pct) = num_str.parse::<u8>()
        && pct <= 100
    {
        let v = ((f64::from(pct) * 255.0 / 100.0).round()) as u8;
        return Some(RgbaColor::rgb(v, v, v));
    }
    None
}

/// Sorted lookup table of SVG & X11 named colors from `lib/common/colortbl.h`.
/// Sorted strictly in ascending ASCII lexicographical order by name for `binary_search_by`.
static NAMED_COLORS: &[(&str, RgbaColor)] = &[
    ("/accent3/1", RgbaColor::rgba(127, 201, 127, 255)),
    ("/accent3/2", RgbaColor::rgba(190, 174, 212, 255)),
    ("/accent3/3", RgbaColor::rgba(253, 192, 134, 255)),
    ("/blues3/1", RgbaColor::rgba(222, 235, 247, 255)),
    ("/blues3/2", RgbaColor::rgba(158, 202, 225, 255)),
    ("/blues3/3", RgbaColor::rgba(49, 130, 189, 255)),
    ("/greens3/1", RgbaColor::rgba(229, 245, 224, 255)),
    ("/greens3/2", RgbaColor::rgba(161, 217, 155, 255)),
    ("/greens3/3", RgbaColor::rgba(49, 163, 84, 255)),
    ("/paired3/1", RgbaColor::rgba(166, 206, 227, 255)),
    ("/paired3/2", RgbaColor::rgba(31, 120, 180, 255)),
    ("/paired3/3", RgbaColor::rgba(178, 223, 138, 255)),
    ("/reds3/1", RgbaColor::rgba(254, 224, 210, 255)),
    ("/reds3/2", RgbaColor::rgba(252, 146, 114, 255)),
    ("/reds3/3", RgbaColor::rgba(222, 45, 38, 255)),
    ("/set13/1", RgbaColor::rgba(228, 26, 28, 255)),
    ("/set13/2", RgbaColor::rgba(55, 126, 184, 255)),
    ("/set13/3", RgbaColor::rgba(77, 175, 74, 255)),
    ("/svg/gray", RgbaColor::rgba(128, 128, 128, 255)),
    ("/svg/green", RgbaColor::rgba(0, 128, 0, 255)),
    ("/svg/grey", RgbaColor::rgba(128, 128, 128, 255)),
    ("/svg/maroon", RgbaColor::rgba(128, 0, 0, 255)),
    ("/svg/purple", RgbaColor::rgba(128, 0, 128, 255)),
    ("aliceblue", RgbaColor::rgba(240, 248, 255, 255)),
    ("antiquewhite", RgbaColor::rgba(250, 235, 215, 255)),
    ("antiquewhite1", RgbaColor::rgba(255, 239, 219, 255)),
    ("antiquewhite2", RgbaColor::rgba(238, 223, 204, 255)),
    ("antiquewhite3", RgbaColor::rgba(205, 192, 176, 255)),
    ("antiquewhite4", RgbaColor::rgba(139, 131, 120, 255)),
    ("aqua", RgbaColor::rgba(0, 255, 255, 255)),
    ("aquamarine", RgbaColor::rgba(127, 255, 212, 255)),
    ("aquamarine1", RgbaColor::rgba(127, 255, 212, 255)),
    ("aquamarine2", RgbaColor::rgba(118, 238, 198, 255)),
    ("aquamarine3", RgbaColor::rgba(102, 205, 170, 255)),
    ("aquamarine4", RgbaColor::rgba(69, 139, 116, 255)),
    ("azure", RgbaColor::rgba(240, 255, 255, 255)),
    ("azure1", RgbaColor::rgba(240, 255, 255, 255)),
    ("azure2", RgbaColor::rgba(224, 238, 238, 255)),
    ("azure3", RgbaColor::rgba(193, 205, 205, 255)),
    ("azure4", RgbaColor::rgba(131, 139, 139, 255)),
    ("beige", RgbaColor::rgba(245, 245, 220, 255)),
    ("bisque", RgbaColor::rgba(255, 228, 196, 255)),
    ("bisque1", RgbaColor::rgba(255, 228, 196, 255)),
    ("bisque2", RgbaColor::rgba(238, 213, 183, 255)),
    ("bisque3", RgbaColor::rgba(205, 183, 158, 255)),
    ("bisque4", RgbaColor::rgba(139, 125, 107, 255)),
    ("black", RgbaColor::rgba(0, 0, 0, 255)),
    ("blanchedalmond", RgbaColor::rgba(255, 235, 205, 255)),
    ("blue", RgbaColor::rgba(0, 0, 255, 255)),
    ("blue1", RgbaColor::rgba(0, 0, 255, 255)),
    ("blue2", RgbaColor::rgba(0, 0, 238, 255)),
    ("blue3", RgbaColor::rgba(0, 0, 205, 255)),
    ("blue4", RgbaColor::rgba(0, 0, 139, 255)),
    ("blueviolet", RgbaColor::rgba(138, 43, 226, 255)),
    ("brown", RgbaColor::rgba(165, 42, 42, 255)),
    ("brown1", RgbaColor::rgba(255, 64, 64, 255)),
    ("brown2", RgbaColor::rgba(238, 59, 59, 255)),
    ("brown3", RgbaColor::rgba(205, 51, 51, 255)),
    ("brown4", RgbaColor::rgba(139, 35, 35, 255)),
    ("burlywood", RgbaColor::rgba(222, 184, 135, 255)),
    ("burlywood1", RgbaColor::rgba(255, 211, 155, 255)),
    ("burlywood2", RgbaColor::rgba(238, 197, 145, 255)),
    ("burlywood3", RgbaColor::rgba(205, 170, 125, 255)),
    ("burlywood4", RgbaColor::rgba(139, 115, 85, 255)),
    ("cadetblue", RgbaColor::rgba(95, 158, 160, 255)),
    ("cadetblue1", RgbaColor::rgba(152, 245, 255, 255)),
    ("cadetblue2", RgbaColor::rgba(142, 229, 238, 255)),
    ("cadetblue3", RgbaColor::rgba(122, 197, 205, 255)),
    ("cadetblue4", RgbaColor::rgba(83, 134, 139, 255)),
    ("chartreuse", RgbaColor::rgba(127, 255, 0, 255)),
    ("chartreuse1", RgbaColor::rgba(127, 255, 0, 255)),
    ("chartreuse2", RgbaColor::rgba(118, 238, 0, 255)),
    ("chartreuse3", RgbaColor::rgba(102, 205, 0, 255)),
    ("chartreuse4", RgbaColor::rgba(69, 139, 0, 255)),
    ("chocolate", RgbaColor::rgba(210, 105, 30, 255)),
    ("chocolate1", RgbaColor::rgba(255, 127, 36, 255)),
    ("chocolate2", RgbaColor::rgba(238, 118, 33, 255)),
    ("chocolate3", RgbaColor::rgba(205, 102, 29, 255)),
    ("chocolate4", RgbaColor::rgba(139, 69, 19, 255)),
    ("coral", RgbaColor::rgba(255, 127, 80, 255)),
    ("coral1", RgbaColor::rgba(255, 114, 86, 255)),
    ("coral2", RgbaColor::rgba(238, 106, 80, 255)),
    ("coral3", RgbaColor::rgba(205, 91, 69, 255)),
    ("coral4", RgbaColor::rgba(139, 62, 47, 255)),
    ("cornflowerblue", RgbaColor::rgba(100, 149, 237, 255)),
    ("cornsilk", RgbaColor::rgba(255, 248, 220, 255)),
    ("cornsilk1", RgbaColor::rgba(255, 248, 220, 255)),
    ("cornsilk2", RgbaColor::rgba(238, 232, 205, 255)),
    ("cornsilk3", RgbaColor::rgba(205, 200, 177, 255)),
    ("cornsilk4", RgbaColor::rgba(139, 136, 120, 255)),
    ("crimson", RgbaColor::rgba(220, 20, 60, 255)),
    ("cyan", RgbaColor::rgba(0, 255, 255, 255)),
    ("cyan1", RgbaColor::rgba(0, 255, 255, 255)),
    ("cyan2", RgbaColor::rgba(0, 238, 238, 255)),
    ("cyan3", RgbaColor::rgba(0, 205, 205, 255)),
    ("cyan4", RgbaColor::rgba(0, 139, 139, 255)),
    ("darkblue", RgbaColor::rgba(0, 0, 139, 255)),
    ("darkcyan", RgbaColor::rgba(0, 139, 139, 255)),
    ("darkgoldenrod", RgbaColor::rgba(184, 134, 11, 255)),
    ("darkgoldenrod1", RgbaColor::rgba(255, 185, 15, 255)),
    ("darkgoldenrod2", RgbaColor::rgba(238, 173, 14, 255)),
    ("darkgoldenrod3", RgbaColor::rgba(205, 149, 12, 255)),
    ("darkgoldenrod4", RgbaColor::rgba(139, 101, 8, 255)),
    ("darkgray", RgbaColor::rgba(169, 169, 169, 255)),
    ("darkgreen", RgbaColor::rgba(0, 100, 0, 255)),
    ("darkgrey", RgbaColor::rgba(169, 169, 169, 255)),
    ("darkkhaki", RgbaColor::rgba(189, 183, 107, 255)),
    ("darkmagenta", RgbaColor::rgba(139, 0, 139, 255)),
    ("darkolivegreen", RgbaColor::rgba(85, 107, 47, 255)),
    ("darkolivegreen1", RgbaColor::rgba(202, 255, 112, 255)),
    ("darkolivegreen2", RgbaColor::rgba(188, 238, 104, 255)),
    ("darkolivegreen3", RgbaColor::rgba(162, 205, 90, 255)),
    ("darkolivegreen4", RgbaColor::rgba(110, 139, 61, 255)),
    ("darkorange", RgbaColor::rgba(255, 140, 0, 255)),
    ("darkorange1", RgbaColor::rgba(255, 127, 0, 255)),
    ("darkorange2", RgbaColor::rgba(238, 118, 0, 255)),
    ("darkorange3", RgbaColor::rgba(205, 102, 0, 255)),
    ("darkorange4", RgbaColor::rgba(139, 69, 0, 255)),
    ("darkorchid", RgbaColor::rgba(153, 50, 204, 255)),
    ("darkorchid1", RgbaColor::rgba(191, 62, 255, 255)),
    ("darkorchid2", RgbaColor::rgba(178, 58, 238, 255)),
    ("darkorchid3", RgbaColor::rgba(154, 50, 205, 255)),
    ("darkorchid4", RgbaColor::rgba(104, 34, 139, 255)),
    ("darkred", RgbaColor::rgba(139, 0, 0, 255)),
    ("darksalmon", RgbaColor::rgba(233, 150, 122, 255)),
    ("darkseagreen", RgbaColor::rgba(143, 188, 143, 255)),
    ("darkseagreen1", RgbaColor::rgba(193, 255, 193, 255)),
    ("darkseagreen2", RgbaColor::rgba(180, 238, 180, 255)),
    ("darkseagreen3", RgbaColor::rgba(155, 205, 155, 255)),
    ("darkseagreen4", RgbaColor::rgba(105, 139, 105, 255)),
    ("darkslateblue", RgbaColor::rgba(72, 61, 139, 255)),
    ("darkslategray", RgbaColor::rgba(47, 79, 79, 255)),
    ("darkslategray1", RgbaColor::rgba(151, 255, 255, 255)),
    ("darkslategray2", RgbaColor::rgba(141, 238, 238, 255)),
    ("darkslategray3", RgbaColor::rgba(121, 205, 205, 255)),
    ("darkslategray4", RgbaColor::rgba(82, 139, 139, 255)),
    ("darkslategrey", RgbaColor::rgba(47, 79, 79, 255)),
    ("darkturquoise", RgbaColor::rgba(0, 206, 209, 255)),
    ("darkviolet", RgbaColor::rgba(148, 0, 211, 255)),
    ("deeppink", RgbaColor::rgba(255, 20, 147, 255)),
    ("deeppink1", RgbaColor::rgba(255, 20, 147, 255)),
    ("deeppink2", RgbaColor::rgba(238, 18, 137, 255)),
    ("deeppink3", RgbaColor::rgba(205, 16, 118, 255)),
    ("deeppink4", RgbaColor::rgba(139, 10, 80, 255)),
    ("deepskyblue", RgbaColor::rgba(0, 191, 255, 255)),
    ("deepskyblue1", RgbaColor::rgba(0, 191, 255, 255)),
    ("deepskyblue2", RgbaColor::rgba(0, 178, 238, 255)),
    ("deepskyblue3", RgbaColor::rgba(0, 154, 205, 255)),
    ("deepskyblue4", RgbaColor::rgba(0, 104, 139, 255)),
    ("dimgray", RgbaColor::rgba(105, 105, 105, 255)),
    ("dimgrey", RgbaColor::rgba(105, 105, 105, 255)),
    ("dodgerblue", RgbaColor::rgba(30, 144, 255, 255)),
    ("dodgerblue1", RgbaColor::rgba(30, 144, 255, 255)),
    ("dodgerblue2", RgbaColor::rgba(28, 134, 238, 255)),
    ("dodgerblue3", RgbaColor::rgba(24, 116, 205, 255)),
    ("dodgerblue4", RgbaColor::rgba(16, 78, 139, 255)),
    ("firebrick", RgbaColor::rgba(178, 34, 34, 255)),
    ("firebrick1", RgbaColor::rgba(255, 48, 48, 255)),
    ("firebrick2", RgbaColor::rgba(238, 44, 44, 255)),
    ("firebrick3", RgbaColor::rgba(205, 38, 38, 255)),
    ("firebrick4", RgbaColor::rgba(139, 26, 26, 255)),
    ("floralwhite", RgbaColor::rgba(255, 250, 240, 255)),
    ("forestgreen", RgbaColor::rgba(34, 139, 34, 255)),
    ("fuchsia", RgbaColor::rgba(255, 0, 255, 255)),
    ("gainsboro", RgbaColor::rgba(220, 220, 220, 255)),
    ("ghostwhite", RgbaColor::rgba(248, 248, 255, 255)),
    ("gold", RgbaColor::rgba(255, 215, 0, 255)),
    ("gold1", RgbaColor::rgba(255, 215, 0, 255)),
    ("gold2", RgbaColor::rgba(238, 201, 0, 255)),
    ("gold3", RgbaColor::rgba(205, 173, 0, 255)),
    ("gold4", RgbaColor::rgba(139, 117, 0, 255)),
    ("goldenrod", RgbaColor::rgba(218, 165, 32, 255)),
    ("goldenrod1", RgbaColor::rgba(255, 193, 37, 255)),
    ("goldenrod2", RgbaColor::rgba(238, 180, 34, 255)),
    ("goldenrod3", RgbaColor::rgba(205, 155, 29, 255)),
    ("goldenrod4", RgbaColor::rgba(139, 105, 20, 255)),
    ("gray", RgbaColor::rgba(192, 192, 192, 255)),
    ("gray0", RgbaColor::rgba(0, 0, 0, 255)),
    ("gray10", RgbaColor::rgba(26, 26, 26, 255)),
    ("gray100", RgbaColor::rgba(255, 255, 255, 255)),
    ("gray20", RgbaColor::rgba(51, 51, 51, 255)),
    ("gray25", RgbaColor::rgba(64, 64, 64, 255)),
    ("gray30", RgbaColor::rgba(77, 77, 77, 255)),
    ("gray40", RgbaColor::rgba(102, 102, 102, 255)),
    ("gray50", RgbaColor::rgba(127, 127, 127, 255)),
    ("gray60", RgbaColor::rgba(153, 153, 153, 255)),
    ("gray70", RgbaColor::rgba(179, 179, 179, 255)),
    ("gray75", RgbaColor::rgba(191, 191, 191, 255)),
    ("gray80", RgbaColor::rgba(204, 204, 204, 255)),
    ("gray90", RgbaColor::rgba(229, 229, 229, 255)),
    ("green", RgbaColor::rgba(0, 255, 0, 255)),
    ("green1", RgbaColor::rgba(0, 255, 0, 255)),
    ("green2", RgbaColor::rgba(0, 238, 0, 255)),
    ("green3", RgbaColor::rgba(0, 205, 0, 255)),
    ("green4", RgbaColor::rgba(0, 139, 0, 255)),
    ("greenyellow", RgbaColor::rgba(173, 255, 47, 255)),
    ("grey", RgbaColor::rgba(192, 192, 192, 255)),
    ("grey0", RgbaColor::rgba(0, 0, 0, 255)),
    ("grey10", RgbaColor::rgba(26, 26, 26, 255)),
    ("grey100", RgbaColor::rgba(255, 255, 255, 255)),
    ("grey20", RgbaColor::rgba(51, 51, 51, 255)),
    ("grey25", RgbaColor::rgba(64, 64, 64, 255)),
    ("grey30", RgbaColor::rgba(77, 77, 77, 255)),
    ("grey40", RgbaColor::rgba(102, 102, 102, 255)),
    ("grey50", RgbaColor::rgba(127, 127, 127, 255)),
    ("grey60", RgbaColor::rgba(153, 153, 153, 255)),
    ("grey70", RgbaColor::rgba(179, 179, 179, 255)),
    ("grey75", RgbaColor::rgba(191, 191, 191, 255)),
    ("grey80", RgbaColor::rgba(204, 204, 204, 255)),
    ("grey90", RgbaColor::rgba(229, 229, 229, 255)),
    ("honeydew", RgbaColor::rgba(240, 255, 240, 255)),
    ("honeydew1", RgbaColor::rgba(240, 255, 240, 255)),
    ("honeydew2", RgbaColor::rgba(224, 238, 224, 255)),
    ("honeydew3", RgbaColor::rgba(193, 205, 193, 255)),
    ("honeydew4", RgbaColor::rgba(131, 139, 131, 255)),
    ("hotpink", RgbaColor::rgba(255, 105, 180, 255)),
    ("hotpink1", RgbaColor::rgba(255, 110, 180, 255)),
    ("hotpink2", RgbaColor::rgba(238, 106, 167, 255)),
    ("hotpink3", RgbaColor::rgba(205, 96, 144, 255)),
    ("hotpink4", RgbaColor::rgba(139, 58, 98, 255)),
    ("indianred", RgbaColor::rgba(205, 92, 92, 255)),
    ("indianred1", RgbaColor::rgba(255, 106, 106, 255)),
    ("indianred2", RgbaColor::rgba(238, 99, 99, 255)),
    ("indianred3", RgbaColor::rgba(205, 85, 85, 255)),
    ("indianred4", RgbaColor::rgba(139, 58, 58, 255)),
    ("indigo", RgbaColor::rgba(75, 0, 130, 255)),
    ("invis", RgbaColor::rgba(255, 255, 254, 0)),
    ("ivory", RgbaColor::rgba(255, 255, 240, 255)),
    ("ivory1", RgbaColor::rgba(255, 255, 240, 255)),
    ("ivory2", RgbaColor::rgba(238, 238, 224, 255)),
    ("ivory3", RgbaColor::rgba(205, 205, 193, 255)),
    ("ivory4", RgbaColor::rgba(139, 139, 131, 255)),
    ("khaki", RgbaColor::rgba(240, 230, 140, 255)),
    ("khaki1", RgbaColor::rgba(255, 246, 143, 255)),
    ("khaki2", RgbaColor::rgba(238, 230, 133, 255)),
    ("khaki3", RgbaColor::rgba(205, 198, 115, 255)),
    ("khaki4", RgbaColor::rgba(139, 134, 78, 255)),
    ("lavender", RgbaColor::rgba(230, 230, 250, 255)),
    ("lavenderblush", RgbaColor::rgba(255, 240, 245, 255)),
    ("lavenderblush1", RgbaColor::rgba(255, 240, 245, 255)),
    ("lavenderblush2", RgbaColor::rgba(238, 224, 229, 255)),
    ("lavenderblush3", RgbaColor::rgba(205, 193, 197, 255)),
    ("lavenderblush4", RgbaColor::rgba(139, 131, 134, 255)),
    ("lawngreen", RgbaColor::rgba(124, 252, 0, 255)),
    ("lemonchiffon", RgbaColor::rgba(255, 250, 205, 255)),
    ("lemonchiffon1", RgbaColor::rgba(255, 250, 205, 255)),
    ("lemonchiffon2", RgbaColor::rgba(238, 233, 191, 255)),
    ("lemonchiffon3", RgbaColor::rgba(205, 201, 165, 255)),
    ("lemonchiffon4", RgbaColor::rgba(139, 137, 112, 255)),
    ("lightblue", RgbaColor::rgba(173, 216, 230, 255)),
    ("lightblue1", RgbaColor::rgba(191, 239, 255, 255)),
    ("lightblue2", RgbaColor::rgba(178, 223, 238, 255)),
    ("lightblue3", RgbaColor::rgba(154, 192, 205, 255)),
    ("lightblue4", RgbaColor::rgba(104, 131, 139, 255)),
    ("lightcoral", RgbaColor::rgba(240, 128, 128, 255)),
    ("lightcyan", RgbaColor::rgba(224, 255, 255, 255)),
    ("lightcyan1", RgbaColor::rgba(224, 255, 255, 255)),
    ("lightcyan2", RgbaColor::rgba(209, 238, 238, 255)),
    ("lightcyan3", RgbaColor::rgba(180, 205, 205, 255)),
    ("lightcyan4", RgbaColor::rgba(122, 139, 139, 255)),
    ("lightgoldenrod", RgbaColor::rgba(238, 221, 130, 255)),
    ("lightgoldenrod1", RgbaColor::rgba(255, 236, 139, 255)),
    ("lightgoldenrod2", RgbaColor::rgba(238, 220, 130, 255)),
    ("lightgoldenrod3", RgbaColor::rgba(205, 190, 112, 255)),
    ("lightgoldenrod4", RgbaColor::rgba(139, 129, 76, 255)),
    ("lightgoldenrodyellow", RgbaColor::rgba(250, 250, 210, 255)),
    ("lightgray", RgbaColor::rgba(211, 211, 211, 255)),
    ("lightgreen", RgbaColor::rgba(144, 238, 144, 255)),
    ("lightgrey", RgbaColor::rgba(211, 211, 211, 255)),
    ("lightpink", RgbaColor::rgba(255, 182, 193, 255)),
    ("lightpink1", RgbaColor::rgba(255, 174, 185, 255)),
    ("lightpink2", RgbaColor::rgba(238, 162, 173, 255)),
    ("lightpink3", RgbaColor::rgba(205, 140, 149, 255)),
    ("lightpink4", RgbaColor::rgba(139, 95, 101, 255)),
    ("lightsalmon", RgbaColor::rgba(255, 160, 122, 255)),
    ("lightsalmon1", RgbaColor::rgba(255, 160, 122, 255)),
    ("lightsalmon2", RgbaColor::rgba(238, 149, 114, 255)),
    ("lightsalmon3", RgbaColor::rgba(205, 129, 98, 255)),
    ("lightsalmon4", RgbaColor::rgba(139, 87, 66, 255)),
    ("lightseagreen", RgbaColor::rgba(32, 178, 170, 255)),
    ("lightskyblue", RgbaColor::rgba(135, 206, 250, 255)),
    ("lightskyblue1", RgbaColor::rgba(176, 226, 255, 255)),
    ("lightskyblue2", RgbaColor::rgba(164, 211, 238, 255)),
    ("lightskyblue3", RgbaColor::rgba(141, 182, 205, 255)),
    ("lightskyblue4", RgbaColor::rgba(96, 123, 139, 255)),
    ("lightslateblue", RgbaColor::rgba(132, 112, 255, 255)),
    ("lightslategray", RgbaColor::rgba(119, 136, 153, 255)),
    ("lightslategrey", RgbaColor::rgba(119, 136, 153, 255)),
    ("lightsteelblue", RgbaColor::rgba(176, 196, 222, 255)),
    ("lightsteelblue1", RgbaColor::rgba(202, 225, 255, 255)),
    ("lightsteelblue2", RgbaColor::rgba(188, 210, 238, 255)),
    ("lightsteelblue3", RgbaColor::rgba(162, 181, 205, 255)),
    ("lightsteelblue4", RgbaColor::rgba(110, 123, 139, 255)),
    ("lightyellow", RgbaColor::rgba(255, 255, 224, 255)),
    ("lightyellow1", RgbaColor::rgba(255, 255, 224, 255)),
    ("lightyellow2", RgbaColor::rgba(238, 238, 209, 255)),
    ("lightyellow3", RgbaColor::rgba(205, 205, 180, 255)),
    ("lightyellow4", RgbaColor::rgba(139, 139, 122, 255)),
    ("lime", RgbaColor::rgba(0, 255, 0, 255)),
    ("limegreen", RgbaColor::rgba(50, 205, 50, 255)),
    ("linen", RgbaColor::rgba(250, 240, 230, 255)),
    ("magenta", RgbaColor::rgba(255, 0, 255, 255)),
    ("magenta1", RgbaColor::rgba(255, 0, 255, 255)),
    ("magenta2", RgbaColor::rgba(238, 0, 238, 255)),
    ("magenta3", RgbaColor::rgba(205, 0, 205, 255)),
    ("magenta4", RgbaColor::rgba(139, 0, 139, 255)),
    ("maroon", RgbaColor::rgba(176, 48, 96, 255)),
    ("maroon1", RgbaColor::rgba(255, 52, 179, 255)),
    ("maroon2", RgbaColor::rgba(238, 48, 167, 255)),
    ("maroon3", RgbaColor::rgba(205, 41, 144, 255)),
    ("maroon4", RgbaColor::rgba(139, 28, 98, 255)),
    ("mediumaquamarine", RgbaColor::rgba(102, 205, 170, 255)),
    ("mediumblue", RgbaColor::rgba(0, 0, 205, 255)),
    ("mediumorchid", RgbaColor::rgba(186, 85, 211, 255)),
    ("mediumorchid1", RgbaColor::rgba(224, 102, 255, 255)),
    ("mediumorchid2", RgbaColor::rgba(209, 95, 238, 255)),
    ("mediumorchid3", RgbaColor::rgba(180, 82, 205, 255)),
    ("mediumorchid4", RgbaColor::rgba(122, 55, 139, 255)),
    ("mediumpurple", RgbaColor::rgba(147, 112, 219, 255)),
    ("mediumpurple1", RgbaColor::rgba(171, 130, 255, 255)),
    ("mediumpurple2", RgbaColor::rgba(159, 121, 238, 255)),
    ("mediumpurple3", RgbaColor::rgba(137, 104, 205, 255)),
    ("mediumpurple4", RgbaColor::rgba(93, 71, 139, 255)),
    ("mediumseagreen", RgbaColor::rgba(60, 179, 113, 255)),
    ("mediumslateblue", RgbaColor::rgba(123, 104, 238, 255)),
    ("mediumspringgreen", RgbaColor::rgba(0, 250, 154, 255)),
    ("mediumturquoise", RgbaColor::rgba(72, 209, 204, 255)),
    ("mediumvioletred", RgbaColor::rgba(199, 21, 133, 255)),
    ("midnightblue", RgbaColor::rgba(25, 25, 112, 255)),
    ("mintcream", RgbaColor::rgba(245, 255, 250, 255)),
    ("mistyrose", RgbaColor::rgba(255, 228, 225, 255)),
    ("mistyrose1", RgbaColor::rgba(255, 228, 225, 255)),
    ("mistyrose2", RgbaColor::rgba(238, 213, 210, 255)),
    ("mistyrose3", RgbaColor::rgba(205, 183, 181, 255)),
    ("mistyrose4", RgbaColor::rgba(139, 125, 123, 255)),
    ("moccasin", RgbaColor::rgba(255, 228, 181, 255)),
    ("navajowhite", RgbaColor::rgba(255, 222, 173, 255)),
    ("navajowhite1", RgbaColor::rgba(255, 222, 173, 255)),
    ("navajowhite2", RgbaColor::rgba(238, 207, 161, 255)),
    ("navajowhite3", RgbaColor::rgba(205, 179, 139, 255)),
    ("navajowhite4", RgbaColor::rgba(139, 121, 94, 255)),
    ("navy", RgbaColor::rgba(0, 0, 128, 255)),
    ("navyblue", RgbaColor::rgba(0, 0, 128, 255)),
    ("none", RgbaColor::rgba(255, 255, 254, 0)),
    ("oldlace", RgbaColor::rgba(253, 245, 230, 255)),
    ("olive", RgbaColor::rgba(128, 128, 0, 255)),
    ("olivedrab", RgbaColor::rgba(107, 142, 35, 255)),
    ("olivedrab1", RgbaColor::rgba(192, 255, 62, 255)),
    ("olivedrab2", RgbaColor::rgba(179, 238, 58, 255)),
    ("olivedrab3", RgbaColor::rgba(154, 205, 50, 255)),
    ("olivedrab4", RgbaColor::rgba(105, 139, 34, 255)),
    ("orange", RgbaColor::rgba(255, 165, 0, 255)),
    ("orange1", RgbaColor::rgba(255, 165, 0, 255)),
    ("orange2", RgbaColor::rgba(238, 154, 0, 255)),
    ("orange3", RgbaColor::rgba(205, 133, 0, 255)),
    ("orange4", RgbaColor::rgba(139, 90, 0, 255)),
    ("orangered", RgbaColor::rgba(255, 69, 0, 255)),
    ("orangered1", RgbaColor::rgba(255, 69, 0, 255)),
    ("orangered2", RgbaColor::rgba(238, 64, 0, 255)),
    ("orangered3", RgbaColor::rgba(205, 55, 0, 255)),
    ("orangered4", RgbaColor::rgba(139, 37, 0, 255)),
    ("orchid", RgbaColor::rgba(218, 112, 214, 255)),
    ("orchid1", RgbaColor::rgba(255, 131, 250, 255)),
    ("orchid2", RgbaColor::rgba(238, 122, 233, 255)),
    ("orchid3", RgbaColor::rgba(205, 105, 201, 255)),
    ("orchid4", RgbaColor::rgba(139, 71, 137, 255)),
    ("palegoldenrod", RgbaColor::rgba(238, 232, 170, 255)),
    ("palegreen", RgbaColor::rgba(152, 251, 152, 255)),
    ("palegreen1", RgbaColor::rgba(154, 255, 154, 255)),
    ("palegreen2", RgbaColor::rgba(144, 238, 144, 255)),
    ("palegreen3", RgbaColor::rgba(124, 205, 124, 255)),
    ("palegreen4", RgbaColor::rgba(84, 139, 84, 255)),
    ("paleturquoise", RgbaColor::rgba(175, 238, 238, 255)),
    ("paleturquoise1", RgbaColor::rgba(187, 255, 255, 255)),
    ("paleturquoise2", RgbaColor::rgba(174, 238, 238, 255)),
    ("paleturquoise3", RgbaColor::rgba(150, 205, 205, 255)),
    ("paleturquoise4", RgbaColor::rgba(102, 139, 139, 255)),
    ("palevioletred", RgbaColor::rgba(219, 112, 147, 255)),
    ("palevioletred1", RgbaColor::rgba(255, 130, 171, 255)),
    ("palevioletred2", RgbaColor::rgba(238, 121, 159, 255)),
    ("palevioletred3", RgbaColor::rgba(205, 104, 137, 255)),
    ("palevioletred4", RgbaColor::rgba(139, 71, 93, 255)),
    ("papayawhip", RgbaColor::rgba(255, 239, 213, 255)),
    ("peachpuff", RgbaColor::rgba(255, 218, 185, 255)),
    ("peachpuff1", RgbaColor::rgba(255, 218, 185, 255)),
    ("peachpuff2", RgbaColor::rgba(238, 203, 173, 255)),
    ("peachpuff3", RgbaColor::rgba(205, 175, 149, 255)),
    ("peachpuff4", RgbaColor::rgba(139, 119, 101, 255)),
    ("peru", RgbaColor::rgba(205, 133, 63, 255)),
    ("pink", RgbaColor::rgba(255, 192, 203, 255)),
    ("pink1", RgbaColor::rgba(255, 181, 197, 255)),
    ("pink2", RgbaColor::rgba(238, 169, 184, 255)),
    ("pink3", RgbaColor::rgba(205, 145, 158, 255)),
    ("pink4", RgbaColor::rgba(139, 99, 108, 255)),
    ("plum", RgbaColor::rgba(221, 160, 221, 255)),
    ("plum1", RgbaColor::rgba(255, 187, 255, 255)),
    ("plum2", RgbaColor::rgba(238, 174, 238, 255)),
    ("plum3", RgbaColor::rgba(205, 150, 205, 255)),
    ("plum4", RgbaColor::rgba(139, 102, 139, 255)),
    ("powderblue", RgbaColor::rgba(176, 224, 230, 255)),
    ("purple", RgbaColor::rgba(160, 32, 240, 255)),
    ("purple1", RgbaColor::rgba(155, 48, 255, 255)),
    ("purple2", RgbaColor::rgba(145, 44, 238, 255)),
    ("purple3", RgbaColor::rgba(125, 38, 205, 255)),
    ("purple4", RgbaColor::rgba(85, 26, 139, 255)),
    ("rebeccapurple", RgbaColor::rgba(102, 51, 153, 255)),
    ("red", RgbaColor::rgba(255, 0, 0, 255)),
    ("red1", RgbaColor::rgba(255, 0, 0, 255)),
    ("red2", RgbaColor::rgba(238, 0, 0, 255)),
    ("red3", RgbaColor::rgba(205, 0, 0, 255)),
    ("red4", RgbaColor::rgba(139, 0, 0, 255)),
    ("rosybrown", RgbaColor::rgba(188, 143, 143, 255)),
    ("rosybrown1", RgbaColor::rgba(255, 193, 193, 255)),
    ("rosybrown2", RgbaColor::rgba(238, 180, 180, 255)),
    ("rosybrown3", RgbaColor::rgba(205, 155, 155, 255)),
    ("rosybrown4", RgbaColor::rgba(139, 105, 105, 255)),
    ("royalblue", RgbaColor::rgba(65, 105, 225, 255)),
    ("royalblue1", RgbaColor::rgba(72, 118, 255, 255)),
    ("royalblue2", RgbaColor::rgba(67, 110, 238, 255)),
    ("royalblue3", RgbaColor::rgba(58, 95, 205, 255)),
    ("royalblue4", RgbaColor::rgba(39, 64, 139, 255)),
    ("saddlebrown", RgbaColor::rgba(139, 69, 19, 255)),
    ("salmon", RgbaColor::rgba(250, 128, 114, 255)),
    ("salmon1", RgbaColor::rgba(255, 140, 105, 255)),
    ("salmon2", RgbaColor::rgba(238, 130, 98, 255)),
    ("salmon3", RgbaColor::rgba(205, 112, 84, 255)),
    ("salmon4", RgbaColor::rgba(139, 76, 57, 255)),
    ("sandybrown", RgbaColor::rgba(244, 164, 96, 255)),
    ("seagreen", RgbaColor::rgba(46, 139, 87, 255)),
    ("seagreen1", RgbaColor::rgba(84, 255, 159, 255)),
    ("seagreen2", RgbaColor::rgba(78, 238, 148, 255)),
    ("seagreen3", RgbaColor::rgba(67, 205, 128, 255)),
    ("seagreen4", RgbaColor::rgba(46, 139, 87, 255)),
    ("seashell", RgbaColor::rgba(255, 245, 238, 255)),
    ("seashell1", RgbaColor::rgba(255, 245, 238, 255)),
    ("seashell2", RgbaColor::rgba(238, 229, 222, 255)),
    ("seashell3", RgbaColor::rgba(205, 197, 191, 255)),
    ("seashell4", RgbaColor::rgba(139, 134, 130, 255)),
    ("sienna", RgbaColor::rgba(160, 82, 45, 255)),
    ("sienna1", RgbaColor::rgba(255, 130, 71, 255)),
    ("sienna2", RgbaColor::rgba(238, 121, 66, 255)),
    ("sienna3", RgbaColor::rgba(205, 104, 57, 255)),
    ("sienna4", RgbaColor::rgba(139, 71, 38, 255)),
    ("silver", RgbaColor::rgba(192, 192, 192, 255)),
    ("skyblue", RgbaColor::rgba(135, 206, 235, 255)),
    ("skyblue1", RgbaColor::rgba(135, 206, 255, 255)),
    ("skyblue2", RgbaColor::rgba(126, 192, 238, 255)),
    ("skyblue3", RgbaColor::rgba(108, 166, 205, 255)),
    ("skyblue4", RgbaColor::rgba(74, 112, 139, 255)),
    ("slateblue", RgbaColor::rgba(106, 90, 205, 255)),
    ("slateblue1", RgbaColor::rgba(131, 111, 255, 255)),
    ("slateblue2", RgbaColor::rgba(122, 103, 238, 255)),
    ("slateblue3", RgbaColor::rgba(105, 89, 205, 255)),
    ("slateblue4", RgbaColor::rgba(71, 60, 139, 255)),
    ("slategray", RgbaColor::rgba(112, 128, 144, 255)),
    ("slategray1", RgbaColor::rgba(198, 226, 255, 255)),
    ("slategray2", RgbaColor::rgba(185, 211, 238, 255)),
    ("slategray3", RgbaColor::rgba(159, 182, 205, 255)),
    ("slategray4", RgbaColor::rgba(108, 123, 139, 255)),
    ("slategrey", RgbaColor::rgba(112, 128, 144, 255)),
    ("snow", RgbaColor::rgba(255, 250, 250, 255)),
    ("snow1", RgbaColor::rgba(255, 250, 250, 255)),
    ("snow2", RgbaColor::rgba(238, 233, 233, 255)),
    ("snow3", RgbaColor::rgba(205, 201, 201, 255)),
    ("snow4", RgbaColor::rgba(139, 137, 137, 255)),
    ("springgreen", RgbaColor::rgba(0, 255, 127, 255)),
    ("springgreen1", RgbaColor::rgba(0, 255, 127, 255)),
    ("springgreen2", RgbaColor::rgba(0, 238, 118, 255)),
    ("springgreen3", RgbaColor::rgba(0, 205, 102, 255)),
    ("springgreen4", RgbaColor::rgba(0, 139, 69, 255)),
    ("steelblue", RgbaColor::rgba(70, 130, 180, 255)),
    ("steelblue1", RgbaColor::rgba(99, 184, 255, 255)),
    ("steelblue2", RgbaColor::rgba(92, 172, 238, 255)),
    ("steelblue3", RgbaColor::rgba(79, 148, 205, 255)),
    ("steelblue4", RgbaColor::rgba(54, 100, 139, 255)),
    ("tan", RgbaColor::rgba(210, 180, 140, 255)),
    ("tan1", RgbaColor::rgba(255, 165, 79, 255)),
    ("tan2", RgbaColor::rgba(238, 154, 73, 255)),
    ("tan3", RgbaColor::rgba(205, 133, 63, 255)),
    ("tan4", RgbaColor::rgba(139, 90, 43, 255)),
    ("teal", RgbaColor::rgba(0, 128, 128, 255)),
    ("thistle", RgbaColor::rgba(216, 191, 216, 255)),
    ("thistle1", RgbaColor::rgba(255, 225, 255, 255)),
    ("thistle2", RgbaColor::rgba(238, 210, 238, 255)),
    ("thistle3", RgbaColor::rgba(205, 181, 205, 255)),
    ("thistle4", RgbaColor::rgba(139, 123, 139, 255)),
    ("tomato", RgbaColor::rgba(255, 99, 71, 255)),
    ("tomato1", RgbaColor::rgba(255, 99, 71, 255)),
    ("tomato2", RgbaColor::rgba(238, 92, 66, 255)),
    ("tomato3", RgbaColor::rgba(205, 79, 57, 255)),
    ("tomato4", RgbaColor::rgba(139, 54, 38, 255)),
    ("transparent", RgbaColor::rgba(255, 255, 254, 0)),
    ("turquoise", RgbaColor::rgba(64, 224, 208, 255)),
    ("turquoise1", RgbaColor::rgba(0, 245, 255, 255)),
    ("turquoise2", RgbaColor::rgba(0, 229, 238, 255)),
    ("turquoise3", RgbaColor::rgba(0, 197, 205, 255)),
    ("turquoise4", RgbaColor::rgba(0, 134, 139, 255)),
    ("violet", RgbaColor::rgba(238, 130, 238, 255)),
    ("violetred", RgbaColor::rgba(208, 32, 144, 255)),
    ("violetred1", RgbaColor::rgba(255, 62, 150, 255)),
    ("violetred2", RgbaColor::rgba(238, 58, 140, 255)),
    ("violetred3", RgbaColor::rgba(205, 50, 120, 255)),
    ("violetred4", RgbaColor::rgba(139, 34, 82, 255)),
    ("webgray", RgbaColor::rgba(128, 128, 128, 255)),
    ("webgreen", RgbaColor::rgba(0, 128, 0, 255)),
    ("webgrey", RgbaColor::rgba(128, 128, 128, 255)),
    ("webmaroon", RgbaColor::rgba(128, 0, 0, 255)),
    ("webpurple", RgbaColor::rgba(128, 0, 128, 255)),
    ("wheat", RgbaColor::rgba(245, 222, 179, 255)),
    ("wheat1", RgbaColor::rgba(255, 231, 186, 255)),
    ("wheat2", RgbaColor::rgba(238, 216, 174, 255)),
    ("wheat3", RgbaColor::rgba(205, 186, 150, 255)),
    ("wheat4", RgbaColor::rgba(139, 126, 102, 255)),
    ("white", RgbaColor::rgba(255, 255, 255, 255)),
    ("whitesmoke", RgbaColor::rgba(245, 245, 245, 255)),
    ("x11gray", RgbaColor::rgba(190, 190, 190, 255)),
    ("x11green", RgbaColor::rgba(0, 255, 0, 255)),
    ("x11grey", RgbaColor::rgba(190, 190, 190, 255)),
    ("x11maroon", RgbaColor::rgba(176, 48, 96, 255)),
    ("x11purple", RgbaColor::rgba(160, 32, 240, 255)),
    ("yellow", RgbaColor::rgba(255, 255, 0, 255)),
    ("yellow1", RgbaColor::rgba(255, 255, 0, 255)),
    ("yellow2", RgbaColor::rgba(238, 238, 0, 255)),
    ("yellow3", RgbaColor::rgba(205, 205, 0, 255)),
    ("yellow4", RgbaColor::rgba(139, 139, 0, 255)),
    ("yellowgreen", RgbaColor::rgba(154, 205, 50, 255)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_colors_table_is_sorted() {
        for window in NAMED_COLORS.windows(2) {
            assert!(
                window[0].0 < window[1].0,
                "NAMED_COLORS out of order: {:?} >= {:?}",
                window[0].0,
                window[1].0
            );
        }
    }

    #[test]
    fn test_x11_svg_named_colors() {
        assert_eq!(
            resolve_color("lightblue"),
            Some(RgbaColor::rgb(173, 216, 230))
        );
        assert_eq!(
            resolve_color("lightgrey"),
            Some(RgbaColor::rgb(211, 211, 211))
        );
        assert_eq!(
            resolve_color("LightGray"),
            Some(RgbaColor::rgb(211, 211, 211))
        );
        assert_eq!(resolve_color("red"), Some(RgbaColor::rgb(255, 0, 0)));
        assert_eq!(resolve_color("gold"), Some(RgbaColor::rgb(255, 215, 0)));
        assert_eq!(
            resolve_color("darkslategray"),
            Some(RgbaColor::rgb(47, 79, 79))
        );
        assert_eq!(
            resolve_color("darkslategrey"),
            Some(RgbaColor::rgb(47, 79, 79))
        );
        assert_eq!(
            resolve_color("transparent"),
            Some(RgbaColor::rgba(255, 255, 254, 0))
        );
        assert_eq!(resolve_color("none").unwrap().to_svg_hex(), "none");
        assert!(resolve_color("invis").unwrap().is_transparent());
    }

    #[test]
    fn test_hex_colors() {
        assert_eq!(resolve_color("#ff0000"), Some(RgbaColor::rgb(255, 0, 0)));
        assert_eq!(
            resolve_color("#ff000080"),
            Some(RgbaColor::rgba(255, 0, 0, 128))
        );
        assert_eq!(
            resolve_color("#f0a"),
            Some(RgbaColor::rgb(0xff, 0x00, 0xaa))
        );
        assert_eq!(
            resolve_color("#1234"),
            Some(RgbaColor::rgba(0x11, 0x22, 0x33, 0x44))
        );
        assert_eq!(resolve_color("#add8e6").unwrap().to_svg_hex(), "#add8e6");
        assert_eq!(
            resolve_color("#ff000080").unwrap().to_svg_hex(),
            "#ff000080"
        );
        // Invalid 5-digit hex is rejected (fixes C sscanf bug)
        assert_eq!(resolve_color("#12345"), None);
    }

    #[test]
    fn test_hsv_colors() {
        // Pure red: h=0, s=1, v=1
        assert_eq!(
            resolve_color("0.0,1.0,1.0"),
            Some(RgbaColor::rgb(255, 0, 0))
        );
        // Pure green: h=1/3, s=1, v=1
        let green = resolve_color("0.333333, 1.0, 1.0").unwrap();
        assert_eq!(green.g, 255);
        assert!(green.r <= 1 && green.b == 0);
        // Space-separated HSVA
        let semi = resolve_color("0.0 0.0 0.5 0.5").unwrap();
        assert_eq!(semi, RgbaColor::rgba(127, 127, 127, 127));
    }
}
