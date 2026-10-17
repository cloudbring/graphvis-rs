//! Pure-Rust implementations used off macOS (and with the `portable-math` feature).
//!
//! `hypot` reproduces macOS arm64 libm bit for bit (an original formula found by black-box
//! comparison). Everything else comes from the `libm` crate (MIT, a port of musl; its `cbrt`
//! is correctly rounded, from CORE-MATH): the same code Rust's own wasm32 `f64` methods use,
//! pinned by `Cargo.lock` rather than by the toolchain, and identical on every target (unlike
//! glibc, whose results depend on the CPU). Apple's `pow`, `sin`, `cos`, `atan2`, ... are
//! closed-source assembly that no published algorithm reproduces. See `docs/porting/libm.md`.

mod hypot;

pub use hypot::hypot;
pub use libm::{acos, asin, atan2, cbrt, cos, exp, log, log2, pow, sin, tan};
