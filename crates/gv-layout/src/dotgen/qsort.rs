//! Exact port of the platform `qsort(3)` used to produce the C goldens.
//!
//! Graphviz sorts with `qsort`, which is not stable, so the order of equal
//! elements leaks into layouts (e.g. `TB_balance` in `ns.c`). The goldens were
//! produced on macOS, whose libc `qsort` is Apple's FreeBSD-derived
//! Bentley-McIlroy quicksort (`Libc/stdlib/FreeBSD/qsort.c`) with a bounded
//! insertion-sort switch and a heapsort fallback past `2 * (fls(n) - 1)`
//! recursion levels (`Libc/stdlib/FreeBSD/heapsort.c`). This module reproduces
//! both element-move for element-move; `qsort_vectors.txt` holds permutations
//! recorded from the system `qsort` to guard the port.

use std::cmp::Ordering;

/// Sorts `a` with the exact element movement of Apple libc `qsort`.
pub fn qsort<T: Copy>(a: &mut [T], cmp: &mut impl FnMut(&T, &T) -> Ordering) {
    let n = a.len();
    let depth = if n == 0 {
        0
    } else {
        2 * ((usize::BITS - n.leading_zeros()) as i32 - 1)
    };
    qsort_inner(a, 0, n, cmp, depth);
}

#[inline]
fn ord_to_int(o: Ordering) -> i32 {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// Insertion sort; returns false when `swap_limit` (non-zero) is exceeded.
fn isort<T: Copy>(
    a: &mut [T],
    base: usize,
    n: usize,
    cmp: &mut impl FnMut(&T, &T) -> Ordering,
    swap_limit: usize,
) -> bool {
    let mut swap_cnt = 0usize;
    let mut pm = base + 1;
    while pm < base + n {
        let mut pl = pm;
        while pl > base && cmp(&a[pl - 1], &a[pl]) == Ordering::Greater {
            a.swap(pl, pl - 1);
            if swap_limit != 0 {
                swap_cnt += 1;
                if swap_cnt > swap_limit {
                    return false;
                }
            }
            pl -= 1;
        }
        pm += 1;
    }
    true
}

fn med3<T: Copy>(
    a: &[T],
    x: usize,
    y: usize,
    z: usize,
    cmp: &mut impl FnMut(&T, &T) -> Ordering,
) -> usize {
    if ord_to_int(cmp(&a[x], &a[y])) < 0 {
        if ord_to_int(cmp(&a[y], &a[z])) < 0 {
            y
        } else if ord_to_int(cmp(&a[x], &a[z])) < 0 {
            z
        } else {
            x
        }
    } else if ord_to_int(cmp(&a[y], &a[z])) > 0 {
        y
    } else if ord_to_int(cmp(&a[x], &a[z])) < 0 {
        x
    } else {
        z
    }
}

fn vecswap<T: Copy>(a: &mut [T], x: usize, y: usize, n: usize) {
    for i in 0..n {
        a.swap(x + i, y + i);
    }
}

fn qsort_inner<T: Copy>(
    arr: &mut [T],
    mut a: usize,
    mut n: usize,
    cmp: &mut impl FnMut(&T, &T) -> Ordering,
    mut depth_limit: i32,
) {
    loop {
        let mut swap_cnt = 0;
        let dl = depth_limit;
        depth_limit -= 1;
        if dl <= 0 {
            heapsort(&mut arr[a..a + n], cmp);
            return;
        }
        if n <= 7 {
            isort(arr, a, n, cmp, 0);
            return;
        }
        let mut pl = a;
        let mut pm = a + n / 2;
        let mut pn = a + n - 1;
        if n > 40 {
            let d = n / 8;
            pl = med3(arr, pl, pl + d, pl + 2 * d, cmp);
            pm = med3(arr, pm - d, pm, pm + d, cmp);
            pn = med3(arr, pn - 2 * d, pn - d, pn, cmp);
        }
        pm = med3(arr, pl, pm, pn, cmp);
        arr.swap(a, pm);
        // Positions are kept as signed offsets so `pc` may step below `pb`.
        let mut pa = a + 1;
        let mut pb = a + 1;
        let mut pc: isize = (a + n - 1) as isize;
        let mut pd: isize = (a + n - 1) as isize;
        loop {
            while pb as isize <= pc {
                let r = ord_to_int(cmp(&arr[pb], &arr[a]));
                if r > 0 {
                    break;
                }
                if r == 0 {
                    swap_cnt = 1;
                    arr.swap(pa, pb);
                    pa += 1;
                }
                pb += 1;
            }
            while pb as isize <= pc {
                let r = ord_to_int(cmp(&arr[pc as usize], &arr[a]));
                if r < 0 {
                    break;
                }
                if r == 0 {
                    swap_cnt = 1;
                    arr.swap(pc as usize, pd as usize);
                    pd -= 1;
                }
                pc -= 1;
            }
            if pb as isize > pc {
                break;
            }
            arr.swap(pb, pc as usize);
            swap_cnt = 1;
            pb += 1;
            pc -= 1;
        }
        let pn = a + n;
        let d1 = (pa - a).min(pb - pa);
        if d1 > 0 {
            vecswap(arr, a, pb - d1, d1);
        }
        let pdu = pd as isize;
        let d1b = ((pdu - pc) as usize).min((pn as isize - pdu - 1) as usize);
        if d1b > 0 {
            vecswap(arr, pb, pn - d1b, d1b);
        }
        if swap_cnt == 0 {
            let r = 1 + n / 4;
            if isort(arr, a, n, cmp, r) {
                return;
            }
        }
        let d1 = pb - pa;
        let d2 = (pdu - pc) as usize;
        if d1 <= d2 {
            if d1 > 1 {
                qsort_inner(arr, a, d1, cmp, depth_limit);
            }
            if d2 > 1 {
                a = pn - d2;
                n = d2;
                continue;
            }
        } else {
            if d2 > 1 {
                qsort_inner(arr, pn - d2, d2, cmp, depth_limit);
            }
            if d1 > 1 {
                n = d1;
                continue;
            }
        }
        return;
    }
}

/// Apple libc `heapsort` (Knuth vol. 3, with the bottom-up SELECT optimisation).
fn heapsort<T: Copy>(v: &mut [T], cmp: &mut impl FnMut(&T, &T) -> Ordering) {
    let mut nmemb = v.len();
    if nmemb <= 1 {
        return;
    }
    // 1-based indexing as in the C source: element i lives at v[i - 1].
    let lt = |cmp: &mut dyn FnMut(&T, &T) -> Ordering, v: &[T], i: usize, j: usize| {
        cmp(&v[i - 1], &v[j - 1])
    };
    let mut cmpd = |x: &T, y: &T| cmp(x, y);
    // CREATE
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
            if child_i < nmemb && lt(&mut cmpd, v, child_i, child_i + 1) == Ordering::Less {
                child_i += 1;
            }
            if lt(&mut cmpd, v, child_i, par_i) != Ordering::Greater {
                break;
            }
            v.swap(par_i - 1, child_i - 1);
            par_i = child_i;
        }
    }
    while nmemb > 1 {
        let k = v[nmemb - 1];
        v[nmemb - 1] = v[0];
        nmemb -= 1;
        // SELECT
        let mut par_i = 1;
        loop {
            let mut child_i = par_i * 2;
            if child_i > nmemb {
                break;
            }
            if child_i < nmemb && lt(&mut cmpd, v, child_i, child_i + 1) == Ordering::Less {
                child_i += 1;
            }
            v[par_i - 1] = v[child_i - 1];
            par_i = child_i;
        }
        loop {
            let child_i = par_i;
            par_i = child_i / 2;
            if child_i == 1 || cmpd(&k, &v[par_i - 1]) == Ordering::Less {
                v[child_i - 1] = k;
                break;
            }
            v[child_i - 1] = v[par_i - 1];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_recorded_libc_permutations() {
        let data = include_str!("qsort_vectors.txt");
        let mut lines = data.lines();
        let mut cases = 0;
        while let (Some(keys), Some(ids)) = (lines.next(), lines.next()) {
            let keys: Vec<i32> = keys.split_whitespace().map(|s| s.parse().unwrap()).collect();
            let want: Vec<usize> = ids.split_whitespace().map(|s| s.parse().unwrap()).collect();
            let mut items: Vec<(i32, usize)> = keys.iter().copied().zip(0..).collect();
            qsort(&mut items, &mut |a, b| a.0.cmp(&b.0));
            let got: Vec<usize> = items.iter().map(|x| x.1).collect();
            assert_eq!(got, want, "case {cases} n={}", keys.len());
            cases += 1;
        }
        assert!(cases > 100);
    }
}
