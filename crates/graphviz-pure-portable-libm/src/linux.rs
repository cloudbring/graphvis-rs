//! The libm names, exported for `LD_PRELOAD` on Linux.

use graphviz_pure_core::math::portable;

macro_rules! unary {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: f64) -> f64 {
            $f(x)
        }
    )*};
}

macro_rules! binary {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: f64, y: f64) -> f64 {
            $f(x, y)
        }
    )*};
}

unary! {
    sin => portable::sin,
    cos => portable::cos,
    tan => portable::tan,
    asin => portable::asin,
    acos => portable::acos,
    atan => libm::atan,
    exp => portable::exp,
    log => portable::log,
    log2 => portable::log2,
    log10 => libm::log10,
    cbrt => portable::cbrt,
}

binary! {
    hypot => portable::hypot,
    pow => portable::pow,
    atan2 => portable::atan2,
}

/// glibc's `sincos`, which compilers emit for adjacent `sin` and `cos` calls.
///
/// # Safety
/// `s` and `c` must be valid for writes, as C requires.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sincos(x: f64, s: *mut f64, c: *mut f64) {
    // SAFETY: the caller passes valid pointers, as for glibc's sincos.
    unsafe {
        *s = portable::sin(x);
        *c = portable::cos(x);
    }
}
