//! On Apple Silicon, checks each portable function against the system libm it reproduces.

// The point of these tests is to call the system libm, which the rest of the workspace may not.
#![allow(clippy::disallowed_methods)]

use super::portable;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
    /// `±2^e`, `e` uniform in `[lo, hi)`.
    fn log_uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let v = (lo + (hi - lo) * self.unit()).exp2();
        if self.next() & 1 == 1 { -v } else { v }
    }
    /// Any finite or special bit pattern.
    fn bits(&mut self) -> f64 {
        f64::from_bits(self.next())
    }
}

const EDGES: [f64; 16] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    f64::MIN_POSITIVE,
    -f64::MIN_POSITIVE,
    5e-324,
    1e-310,
    f64::MAX,
    f64::MIN,
    1e300,
    1e-300,
];

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn check2(
    name: &str,
    got: impl Fn(f64, f64) -> f64,
    want: impl Fn(f64, f64) -> f64,
    xs: &[(f64, f64)],
) {
    let bad: Vec<_> = xs
        .iter()
        .filter(|&&(x, y)| !same(got(x, y), want(x, y)))
        .collect();
    assert!(
        bad.is_empty(),
        "{name}: {} of {} differ, e.g. {:?}",
        bad.len(),
        xs.len(),
        &bad[..bad.len().min(5)]
    );
}

const N: usize = 200_000;

#[test]
fn hypot_matches_apple() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let mut xs = Vec::new();
    for &a in &EDGES {
        for &b in &EDGES {
            xs.push((a, b));
        }
    }
    for _ in 0..N {
        let a = r.log_uniform(-40.0, 40.0);
        xs.push((a, a * r.log_uniform(-30.0, 30.0)));
        xs.push((
            r.log_uniform(-1074.0, 1024.0),
            r.log_uniform(-1074.0, 1024.0),
        ));
        let a = r.log_uniform(-1074.0, 1024.0);
        xs.push((a, a * r.log_uniform(-60.0, 60.0)));
        xs.push((r.bits(), r.bits()));
        xs.push((r.unit() * 1000.0, r.unit() * 1000.0));
    }
    check2("hypot", portable::hypot, f64::hypot, &xs);
}
