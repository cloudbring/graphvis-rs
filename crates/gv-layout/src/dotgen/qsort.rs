//! Exact port of the platform `qsort(3)` used to produce the C goldens.
//!
//! Graphviz sorts with `qsort`, which is not stable, so the order of equal
//! elements leaks into layouts (e.g. `TB_balance` in `ns.c`). The goldens were
//! produced on macOS, whose libc `qsort` is Apple's FreeBSD-derived
//! Bentley-McIlroy quicksort with a bounded insertion-sort switch and a
//! heapsort fallback. The implementation is shared with the other engines in
//! [`crate::cutil::qsort_by`]; `qsort_vectors.txt` holds permutations recorded
//! from the system `qsort` to guard it.

use std::cmp::Ordering;

/// Sorts `a` with the exact element movement of Apple libc `qsort`.
pub fn qsort<T: Copy>(a: &mut [T], cmp: &mut impl FnMut(&T, &T) -> Ordering) {
    crate::cutil::qsort_by(a, |x, y| match cmp(x, y) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    });
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
            let keys: Vec<i32> = keys
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
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
