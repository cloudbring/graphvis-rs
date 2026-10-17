//! Transcendental functions, called as `math::hypot(x, y)` the way the C sources call
//! `hypot(x, y)`.
//!
//! On macOS, `f64::hypot` and friends call the system libm, the same one C Graphviz uses, so
//! the port reproduces C's last bits. On other targets Rust takes them from a different libm
//! (glibc on Linux, compiler-builtins' musl port on wasm32) that disagrees in the last ulp on
//! some inputs, which is enough to change a force-directed layout. Every call in the port goes
//! through this module so that those targets can use implementations that reproduce Apple's
//! results instead. See `docs/porting/libm.md`.

#[cfg(feature = "math-trace")]
mod trace;
#[cfg(not(feature = "math-trace"))]
mod trace {
    #[inline(always)]
    pub fn log(_name: u8, _x: f64, _y: f64) {}
}

macro_rules! unary {
    ($($(#[$doc:meta])* $id:literal $name:ident;)*) => {$(
        $(#[$doc])*
        #[inline]
        #[must_use]
        pub fn $name(x: f64) -> f64 {
            trace::log($id, x, 0.0);
            x.$name()
        }
    )*};
}

macro_rules! binary {
    ($($(#[$doc:meta])* $id:literal $name:ident;)*) => {$(
        $(#[$doc])*
        #[inline]
        #[must_use]
        pub fn $name(x: f64, y: f64) -> f64 {
            trace::log($id, x, y);
            x.$name(y)
        }
    )*};
}

binary! {
    /// C `hypot(x, y)`.
    0 hypot;
    /// C `pow(x, y)`.
    1 powf;
    /// C `atan2(y, x)` is `math::atan2(y, x)`.
    2 atan2;
}

unary! {
    /// C `sin(x)`.
    3 sin;
    /// C `cos(x)`.
    4 cos;
    /// C `tan(x)`.
    5 tan;
    /// C `asin(x)`.
    6 asin;
    /// C `acos(x)`.
    7 acos;
    /// C `exp(x)`.
    8 exp;
    /// C `log(x)`.
    9 ln;
    /// C `log2(x)`.
    10 log2;
    /// C `cbrt(x)`.
    11 cbrt;
}
