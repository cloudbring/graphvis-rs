//! Pure-Rust implementations used off macOS (and with the `portable-math` feature).
//!
//! `hypot` and `cbrt` reproduce macOS arm64 libm bit for bit. Apple's `pow`, `sin`, `cos`,
//! `atan2`, `tan`, `asin`, `acos`, `exp`, `log` and `log2` are closed-source assembly that no
//! published algorithm reproduces, so these come from the `libm` crate: the same musl-derived
//! code Rust's own wasm32 `f64` methods use, pinned by `Cargo.lock` rather than by the
//! toolchain, and identical on every target (unlike glibc, whose results depend on the CPU).

mod cbrt;
mod hypot;

pub use cbrt::cbrt;
pub use hypot::hypot;
pub use libm::{acos, asin, atan2, cos, exp, log, log2, pow, sin, tan};
