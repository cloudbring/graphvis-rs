//! Byte-faithful strings and the graph `charset` (`lib/common/input.c`, `utils.c`).
//!
//! C works on the bytes of its input: attribute values, names and the text of
//! `-Tdot`/`-Tsvg`/... output are whatever bytes the file held, and only label text
//! is converted to UTF-8 (`latin1ToUTF8` for `charset=latin1`, `htmlEntityUTF8`
//! otherwise, whose invalid-UTF-8 recovery treats the offending byte as Latin-1).
//!
//! The port keeps strings as `String`. Input that is not valid UTF-8 is decoded
//! with [`decode`]: valid UTF-8 sequences stay as they are and every other byte
//! `b` becomes the private-use code point `U+10FF00 + b` (a "raw byte"). [`encode`]
//! maps them back, so the CLI writes C's exact bytes, and the C conversions run on
//! [`encode`]d bytes, so `charset=latin1` text is converted exactly once, where C
//! converts it. Text measurement (which C does byte by byte) also uses the encoded
//! bytes. Input that genuinely contains `U+10FF00..=U+10FFFF` is not distinguished
//! from raw bytes (no real-world encoding assigns these code points).

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::io::Write;

/// First code point of the raw-byte range (`U+10FF00 + byte`).
const RAW_BASE: u32 = 0x10_FF00;

/// Decodes input bytes: valid UTF-8 is kept, every other byte becomes a raw-byte char.
pub fn decode(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => decode_slice(e.as_bytes()),
    }
}

/// [`decode`] for a borrowed slice.
pub fn decode_slice(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let mut out = String::with_capacity(bytes.len() + 8);
    for chunk in bytes.utf8_chunks() {
        out.push_str(chunk.valid());
        for &b in chunk.invalid() {
            out.push(raw_char(b));
        }
    }
    out
}

/// The char that stands for the raw (non-UTF-8) byte `b`; [`encode`] turns it back.
pub fn raw_char(b: u8) -> char {
    char::from_u32(RAW_BASE + u32::from(b)).expect("raw byte code point")
}

/// Whether `s` holds raw-byte chars (non-UTF-8 input bytes).
pub fn has_raw(s: &str) -> bool {
    // U+10FFxx is encoded as F4 8F Bx xx.
    s.as_bytes().contains(&0xF4) && s.chars().any(|c| u32::from(c) >= RAW_BASE)
}

/// The bytes C would hold for `s`: raw-byte chars become their byte again.
pub fn encode(s: &str) -> Cow<'_, [u8]> {
    if !has_raw(s) {
        return Cow::Borrowed(s.as_bytes());
    }
    let mut out = Vec::with_capacity(s.len());
    let mut buf = [0u8; 4];
    for c in s.chars() {
        let v = u32::from(c);
        if v >= RAW_BASE {
            out.push((v - RAW_BASE) as u8);
        } else {
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }
    Cow::Owned(out)
}

/// Writes `msg` to stderr as C's bytes, as cgraph's `default_usererrf`
/// (`lib/cgraph/agerror.c`) does: up to the first NUL, with control characters other than
/// white space written as `\ooo`.
pub fn eprint_bytes(msg: &str) {
    let bytes = encode(msg);
    let bytes = bytes.split(|&b| b == 0).next().unwrap_or_default();
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        // gv_iscntrl && !gv_isspace
        if (b <= 31 || b == 127) && !matches!(b, b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
            out.extend_from_slice(format!("\\{b:03o}").as_bytes());
        } else {
            out.push(b);
        }
    }
    let _ = std::io::stderr().write_all(&out);
}

/// `GD_charset(g)`: the input character set of a graph (`CHAR_UTF8`, `CHAR_LATIN1`,
/// `CHAR_BIG5` in `lib/common/const.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Charset {
    /// `CHAR_UTF8` (the default, and the fallback for unknown names).
    #[default]
    Utf8,
    /// `CHAR_LATIN1`.
    Latin1,
    /// `CHAR_BIG5`: labels are converted like UTF-8, but `make_simple_label` keeps the
    /// byte after a Big5 lead byte (`0xA1..=0xFE`) with it, and HTML labels fail
    /// (expat has no `BIG-5` decoder).
    Big5,
}

impl Charset {
    /// `findCharset` from `lib/common/input.c`. The second value is `false` when the name
    /// is not recognised (C warns and assumes UTF-8).
    pub fn parse(charset: Option<&str>) -> (Self, bool) {
        let p = charset.unwrap_or("utf-8");
        let is = |names: &[&str]| names.iter().any(|n| n.eq_ignore_ascii_case(p));
        if is(&[
            "latin-1",
            "latin1",
            "l1",
            "ISO-8859-1",
            "ISO_8859-1",
            "ISO8859-1",
            "ISO-IR-100",
        ]) {
            (Self::Latin1, true)
        } else if is(&["big-5", "big5"]) {
            (Self::Big5, true)
        } else if is(&["utf-8", "utf8"]) {
            (Self::Utf8, true)
        } else {
            (Self::Utf8, false)
        }
    }

    /// [`Self::parse`] without the validity flag.
    pub fn from_attr(charset: Option<&str>) -> Self {
        Self::parse(charset).0
    }
}

thread_local! {
    static CURRENT: Cell<Charset> = const { Cell::new(Charset::Utf8) };
    static GRAPH: RefCell<String> = const { RefCell::new(String::new()) };
    static WARNED: Cell<bool> = const { Cell::new(false) };
}

/// `graph_init` for charset purposes: records the root graph's charset and name for the
/// label conversions that follow, clears `htmlEntityUTF8`'s once-per-graph warning flag
/// and warns about an unsupported charset name.
pub fn begin_layout(graph_name: &str, charset: Option<&str>) {
    let (cs, ok) = Charset::parse(charset);
    if !ok {
        eprint_bytes(&format!(
            "Warning: Unsupported charset \"{}\" - assuming utf-8\n",
            charset.unwrap_or("")
        ));
    }
    CURRENT.with(|c| c.set(cs));
    WARNED.with(|w| w.set(false));
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        g.clear();
        g.push_str(graph_name);
    });
}

/// Sets the charset of the graph being rendered (no warnings; the layout already gave them).
pub fn begin_render(graph_name: &str, charset: Option<&str>) {
    let cs = Charset::from_attr(charset);
    CURRENT.with(|c| c.set(cs));
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        if *g != graph_name {
            g.clear();
            g.push_str(graph_name);
            WARNED.with(|w| w.set(false));
        }
    });
}

/// Charset of the graph being laid out or rendered on this thread.
pub fn current() -> Charset {
    CURRENT.with(Cell::get)
}

/// `htmlEntityUTF8`'s warning: printed at most once per graph.
pub(crate) fn warn_invalid_utf8(msg: impl FnOnce(&str) -> String) {
    if WARNED.with(|w| w.replace(true)) {
        return;
    }
    let text = GRAPH.with(|g| msg(&g.borrow()));
    eprint_bytes(&text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let bytes = b"caf\xe9 \xc3\xa9 \xe4\xbcz \xff".to_vec();
        let s = decode(bytes.clone());
        assert!(s.contains('\u{e9}'));
        assert!(has_raw(&s));
        assert_eq!(&*encode(&s), &bytes[..]);
        assert_eq!(decode(b"plain".to_vec()), "plain");
        assert!(matches!(encode("plain \u{10ffff}x"), Cow::Owned(_)));
    }

    #[test]
    fn find_charset() {
        assert_eq!(Charset::parse(Some("ISO-8859-1")), (Charset::Latin1, true));
        assert_eq!(Charset::parse(Some("BIG5")), (Charset::Big5, true));
        assert_eq!(Charset::parse(None), (Charset::Utf8, true));
        assert_eq!(Charset::parse(Some("koi8-r")), (Charset::Utf8, false));
    }
}
