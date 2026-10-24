//! Port of `lib/sfdpgen/stress_model.c`: sparse stress majorization
//! towards the edge lengths of `B`. Graphviz 16.1.0 compiles it into the
//! sfdp library, but `sfdp_layout` does not call it.

use super::post_process::sparse_stress_majorization_smoother_new;
use crate::sparse::SparseMatrix;

/// `stress_model(B, x, maxit_sm)` in two dimensions; `Err(())` when the
/// smoother cannot be built (C returns -1).
#[allow(clippy::result_unit_err)]
pub fn stress_model(b: &SparseMatrix, x: &mut [f64], maxit_sm: i32) -> Result<(), ()> {
    let dim = 2;
    let mut a = b.clone();
    if !a.is_symmetric(false) || !a.is_real() {
        if a.is_real() {
            a = a.symmetrize(false);
            a.remove_diagonal();
        } else {
            a = a.real_adjacency_symmetrized();
        }
    }
    a.remove_diagonal();
    let m = a.m;
    let Some((mut sm, scaling)) = sparse_stress_majorization_smoother_new(&a, dim, x) else {
        return Err(());
    };
    sm.tol_cg = 0.1;
    sm.smooth(dim, x, maxit_sm);
    for v in x.iter_mut().take(dim * m) {
        *v /= scaling;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sparse::from_coordinate_arrays;

    #[test]
    fn stress_model_reaches_edge_lengths() {
        // A path a-b-c with lengths 1 and 2.
        let irn = [0, 1];
        let jcn = [1, 2];
        let val = [1.0, 2.0];
        let b = from_coordinate_arrays(2, 3, 3, &irn, &jcn, Some(&val), true);
        let mut x = vec![0.0, 0.0, 1.0, 0.1, 2.0, -0.1];
        crate::cutil::with_rand_state(&mut crate::cutil::RandState::default(), || {
            stress_model(&b, &mut x, 100).unwrap();
        });
        let d = |i: usize, j: usize| {
            let (dx, dy) = (x[2 * i] - x[2 * j], x[2 * i + 1] - x[2 * j + 1]);
            (dx * dx + dy * dy).sqrt()
        };
        assert!((d(1, 2) / d(0, 1) - 2.0).abs() < 0.05, "{x:?}");
    }
}
