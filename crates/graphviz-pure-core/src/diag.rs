//! Diagnostics as `lib/cgraph/agerror.c` reports them: `agerrorf` prints `Error: ...` and
//! raises the error level that `dot`'s `main` returns as its exit status
//! (`graphviz_errors + agerrors()`), while the layout and the output go on. `agwarningf`
//! prints `Warning: ...` and does not change the status.
//!
//! The level is per thread, so concurrent in-process renders do not see each other's errors.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

thread_local! {
    static ERRORS: Cell<usize> = const { Cell::new(0) };
    static EMITTED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static QUIET: Cell<u32> = const { Cell::new(0) };
    static SILENT: Cell<u32> = const { Cell::new(0) };
}

/// Increments `depth` for the life of the returned guard.
fn enter(depth: &'static std::thread::LocalKey<Cell<u32>>) -> impl Drop {
    struct Guard(&'static std::thread::LocalKey<Cell<u32>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.with(|q| q.set(q.get() - 1));
        }
    }
    depth.with(|q| q.set(q.get() + 1));
    Guard(depth)
}

/// Runs `f` as a repeat of work C does once. The port sizes labels and nodes again after
/// C's `common_init_node`/`common_init_edge`/`do_graph_label` (for ports, splines and
/// rendering), and the diagnostics of those calls (HTML label warnings and errors,
/// `bad label format`, `size too small for label`) must not print a second time.
///
/// Port warnings still print: C resolves ports again in some of these places and warns again.
pub fn quietly<R>(f: impl FnOnce() -> R) -> R {
    let _guard = enter(&QUIET);
    f()
}

/// [`quietly`], and port warnings too, for a repeat that C does not make at all.
pub fn silently<R>(f: impl FnOnce() -> R) -> R {
    let _guard = enter(&SILENT);
    f()
}

/// False inside [`quietly`] and [`silently`]: label and node sizing diagnostics are reported
/// only outside them.
pub fn reporting() -> bool {
    QUIET.with(|q| q.get() == 0) && reporting_ports()
}

/// False inside [`silently`]: port warnings are reported only outside it.
pub fn reporting_ports() -> bool {
    SILENT.with(|q| q.get() == 0)
}

/// `emit_once(str)`: true the first time `key` is seen on this thread.
pub fn emit_once(key: &str) -> bool {
    EMITTED.with(|e| e.borrow_mut().insert(key.to_string()))
}

/// `agerrorf(msg)`: `msg` is the C format output, normally ending in a newline.
pub fn agerrorf(msg: &str) {
    crate::charset::eprint_bytes(&format!("Error: {msg}"));
    ERRORS.with(|c| c.set(c.get() + 1));
}

/// `agwarningf(msg)`.
pub fn agwarningf(msg: &str) {
    crate::charset::eprint_bytes(&format!("Warning: {msg}"));
}

/// `agerr(AGPREV, msg)`: continues the previous message.
pub fn agerr_prev(msg: &str) {
    crate::charset::eprint_bytes(msg);
}

/// C `printf("%.{prec}g", v)` on the reference platform (macOS libc), as diagnostics and
/// renderers print numbers.
pub fn fmt_g(v: f64, prec: usize) -> String {
    fn trim(mut s: String) -> String {
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    }
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
        let mant = trim(mant.to_string());
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mant}e{sign}{:02}", exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        trim(format!("{v:.decimals$}"))
    }
}

/// Number of errors reported on this thread since the last [`reset_errors`].
pub fn error_count() -> usize {
    ERRORS.with(Cell::get)
}

/// Clears the error count and the once-only messages of this thread.
pub fn reset_errors() {
    ERRORS.with(|c| c.set(0));
    EMITTED.with(|e| e.borrow_mut().clear());
}
