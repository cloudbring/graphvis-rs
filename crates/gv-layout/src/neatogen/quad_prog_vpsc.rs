//! Port of `lib/neatogen/quad_prog_vpsc.c`: the VPSC environment for
//! constrained stress majorization (`initCMajVPSC`,
//! `constrained_majorization_vpsc`, `generateNonoverlapConstraints`,
//! `deleteCMajEnvVPSC`), `removeoverlaps` (used by `overlap=vpsc`) and the
//! DiG-CoLa level helpers.

use crate::vpsc::{self, BoxF, Solver};

/// `cluster_data`.
#[derive(Debug, Clone, Default)]
pub struct ClusterData {
    /// Total count of vars in clusters.
    pub nvars: usize,
    pub clusters: Vec<Vec<usize>>,
    pub toplevel: Vec<usize>,
    pub bb: Vec<BoxF>,
}

impl ClusterData {
    pub fn nclusters(&self) -> usize {
        self.clusters.len()
    }
}

/// `ipsep_options`.
#[derive(Debug, Clone, Default)]
pub struct IpsepOptions {
    /// 1 = directed edge constraints, 2 = DiG-CoLa level constraints.
    pub diredges: i32,
    pub edge_gap: f64,
    /// 1 = non-overlap constraints during layout, 2 = remove afterwards.
    pub noverlap: i32,
    pub gap: (f64, f64),
    /// Node widths and heights.
    pub nsize: Vec<(f64, f64)>,
    pub clusters: ClusterData,
}

/// `DigColaLevel`.
#[derive(Debug, Clone, Default)]
pub struct DigColaLevel {
    pub nodes: Vec<i32>,
}

/// `CMajEnvVPSC`.
#[derive(Debug, Clone, Default)]
pub struct CMajEnvVpsc {
    pub a: Vec<Vec<f32>>,
    /// Number of actual vars.
    pub nv: usize,
    /// Dummy nodes included in the Laplacian (cluster boundaries).
    pub nldv: usize,
    /// Dummy nodes not included in the Laplacian (DiG-CoLa boundaries).
    pub ndv: usize,
    pub vs: Vec<usize>,
    /// Total number of constraints for the next iteration (`cs.len()`).
    pub m: usize,
    /// Number of global constraints (a prefix of `cs`).
    pub gm: usize,
    pub cs: Vec<usize>,
    pub gcs: Vec<usize>,
    pub solver: Solver,
    pub f_array1: Vec<f32>,
    pub f_array2: Vec<f32>,
    pub f_array3: Vec<f32>,
}

const QUAD_PROG_TOL: f32 = 1e-4;

/// `unpackMatrix(packedMat, n)` from `matrix_ops.c`.
pub fn unpack_matrix(packed: &[f32], n: usize) -> Vec<Vec<f32>> {
    let mut mat = vec![vec![0.0f32; n]; n];
    let mut k = 0;
    for i in 0..n {
        for j in i..n {
            let v = packed[k];
            mat[i][j] = v;
            mat[j][i] = v;
            k += 1;
        }
    }
    mat
}

impl CMajEnvVpsc {
    fn satisfy_place(&mut self, place: &mut [f32], n: usize) {
        for i in 0..n {
            self.solver.vars[self.vs[i]].desired_position = place[i] as f64;
        }
        // satisfyVPSC: a thrown std::runtime_error terminates C; ignore.
        let _ = self.solver.satisfy();
        for i in 0..n {
            place[i] = self.solver.position(self.vs[i]) as f32;
        }
    }

    /// `constrained_majorization_vpsc(e, b, place, max_iterations)`:
    /// gradient projection; returns the iteration count.
    pub fn constrained_majorization_vpsc(
        &mut self,
        b: &[f32],
        place: &mut [f32],
        max_iterations: i32,
    ) -> i32 {
        let n = self.nv + self.nldv;
        let mut converged = false;
        if max_iterations == 0 {
            return 0;
        }
        let mut g = std::mem::take(&mut self.f_array1);
        let mut old_place = std::mem::take(&mut self.f_array2);
        let mut d = std::mem::take(&mut self.f_array3);
        if self.m > 0 {
            self.satisfy_place(place, n);
        }
        let mut counter = 0;
        while counter < max_iterations && !converged {
            let mut test = 0.0f32;
            let mut numerator = 0.0f32;
            let mut denominator = 0.0f32;
            converged = true;
            // find steepest descent direction
            for i in 0..n {
                old_place[i] = place[i];
                g[i] = 2.0 * b[i];
                for j in 0..n {
                    g[i] = (-(2.0 * self.a[i][j])).mul_add(place[j], g[i]);
                }
            }
            for i in 0..n {
                numerator = g[i].mul_add(g[i], numerator);
                let mut r = 0.0f32;
                for j in 0..n {
                    r = (2.0 * self.a[i][j]).mul_add(g[j], r);
                }
                denominator = (-r).mul_add(g[i], denominator);
            }
            let alpha = if denominator != 0.0 {
                numerator / denominator
            } else {
                1.0
            };
            for i in 0..n {
                place[i] = (-alpha).mul_add(g[i], place[i]);
            }
            if self.m > 0 {
                // project to constraint boundary
                self.satisfy_place(place, n);
            }
            for i in 0..n {
                d[i] = place[i] - old_place[i];
            }
            // now compute beta
            numerator = 0.0;
            denominator = 0.0;
            for i in 0..n {
                numerator = g[i].mul_add(d[i], numerator);
                let mut r = 0.0f32;
                for j in 0..n {
                    r = (2.0 * self.a[i][j]).mul_add(d[j], r);
                }
                denominator = r.mul_add(d[i], denominator);
            }
            let beta = if denominator != 0.0 {
                numerator / denominator
            } else {
                1.0
            };
            for i in 0..n {
                // beta > 1.0 takes us back outside the feasible region;
                // beta < 0 is not useful
                if beta > 0.0 && beta < 1.0 {
                    place[i] = beta.mul_add(d[i], old_place[i]);
                }
                test += (place[i] - old_place[i]).abs();
            }
            if test > QUAD_PROG_TOL {
                converged = false;
            }
            counter += 1;
        }
        self.f_array1 = g;
        self.f_array2 = old_place;
        self.f_array3 = d;
        counter
    }
}

/// `initCMajVPSC(n, packedMat, graph, opt, diredges)` for `diredges == 0`
/// (no directed-edge constraints); levels (DiG-CoLa) and edge constraints
/// are added by the caller through [`init_cmaj_vpsc_with`].
pub fn init_cmaj_vpsc(n: usize, packed_mat: Option<&[f32]>, opt: &IpsepOptions) -> CMajEnvVpsc {
    init_cmaj_vpsc_with(n, packed_mat, opt, |_, _| {})
}

/// `initCMajVPSC` with a hook creating the global (`gcs`) constraints and
/// dummy variables between the variable creation and the cluster
/// containment constraints, as the `diredges` branches do in C.
pub fn init_cmaj_vpsc_with(
    n: usize,
    packed_mat: Option<&[f32]>,
    opt: &IpsepOptions,
    gen_global: impl FnOnce(&mut CMajEnvVpsc, usize),
) -> CMajEnvVpsc {
    let mut e = CMajEnvVpsc {
        nldv: 2 * opt.clusters.nclusters(),
        ..Default::default()
    };
    e.nv = n - e.nldv;
    e.ndv = 0;
    for i in 0..n {
        let v = e.solver.new_variable(i as i32, 1.0, 1.0);
        e.vs.push(v);
    }
    gen_global(&mut e, n);
    e.gm = e.gcs.len();
    if opt.clusters.nclusters() > 0 {
        for (i, cl) in opt.clusters.clusters.iter().enumerate() {
            for &j in cl {
                let v = e.vs[j];
                let cl = e.vs[e.nv + 2 * i];
                let cr = e.vs[e.nv + 2 * i + 1];
                let c1 = e.solver.new_constraint(cl, v, 0.0);
                e.gcs.push(c1);
                let c2 = e.solver.new_constraint(v, cr, 0.0);
                e.gcs.push(c2);
            }
        }
        e.gm = e.gcs.len();
    }
    e.m = 0;
    e.cs.clear();
    if e.gm > 0 {
        let nvars = n + e.ndv;
        let vs = e.vs[..nvars].to_vec();
        e.solver.new_inc_vpsc(&vs, &e.gcs);
        e.m = e.gm;
        e.cs = e.gcs.clone();
    }
    if let Some(pm) = packed_mat {
        e.a = unpack_matrix(pm, n);
    }
    e.f_array1 = vec![0.0; n];
    e.f_array2 = vec![0.0; n];
    e.f_array3 = vec![0.0; n];
    e
}

/// `expandbb`/`EXPANDBB` for `boxf`.
fn expand_bb(b: &mut BoxF, o: &BoxF) {
    b.llx = b.llx.min(o.llx);
    b.lly = b.lly.min(o.lly);
    b.urx = b.urx.max(o.urx);
    b.ury = b.ury.max(o.ury);
}

/// `generateNonoverlapConstraints(e, nsizeScale, coords, k, transitiveClosure, opt)`.
pub fn generate_nonoverlap_constraints(
    e: &mut CMajEnvVpsc,
    mut nsize_scale: f32,
    coords: &[Vec<f32>],
    k: usize,
    transitive_closure: bool,
    opt: &mut IpsepOptions,
) {
    let mut n = e.nv + e.nldv;
    let genclusters = opt.clusters.nclusters() > 0;
    if genclusters {
        // n is the number of real variables, not dummy cluster vars
        n -= 2 * opt.clusters.nclusters();
    }
    if k == 0 {
        // grow a bit in the x dimension, so that if overlap is resolved
        // horizontally then it won't be considered overlapping vertically
        nsize_scale *= 1.0001f32;
    }
    let s = nsize_scale as f64;
    let mut bb = vec![BoxF::default(); n];
    for i in 0..n {
        let (nx, ny) = opt.nsize[i];
        bb[i].llx = coords[0][i] as f64 - s * nx / 2.0 - opt.gap.0 / 2.0;
        bb[i].urx = coords[0][i] as f64 + s * nx / 2.0 + opt.gap.0 / 2.0;
        bb[i].lly = coords[1][i] as f64 - s * ny / 2.0 - opt.gap.1 / 2.0;
        bb[i].ury = coords[1][i] as f64 + s * ny / 2.0 + opt.gap.1 / 2.0;
    }
    let csol: Vec<usize> = if genclusters {
        let ncl = opt.clusters.nclusters();
        let mut cscl: Vec<Vec<usize>> = Vec::with_capacity(ncl + 1);
        if opt.clusters.bb.len() < ncl {
            opt.clusters.bb.resize(ncl, BoxF::default());
        }
        for i in 0..ncl {
            let cn = opt.clusters.clusters[i].len();
            let mut cvs = Vec::with_capacity(cn + 2);
            let mut cbb = Vec::with_capacity(cn + 2);
            let mut container = BoxF {
                llx: f64::MAX,
                lly: f64::MAX,
                urx: -f64::MAX,
                ury: -f64::MAX,
            };
            for j in 0..cn {
                let iv = opt.clusters.clusters[i][j];
                cvs.push(e.vs[iv]);
                cbb.push(bb[iv]);
                expand_bb(&mut container, &bb[iv]);
            }
            opt.clusters.bb[i] = container;
            cvs.push(e.vs[n + 2 * i]);
            cvs.push(e.vs[n + 2 * i + 1]);
            cbb.push(container);
            cbb.push(container);
            if k == 0 {
                cbb[cn].urx = container.llx + 0.0001;
                cbb[cn + 1].llx = container.urx - 0.0001;
                cscl.push(vpsc::gen_x_constraints(
                    &mut e.solver,
                    &cbb,
                    &cvs,
                    transitive_closure,
                ));
            } else {
                cbb[cn].ury = container.lly + 0.0001;
                cbb[cn + 1].lly = container.ury - 0.0001;
                cscl.push(vpsc::gen_y_constraints(&mut e.solver, &cbb, &cvs));
            }
        }
        // generate top level constraints
        {
            let ntop = opt.clusters.toplevel.len();
            let cn = ntop + ncl;
            let mut cvs = Vec::with_capacity(cn);
            let mut cbb = Vec::with_capacity(cn);
            for i in 0..ntop {
                let iv = opt.clusters.toplevel[i];
                cvs.push(e.vs[iv]);
                cbb.push(bb[iv]);
            }
            // make dummy variables for clusters
            for i in ntop..cn {
                let v = e.solver.new_variable(123 + i as i32, 1.0, 1.0);
                cvs.push(v);
                cbb.push(opt.clusters.bb[i - ntop]);
            }
            if k == 0 {
                cscl.push(vpsc::gen_x_constraints(
                    &mut e.solver,
                    &cbb,
                    &cvs,
                    transitive_closure,
                ));
            } else {
                cscl.push(vpsc::gen_y_constraints(&mut e.solver, &cbb, &cvs));
            }
            // remap constraints from tmp dummy vars to cluster l and r vars
            for i in ntop..cn {
                let j = i - ntop;
                let dgap = if k == 0 {
                    -(cbb[i].urx - cbb[i].llx) / 2.0
                } else {
                    -(cbb[i].ury - cbb[i].lly) / 2.0
                };
                e.solver.remap_in_constraints(cvs[i], e.vs[n + 2 * j], dgap);
                e.solver
                    .remap_out_constraints(cvs[i], e.vs[n + 2 * j + 1], dgap);
            }
        }
        cscl.into_iter().flatten().collect()
    } else if k == 0 {
        let vs = e.vs[..n].to_vec();
        vpsc::gen_x_constraints(&mut e.solver, &bb, &vs, transitive_closure)
    } else {
        let vs = e.vs[..n].to_vec();
        vpsc::gen_y_constraints(&mut e.solver, &bb, &vs)
    };
    // remove constraints from previous iteration
    if e.m > 0 {
        e.solver.delete_vpsc();
        for i in e.gm..e.m {
            e.solver.delete_constraint(e.cs[i]);
        }
    }
    let mut cs = e.gcs[..e.gm].to_vec();
    cs.extend_from_slice(&csol);
    e.m = cs.len();
    e.cs = cs;
    let nvars = e.nv + e.nldv + e.ndv;
    let vs = e.vs[..nvars].to_vec();
    let cs = e.cs.clone();
    e.solver.new_inc_vpsc(&vs, &cs);
}

/// `removeoverlaps(n, coords, opt)`: statically remove all overlaps, moving
/// each node as little as possible (x pass, then y pass).
pub fn removeoverlaps(n: usize, coords: &mut [Vec<f32>], opt: &mut IpsepOptions) {
    let mut e = init_cmaj_vpsc(n, None, opt);
    generate_nonoverlap_constraints(&mut e, 1.0, coords, 0, true, opt);
    let _ = e.solver.solve();
    for i in 0..n {
        coords[0][i] = e.solver.position(e.vs[i]) as f32;
    }
    generate_nonoverlap_constraints(&mut e, 1.0, coords, 1, false, opt);
    let _ = e.solver.solve();
    for i in 0..n {
        coords[1][i] = e.solver.position(e.vs[i]) as f32;
    }
}

/// `assign_digcola_levels(ordering, n, level_inds, num_divisions)`.
pub fn assign_digcola_levels(
    ordering: &[i32],
    n: usize,
    level_inds: &[i32],
    num_divisions: usize,
) -> Vec<DigColaLevel> {
    let mut l = vec![DigColaLevel::default(); num_divisions + 1];
    // first level
    let n0 = level_inds.first().copied().unwrap_or(n as i32) as usize;
    l[0].nodes = ordering[..n0].to_vec();
    // second through second last level
    for i in 1..num_divisions {
        let (a, b) = (level_inds[i - 1] as usize, level_inds[i] as usize);
        l[i].nodes = ordering[a..b].to_vec();
    }
    // last level
    if num_divisions > 0 {
        let a = level_inds[num_divisions - 1] as usize;
        l[num_divisions].nodes = ordering[a..n].to_vec();
    }
    l
}

/// `get_num_digcola_constraints(levels, num_levels)`.
pub fn get_num_digcola_constraints(levels: &[DigColaLevel], num_levels: usize) -> usize {
    let mut nc = 0;
    for i in 1..num_levels {
        nc += levels[i].nodes.len() + levels[i - 1].nodes.len();
    }
    nc += levels[0].nodes.len() + levels[num_levels - 1].nodes.len();
    nc
}
