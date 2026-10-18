//! 256-bit floating-point arithmetic (`qint64_t`), ported from CORE-MATH's
//! `src/binary64/pow/qint.h`. See `mod.rs` for provenance.

/// `(-1)^sgn * (hh + hl/2^64 + lh/2^128 + ll/2^192) * 2^(ex - 63)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Qint {
    pub hh: u64,
    pub hl: u64,
    pub lh: u64,
    pub ll: u64,
    pub ex: i64,
    pub sgn: u64,
}

impl Qint {
    pub const fn new(hh: u64, hl: u64, lh: u64, ll: u64, ex: i64, sgn: u64) -> Self {
        Self {
            hh,
            hl,
            lh,
            ll,
            ex,
            sgn,
        }
    }
    #[inline]
    pub fn rh(&self) -> u128 {
        (u128::from(self.hh) << 64) | u128::from(self.hl)
    }
    #[inline]
    pub fn rl(&self) -> u128 {
        (u128::from(self.lh) << 64) | u128::from(self.ll)
    }
    #[inline]
    fn set(&mut self, rh: u128, rl: u128) {
        self.hh = (rh >> 64) as u64;
        self.hl = rh as u64;
        self.lh = (rl >> 64) as u64;
        self.ll = rl as u64;
    }
    fn with(rh: u128, rl: u128, ex: i64, sgn: u64) -> Self {
        let mut q = Self::new(0, 0, 0, 0, ex, sgn);
        q.set(rh, rl);
        q
    }
}

/// -1.
pub const M_ONE_Q: Qint = Qint::new(0x8000_0000_0000_0000, 0, 0, 0, 0, 1);
/// log(2) (absolute error < 2^-256.14).
pub const LOG2_Q: Qint = Qint::new(
    0xb172_17f7_d1cf_79ab,
    0xc9e3_b398_03f2_f6af,
    0x40f3_4326_7298_b62d,
    0x8a0d_175b_8baa_fa2b,
    -1,
    0,
);
/// 2^12 / log(2) (absolute error < 2^-52.96).
pub const LOG2_INV_Q: Qint = Qint::new(0xb8aa_3b29_5c17_f0bc, 0, 0, 0, 12, 0);
/// 0.
pub const ZERO_Q: Qint = Qint::new(0, 0, 0, 0, 0, 0);

fn cmp<T: Ord>(a: T, b: T) -> i32 {
    i32::from(a > b) - i32::from(a < b)
}

fn cmp_qint(a: &Qint, b: &Qint) -> i32 {
    let c = cmp(a.ex, b.ex);
    if c != 0 {
        return c;
    }
    let c = cmp(a.rh(), b.rh());
    if c != 0 {
        return c;
    }
    cmp(a.rl(), b.rl())
}

fn cmp_qint_22(a: &Qint, b: &Qint) -> i32 {
    let c = cmp(a.ex, b.ex);
    if c != 0 { c } else { cmp(a.rh(), b.rh()) }
}

/// `clz` of the 256-bit value `ch:cl` (non-zero), as the C's nested `__builtin_clzll`.
fn clz256(ch: u128, cl: u128) -> u64 {
    let chh = (ch >> 64) as u64;
    let clh = (cl >> 64) as u64;
    if chh != 0 {
        u64::from(chh.leading_zeros())
    } else if ch != 0 {
        64 + u64::from((ch as u64).leading_zeros())
    } else if clh != 0 {
        128 + u64::from(clh.leading_zeros())
    } else {
        192 + u64::from((cl as u64).leading_zeros())
    }
}

/// `a + b`, with error bounded by 2 ulps (exact in the Sterbenz case).
pub fn add_qint(a: &Qint, b: &Qint) -> Qint {
    if a.rh() == 0 && a.rl() == 0 {
        return *b;
    }
    if b.rh() == 0 && b.rl() == 0 {
        return *a;
    }
    match cmp_qint(a, b) {
        0 => {
            if a.sgn ^ b.sgn != 0 {
                return ZERO_Q;
            }
            let mut r = *a;
            r.ex += 1;
            return r;
        }
        -1 => return add_qint(b, a),
        _ => {}
    }
    let (mut ah, mut al, mut bh, mut bl) = (a.rh(), a.rl(), b.rh(), b.rl());
    let k = a.ex - b.ex;
    if k > 0 {
        if k >= 128 {
            bl = if k < 256 { bh >> (k - 128) } else { 0 };
            bh = 0;
        } else {
            bl = (bl >> k) | (bh << (128 - k));
            bh >>= k;
        }
    }
    let sgn = a.sgn;
    let mut ex_r = a.ex;
    let (mut ch, mut cl);
    if a.sgn ^ b.sgn != 0 {
        ch = ah.wrapping_sub(bh);
        let (d, borrow) = al.overflowing_sub(bl);
        cl = d;
        if borrow {
            ch = ch.wrapping_sub(1);
        }
        let mut ex = clz256(ch, cl);
        if ex > 0 {
            if ex >= 128 {
                ah = al << (ex - 128);
                al = 0;
            } else {
                ah = (ah << ex) | (al >> (128 - ex));
                al <<= ex;
            }
            let sh = ex as i64 - k;
            bh = b.rh();
            bl = b.rl();
            if sh >= 0 {
                if sh >= 128 {
                    bh = bl << (sh - 128);
                    bl = 0;
                } else if sh > 0 {
                    bh = (bh << sh) | (bl >> (128 - sh));
                    bl <<= sh;
                }
            } else {
                let j = -sh;
                if j >= 128 {
                    bl = bh >> (j - 128);
                    bh = 0;
                } else {
                    bl = (bh << (128 - j)) | (bl >> j);
                    bh >>= j;
                }
            }
            ex_r -= ex as i64;
            ch = ah.wrapping_sub(bh);
            let (d, borrow) = al.overflowing_sub(bl);
            cl = d;
            if borrow {
                ch = ch.wrapping_sub(1);
            }
            ex = clz256(ch, cl);
        }
        if ex != 0 {
            ch = (ch << ex) | (cl >> (128 - ex));
            cl <<= ex;
        }
        ex_r -= ex as i64;
    } else {
        let (s, c1) = ah.overflowing_add(bh);
        ch = s;
        let mut cy = u8::from(c1);
        let (s, c2) = al.overflowing_add(bl);
        cl = s;
        if c2 {
            ch = ch.wrapping_add(1);
            cy += u8::from(ch == 0);
        }
        if cy != 0 {
            cl = (ch << 127) | (cl >> 1);
            ch = (1u128 << 127) | (ch >> 1);
            ex_r += 1;
        }
    }
    Qint::with(ch, cl, ex_r, sgn)
}

/// [`add_qint`] on the upper 2 limbs of `a` and `b` (error < 2 ulps of 128 bits).
pub fn add_qint_22(a: &Qint, b: &Qint) -> Qint {
    if a.rh() == 0 {
        return *b;
    }
    if b.rh() == 0 {
        return *a;
    }
    match cmp_qint_22(a, b) {
        0 => {
            if a.sgn ^ b.sgn != 0 {
                return ZERO_Q;
            }
            let mut r = *a;
            r.ex += 1;
            return r;
        }
        -1 => return add_qint_22(b, a),
        _ => {}
    }
    let mut ah = a.rh();
    let mut bh = b.rh();
    let k = a.ex.wrapping_sub(b.ex) as u64;
    if k > 0 {
        bh = if k >= 128 { 0 } else { bh >> k };
    }
    let sgn = a.sgn;
    let mut ex_r = a.ex;
    let mut ch;
    if a.sgn ^ b.sgn != 0 {
        ch = ah.wrapping_sub(bh);
        let clz = |ch: u128| {
            let chh = (ch >> 64) as u64;
            if chh != 0 {
                u64::from(chh.leading_zeros())
            } else {
                64 + u64::from((ch as u64).leading_zeros())
            }
        };
        let mut ex = clz(ch);
        if ex > 0 {
            ah <<= ex;
            bh = if ex >= k {
                b.rh() << (ex - k)
            } else {
                b.rh() >> (k - ex)
            };
            ex_r -= ex as i64;
            ch = ah.wrapping_sub(bh);
            ex = clz(ch);
        }
        ch <<= ex;
        ex_r -= ex as i64;
    } else {
        let (s, cy) = ah.overflowing_add(bh);
        ch = s;
        if cy {
            ch = (1u128 << 127) | (ch >> 1);
            ex_r += 1;
        }
    }
    Qint::with(ch, 0, ex_r, sgn)
}

fn mul(a: u64, b: u64) -> u128 {
    u128::from(a) * u128::from(b)
}

fn add_cy(a: u128, b: u128) -> (u128, u128) {
    let (s, c) = a.overflowing_add(b);
    (s, u128::from(c))
}

/// The common tail of the `mul_qint*` routines: normalizes `t6:t5:low(t4)`.
fn finish(t6: u128, t5: u128, t4: u128, a: &Qint, b: &Qint) -> Qint {
    let ex = u64::from(t6 >> 127 == 0);
    let t5 = (t5 << 64) | (t4 & 0xffff_ffff_ffff_ffff);
    let (rh, rl) = if ex != 0 {
        ((t6 << 1) | (t5 >> 127), t5 << 1)
    } else {
        (t6, t5)
    };
    Qint::with(rh, rl, a.ex + b.ex + 1 - ex as i64, a.sgn ^ b.sgn)
}

/// `a * b`, with error < 14 ulps.
pub fn mul_qint(a: &Qint, b: &Qint) -> Qint {
    let r33 = mul(a.hh, b.hh);
    let r32 = mul(a.hh, b.hl);
    let r23 = mul(a.hl, b.hh);
    let r31 = mul(a.hh, b.lh);
    let r13 = mul(a.lh, b.hh);
    let r22 = mul(a.hl, b.hl);
    let r30 = mul(a.hh, b.ll);
    let r03 = mul(a.ll, b.hh);
    let r21 = mul(a.hl, b.lh);
    let r12 = mul(a.lh, b.hl);
    let t3 = (r12 >> 64) + (r21 >> 64) + (r03 >> 64) + (r30 >> 64);
    let (t4, mut c4) = add_cy(r22, t3);
    let (t4, c) = add_cy(r13, t4);
    c4 += c;
    let (t4, c) = add_cy(r31, t4);
    c4 += c;
    let (t5, mut c5) = add_cy(r23, t4 >> 64);
    let (t5, c) = add_cy(r32, t5);
    c5 += c;
    let t6 = r33.wrapping_add((c5 << 64) | (t5 >> 64)).wrapping_add(c4);
    finish(t6, t5, t4, a, b)
}

/// [`mul_qint`] on the upper 3 limbs of `a` and `b` (error < 6 ulps).
pub fn mul_qint_33(a: &Qint, b: &Qint) -> Qint {
    let r33 = mul(a.hh, b.hh);
    let r32 = mul(a.hh, b.hl);
    let r23 = mul(a.hl, b.hh);
    let r31 = mul(a.hh, b.lh);
    let r13 = mul(a.lh, b.hh);
    let r22 = mul(a.hl, b.hl);
    let r21 = mul(a.hl, b.lh);
    let r12 = mul(a.lh, b.hl);
    let t3 = (r12 >> 64) + (r21 >> 64);
    let (t4, mut c4) = add_cy(r22, t3);
    let (t4, c) = add_cy(r13, t4);
    c4 += c;
    let (t4, c) = add_cy(r31, t4);
    c4 += c;
    let (t5, mut c5) = add_cy(r23, t4 >> 64);
    let (t5, c) = add_cy(r32, t5);
    c5 += c;
    let t6 = r33.wrapping_add((c5 << 64) | (t5 >> 64)).wrapping_add(c4);
    finish(t6, t5, t4, a, b)
}

/// [`mul_qint`] with the upper limb of `b` only (error < 2 ulps).
pub fn mul_qint_41(a: &Qint, b: &Qint) -> Qint {
    let r33 = mul(a.hh, b.hh);
    let r23 = mul(a.hl, b.hh);
    let r13 = mul(a.lh, b.hh);
    let r03 = mul(a.ll, b.hh);
    let t3 = r03 >> 64;
    let (t4, c4) = add_cy(r13, t3);
    let (t5, c5) = add_cy(r23, t4 >> 64);
    let t6 = r33.wrapping_add((c5 << 64) | (t5 >> 64)).wrapping_add(c4);
    finish(t6, t5, t4, a, b)
}

/// [`mul_qint`] with the upper 3 limbs of `a` and the upper limb of `b` (exact).
pub fn mul_qint_31(a: &Qint, b: &Qint) -> Qint {
    let r33 = mul(a.hh, b.hh);
    let r23 = mul(a.hl, b.hh);
    let r13 = mul(a.lh, b.hh);
    let t4 = r13;
    let (t5, c5) = add_cy(r23, t4 >> 64);
    let t6 = r33.wrapping_add((c5 << 64) | (t5 >> 64));
    finish(t6, t5, t4, a, b)
}

/// [`mul_qint`] with the upper 2 limbs of `a` and `b` (exact).
pub fn mul_qint_22(a: &Qint, b: &Qint) -> Qint {
    let r33 = mul(a.hh, b.hh);
    let r32 = mul(a.hh, b.hl);
    let r23 = mul(a.hl, b.hh);
    let r22 = mul(a.hl, b.hl);
    let t4 = r22;
    let (t5, mut c5) = add_cy(r23, t4 >> 64);
    let (t5, c) = add_cy(r32, t5);
    c5 += c;
    let t6 = r33.wrapping_add((c5 << 64) | (t5 >> 64));
    finish(t6, t5, t4, a, b)
}

/// [`mul_qint`] with the upper 2 limbs of `a` and the upper limb of `b` (exact).
pub fn mul_qint_21(a: &Qint, b: &Qint) -> Qint {
    let r33 = mul(a.hh, b.hh);
    let r23 = mul(a.hl, b.hh);
    let t6 = r33.wrapping_add(r23 >> 64);
    let ex = u64::from(t6 >> 127 == 0);
    let t5 = r23 << 64;
    let (rh, rl) = if ex != 0 {
        ((t6 << 1) | (t5 >> 127), t5 << 1)
    } else {
        (t6, t5)
    };
    Qint::with(rh, rl, a.ex + b.ex + 1 - ex as i64, a.sgn ^ b.sgn)
}

/// [`mul_qint`] with the upper limbs of `a` and `b` (exact).
pub fn mul_qint_11(a: &Qint, b: &Qint) -> Qint {
    let t6 = mul(a.hh, b.hh);
    let ex = u64::from(t6 >> 127 == 0);
    Qint::with(t6 << ex, 0, a.ex + b.ex + 1 - ex as i64, a.sgn ^ b.sgn)
}

/// `b * a` for an integer `b`, with error < 2 ulps.
pub fn mul_qint_2(b: i64, a: &Qint) -> Qint {
    if b == 0 {
        return ZERO_Q;
    }
    let mut c = b.unsigned_abs();
    let sgn = u64::from(b < 0) ^ a.sgn;
    if c == 1 {
        let mut r = *a;
        r.sgn = sgn;
        return r;
    }
    let mut ex_r = a.ex + 64;
    let k = c.leading_zeros();
    c <<= k;
    ex_r -= i64::from(k);
    let mut t3 = mul(a.hh, c);
    let t2 = mul(a.hl, c);
    let t1 = mul(a.lh, c);
    let t0 = mul(a.ll, c);
    let t = t0 >> 64;
    let (t1, cy) = add_cy(t, t1);
    let t = (cy << 64) | (t1 >> 64);
    let (t2, cy) = add_cy(t, t2);
    t3 = t3.wrapping_add((cy << 64) | (t2 >> 64));
    let ex = ((t3 >> 64) as u64).leading_zeros();
    let t2 = (t2 << 64) | (t1 & 0xffff_ffff_ffff_ffff);
    if ex != 0 {
        Qint::with((t3 << 1) | (t2 >> 127), t2 << 1, ex_r - 1, sgn)
    } else {
        Qint::with(t3, t2, ex_r, sgn)
    }
}

/// The `qint64_t` of a double (exact).
pub fn qint_fromd(b: f64) -> Qint {
    let (e, m) = super::fast_extract(b);
    let t = m.leading_zeros();
    Qint::new(
        m << t,
        0,
        0,
        0,
        e - if t > 11 { i64::from(t) - 12 } else { 0 },
        u64::from(b < 0.0),
    )
}

/// `trunc(a)` (rounded toward zero, exact).
pub fn qint_toi(a: &Qint) -> i64 {
    if a.ex < 0 {
        return 0;
    }
    let r = (a.hh >> (63 - a.ex)) as i64;
    if a.sgn != 0 { -r } else { r }
}
