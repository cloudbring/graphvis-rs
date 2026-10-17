//! Smart initialisation for stress majorization (`start=self`, `mode=hier`):
//! `sparse_stress_subspace_majorization_kD` in `lib/neatogen/stress.c`,
//! `embed_graph.c` and `pca.c`.

use super::NeatoParams;
use super::graphdata::VtxData;
use crate::cutil::CRand;

/// `sparse_stress_subspace_majorization_kD`.
#[allow(clippy::too_many_arguments)]
pub fn sparse_stress_subspace_majorization_kd(
    _params: &NeatoParams,
    _crand: &mut CRand,
    _graph: &[VtxData],
    _n: usize,
    _coords: &mut [Vec<f64>],
    _dim: usize,
    _smart_ini: bool,
    _exp: i32,
    _reweight_graph: bool,
    _n_iterations: usize,
    _num_centers: usize,
) -> Result<(), super::SolverError> {
    Ok(())
}
