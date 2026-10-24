//! Port of `lib/sfdpgen/Multilevel.c`: the coarsening hierarchy of sfdp
//! (heaviest-edge matching with supervariables first).
//!
//! The levels are a `Vec` (index 0 is the finest); C's `next`/`prev`
//! pointers become index arithmetic.

use crate::cutil::gv_permutation;
use crate::sparse::SparseMatrix;

const MINSIZE: usize = 4;
const MIN_COARSEN_FACTOR: f64 = 0.75;
/// `MAX_CLUSTER_SIZE`.
pub const MAX_CLUSTER_SIZE: i32 = 4;

/// One level of `Multilevel_struct`.
#[derive(Debug, Clone)]
pub struct Level {
    pub level: i32,
    pub n: usize,
    /// The weighting matrix.
    pub a: SparseMatrix,
    /// Prolongation from the next coarser level to this one is stored on
    /// the coarser level, as in C (`cgrid->P`).
    pub p: Option<SparseMatrix>,
    /// Restriction from this level to the next coarser one (`grid->R`).
    pub r: Option<SparseMatrix>,
}

/// `Multilevel_control`.
#[derive(Debug, Clone, Copy)]
pub struct MultilevelControl {
    pub maxlevel: i32,
}

/// `maximal_independent_edge_set_heavest_edge_pernode_supernodes_first(A, ...)`.
fn maximal_independent_edge_set_heavest_edge_pernode_supernodes_first(
    a: &SparseMatrix,
) -> (Vec<i32>, Vec<i32>, usize) {
    const MATCHED: usize = usize::MAX;
    let (ia, ja) = (&a.ia, &a.ja);
    let m = a.m;
    let av = a.a.as_ref().expect("real matrix");
    let mut cluster = vec![0i32; m];
    let mut clusterp = vec![0i32; m + 1];
    let mut matched: Vec<usize> = (0..m).collect();
    let (sup, superp) = a.decompose_to_supervariables();
    let nsuper = superp.len() - 1;
    let mut ncluster = 0usize;
    clusterp[0] = 0;
    let mut nz = 0i32;
    for i in 0..nsuper {
        if superp[i + 1] - superp[i] <= 1 {
            continue;
        }
        let mut nz0 = clusterp[ncluster];
        for j in superp[i]..superp[i + 1] {
            let s = sup[j as usize];
            matched[s as usize] = MATCHED;
            cluster[nz as usize] = s;
            nz += 1;
            if nz - nz0 >= MAX_CLUSTER_SIZE {
                ncluster += 1;
                clusterp[ncluster] = nz;
                nz0 = nz;
            }
        }
        if nz > nz0 {
            ncluster += 1;
            clusterp[ncluster] = nz;
        }
    }
    let p = gv_permutation(m);
    for &i in &p {
        if matched[i] == MATCHED {
            continue;
        }
        let mut first = true;
        let mut amax = 0.0f64;
        let mut jamax = 0usize;
        for j in ia[i]..ia[i + 1] {
            let j = j as usize;
            let c = ja[j] as usize;
            if i == c {
                continue;
            }
            if matched[c] != MATCHED && matched[i] != MATCHED {
                if first {
                    amax = av[j];
                    jamax = c;
                    first = false;
                } else if av[j] > amax {
                    amax = av[j];
                    jamax = c;
                }
            }
        }
        if !first {
            matched[jamax] = MATCHED;
            matched[i] = MATCHED;
            cluster[nz as usize] = i as i32;
            nz += 1;
            cluster[nz as usize] = jamax as i32;
            nz += 1;
            ncluster += 1;
            clusterp[ncluster] = nz;
        }
    }
    for i in 0..m {
        if matched[i] == i {
            cluster[nz as usize] = i as i32;
            nz += 1;
            ncluster += 1;
            clusterp[ncluster] = nz;
        }
    }
    (cluster, clusterp, ncluster)
}

/// `Multilevel_coarsen_internal(A, &cA, &P, &R)`.
fn multilevel_coarsen_internal(
    a: &SparseMatrix,
) -> Option<(SparseMatrix, SparseMatrix, SparseMatrix)> {
    let n = a.m;
    let (cluster, clusterp, ncluster) =
        maximal_independent_edge_set_heavest_edge_pernode_supernodes_first(a);
    let nc = ncluster;
    if nc == n || nc < MINSIZE {
        return None;
    }
    let mut irn = Vec::with_capacity(n);
    let mut jcn = Vec::with_capacity(n);
    let mut val = Vec::with_capacity(n);
    for i in 0..ncluster {
        for j in clusterp[i]..clusterp[i + 1] {
            irn.push(cluster[j as usize]);
            jcn.push(i as i32);
            val.push(1.0);
        }
    }
    let p = crate::sparse::from_coordinate_arrays(irn.len(), n, nc, &irn, &jcn, Some(&val), true);
    let mut r = p.transpose();
    let mut ca = r.multiply3(a, &p)?;
    r.divide_row_by_degree();
    ca.is_symmetric = true;
    ca.is_pattern_symmetric = true;
    ca.remove_diagonal();
    Some((ca, p, r))
}

/// `Multilevel_coarsen(A, &cA, &P, &R)`.
fn multilevel_coarsen(a0: &SparseMatrix) -> Option<(SparseMatrix, SparseMatrix, SparseMatrix)> {
    let n = a0.n;
    let mut out: Option<(SparseMatrix, SparseMatrix, SparseMatrix)> = None;
    loop {
        let a = out.as_ref().map_or(a0, |o| &o.0);
        let Some((ca0, p0, r0)) = multilevel_coarsen_internal(a) else {
            return out;
        };
        let nc = ca0.n;
        out = Some(match out.take() {
            Some((_, p, r)) => {
                let pm = p.multiply(&p0).expect("P product");
                let rm = r0.multiply(&r).expect("R product");
                (ca0, pm, rm)
            }
            None => (ca0, p0, r0),
        });
        if (nc as f64) <= MIN_COARSEN_FACTOR * n as f64 {
            return out;
        }
    }
}

/// `Multilevel_new(A0, ctrl)`: the hierarchy, finest level first.
pub fn multilevel_new(a0: &SparseMatrix, ctrl: MultilevelControl) -> Vec<Level> {
    let mut a = a0.clone();
    if !a.is_symmetric(false) || !a.is_real() {
        a = a0.real_adjacency_symmetrized();
    }
    let mut grid = vec![Level {
        level: 0,
        n: a.n,
        a,
        p: None,
        r: None,
    }];
    // Multilevel_establish
    loop {
        let cur = grid.len() - 1;
        if grid[cur].level >= ctrl.maxlevel - 1 {
            break;
        }
        let Some((ca, p, r)) = multilevel_coarsen(&grid[cur].a) else {
            break;
        };
        grid[cur].r = Some(r);
        let level = grid[cur].level + 1;
        grid.push(Level {
            level,
            n: ca.m,
            a: ca,
            p: Some(p),
            r: None,
        });
    }
    grid
}
