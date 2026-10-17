//! Diagnostics as `lib/cgraph/agerror.c` reports them: `agerrorf` prints `Error: ...` and
//! raises the error level that `dot`'s `main` returns as its exit status
//! (`graphviz_errors + agerrors()`), while the layout and the output go on. `agwarningf`
//! prints `Warning: ...` and does not change the status.
//!
//! The level is per thread, so concurrent in-process renders do not see each other's errors.

use std::cell::Cell;

thread_local! {
    static ERRORS: Cell<usize> = const { Cell::new(0) };
}

/// `agerrorf(msg)`: `msg` is the C format output, normally ending in a newline.
pub fn agerrorf(msg: &str) {
    eprint!("Error: {msg}");
    ERRORS.with(|c| c.set(c.get() + 1));
}

/// `agwarningf(msg)`.
pub fn agwarningf(msg: &str) {
    eprint!("Warning: {msg}");
}

/// `agerr(AGPREV, msg)`: continues the previous message.
pub fn agerr_prev(msg: &str) {
    eprint!("{msg}");
}

/// Number of errors reported on this thread since the last [`reset_errors`].
pub fn error_count() -> usize {
    ERRORS.with(Cell::get)
}

/// Clears the error count of this thread.
pub fn reset_errors() {
    ERRORS.with(|c| c.set(0));
}
