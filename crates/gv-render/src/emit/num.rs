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
    if v.is_nan() {
        "nan".to_string()
    } else if v.is_infinite() {
        if v < 0.0 { "-inf" } else { "inf" }.to_string()
    } else {
        format!("{v:.prec$}")
    }
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
    if v.is_nan() {
        // macOS libc prints `nan` for either sign.
        return "nan".to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    let p = prec.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    // The exponent after rounding to `p` significant digits.
    let e_form = format!("{:.*e}", p - 1, v);
    let exp: i32 = e_form
        .rsplit_once('e')
        .and_then(|(_, e)| e.parse().ok())
        .unwrap_or(0);
    if exp < -4 || exp >= p as i32 {
        let (mant, _) = e_form.rsplit_once('e').unwrap_or((&e_form, "0"));
        let mant = trim_zeros(mant.to_string());
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mant}e{sign}{:02}", exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        trim_zeros(format!("{v:.decimals$}"))
    }
}

/// `%f` (six decimals).
pub(crate) fn fmt_f6(v: f64) -> String {
    format!("{v:.6}")
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
