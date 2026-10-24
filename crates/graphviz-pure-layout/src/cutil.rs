//! C library behaviour that Graphviz layouts depend on, reproduced bit-for-bit.
//!
//! Graphviz 16.1.0 as shipped on macOS calls the platform's `rand`/`srand`,
//! `drand48`/`srand48` and `qsort`. Layout results depend on their exact
//! sequences (random initial placements, pivot selection) and on the order
//! `qsort` leaves equal elements in, so this module ports them:
//!
//! - [`drand48`]/[`srand48`]: the POSIX 48-bit linear congruential generator.
//! - [`rand`]/[`srand`]: Apple libc `rand()` (Park–Miller "minimal standard", seed 1).
//! - [`qsort_by`]: Apple libc `qsort` (BSD Bentley–McIlroy quicksort with an
//!   insertion-sort shortcut and heapsort fallback); not stable, so ties end
//!   up in the same order as in C.
//!
//! # One generator state per run
//!
//! C has one process-wide state for each generator. Every `srand48` (neato's `setSeed`,
//! fdp, ortho's `partition`) and `srand` (sfdp) reseeds it, and every draw advances it, so
//! a graph's unseeded draws (neato's `user_pos` jitter, `-n`, the `rand()` of stress
//! pivots and vpsc) continue from the state the run's earlier layouts left. The functions
//! [`drand48`], [`srand48`], [`rand`] and [`srand`] work on such a state, a [`RandState`],
//! as C's do on its globals: the caller that stands for
//! the C process owns it and installs it with [`with_rand_state`] while it lays graphs out.
//! `dot`'s job pipeline keeps one for the whole run. A layout that runs outside any
//! [`with_rand_state`] gets a fresh state ([`in_rand_run`]), so the library's top-level
//! calls are deterministic: each starts like a new process unless its caller passes a state
//! in. [`Drand48State`] and [`CRandState`] are the generators as plain values.
//!
//! The installed state is per thread. Every draw must happen inside a run (checked in debug
//! builds): engine internals called directly, outside the layout entry points, need their
//! own [`with_rand_state`].
//!
//! The two states are separate, as in Apple's (and every POSIX) libc. Under `_WIN32` C
//! builds `drand48` on `rand()` (`lib/common/utils.c`) and `srand48` is `srand`, so there
//! the two would share one state; the port doesn't model that.

use std::cell::Cell;

/// POSIX `drand48` / `srand48` / `erand48` generator state, as a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drand48State {
    x: u64,
}

const DRAND48_A: u64 = 0x5_DEEC_E66D;
const DRAND48_C: u64 = 0xB;
const DRAND48_MASK: u64 = (1 << 48) - 1;

impl Default for Drand48State {
    /// The state before any `srand48` call (`x = 0x1234ABCD330E`).
    fn default() -> Self {
        Self::new_unseeded()
    }
}

impl Drand48State {
    /// The state before any `srand48` call, usable in a `const`.
    #[must_use]
    pub const fn new_unseeded() -> Self {
        Self {
            x: 0x1234_ABCD_330E,
        }
    }

    /// `srand48(seed)`: the high 32 bits of the state are the low 32 bits of `seed`.
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

/// Apple libc `rand()` / `srand()` state, as a value; `RAND_MAX` is `0x7fffffff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CRandState {
    next: u64,
}

pub const RAND_MAX: i32 = 0x7fff_ffff;

impl Default for CRandState {
    /// A process that never called `srand` starts with seed 1.
    fn default() -> Self {
        Self::new_unseeded()
    }
}

impl CRandState {
    /// The state of a process that never called `srand` (seed 1), usable in a `static`.
    #[must_use]
    pub const fn new_unseeded() -> Self {
        Self { next: 1 }
    }

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

/// The C process's generator states (see the module docs): `drand48`'s and `rand`'s.
/// [`Default`] is a process that has drawn nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RandState {
    pub drand48: Drand48State,
    pub rand: CRandState,
}

thread_local! {
    /// `drand48`'s state, `rand`'s state, and whether a [`with_rand_state`] installed them.
    static DRAND48: Cell<Drand48State> = const { Cell::new(Drand48State::new_unseeded()) };
    static RAND: Cell<CRandState> = const { Cell::new(CRandState::new_unseeded()) };
    static IN_RUN: Cell<bool> = const { Cell::new(false) };
}

fn installed() -> RandState {
    RandState {
        drand48: DRAND48.with(Cell::get),
        rand: RAND.with(Cell::get),
    }
}

fn install(state: RandState, in_run: bool) {
    DRAND48.with(|c| c.set(state.drand48));
    RAND.with(|c| c.set(state.rand));
    IN_RUN.with(|c| c.set(in_run));
}

/// Runs `f` with `state` as the process's generator state, and leaves in `state` what
/// `f`'s draws and seedings made of it, also when `f` panics.
pub fn with_rand_state<R>(state: &mut RandState, f: impl FnOnce() -> R) -> R {
    struct Run<'s> {
        state: &'s mut RandState,
        outer: (RandState, bool),
    }
    impl Drop for Run<'_> {
        fn drop(&mut self) {
            *self.state = installed();
            install(self.outer.0, self.outer.1);
        }
    }
    let outer = (installed(), IN_RUN.with(Cell::get));
    install(*state, true);
    let _run = Run { state, outer };
    f()
}

/// Runs `f` in the current [`with_rand_state`], or, outside any, with a fresh state: a
/// layout called on its own starts like a new process.
pub fn in_rand_run<R>(f: impl FnOnce() -> R) -> R {
    if IN_RUN.with(Cell::get) {
        f()
    } else {
        with_rand_state(&mut RandState::default(), f)
    }
}

/// Every draw belongs to a run: the layout entry points start one ([`in_rand_run`]), and
/// a test that calls an engine's internals directly must too ([`with_rand_state`]), so that
/// its results don't depend on what ran before it on the thread.
fn debug_assert_in_run() {
    debug_assert!(
        IN_RUN.with(Cell::get),
        "drand48/rand outside a with_rand_state run"
    );
}

/// Runs a test of an engine's internals in a fresh run, as if in a new process.
#[cfg(test)]
pub(crate) fn in_fresh_run<R>(f: impl FnOnce() -> R) -> R {
    with_rand_state(&mut RandState::default(), f)
}

/// C's `srand48(seed)`, on the process's state.
pub fn srand48(seed: i64) {
    debug_assert_in_run();
    DRAND48.with(|c| {
        let mut s = c.get();
        s.srand48(seed);
        c.set(s);
    });
}

/// C's `drand48()`, on the process's state.
pub fn drand48() -> f64 {
    debug_assert_in_run();
    DRAND48.with(|c| {
        let mut s = c.get();
        let v = s.drand48();
        c.set(s);
        v
    })
}

/// C's `srand(seed)`, on the process's state.
pub fn srand(seed: u32) {
    debug_assert_in_run();
    RAND.with(|c| {
        let mut s = c.get();
        s.srand(seed);
        c.set(s);
    });
}

/// C's `rand()`, on the process's state.
pub fn rand() -> i32 {
    debug_assert_in_run();
    RAND.with(|c| {
        let mut s = c.get();
        let v = s.rand();
        c.set(s);
        v
    })
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

/// `random_small(bound)` (`lib/util/random.c`): uniform in `[0, bound)` from `rand()`.
fn random_small(bound: i32) -> i32 {
    debug_assert!(bound > 0);
    let discard_threshold = RAND_MAX - ((RAND_MAX as u32 + 1) % bound as u32) as i32;
    loop {
        let v = rand();
        if v <= discard_threshold {
            return v % bound;
        }
    }
}

/// `gv_random_u64(bound)` (`lib/util/random.c`).
fn gv_random_u64(bound: u64) -> u64 {
    let discard_threshold = u64::MAX - (u64::MAX - bound + 1) % bound;
    loop {
        let mut bytes = [0u8; 8];
        for b in &mut bytes {
            *b = random_small(256) as u8;
        }
        let v = u64::from_le_bytes(bytes);
        if v <= discard_threshold {
            return v % bound;
        }
    }
}

/// `gv_random(bound)` (`lib/util/random.c`): uniform in `[0, bound)` from `rand()`.
pub fn gv_random(bound: usize) -> usize {
    if bound > RAND_MAX as usize {
        return gv_random_u64(bound as u64) as usize;
    }
    random_small(bound as i32) as usize
}

/// `gv_permutation(bound)` (`lib/util/random.c`): Fisher–Yates shuffle of `0..bound`.
pub fn gv_permutation(bound: usize) -> Vec<usize> {
    let mut p: Vec<usize> = (0..bound).collect();
    if bound == 0 {
        return p;
    }
    for i in (1..bound).rev() {
        let j = gv_random(i + 1);
        p.swap(i, j);
    }
    p
}

/// Mersenne Twister of `lib/neatogen/randomkit.c` (`rk_state`), with
/// 64-bit `unsigned long` as on macOS/arm64.
#[derive(Clone)]
pub struct RkState {
    key: [u64; 624],
    pos: usize,
}

impl RkState {
    /// `rk_seed(seed, state)`.
    pub fn new(seed: u64) -> Self {
        let mut key = [0u64; 624];
        let mut seed = seed & 0xffff_ffff;
        for (pos, k) in key.iter_mut().enumerate() {
            *k = seed;
            seed =
                (1_812_433_253u64.wrapping_mul(seed ^ (seed >> 30)) + pos as u64 + 1) & 0xffff_ffff;
        }
        RkState { key, pos: 624 }
    }

    /// `rk_random(state)`: 32 random bits.
    pub fn random(&mut self) -> u64 {
        const N: usize = 624;
        const M: usize = 397;
        const MATRIX_A: u64 = 0x9908_b0df;
        const UPPER: u64 = 0x8000_0000;
        const LOWER: u64 = 0x7fff_ffff;
        if self.pos == N {
            let k = &mut self.key;
            let mut i = 0;
            while i < N - M {
                let y = (k[i] & UPPER) | (k[i + 1] & LOWER);
                k[i] = k[i + M] ^ (y >> 1) ^ ((y & 1) * MATRIX_A);
                i += 1;
            }
            while i < N - 1 {
                let y = (k[i] & UPPER) | (k[i + 1] & LOWER);
                k[i] = k[i + M - N] ^ (y >> 1) ^ ((y & 1) * MATRIX_A);
                i += 1;
            }
            let y = (k[N - 1] & UPPER) | (k[0] & LOWER);
            k[N - 1] = k[M - 1] ^ (y >> 1) ^ ((y & 1) * MATRIX_A);
            self.pos = 0;
        }
        let mut y = self.key[self.pos];
        self.pos += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `rk_interval(max, state)`: uniform integer in `[0, max]`.
    pub fn interval(&mut self, max: u64) -> u64 {
        if max == 0 {
            return 0;
        }
        let mut mask = max;
        for s in [1, 2, 4, 8, 16, 32] {
            mask |= mask >> s;
        }
        if max <= 0xffff_ffff {
            loop {
                let v = self.random() & mask;
                if v <= max {
                    return v;
                }
            }
        }
        loop {
            let hi = self.random();
            let lo = self.random();
            let v = ((hi << 32) | lo) & mask;
            if v <= max {
                return v;
            }
        }
    }
}

/// `Dtoset`/`Dtobag` of `lib/cdt` (`dttree.c`): a splay tree of item ids,
/// insert-only, flattened in order. The position of a new element among
/// equal keys depends on the splay shape (a bag puts it just before the equal
/// element the search stops at), so the tree is reproduced exactly.
#[derive(Debug, Clone)]
pub struct DtTree {
    left: Vec<usize>,
    right: Vec<usize>,
    root: usize,
    bag: bool,
}

const DT_NIL: usize = usize::MAX;

impl DtTree {
    /// `dtopen(disc, Dtobag)` (`bag`) or `Dtoset`.
    pub fn new(bag: bool) -> Self {
        // Slot 0 is the C `link` sentinel; item `i` lives in slot `i + 1`.
        DtTree {
            left: vec![DT_NIL],
            right: vec![DT_NIL],
            root: DT_NIL,
            bag,
        }
    }

    /// `dtinsert(dt, obj)`; `cmp(a, b)` compares the keys of items `a` and
    /// `b`. Returns false when a set already holds an equal key.
    pub fn insert(
        &mut self,
        obj: usize,
        mut cmp: impl FnMut(usize, usize) -> std::cmp::Ordering,
    ) -> bool {
        let me = obj + 1;
        if self.left.len() <= me {
            self.left.resize(me + 1, DT_NIL);
            self.right.resize(me + 1, DT_NIL);
        }
        let c = |a: usize, b: usize, f: &mut dyn FnMut(usize, usize) -> std::cmp::Ordering| {
            f(a - 1, b - 1)
        };
        let (left, right) = (&mut self.left, &mut self.right);
        const S: usize = 0;
        left[S] = DT_NIL;
        right[S] = DT_NIL;
        let (mut l, mut r) = (S, S);
        let mut root = self.root;
        if root != DT_NIL {
            loop {
                match c(me, root, &mut cmp) {
                    std::cmp::Ordering::Equal => break,
                    std::cmp::Ordering::Less => {
                        let t = left[root];
                        if t != DT_NIL {
                            match c(me, t, &mut cmp) {
                                std::cmp::Ordering::Less => {
                                    left[root] = right[t];
                                    right[t] = root;
                                    left[r] = t;
                                    r = t;
                                    root = left[t];
                                    if root == DT_NIL {
                                        break;
                                    }
                                }
                                std::cmp::Ordering::Equal => {
                                    left[r] = root;
                                    r = root;
                                    root = t;
                                    break;
                                }
                                std::cmp::Ordering::Greater => {
                                    right[l] = t;
                                    l = t;
                                    left[r] = root;
                                    r = root;
                                    root = right[t];
                                    if root == DT_NIL {
                                        break;
                                    }
                                }
                            }
                        } else {
                            left[r] = root;
                            r = root;
                            root = DT_NIL;
                            break;
                        }
                    }
                    std::cmp::Ordering::Greater => {
                        let t = right[root];
                        if t != DT_NIL {
                            match c(me, t, &mut cmp) {
                                std::cmp::Ordering::Greater => {
                                    right[root] = left[t];
                                    left[t] = root;
                                    right[l] = t;
                                    l = t;
                                    root = right[t];
                                    if root == DT_NIL {
                                        break;
                                    }
                                }
                                std::cmp::Ordering::Equal => {
                                    right[l] = root;
                                    l = root;
                                    root = t;
                                    break;
                                }
                                std::cmp::Ordering::Less => {
                                    left[r] = t;
                                    r = t;
                                    right[l] = root;
                                    l = root;
                                    root = left[t];
                                    if root == DT_NIL {
                                        break;
                                    }
                                }
                            }
                        } else {
                            right[l] = root;
                            l = root;
                            root = DT_NIL;
                            break;
                        }
                    }
                }
            }
        }
        if root != DT_NIL {
            right[l] = left[root];
            left[r] = right[root];
            if !self.bag {
                // has_root: the equal element stays.
                left[root] = right[S];
                right[root] = left[S];
                self.root = root;
                return false;
            }
            left[root] = DT_NIL;
            right[root] = left[S];
            left[S] = root;
        } else {
            left[r] = DT_NIL;
            right[l] = DT_NIL;
        }
        left[me] = right[S];
        right[me] = left[S];
        self.root = me;
        true
    }

    /// `dtflatten` order: item ids in key order.
    pub fn flatten(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = Vec::new();
        let mut cur = self.root;
        while cur != DT_NIL || !stack.is_empty() {
            while cur != DT_NIL {
                stack.push(cur);
                cur = self.left[cur];
            }
            let n = stack.pop().unwrap();
            out.push(n - 1);
            cur = self.right[n];
        }
        out
    }
}

/// libc++ (`<algorithm>`, as shipped with the macOS SDK) `std::sort` and heap
/// operations for non-arithmetic element types, reproduced exactly: element
/// movement decides the order of equivalent elements, which leaks into C++
/// code such as `lib/vpsc`. `less(a, b)` is the C++ comparator.
pub mod libcxx {
    fn sort3<T: Clone>(
        v: &mut [T],
        x: usize,
        y: usize,
        z: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) -> bool {
        if !c(&v[y], &v[x]) {
            if !c(&v[z], &v[y]) {
                return false;
            }
            v.swap(y, z);
            if c(&v[y], &v[x]) {
                v.swap(x, y);
            }
            return true;
        }
        if c(&v[z], &v[y]) {
            v.swap(x, z);
            return true;
        }
        v.swap(x, y);
        if c(&v[z], &v[y]) {
            v.swap(y, z);
        }
        true
    }

    fn sort4<T: Clone>(
        v: &mut [T],
        x1: usize,
        x2: usize,
        x3: usize,
        x4: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        sort3(v, x1, x2, x3, c);
        if c(&v[x4], &v[x3]) {
            v.swap(x3, x4);
            if c(&v[x3], &v[x2]) {
                v.swap(x2, x3);
                if c(&v[x2], &v[x1]) {
                    v.swap(x1, x2);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sort5<T: Clone>(
        v: &mut [T],
        x1: usize,
        x2: usize,
        x3: usize,
        x4: usize,
        x5: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        sort4(v, x1, x2, x3, x4, c);
        if c(&v[x5], &v[x4]) {
            v.swap(x4, x5);
            if c(&v[x4], &v[x3]) {
                v.swap(x3, x4);
                if c(&v[x3], &v[x2]) {
                    v.swap(x2, x3);
                    if c(&v[x2], &v[x1]) {
                        v.swap(x1, x2);
                    }
                }
            }
        }
    }

    fn insertion_sort<T: Clone>(
        v: &mut [T],
        first: usize,
        last: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        if first == last {
            return;
        }
        for i in first + 1..last {
            let mut j = i - 1;
            if c(&v[i], &v[j]) {
                let t = v[i].clone();
                let mut k = j;
                j = i;
                loop {
                    v[j] = v[k].clone();
                    j = k;
                    if j == first {
                        break;
                    }
                    k -= 1;
                    if !c(&t, &v[k]) {
                        break;
                    }
                }
                v[j] = t;
            }
        }
    }

    fn insertion_sort_unguarded<T: Clone>(
        v: &mut [T],
        first: usize,
        last: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        if first == last {
            return;
        }
        for i in first + 1..last {
            let mut j = i - 1;
            if c(&v[i], &v[j]) {
                let t = v[i].clone();
                let mut k = j;
                j = i;
                loop {
                    v[j] = v[k].clone();
                    j = k;
                    k -= 1;
                    if !c(&t, &v[k]) {
                        break;
                    }
                }
                v[j] = t;
            }
        }
    }

    fn insertion_sort_incomplete<T: Clone>(
        v: &mut [T],
        first: usize,
        last: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) -> bool {
        match last - first {
            0 | 1 => return true,
            2 => {
                if c(&v[last - 1], &v[first]) {
                    v.swap(first, last - 1);
                }
                return true;
            }
            3 => {
                sort3(v, first, first + 1, last - 1, c);
                return true;
            }
            4 => {
                sort4(v, first, first + 1, first + 2, last - 1, c);
                return true;
            }
            5 => {
                sort5(v, first, first + 1, first + 2, first + 3, last - 1, c);
                return true;
            }
            _ => {}
        }
        let mut j = first + 2;
        sort3(v, first, first + 1, j, c);
        const LIMIT: u32 = 8;
        let mut count = 0;
        let mut i = j + 1;
        while i != last {
            if c(&v[i], &v[j]) {
                let t = v[i].clone();
                let mut k = j;
                j = i;
                loop {
                    v[j] = v[k].clone();
                    j = k;
                    if j == first {
                        break;
                    }
                    k -= 1;
                    if !c(&t, &v[k]) {
                        break;
                    }
                }
                v[j] = t;
                count += 1;
                if count == LIMIT {
                    return i + 1 == last;
                }
            }
            j = i;
            i += 1;
        }
        true
    }

    fn partition_with_equals_on_right<T: Clone>(
        v: &mut [T],
        mut first: usize,
        mut last: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) -> (usize, bool) {
        let begin = first;
        let pivot = v[first].clone();
        loop {
            first += 1;
            if !c(&v[first], &pivot) {
                break;
            }
        }
        if begin == first - 1 {
            while first < last {
                last -= 1;
                if c(&v[last], &pivot) {
                    break;
                }
            }
        } else {
            loop {
                last -= 1;
                if c(&v[last], &pivot) {
                    break;
                }
            }
        }
        let already = first >= last;
        while first < last {
            v.swap(first, last);
            loop {
                first += 1;
                if !c(&v[first], &pivot) {
                    break;
                }
            }
            loop {
                last -= 1;
                if c(&v[last], &pivot) {
                    break;
                }
            }
        }
        let pivot_pos = first - 1;
        if begin != pivot_pos {
            v[begin] = v[pivot_pos].clone();
        }
        v[pivot_pos] = pivot;
        (pivot_pos, already)
    }

    fn partition_with_equals_on_left<T: Clone>(
        v: &mut [T],
        mut first: usize,
        mut last: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) -> usize {
        let begin = first;
        let pivot = v[first].clone();
        if c(&pivot, &v[last - 1]) {
            loop {
                first += 1;
                if c(&pivot, &v[first]) {
                    break;
                }
            }
        } else {
            loop {
                first += 1;
                if !(first < last && !c(&pivot, &v[first])) {
                    break;
                }
            }
        }
        if first < last {
            loop {
                last -= 1;
                if !c(&pivot, &v[last]) {
                    break;
                }
            }
        }
        while first < last {
            v.swap(first, last);
            loop {
                first += 1;
                if c(&pivot, &v[first]) {
                    break;
                }
            }
            loop {
                last -= 1;
                if !c(&pivot, &v[last]) {
                    break;
                }
            }
        }
        let pivot_pos = first - 1;
        if begin != pivot_pos {
            v[begin] = v[pivot_pos].clone();
        }
        v[pivot_pos] = pivot;
        first
    }

    fn sift_down<T: Clone>(
        v: &mut [T],
        first: usize,
        len: usize,
        mut start: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        let f = first;
        let len = len as isize;
        let mut child = start as isize;
        if len < 2 || (len - 2) / 2 < child {
            return;
        }
        child = 2 * child + 1;
        if child + 1 < len && c(&v[f + child as usize], &v[f + child as usize + 1]) {
            child += 1;
        }
        if c(&v[f + child as usize], &v[f + start]) {
            return;
        }
        let top = v[f + start].clone();
        loop {
            v[f + start] = v[f + child as usize].clone();
            start = child as usize;
            if (len - 2) / 2 < child {
                break;
            }
            child = 2 * child + 1;
            if child + 1 < len && c(&v[f + child as usize], &v[f + child as usize + 1]) {
                child += 1;
            }
            if c(&v[f + child as usize], &top) {
                break;
            }
        }
        v[f + start] = top;
    }

    fn floyd_sift_down<T: Clone>(
        v: &mut [T],
        first: usize,
        len: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) -> usize {
        let mut hole = first;
        let mut child_i = first;
        let mut child: usize = 0;
        loop {
            child_i += child + 1;
            child = 2 * child + 1;
            if child + 1 < len && c(&v[child_i], &v[child_i + 1]) {
                child_i += 1;
                child += 1;
            }
            v[hole] = v[child_i].clone();
            hole = child_i;
            if child as isize > (len as isize - 2) / 2 {
                return hole;
            }
        }
    }

    fn sift_up<T: Clone>(
        v: &mut [T],
        first: usize,
        mut last: usize,
        len: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        if len > 1 {
            let mut len = (len - 2) / 2;
            let mut ptr = first + len;
            last -= 1;
            if c(&v[ptr], &v[last]) {
                let t = v[last].clone();
                loop {
                    v[last] = v[ptr].clone();
                    last = ptr;
                    if len == 0 {
                        break;
                    }
                    len = (len - 1) / 2;
                    ptr = first + len;
                    if !c(&v[ptr], &t) {
                        break;
                    }
                }
                v[last] = t;
            }
        }
    }

    fn make_heap_range<T: Clone>(
        v: &mut [T],
        first: usize,
        last: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        let n = last - first;
        if n > 1 {
            let mut start = (n as isize - 2) / 2;
            while start >= 0 {
                sift_down(v, first, n, start as usize, c);
                start -= 1;
            }
        }
    }

    fn pop_heap_range<T: Clone>(
        v: &mut [T],
        first: usize,
        mut last: usize,
        len: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        if len > 1 {
            let top = v[first].clone();
            let mut hole = floyd_sift_down(v, first, len, c);
            last -= 1;
            if hole == last {
                v[hole] = top;
            } else {
                v[hole] = v[last].clone();
                hole += 1;
                v[last] = top;
                sift_up(v, first, hole, hole - first, c);
            }
        }
    }

    /// `std::make_heap(v.begin(), v.end(), less)`.
    pub fn make_heap<T: Clone>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
        let n = v.len();
        make_heap_range(v, 0, n, &mut less);
    }

    /// `std::push_heap(v.begin(), v.end(), less)` (the new element is last).
    pub fn push_heap<T: Clone>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
        let n = v.len();
        sift_up(v, 0, n, n, &mut less);
    }

    /// `std::pop_heap(v.begin(), v.end(), less)` (the top moves last).
    pub fn pop_heap<T: Clone>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
        let n = v.len();
        pop_heap_range(v, 0, n, n, &mut less);
    }

    fn partial_sort_all<T: Clone>(
        v: &mut [T],
        first: usize,
        last: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
    ) {
        if first == last {
            return;
        }
        make_heap_range(v, first, last, c);
        let mut l = last;
        let mut n = last - first;
        while n > 1 {
            pop_heap_range(v, first, l, n, c);
            l -= 1;
            n -= 1;
        }
    }

    fn introsort<T: Clone>(
        v: &mut [T],
        mut first: usize,
        mut last: usize,
        c: &mut impl FnMut(&T, &T) -> bool,
        mut depth: isize,
        mut leftmost: bool,
    ) {
        const LIMIT: usize = 24;
        const NINTHER: usize = 128;
        loop {
            let len = last - first;
            match len {
                0 | 1 => return,
                2 => {
                    if c(&v[last - 1], &v[first]) {
                        v.swap(first, last - 1);
                    }
                    return;
                }
                3 => {
                    sort3(v, first, first + 1, last - 1, c);
                    return;
                }
                4 => {
                    sort4(v, first, first + 1, first + 2, last - 1, c);
                    return;
                }
                5 => {
                    sort5(v, first, first + 1, first + 2, first + 3, last - 1, c);
                    return;
                }
                _ => {}
            }
            if len < LIMIT {
                if leftmost {
                    insertion_sort(v, first, last, c);
                } else {
                    insertion_sort_unguarded(v, first, last, c);
                }
                return;
            }
            if depth == 0 {
                partial_sort_all(v, first, last, c);
                return;
            }
            depth -= 1;
            let half = len / 2;
            if len > NINTHER {
                sort3(v, first, first + half, last - 1, c);
                sort3(v, first + 1, first + (half - 1), last - 2, c);
                sort3(v, first + 2, first + (half + 1), last - 3, c);
                sort3(v, first + (half - 1), first + half, first + (half + 1), c);
                v.swap(first, first + half);
            } else {
                sort3(v, first + half, first, last - 1, c);
            }
            if !leftmost && !c(&v[first - 1], &v[first]) {
                first = partition_with_equals_on_left(v, first, last, c);
                continue;
            }
            let (mut i, already) = partition_with_equals_on_right(v, first, last, c);
            if already {
                let fs = insertion_sort_incomplete(v, first, i, c);
                if insertion_sort_incomplete(v, i + 1, last, c) {
                    if fs {
                        return;
                    }
                    last = i;
                    continue;
                } else if fs {
                    i += 1;
                    first = i;
                    continue;
                }
            }
            introsort(v, first, i, c, depth, leftmost);
            leftmost = false;
            first = i + 1;
        }
    }

    /// `std::sort(v.begin(), v.end(), less)`.
    pub fn sort<T: Clone>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
        let n = v.len();
        if n == 0 {
            return;
        }
        let depth = 2 * (usize::BITS - 1 - n.leading_zeros()) as isize;
        introsort(v, 0, n, &mut less, depth, true);
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn sorts_and_heaps() {
            let mut s: u32 = 7;
            for n in 0..400usize {
                let mut v: Vec<(u32, usize)> = (0..n)
                    .map(|i| {
                        s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
                        ((s >> 16) % 13, i)
                    })
                    .collect();
                super::sort(&mut v, |a, b| a.0 < b.0);
                assert!(v.windows(2).all(|w| w[0].0 <= w[1].0));
                let mut h: Vec<u32> = v.iter().map(|x| x.0 * 7 % 11).collect();
                super::make_heap(&mut h, |a, b| a < b);
                let mut out = Vec::new();
                while !h.is_empty() {
                    super::pop_heap(&mut h, |a, b| a < b);
                    out.push(h.pop().unwrap());
                }
                assert!(out.windows(2).all(|w| w[0] >= w[1]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drand48_matches_libc() {
        let mut r = Drand48State::default();
        r.srand48(1);
        assert_eq!(r.drand48(), 0.041_630_344_771_878_214);
        assert_eq!(r.drand48(), 0.454_492_444_728_629_15);
        assert_eq!(r.drand48(), 0.834_817_218_166_914_9);
    }

    // Issue #290: handles share the installed process state, which carries over between the
    // layouts of one run; outside a run each layout starts fresh.
    #[test]
    fn generators_share_one_state_per_run() {
        let mut expected = Drand48State::default();
        let mut state = RandState::default();
        let first = with_rand_state(&mut state, || {
            let a = drand48();
            in_rand_run(|| (a, drand48(), rand()))
        });
        assert_eq!(first, (expected.drand48(), expected.drand48(), 16807));
        assert_eq!(state.drand48, expected);
        // The next layout of the run continues where the last one stopped.
        let next = with_rand_state(&mut state, drand48);
        assert_eq!(next, expected.drand48());
        // srand48 reseeds the shared state.
        with_rand_state(&mut state, || {
            srand48(173);
            drand48()
        });
        let mut seeded = Drand48State::default();
        seeded.srand48(173);
        seeded.drand48();
        assert_eq!(state.drand48, seeded);
        // Outside a run, a layout starts like a new process, and leaves nothing behind.
        let fresh = in_rand_run(drand48);
        assert_eq!(fresh, Drand48State::default().drand48());
        assert_eq!(in_rand_run(drand48), fresh);
        // A panic inside a run still leaves its draws in the run's state.
        let before = state;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_rand_state(&mut state, || {
                drand48();
                panic!("layout failed");
            })
        }));
        assert!(caught.is_err());
        let mut after = before.drand48;
        after.drand48();
        assert_eq!(state.drand48, after);
    }

    #[test]
    fn rand_matches_libc() {
        let mut r = CRandState::default();
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
