//! The C library's number scanning, as Graphviz reaches it through `strtod`, `atof` and
//! `sscanf("%lf…")` (C17 7.22.1.3 and 7.21.6.2, with the choices of the macOS libc that
//! the reference build links).
//!
//! The grammar, after leading white space (`isspace` in the C locale) and an optional sign:
//! - a decimal number: digits with an optional `.`, at least one digit, then an optional
//!   exponent `e`/`E` with an optional sign and at least one digit;
//! - a hexadecimal float: `0x`/`0X`, hex digits with an optional `.`, at least one digit,
//!   then an optional binary exponent `p`/`P` with an optional sign and at least one
//!   decimal digit;
//! - `inf` or `infinity`, `nan` or `nan(…)`, in any case.
//!
//! The longest prefix that has this form is converted, correctly rounded to nearest even;
//! values beyond the range give ±infinity and values below it ±0 or a subnormal. A partial
//! form gives its longest valid prefix: `1e+` reads `1`, `0x` reads `0`, `infin` reads
//! `inf`.
//!
//! The two entry points differ only in `nan(…)`: `strtod` takes every character up to the
//! first `)`, while `%lf` takes only letters, digits and `_` before it (FreeBSD's
//! `parsefloat`); otherwise both read just `nan`. The NaN payload is not kept.

/// C `strtod(s, &end)`: the value and the number of bytes read (`end - s`), or `(0.0, 0)`
/// when no conversion can be performed.
pub fn strtod(s: &str) -> (f64, usize) {
    scan(s.as_bytes(), NanParens::UpToClose).map_or((0.0, 0), |(v, n)| (v.to_f64(), n))
}

/// C `strtod(s, &end)` for callers that test `end == s`: the value and the rest of `s`, or
/// `None` when no conversion can be performed.
pub fn strtod_rest(s: &str) -> Option<(f64, &str)> {
    scan(s.as_bytes(), NanParens::UpToClose).map(|(v, n)| (v.to_f64(), &s[n..]))
}

/// C `atof(s)`: `strtod(s, NULL)`.
pub fn atof(s: &str) -> f64 {
    strtod(s).0
}

/// One `sscanf` `%lf` conversion at the start of `s`: the value and the rest of `s`, or
/// `None` for a matching or input failure.
pub fn scan_lf(s: &str) -> Option<(f64, &str)> {
    scan(s.as_bytes(), NanParens::NChars).map(|(v, n)| (v.to_f64(), &s[n..]))
}

/// One `sscanf` `%f` conversion (into a `float`) at the start of `s`, like [`scan_lf`]
/// but rounded once, to single precision.
pub fn scan_f(s: &str) -> Option<(f32, &str)> {
    scan(s.as_bytes(), NanParens::NChars).map(|(v, n)| (v.to_f32(), &s[n..]))
}

/// The result of [`sscanf`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scanned<'a> {
    /// The return value of `sscanf`: the number of conversions assigned, or `-1` (`EOF`)
    /// when the input ended before the first conversion.
    pub count: i32,
    /// The `%lf` values, in order (unassigned ones stay 0).
    pub values: [f64; 5],
    /// The `%c` byte, if one was assigned.
    pub c: Option<u8>,
    /// The `%[^…]` string, if one was assigned.
    pub set: Option<&'a str>,
    /// What follows the last directive that succeeded (where `%n` would point).
    pub rest: &'a str,
}

/// C `sscanf(s, fmt, …)` for formats made of `%lf` (at most five), `%c`, one `%[^…]` with
/// ASCII characters in its set, white space and literal characters, such as
/// `"%lf,%lf,%lf%c"`.
///
/// # Panics
/// On another conversion, or a sixth `%lf`: those are mistakes in the format, which is
/// always a literal in the caller.
pub fn sscanf<'a>(s: &'a str, fmt: &str) -> Scanned<'a> {
    let b = s.as_bytes();
    let f = fmt.as_bytes();
    let mut out = Scanned {
        count: 0,
        values: [0.0; 5],
        c: None,
        set: None,
        rest: s,
    };
    let (mut i, mut k, mut nf) = (0, 0, 0);
    // Whether an input failure came before the first conversion: sscanf returns EOF.
    let mut eof = false;
    while k < f.len() {
        if is_space(f[k]) {
            while k < f.len() && is_space(f[k]) {
                k += 1;
            }
            while i < b.len() && is_space(b[i]) {
                i += 1;
            }
            continue;
        }
        if f[k] != b'%' {
            if i >= b.len() {
                eof = true;
                break;
            }
            if b[i] != f[k] {
                break;
            }
            i += 1;
            k += 1;
            continue;
        }
        let spec = &f[k + 1..];
        if spec.starts_with(b"lf") {
            k += 3;
            let t = skip_space(b, i);
            if t >= b.len() {
                eof = true;
                break;
            }
            let Some((v, n)) = scan(&b[t..], NanParens::NChars) else {
                break;
            };
            assert!(
                nf < out.values.len(),
                "sscanf: more than five %lf in {fmt:?}"
            );
            out.values[nf] = v.to_f64();
            nf += 1;
            i = t + n;
        } else if spec.starts_with(b"c") {
            k += 2;
            if i >= b.len() {
                eof = true;
                break;
            }
            out.c = Some(b[i]);
            i += 1;
        } else if spec.starts_with(b"[^") {
            let close = spec[2..]
                .iter()
                .position(|&c| c == b']')
                .unwrap_or_else(|| panic!("sscanf: unterminated %[ in {fmt:?}"));
            let excluded = &spec[2..2 + close];
            k += 4 + close;
            if i >= b.len() {
                eof = true;
                break;
            }
            let n = b[i..].iter().take_while(|c| !excluded.contains(c)).count();
            if n == 0 {
                break;
            }
            // The set is ASCII, so the run ends on a character boundary.
            out.set = s.get(i..i + n);
            i += n;
        } else {
            panic!("sscanf: unsupported conversion in {fmt:?}");
        }
        out.count += 1;
        out.rest = s.get(i..).unwrap_or("");
    }
    if eof && out.count == 0 {
        out.count = -1;
    }
    out
}

/// Which characters `nan(…)` may hold.
#[derive(Clone, Copy, PartialEq)]
enum NanParens {
    /// `strtod`: anything up to the first `)`.
    UpToClose,
    /// `%lf`: letters, digits and `_`.
    NChars,
}

/// `isspace` in the C locale.
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn skip_space(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && is_space(b[i]) {
        i += 1;
    }
    i
}

/// Whether `b[i..]` starts with the lower-case ASCII word `w`, in any case.
fn starts_with_ci(b: &[u8], i: usize, w: &[u8]) -> bool {
    b.len() >= i + w.len() && b[i..i + w.len()].eq_ignore_ascii_case(w)
}

/// A number read by [`scan`], before rounding to a type.
enum Num<'a> {
    /// Decimal text, sign included: a valid Rust float literal (`1.`, `.5` and `+1` all
    /// parse), and Rust's conversion is correctly rounded like the C library's.
    Decimal(&'a str),
    /// `±m · 2^exp`, with `sticky` set when non-zero bits follow `m`'s last bit.
    Hex {
        neg: bool,
        m: u64,
        exp: i64,
        sticky: bool,
    },
    /// `±0x` with no hex digit, which reads as `±0`.
    Zero(bool),
    Inf(bool),
    Nan(bool),
}

impl Num<'_> {
    fn to_f64(&self) -> f64 {
        match *self {
            Num::Decimal(t) => t.parse().unwrap_or(0.0),
            Num::Hex {
                neg,
                m,
                exp,
                sticky,
            } => signed(neg, compose(m, exp, sticky, F64)),
            Num::Zero(neg) => signed(neg, 0.0),
            Num::Inf(neg) => signed(neg, f64::INFINITY),
            Num::Nan(neg) => signed(neg, f64::NAN),
        }
    }

    fn to_f32(&self) -> f32 {
        match *self {
            Num::Decimal(t) => t.parse().unwrap_or(0.0),
            // Rounded to single precision by `compose`, so the conversion is exact.
            Num::Hex {
                neg,
                m,
                exp,
                sticky,
            } => signed(neg, compose(m, exp, sticky, F32)) as f32,
            _ => self.to_f64() as f32,
        }
    }
}

fn signed(neg: bool, v: f64) -> f64 {
    if neg { -v } else { v }
}

/// The longest number at the start of `b`, and its length.
fn scan(b: &[u8], nan: NanParens) -> Option<(Num<'_>, usize)> {
    let start = skip_space(b, 0);
    let mut i = start;
    let neg = match b.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    if starts_with_ci(b, i, b"inf") {
        let n = if starts_with_ci(b, i, b"infinity") {
            8
        } else {
            3
        };
        return Some((Num::Inf(neg), i + n));
    }
    if starts_with_ci(b, i, b"nan") {
        let mut end = i + 3;
        if b.get(end) == Some(&b'(') {
            let inner = &b[end + 1..];
            let close = match nan {
                NanParens::UpToClose => inner.iter().position(|&c| c == b')'),
                NanParens::NChars => inner
                    .iter()
                    .position(|&c| !(c.is_ascii_alphanumeric() || c == b'_'))
                    .filter(|&p| inner[p] == b')'),
            };
            if let Some(p) = close {
                end += p + 2;
            }
        }
        return Some((Num::Nan(neg), end));
    }
    if b.get(i) == Some(&b'0') && matches!(b.get(i + 1), Some(b'x' | b'X')) {
        return Some(match hex(b, i + 2) {
            Some((m, exp, sticky, end)) => (
                Num::Hex {
                    neg,
                    m,
                    exp,
                    sticky,
                },
                end,
            ),
            None => (Num::Zero(neg), i + 1),
        });
    }
    // Decimal: digits, an optional point and digits, at least one digit in all.
    let mut j = i;
    let int_start = j;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    let mut digits = j - int_start;
    if b.get(j) == Some(&b'.') {
        let frac_start = j + 1;
        let mut e = frac_start;
        while e < b.len() && b[e].is_ascii_digit() {
            e += 1;
        }
        digits += e - frac_start;
        j = e;
    }
    if digits == 0 {
        return None;
    }
    let end = exponent_end(b, j, b'e');
    let text = std::str::from_utf8(&b[start..end]).ok()?;
    Some((Num::Decimal(text), end))
}

/// The end of an optional exponent at `b[j..]` introduced by `marker` (either case): an
/// optional sign and at least one decimal digit, or `j` when there is none.
fn exponent_end(b: &[u8], j: usize, marker: u8) -> usize {
    if !b.get(j).is_some_and(|c| c.eq_ignore_ascii_case(&marker)) {
        return j;
    }
    let mut e = j + 1;
    if matches!(b.get(e), Some(b'+' | b'-')) {
        e += 1;
    }
    let digits = e;
    while e < b.len() && b[e].is_ascii_digit() {
        e += 1;
    }
    if e > digits { e } else { j }
}

/// A hexadecimal float whose digits start at `b[i]` (just after `0x`): `m`, `exp` and
/// `sticky` of its magnitude ([`Num::Hex`]) and its end, or `None` when no hex digit follows
/// (the caller then reads the `0` alone).
fn hex(b: &[u8], mut i: usize) -> Option<(u64, i64, bool, usize)> {
    // The first 16 significant digits go in `m`; later ones only matter as a sticky bit.
    let mut m: u64 = 0;
    let mut sig = 0;
    let mut sticky = false;
    // Binary exponent of `m`'s last bit.
    let mut exp: i64 = 0;
    let mut digits = 0;
    let mut after_point = false;
    loop {
        let c = b.get(i).copied();
        if c == Some(b'.') && !after_point {
            after_point = true;
            i += 1;
            continue;
        }
        let Some(d) = c.and_then(|c| (c as char).to_digit(16)) else {
            break;
        };
        digits += 1;
        i += 1;
        if sig < 16 {
            if m != 0 || d != 0 {
                m = (m << 4) | u64::from(d);
                sig += 1;
            }
            if after_point {
                exp -= 4;
            }
        } else {
            sticky |= d != 0;
            if !after_point {
                exp += 4;
            }
        }
    }
    if digits == 0 {
        return None;
    }
    let end = exponent_end(b, i, b'p');
    if end > i {
        let mut e = i + 1;
        let neg = b[e] == b'-';
        if matches!(b[e], b'+' | b'-') {
            e += 1;
        }
        // Saturate: past about 2^20 the result is 0 or infinity whatever the digits.
        let mut p: i64 = 0;
        for &c in &b[e..end] {
            p = (p * 10 + i64::from(c - b'0')).min(1 << 40);
        }
        exp += if neg { -p } else { p };
    }
    Some((m, exp, sticky, end))
}

/// A binary floating-point format: significand bits and the exponent of the smallest
/// subnormal.
struct Format {
    bits: i64,
    min_exp: i64,
}

const F64: Format = Format {
    bits: 53,
    min_exp: -1074,
};
const F32: Format = Format {
    bits: 24,
    min_exp: -149,
};

/// `m · 2^exp` rounded to the nearest value of format `f` (ties to even), as a double;
/// `sticky` says that non-zero bits follow `m`'s last bit.
fn compose(m: u64, exp: i64, sticky: bool, f: Format) -> f64 {
    if m == 0 {
        return 0.0;
    }
    let len = i64::from(64 - m.leading_zeros());
    // Exponent of the leading bit, and how many bits the format can keep from there:
    // all of them for a normal number, fewer for a subnormal one.
    let top = exp + len - 1;
    let keep = f.bits.min(top - f.min_exp + 1);
    let shift = len - keep;
    let (q, scale) = if shift <= 0 {
        (m, exp)
    } else if shift > 64 {
        return 0.0;
    } else {
        let shift = shift as u32;
        let q = m.checked_shr(shift).unwrap_or(0);
        let rem = if shift == 64 {
            m
        } else {
            m & ((1u64 << shift) - 1)
        };
        let half = 1u64 << (shift - 1);
        let up = rem > half || (rem == half && (sticky || q & 1 == 1));
        (q + u64::from(up), exp + i64::from(shift))
    };
    // `q` has at most 54 bits and `q · 2^scale` is representable as a double (or
    // overflows), so the scaling below is exact.
    ldexp(q as f64, scale)
}

/// `v · 2^e`, exact when the result is representable.
fn ldexp(mut v: f64, e: i64) -> f64 {
    let mut e = e.clamp(-4000, 4000);
    while e > 0 {
        let k = e.min(1000);
        v *= f64::from_bits(((1023 + k) as u64) << 52);
        e -= k;
    }
    while e < 0 {
        let k = (-e).min(1000);
        v *= f64::from_bits(((1023 - k) as u64) << 52);
        e += k;
    }
    v
}

#[cfg(test)]
#[path = "strtod_tests.rs"]
mod tests;
