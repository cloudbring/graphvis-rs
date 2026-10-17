//! `StressMajorizationSmoother_smooth` (`lib/sfdpgen/post_process.c`) and
//! `SparseMatrix_solve` (`lib/sfdpgen/sparse_solve.c`).

use crate::cutil::CRand;
use crate::sparse::{SparseMatrix, distance, drand, vector_product};

/// `StressMajorizationSmoother_struct` (normal scheme, no edge-label data).
pub struct StressMajorizationSmoother {
    pub lw: SparseMatrix,
    pub lwd: SparseMatrix,
    pub lambda: Vec<f64>,
    pub tol_cg: f64,
    pub maxit_cg: f64,
}

fn diag_precon_new(a: &SparseMatrix) -> Vec<f64> {
    let v = a.a.as_ref().unwrap();
    let mut diag = vec![1.0f64; a.m];
    for (i, d) in diag.iter_mut().enumerate() {
        for j in a.ia[i]..a.ia[i + 1] {
            let j = j as usize;
            if a.ja[j] == i as i32 && v[j].abs() > 0.0 {
                *d = 1.0 / v[j];
            }
        }
    }
    diag
}

fn conjugate_gradient(
    a: &SparseMatrix,
    precon: &[f64],
    n: usize,
    x: &mut [f64],
    rhs: &[f64],
    tol: f64,
    maxit: f64,
) -> f64 {
    let mut z = vec![0.0f64; n];
    let mut r = vec![0.0f64; n];
    let mut p = vec![0.0f64; n];
    let mut q = vec![0.0f64; n];
    a.multiply_vector(x, &mut r);
    for i in 0..n {
        r[i] = rhs[i] - r[i];
    }
    let res0 = vector_product(n, &r, &r).sqrt() / n as f64;
    let mut res = res0;
    let mut rho_old = 1.0f64;
    let mut iter = 0i64;
    loop {
        let cont = (iter as f64) < maxit && res > tol * res0;
        iter += 1;
        if !cont {
            break;
        }
        for i in 0..n {
            z[i] = r[i] * precon[i];
        }
        let rho = vector_product(n, &r, &z);
        if iter > 1 {
            let beta = rho / rho_old;
            for i in 0..n {
                p[i] = beta.mul_add(p[i], z[i]);
            }
        } else {
            p.copy_from_slice(&z);
        }
        a.multiply_vector(&p, &mut q);
        let alpha = rho / vector_product(n, &p, &q);
        for i in 0..n {
            x[i] = alpha.mul_add(p[i], x[i]);
        }
        for i in 0..n {
            r[i] = (-alpha).mul_add(q[i], r[i]);
        }
        res = vector_product(n, &r, &r).sqrt() / n as f64;
        rho_old = rho;
    }
    res
}

/// `SparseMatrix_solve(A, dim, x0, rhs, tol, maxit)`: the solution replaces `rhs`.
pub fn sparse_matrix_solve(
    a: &SparseMatrix,
    dim: usize,
    x0: &[f64],
    rhs: &mut [f64],
    tol: f64,
    maxit: f64,
) -> f64 {
    let n = a.m;
    let precon = diag_precon_new(a);
    let mut res = 0.0;
    let mut x = vec![0.0f64; n];
    let mut b = vec![0.0f64; n];
    for k in 0..dim {
        for i in 0..n {
            x[i] = x0[i * dim + k];
            b[i] = rhs[i * dim + k];
        }
        res += conjugate_gradient(a, &precon, n, &mut x, &b, tol, maxit);
        for i in 0..n {
            rhs[i * dim + k] = x[i];
        }
    }
    res
}

fn total_distance(m: usize, dim: usize, x: &[f64], y: &[f64]) -> f64 {
    let mut total = 0.0;
    for i in 0..m {
        let mut d = 0.0f64;
        for j in 0..dim {
            let t = y[i * dim + j] - x[i * dim + j];
            d = t.mul_add(t, d);
        }
        total += d.sqrt();
    }
    total
}

impl StressMajorizationSmoother {
    /// `StressMajorizationSmoother_smooth(sm, dim, x, maxit_sm)`.
    pub fn smooth(&mut self, dim: usize, x: &mut [f64], maxit_sm: i32, crand: &mut CRand) -> f64 {
        let mut lwdd = self.lwd.clone();
        let m = self.lw.m;
        let x0 = x[..dim * m].to_vec();
        let mut y = vec![0.0f64; dim * m];
        let (id, jd) = (&self.lwd.ia, &self.lwd.ja);
        let d = self.lwd.a.as_ref().unwrap();
        let w = self.lw.a.as_ref().unwrap();
        let mut diff = 1.0f64;
        let tol = 0.001;
        let mut iter = 0;
        loop {
            let cont = iter < maxit_sm && diff > tol;
            iter += 1;
            if !cont {
                break;
            }
            {
                let dd = lwdd.a.as_mut().unwrap();
                for i in 0..m {
                    let mut idiag = None;
                    let mut diag = 0.0;
                    for j in id[i]..id[i + 1] {
                        let j = j as usize;
                        let c = jd[j] as usize;
                        if c == i {
                            idiag = Some(j);
                            continue;
                        }
                        let mut dist = distance(x, dim, i, c);
                        if d[j] == 0.0 {
                            dd[j] = 0.0;
                        } else {
                            if dist == 0.0 {
                                let dij = d[j] / w[j];
                                for k in 0..dim {
                                    let r = drand(crand);
                                    x[c * dim + k] =
                                        (0.0001 * (r + 0.0001)).mul_add(dij, x[c * dim + k]);
                                }
                                dist = distance(x, dim, i, c);
                            }
                            dd[j] = d[j] / dist;
                        }
                        diag += dd[j];
                    }
                    dd[idiag.expect("diagonal entry")] = -diag;
                }
            }
            lwdd.multiply_dense(x, &mut y, dim);
            for i in 0..m {
                for j in 0..dim {
                    y[i * dim + j] = self.lambda[i].mul_add(x0[i * dim + j], y[i * dim + j]);
                }
            }
            sparse_matrix_solve(&self.lw, dim, x, &mut y, self.tol_cg, self.maxit_cg);
            diff = total_distance(m, dim, x, &y) / vector_product(m * dim, x, x).sqrt();
            x[..m * dim].copy_from_slice(&y);
        }
        diff
    }
}
