//! 128-bit floating-point arithmetic (`dint64_t`), ported from CORE-MATH's
//! `src/binary64/pow/dint.h`. See `mod.rs` for provenance.

/// `(-1)^sgn * (hi + lo / 2^64) * 2^(ex - 63)`: the binary point follows the top bit of `hi`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dint {
    pub hi: u64,
    pub lo: u64,
    pub ex: i64,
    pub sgn: u64,
}

impl Dint {
    pub const fn new(hi: u64, lo: u64, ex: i64, sgn: u64) -> Self {
        Self { hi, lo, ex, sgn }
    }
    #[inline]
    pub fn r(&self) -> u128 {
        (u128::from(self.hi) << 64) | u128::from(self.lo)
    }
    #[inline]
    pub fn set_r(&mut self, r: u128) {
        self.hi = (r >> 64) as u64;
        self.lo = r as u64;
    }
}

/// -1.
pub const M_ONE: Dint = Dint::new(0x8000_0000_0000_0000, 0, 0, 1);
/// log(2), to nearest (absolute error < 2^-129.97).
pub const LOG2: Dint = Dint::new(0xb172_17f7_d1cf_79ab, 0xc9e3_b398_03f2_f6af, -1, 0);
/// 2^12 / log(2) (absolute error < 2^-52.96).
pub const LOG2_INV: Dint = Dint::new(0xb8aa_3b29_5c17_f0bc, 0, 12, 0);
/// 0, with an exponent such that `dint_tod(ZERO) = 0`.
pub const ZERO: Dint = Dint::new(0, 0, -1076, 0);

fn cmp<T: Ord>(a: T, b: T) -> i32 {
    i32::from(a > b) - i32::from(a < b)
}

fn dint_zero_p(a: &Dint) -> bool {
    a.hi == 0
}

/// Compares |a| and |b|: -1, 0 or +1.
pub fn cmp_dint_abs(a: &Dint, b: &Dint) -> i32 {
    if dint_zero_p(a) {
        return if dint_zero_p(b) { 0 } else { -1 };
    }
    if dint_zero_p(b) {
        return 1;
    }
    let c1 = cmp(a.ex, b.ex);
    if c1 != 0 { c1 } else { cmp(a.r(), b.r()) }
}

fn cmp_dint_11(a: &Dint, b: &Dint) -> i32 {
    let c1 = cmp(a.ex, b.ex);
    if c1 != 0 { c1 } else { cmp(a.hi, b.hi) }
}

/// `a + b`, with error bounded by 2 ulps (exact in the Sterbenz case).
pub fn add_dint(a: &Dint, b: &Dint) -> Dint {
    if a.hi | a.lo == 0 {
        return *b;
    }
    let (a, b) = match cmp_dint_abs(a, b) {
        0 => {
            if a.sgn ^ b.sgn != 0 {
                return ZERO;
            }
            let mut r = *a;
            r.ex += 1;
            return r;
        }
        -1 => (b, a),
        _ => (a, b),
    };
    // From now on |a| > |b|, thus a.ex >= b.ex.
    let a_r = a.r();
    let mut bb = b.r();
    let k = a.ex.wrapping_sub(b.ex) as u64;
    if k > 0 {
        bb = if k < 128 { bb >> k } else { 0 };
    }
    let sgn = a.sgn;
    let mut ex_r = a.ex;
    let mut c: u128;
    if a.sgn ^ b.sgn != 0 {
        c = a_r.wrapping_sub(bb);
        let ch = (c >> 64) as u64;
        let mut ex = if ch != 0 {
            u64::from(ch.leading_zeros())
        } else {
            64 + u64::from((c as u64).leading_zeros())
        };
        if ex > 0 {
            if k == 1 {
                c = (a_r << ex).wrapping_sub(b.r() << (ex - 1));
            } else {
                c = (a_r << ex).wrapping_sub(bb << ex);
            }
            ex_r -= ex as i64;
            ex = u64::from(((c >> 64) as u64).leading_zeros());
        }
        c <<= ex;
        ex_r -= ex as i64;
    } else {
        c = a_r.wrapping_add(bb);
        if c < a_r {
            c = (1u128 << 127) | (c >> 1);
            ex_r += 1;
        }
    }
    let mut r = Dint::new(0, 0, ex_r, sgn);
    r.set_r(c);
    r
}

/// [`add_dint`] assuming the low limbs of `a` and `b` are zero (error < 2 ulps of 64 bits).
///
/// Like the C, which writes only `hi`, `ex` and `sgn` of its destination, the result keeps
/// the low limb of `dst` (the destination's previous value) outside the early returns.
pub fn add_dint_11(dst: Dint, a: Dint, b: Dint) -> Dint {
    if a.hi == 0 {
        return b;
    }
    if b.hi == 0 {
        return a;
    }
    let mut r = dst;
    let (a, b) = match cmp_dint_11(&a, &b) {
        0 => {
            if a.sgn ^ b.sgn != 0 {
                return ZERO;
            }
            r = a;
            r.ex += 1;
            return r;
        }
        -1 => (b, a),
        _ => (a, b),
    };
    let ah = a.hi;
    let mut bh = b.hi;
    if a.ex > b.ex {
        let k = (a.ex - b.ex) as u64;
        bh = if k < 64 { bh >> k } else { 0 };
    }
    let sgn = a.sgn;
    r.ex = a.ex;
    // The C computes in 64 bits here (the operands are uint64_t).
    let mut c: u64;
    if a.sgn ^ b.sgn != 0 {
        c = ah.wrapping_sub(bh);
        let mut ex = c.leading_zeros();
        if ex > 0 {
            c = (ah << ex).wrapping_sub(bh << ex);
            r.ex -= i64::from(ex);
            ex = c.leading_zeros();
        }
        c <<= ex;
        r.ex -= i64::from(ex);
    } else {
        c = ah.wrapping_add(bh);
        if c < ah {
            c = (1u64 << 63) | (c >> 1);
            r.ex += 1;
        }
    }
    r.sgn = sgn;
    r.hi = c;
    r
}

/// `a * b`, with error bounded by 6 ulps.
pub fn mul_dint(a: &Dint, b: &Dint) -> Dint {
    let bh = u128::from(b.hi);
    let bl = u128::from(b.lo);
    let m1 = u128::from(a.hi) * bl;
    let m2 = u128::from(a.lo) * bh;
    let mut r = u128::from(a.hi) * bh;
    r += (m1 >> 64) + (m2 >> 64);
    let ex = (r >> 127) as u64;
    r <<= 1 - ex;
    let mut d = Dint::new(0, 0, a.ex + b.ex + ex as i64, a.sgn ^ b.sgn);
    d.set_r(r);
    d
}

/// `a * b` assuming the low limb of `b` is zero, with error bounded by 2 ulps.
pub fn mul_dint_21(a: &Dint, b: &Dint) -> Dint {
    let bh = u128::from(b.hi);
    let hi = u128::from(a.hi) * bh;
    let lo = u128::from(a.lo) * bh;
    let mut r = hi;
    r += lo >> 64;
    let ex = (r >> 127) as u64;
    r <<= 1 - ex;
    let mut d = Dint::new(0, 0, a.ex + b.ex + ex as i64, a.sgn ^ b.sgn);
    d.set_r(r);
    d
}

/// `a * b` assuming the low limbs of `a` and `b` are zero (exact).
pub fn mul_dint_11(a: &Dint, b: &Dint) -> Dint {
    let mut r = u128::from(a.hi) * u128::from(b.hi);
    let ex = (r >> 127) as u64;
    r <<= 1 - ex;
    let mut d = Dint::new(0, 0, a.ex + b.ex + ex as i64, a.sgn ^ b.sgn);
    d.set_r(r);
    d
}

/// `a * b` for an integer `b`, with error < 1 ulp.
pub fn mul_dint_int64(a: &Dint, b: i64) -> Dint {
    if b == 0 {
        return ZERO;
    }
    let c = b.unsigned_abs();
    let sgn = if b < 0 { u64::from(a.sgn == 0) } else { a.sgn };
    let mut ex = a.ex + 64;
    let mut r = u128::from(a.hi) * u128::from(c);
    let rhi = (r >> 64) as u64;
    let m = if rhi != 0 { rhi.leading_zeros() } else { 64 };
    r <<= m;
    ex -= i64::from(m);
    let mut l = u128::from(a.lo) * u128::from(c);
    l = (l << (m - 1)) >> 63;
    r = r.wrapping_add(l);
    if r < l {
        r = (1u128 << 127) | (r >> 1);
        ex += 1;
    }
    let mut d = Dint::new(0, 0, ex, sgn);
    d.set_r(r);
    d
}

/// The `dint64_t` of a non-zero double (exact).
pub fn dint_fromd(b: f64) -> Dint {
    let (e, m) = super::fast_extract(b);
    let t = m.leading_zeros();
    Dint::new(
        m << t,
        0,
        e - if t > 11 { i64::from(t) - 12 } else { 0 },
        u64::from(b < 0.0),
    )
}

/// `trunc(a)` (rounded toward zero, exact).
pub fn dint_toi(a: &Dint) -> i64 {
    if a.ex < 0 {
        return 0;
    }
    let r = (a.hi >> (63 - a.ex)) as i64;
    if a.sgn != 0 { -r } else { r }
}
