//! C library behaviour that Graphviz layouts depend on, reproduced bit-for-bit.
//!
//! Graphviz 16.1.0 as shipped on macOS calls the platform's `rand`/`srand`,
//! `drand48`/`srand48` and `qsort`. Layout results depend on their exact
//! sequences (random initial placements, pivot selection) and on the order
//! `qsort` leaves equal elements in, so this module ports them:
//!
//! - [`Drand48`]: the POSIX 48-bit linear congruential generator.
//! - [`CRand`]: Apple libc `rand()` (Park–Miller "minimal standard", seed 1).
//! - [`qsort_by`]: Apple libc `qsort` (BSD Bentley–McIlroy quicksort with an
//!   insertion-sort shortcut and heapsort fallback); not stable, so ties end
//!   up in the same order as in C.
//!
//! State is held in values rather than process globals; a layout creates fresh
//! generators, which corresponds to C running a single layout per process.

/// POSIX `drand48` / `srand48` / `erand48` generator state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drand48 {
    x: u64,
}

const DRAND48_A: u64 = 0x5_DEEC_E66D;
const DRAND48_C: u64 = 0xB;
const DRAND48_MASK: u64 = (1 << 48) - 1;

impl Default for Drand48 {
    /// The state before any `srand48` call (`x = 0x1234ABCD330E`).
    fn default() -> Self {
        Self {
            x: 0x1234_ABCD_330E,
        }
    }
}

impl Drand48 {
    /// `srand48(seed)`: the high 32 bits of the state are the low 32 bits of `seed`.
    pub fn new(seed: i64) -> Self {
        let mut r = Self::default();
        r.srand48(seed);
        r
    }

    pub fn srand48(&mut self, seed: i64) {
        self.x = (((seed as u64) & 0xFFFF_FFFF) << 16) | 0x330E;
    }

    /// `drand48()`: uniform double in `[0, 1)`, exactly `x / 2^48`.
    pub fn drand48(&mut self) -> f64 {
        self.x = (DRAND48_A.wrapping_mul(self.x).wrapping_add(DRAND48_C)) & DRAND48_MASK;
        // erand48 builds the value as ldexp(x0,-48)+ldexp(x1,-32)+ldexp(x2,-16);
        // every partial sum is exact in a double, so this equals x / 2^48.
        self.x as f64 / (1u64 << 48) as f64
    }
}

/// Apple libc `rand()` / `srand()`; `RAND_MAX` is `0x7fffffff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CRand {
    next: u64,
}

pub const RAND_MAX: i32 = 0x7fff_ffff;

impl Default for CRand {
    /// A process that never called `srand` starts with seed 1.
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl CRand {
    pub fn srand(&mut self, seed: u32) {
        self.next = u64::from(seed);
    }

    /// Park–Miller minimal standard generator, computed without overflow
    /// with Schrage's method as libc does.
    pub fn rand(&mut self) -> i32 {
        let mut ctx = self.next as i64;
        if ctx == 0 {
            ctx = 123_459_876;
        }
        let hi = ctx / 127_773;
        let lo = ctx % 127_773;
        let mut x = 16_807 * lo - 2_836 * hi;
        if x < 0 {
            x += 0x7fff_ffff;
        }
        self.next = x as u64;
        (x % (i64::from(RAND_MAX) + 1)) as i32
    }
}

/// Apple libc `qsort` (FreeBSD-derived introsort) over a slice with a C-style
/// comparator returning negative, zero or positive. Like C it is not stable.
pub fn qsort_by<T: Clone, F>(v: &mut [T], mut cmp: F)
where
    F: FnMut(&T, &T) -> i32,
{
    let n = v.len();
    let depth = if n == 0 {
        0
    } else {
        // DEPTH(x) = 2 * (flsl(x) - 1)
        2 * (usize::BITS as i32 - n.leading_zeros() as i32 - 1)
    };
    qsort_impl(v, &mut cmp, depth);
}

fn med3<T, F: FnMut(&T, &T) -> i32>(v: &[T], a: usize, b: usize, c: usize, cmp: &mut F) -> usize {
    if cmp(&v[a], &v[b]) < 0 {
        if cmp(&v[b], &v[c]) < 0 {
            b
        } else if cmp(&v[a], &v[c]) < 0 {
            c
        } else {
            a
        }
    } else if cmp(&v[b], &v[c]) > 0 {
        b
    } else if cmp(&v[a], &v[c]) < 0 {
        a
    } else {
        c
    }
}

/// `_isort`: insertion sort giving up after `swap_limit` swaps (0 = no limit).
fn isort<T, F: FnMut(&T, &T) -> i32>(v: &mut [T], cmp: &mut F, swap_limit: usize) -> bool {
    let mut swap_cnt = 0usize;
    for m in 1..v.len() {
        let mut l = m;
        while l > 0 && cmp(&v[l - 1], &v[l]) > 0 {
            v.swap(l, l - 1);
            l -= 1;
            if swap_limit != 0 {
                swap_cnt += 1;
                if swap_cnt > swap_limit {
                    return false;
                }
            }
        }
    }
    true
}

fn vecswap<T>(v: &mut [T], a: usize, b: usize, n: usize) {
    for i in 0..n {
        v.swap(a + i, b + i);
    }
}

/// BSD `heapsort` (Knuth vol. 3), used by `qsort` past its recursion limit.
fn heapsort<T: Clone, F: FnMut(&T, &T) -> i32>(v: &mut [T], cmp: &mut F) {
    let mut nmemb = v.len();
    if nmemb <= 1 {
        return;
    }
    // Items are numbered 1..=nmemb.
    let at = |i: usize| i - 1;
    let mut l = nmemb / 2 + 1;
    loop {
        l -= 1;
        if l == 0 {
            break;
        }
        let mut par_i = l;
        loop {
            let mut child_i = par_i * 2;
            if child_i > nmemb {
                break;
            }
            if child_i < nmemb && cmp(&v[at(child_i)], &v[at(child_i + 1)]) < 0 {
                child_i += 1;
            }
            if cmp(&v[at(child_i)], &v[at(par_i)]) <= 0 {
                break;
            }
            v.swap(at(par_i), at(child_i));
            par_i = child_i;
        }
    }
    while nmemb > 1 {
        let k = v[at(nmemb)].clone();
        v[at(nmemb)] = v[at(1)].clone();
        nmemb -= 1;
        let mut par_i = 1;
        loop {
            let mut child_i = par_i * 2;
            if child_i > nmemb {
                break;
            }
            if child_i < nmemb && cmp(&v[at(child_i)], &v[at(child_i + 1)]) < 0 {
                child_i += 1;
            }
            v[at(par_i)] = v[at(child_i)].clone();
            par_i = child_i;
        }
        loop {
            let child_i = par_i;
            par_i = child_i / 2;
            if child_i == 1 || cmp(&k, &v[at(par_i)]) < 0 {
                v[at(child_i)] = k.clone();
                break;
            }
            v[at(child_i)] = v[at(par_i)].clone();
        }
    }
}

fn qsort_impl<T: Clone, F: FnMut(&T, &T) -> i32>(
    mut v: &mut [T],
    cmp: &mut F,
    mut depth_limit: i32,
) {
    loop {
        let n = v.len();
        if depth_limit <= 0 {
            heapsort(v, cmp);
            return;
        }
        depth_limit -= 1;
        if n <= 7 {
            isort(v, cmp, 0);
            return;
        }
        let mut pl = 0;
        let mut pm = n / 2;
        let mut pn = n - 1;
        if n > 40 {
            let d = n / 8;
            pl = med3(v, pl, pl + d, pl + 2 * d, cmp);
            pm = med3(v, pm - d, pm, pm + d, cmp);
            pn = med3(v, pn - 2 * d, pn - d, pn, cmp);
        }
        pm = med3(v, pl, pm, pn, cmp);
        v.swap(0, pm);
        let mut pa = 1usize;
        let mut pb = 1usize;
        // pc and pd may step below pb; keep them signed.
        let mut pc = n as isize - 1;
        let mut pd = n as isize - 1;
        let mut swap_cnt = false;
        loop {
            while pb as isize <= pc {
                let r = cmp(&v[pb], &v[0]);
                if r > 0 {
                    break;
                }
                if r == 0 {
                    swap_cnt = true;
                    v.swap(pa, pb);
                    pa += 1;
                }
                pb += 1;
            }
            while pb as isize <= pc {
                let r = cmp(&v[pc as usize], &v[0]);
                if r < 0 {
                    break;
                }
                if r == 0 {
                    swap_cnt = true;
                    v.swap(pc as usize, pd as usize);
                    pd -= 1;
                }
                pc -= 1;
            }
            if pb as isize > pc {
                break;
            }
            v.swap(pb, pc as usize);
            swap_cnt = true;
            pb += 1;
            pc -= 1;
        }
        let pn = n;
        let d1 = pa.min(pb - pa);
        vecswap(v, 0, pb - d1, d1);
        let d1 = ((pd - pc) as usize).min(pn - pd as usize - 1);
        vecswap(v, pb, pn - d1, d1);
        if !swap_cnt && isort(v, cmp, 1 + n / 4) {
            return;
        }
        let d1 = pb - pa;
        let d2 = (pd - pc) as usize;
        if d1 <= d2 {
            if d1 > 1 {
                qsort_impl(&mut v[..d1], cmp, depth_limit);
            }
            if d2 > 1 {
                v = &mut v[pn - d2..];
                continue;
            }
        } else {
            if d2 > 1 {
                qsort_impl(&mut v[pn - d2..], cmp, depth_limit);
            }
            if d1 > 1 {
                v = &mut v[..d1];
                continue;
            }
        }
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drand48_matches_libc() {
        let mut r = Drand48::new(1);
        assert_eq!(r.drand48(), 0.041_630_344_771_878_214);
        assert_eq!(r.drand48(), 0.454_492_444_728_629_15);
        assert_eq!(r.drand48(), 0.834_817_218_166_914_9);
    }

    #[test]
    fn rand_matches_libc() {
        let mut r = CRand::default();
        let got: Vec<i32> = (0..5).map(|_| r.rand()).collect();
        assert_eq!(
            got,
            [
                16807,
                282_475_249,
                1_622_650_073,
                984_943_658,
                1_144_108_930
            ]
        );
        r.srand(5);
        assert_eq!(r.rand(), 84035);
    }

    /// Replays the generator of the C probe that produced `qsort_expected.txt`
    /// with macOS `qsort` and checks the permutations agree, ties included.
    #[test]
    fn qsort_matches_libc_on_ties() {
        let expected = include_str!("../tests/data/qsort_expected.txt");
        let mut lines = expected.lines();
        let mut s: u32 = 12345;
        for t in 0..200u32 {
            let n = 1 + (t * 7) % 97;
            let mut items: Vec<(u32, usize)> = Vec::new();
            for i in 0..n as usize {
                s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
                items.push(((s >> 16) % (1 + t % 5), i));
            }
            qsort_by(&mut items, |x, y| {
                if y.0 < x.0 {
                    -1
                } else if y.0 > x.0 {
                    1
                } else {
                    0
                }
            });
            let got: Vec<String> = items.iter().map(|it| it.1.to_string()).collect();
            let want = lines.next().unwrap();
            assert_eq!(format!("{n}: {}", got.join(" ")), want, "case {t}");
        }
    }
}
