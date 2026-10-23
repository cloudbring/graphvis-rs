//! Runs C Graphviz with the port's math functions: the C reference of the corpus checks, so that
//! the port and C compute with the same bits and every remaining difference is the port's.
//!
//! Built as a dylib and loaded into C `dot` with `DYLD_INSERT_LIBRARIES` (see
//! `scripts/dot-portable-libm.sh`), it replaces every transcendental function Graphviz imports
//! from macOS libm with the one `graphviz_pure_core::math` uses on every target: `hypot` and
//! `pow` from `graphviz_pure_core::math::portable`, the rest from the `libm` crate. C then computes with the same bits as the portable port, and
//! `gv-diff e2e --golden tests/golden/e2e.libm-matched.json` compares the two.
//!
//! The list is the libm symbols that Graphviz 16.1.0's libraries and plugins import
//! (`nm -u`), including clang's combined `__sincos_stret`. `sqrt`, `fma`, `floor`, `fmod`
//! and the like are exact in IEEE arithmetic and are left alone.
//!
//! dyld's interposition (`__DATA,__interpose`) redirects calls from every other image in the
//! process; calls inside this dylib still reach the originals.
//!
//! On Linux the library is used with `LD_PRELOAD` instead: there it exports the libm names
//! themselves (`sin`, ..., and glibc's combined `sincos`), which the dynamic linker then
//! resolves ahead of glibc's (`scripts/c-oracle-linux/`). With the `apple-libc` feature it also
//! exports `qsort`, `rand` and `srand` as macOS libc behaves, which Graphviz's layouts depend on
//! as well (the order `qsort` leaves equal elements in, and the random sequence).

#[cfg(target_vendor = "apple")]
use graphviz_pure_core::math::portable;

#[cfg(all(target_os = "linux", feature = "apple-libc"))]
mod libc;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_vendor = "apple")]
#[repr(C)]
pub struct SinCos {
    sin: f64,
    cos: f64,
}

#[cfg(target_vendor = "apple")]
macro_rules! unary {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: f64) -> f64 {
            $f(x)
        }
    )*};
}

#[cfg(target_vendor = "apple")]
macro_rules! binary {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: f64, y: f64) -> f64 {
            $f(x, y)
        }
    )*};
}

#[cfg(target_vendor = "apple")]
unary! {
    gv_portable_sin => portable::sin,
    gv_portable_cos => portable::cos,
    gv_portable_tan => portable::tan,
    gv_portable_asin => portable::asin,
    gv_portable_acos => portable::acos,
    gv_portable_atan => libm::atan,
    gv_portable_exp => portable::exp,
    gv_portable_log => portable::log,
    gv_portable_log2 => portable::log2,
    gv_portable_log10 => libm::log10,
    gv_portable_cbrt => portable::cbrt,
}

#[cfg(target_vendor = "apple")]
binary! {
    gv_portable_hypot => portable::hypot,
    gv_portable_pow => portable::pow,
    gv_portable_atan2 => portable::atan2,
}

#[cfg(target_vendor = "apple")]
#[unsafe(no_mangle)]
pub extern "C" fn gv_portable_sincos_stret(x: f64) -> SinCos {
    SinCos {
        sin: portable::sin(x),
        cos: portable::cos(x),
    }
}

#[cfg(target_vendor = "apple")]
unsafe extern "C" {
    fn sin(x: f64) -> f64;
    fn cos(x: f64) -> f64;
    fn tan(x: f64) -> f64;
    fn asin(x: f64) -> f64;
    fn acos(x: f64) -> f64;
    fn atan(x: f64) -> f64;
    fn exp(x: f64) -> f64;
    fn log(x: f64) -> f64;
    fn log2(x: f64) -> f64;
    fn log10(x: f64) -> f64;
    fn cbrt(x: f64) -> f64;
    fn hypot(x: f64, y: f64) -> f64;
    fn pow(x: f64, y: f64) -> f64;
    fn atan2(y: f64, x: f64) -> f64;
    fn __sincos_stret(x: f64) -> SinCos;
}

#[cfg(target_vendor = "apple")]
/// One dyld interpose tuple: calls to `replacee` from other images go to `replacement`.
#[repr(C)]
pub struct Interpose {
    replacement: *const (),
    replacee: *const (),
}

#[cfg(target_vendor = "apple")]
// SAFETY: the tuples are immutable function addresses, read only by dyld.
unsafe impl Sync for Interpose {}

#[cfg(target_vendor = "apple")]
macro_rules! interpose {
    ($($new:ident => $old:ident),* $(,)?) => {
        #[used]
        #[unsafe(link_section = "__DATA,__interpose")]
        static INTERPOSE: [Interpose; [$(stringify!($old)),*].len()] = [$(
            Interpose { replacement: $new as *const (), replacee: $old as *const () },
        )*];
    };
}

#[cfg(target_vendor = "apple")]
interpose! {
    gv_portable_sin => sin,
    gv_portable_cos => cos,
    gv_portable_tan => tan,
    gv_portable_asin => asin,
    gv_portable_acos => acos,
    gv_portable_atan => atan,
    gv_portable_exp => exp,
    gv_portable_log => log,
    gv_portable_log2 => log2,
    gv_portable_log10 => log10,
    gv_portable_cbrt => cbrt,
    gv_portable_hypot => hypot,
    gv_portable_pow => pow,
    gv_portable_atan2 => atan2,
    gv_portable_sincos_stret => __sincos_stret,
}
