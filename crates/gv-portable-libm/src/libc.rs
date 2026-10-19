//! `qsort`, `rand` and `srand` with Apple libc's behaviour, exported for `LD_PRELOAD` on Linux.

use gv_layout::cutil::{CRand, qsort_by};
use std::ffi::{c_int, c_uint, c_void};
use std::sync::Mutex;

/// C's process-wide `rand` state.
static RAND: Mutex<CRand> = Mutex::new(CRand::new_unseeded());

#[unsafe(no_mangle)]
pub extern "C" fn rand() -> c_int {
    RAND.lock().unwrap().rand()
}

#[unsafe(no_mangle)]
pub extern "C" fn srand(seed: c_uint) {
    RAND.lock().unwrap().srand(seed);
}

/// Apple's `qsort`. The algorithm only compares and moves elements, so sorting element
/// indices with the same comparisons and then permuting the bytes gives the same order. The
/// comparator sees pointers into a copy of the array rather than into the array itself.
///
/// # Safety
/// As for C's `qsort`: `base` holds `n` elements of `size` bytes, and `cmp` is a valid
/// comparator for them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsort(
    base: *mut c_void,
    n: usize,
    size: usize,
    cmp: unsafe extern "C" fn(*const c_void, *const c_void) -> c_int,
) {
    if n < 2 || size == 0 {
        return;
    }
    let bytes = n * size;
    // SAFETY: the caller guarantees `base` holds `n * size` bytes.
    let array = unsafe { std::slice::from_raw_parts_mut(base.cast::<u8>(), bytes) };
    let copy = array.to_vec();
    let at = |i: usize| copy[i * size..].as_ptr().cast::<c_void>();
    let mut order: Vec<usize> = (0..n).collect();
    // SAFETY: both pointers address elements of `copy`, which outlives the sort.
    qsort_by(&mut order, |&a, &b| unsafe { cmp(at(a), at(b)) });
    for (dst, &src) in order.iter().enumerate() {
        array[dst * size..(dst + 1) * size].copy_from_slice(&copy[src * size..(src + 1) * size]);
    }
}
