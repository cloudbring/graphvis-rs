//! Transcendental functions, called as `math::hypot(x, y)` the way the C sources call
//! `hypot(x, y)`.
//!
//! On macOS, `f64::hypot` and friends call the system libm, the same one C Graphviz uses, so
//! the port reproduces C's last bits. On other targets Rust takes them from a different libm
//! (glibc on Linux, compiler-builtins' musl port on wasm32) that disagrees in the last ulp on
//! some inputs, which is enough to change a force-directed layout. Every call in the port goes
//! through this module, and on those targets it uses [`portable`] implementations: ports that
//! reproduce Apple's results where Apple's algorithm is known (`hypot`, `cbrt`), and the `libm`
//! crate (the musl port Rust itself uses on wasm32) for the rest, so that every non-Apple
//! target computes the same bits. See `docs/porting/libm.md`.
//!
//! `sqrt` and `mul_add` are correctly rounded everywhere and stay `f64` methods.

mod portable;
#[cfg(feature = "math-trace")]
mod trace;
#[cfg(not(feature = "math-trace"))]
mod trace {
    #[inline(always)]
    pub fn log(_name: u8, _x: f64, _y: f64) {}
}

/// Whether this build uses [`portable`] rather than the platform libm. The `portable-math`
/// feature forces it on macOS, to check the portable code against the corpus natively.
pub const PORTABLE: bool = cfg!(any(not(target_vendor = "apple"), feature = "portable-math"));

/// Which libm this build computes with. The corpus ratchets keep one baseline per profile:
/// `Apple` results are C's bit for bit, `Portable` ones document the gap to C (see
/// `docs/porting/libm.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// The system libm on Apple targets, the one C Graphviz uses there.
    Apple,
    /// [`portable`]: identical on every target (Linux, wasm32, macOS with `portable-math`).
    Portable,
}

impl Profile {
    /// Lower-case name, as used in baseline file names (`baseline.portable.json`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Profile::Apple => "apple",
            Profile::Portable => "portable",
        }
    }
}

/// The math profile of this build.
pub const PROFILE: Profile = if PORTABLE {
    Profile::Portable
} else {
    Profile::Apple
};

/// C `hypot(x, y)`.
#[inline]
#[must_use]
pub fn hypot(x: f64, y: f64) -> f64 {
    trace::log(0, x, y);
    if PORTABLE {
        portable::hypot(x, y)
    } else {
        x.hypot(y)
    }
}

/// C `pow(x, y)`.
#[inline]
#[must_use]
pub fn powf(x: f64, y: f64) -> f64 {
    trace::log(1, x, y);
    if PORTABLE {
        portable::pow(x, y)
    } else {
        x.powf(y)
    }
}

/// C `atan2(y, x)` is `math::atan2(y, x)`.
#[inline]
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    trace::log(2, y, x);
    if PORTABLE {
        portable::atan2(y, x)
    } else {
        y.atan2(x)
    }
}

/// C `sin(x)`.
#[inline]
#[must_use]
pub fn sin(x: f64) -> f64 {
    trace::log(3, x, 0.0);
    if PORTABLE { portable::sin(x) } else { x.sin() }
}

/// C `cos(x)`.
#[inline]
#[must_use]
pub fn cos(x: f64) -> f64 {
    trace::log(4, x, 0.0);
    if PORTABLE { portable::cos(x) } else { x.cos() }
}

/// C `tan(x)`.
#[inline]
#[must_use]
pub fn tan(x: f64) -> f64 {
    trace::log(5, x, 0.0);
    if PORTABLE { portable::tan(x) } else { x.tan() }
}

/// C `asin(x)`.
#[inline]
#[must_use]
pub fn asin(x: f64) -> f64 {
    trace::log(6, x, 0.0);
    if PORTABLE {
        portable::asin(x)
    } else {
        x.asin()
    }
}

/// C `acos(x)`.
#[inline]
#[must_use]
pub fn acos(x: f64) -> f64 {
    trace::log(7, x, 0.0);
    if PORTABLE {
        portable::acos(x)
    } else {
        x.acos()
    }
}

/// C `exp(x)`.
#[inline]
#[must_use]
pub fn exp(x: f64) -> f64 {
    trace::log(8, x, 0.0);
    if PORTABLE { portable::exp(x) } else { x.exp() }
}

/// C `log(x)`.
#[inline]
#[must_use]
pub fn ln(x: f64) -> f64 {
    trace::log(9, x, 0.0);
    if PORTABLE { portable::log(x) } else { x.ln() }
}

/// C `log2(x)`.
#[inline]
#[must_use]
pub fn log2(x: f64) -> f64 {
    trace::log(10, x, 0.0);
    if PORTABLE {
        portable::log2(x)
    } else {
        x.log2()
    }
}

/// C `cbrt(x)`.
#[inline]
#[must_use]
pub fn cbrt(x: f64) -> f64 {
    trace::log(11, x, 0.0);
    if PORTABLE {
        portable::cbrt(x)
    } else {
        x.cbrt()
    }
}

#[cfg(all(test, target_vendor = "apple"))]
mod tests;
