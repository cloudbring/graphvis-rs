//! C number formatting used by the renderers: `gvprintdouble`, `gvprintnum`, `%.Ng`,
//! `agxbuf_trim_zeros`.

/// Strips trailing zeros after a decimal point, and the point itself (`agxbuf_trim_zeros`).
pub(crate) fn trim_zeros(mut s: String) -> String {
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        // agxbuf_trim_zeros turns a remaining "-0" into "0"
        if s == "-0" {
            s = "0".to_string();
        }
    }
    s
}

/// C `printf("%.{prec}f", v)` on the reference platform (macOS libc): non-finite values
/// print as `nan` (whatever the sign), `inf` and `-inf`.
pub(crate) fn c_fixed(v: f64, prec: usize) -> String {
    graphviz_pure_core::cnum::fmt_f(v, prec)
}

/// `gvprintdouble`: `%.02f` with trailing zeros removed; values within 0.005 of zero print `0`.
pub(crate) fn gvdouble(num: f64) -> String {
    if num > -0.005 && num < 0.005 {
        return "0".to_string();
    }
    trim_zeros(c_fixed(num, 2))
}

/// `%.02f` then `agxbuf_trim_zeros` (xdot numbers; `-0` is possible).
pub(crate) fn xdot_num(v: f64) -> String {
    trim_zeros(c_fixed(v, 2))
}

/// `gvprintnum`: `%.03f`, trailing zeros and a leading `0` removed.
#[allow(dead_code)]
pub(crate) fn gvprintnum(number: f64) -> String {
    const MAXNEG: f64 = -999_999_999_999_999.99;
    if number < MAXNEG {
        return "-999999999999999.99".to_string();
    }
    if number > -MAXNEG {
        return "999999999999999.99".to_string();
    }
    let s = trim_zeros(c_fixed(number, 3));
    if let Some(rest) = s.strip_prefix("0.") {
        format!(".{rest}")
    } else if let Some(rest) = s.strip_prefix("-0.") {
        format!("-.{rest}")
    } else {
        s
    }
}

/// C `printf("%.{prec}g", v)`.
pub(crate) fn fmt_g(v: f64, prec: usize) -> String {
    graphviz_pure_core::diag::fmt_g(v, prec)
}

/// `%f` (six decimals).
pub(crate) fn fmt_f6(v: f64) -> String {
    c_fixed(v, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format_matches_printf() {
        assert_eq!(fmt_g(205.8, 5), "205.8");
        assert_eq!(fmt_g(0.75, 5), "0.75");
        assert_eq!(fmt_g(123456.0, 5), "1.2346e+05");
        assert_eq!(fmt_g(0.0001234, 5), "0.0001234");
        assert_eq!(fmt_g(0.00001234, 5), "1.234e-05");
        assert_eq!(fmt_g(99999.5, 5), "1e+05");
        assert_eq!(fmt_g(1.0, 5), "1");
        assert_eq!(fmt_g(-2.5, 5), "-2.5");
        assert_eq!(fmt_g(1.0, 6), "1");
        assert_eq!(fmt_g(0.333333333, 6), "0.333333");
    }

    #[test]
    fn printdouble_and_xdot() {
        assert_eq!(gvdouble(-0.004), "0");
        assert_eq!(gvdouble(12.5), "12.5");
        assert_eq!(gvdouble(-209.8), "-209.8");
        assert_eq!(xdot_num(-0.001), "0");
        assert_eq!(xdot_num(20.0), "20");
        assert_eq!(gvprintnum(0.5), ".5");
    }
}
