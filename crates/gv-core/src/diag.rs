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
}

/// `emit_once(str)`: true the first time `key` is seen on this thread.
pub fn emit_once(key: &str) -> bool {
    EMITTED.with(|e| e.borrow_mut().insert(key.to_string()))
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

/// Clears the error count and the once-only messages of this thread.
pub fn reset_errors() {
    ERRORS.with(|c| c.set(0));
    EMITTED.with(|e| e.borrow_mut().clear());
}
