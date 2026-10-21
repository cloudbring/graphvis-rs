//! Full-pipeline ratchet: every input whose `-Tsvg` matched C byte for byte in
//! `tests/golden/e2e-baseline.json` must still match.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p graphviz-pure-diff -- e2e --update`. Add
//! `--features graphviz-pure-core/portable-math` to refresh the portable baseline (`*.portable.json`), which
//! gv-diff selects on every portable-math build (Linux, wasm32, or macOS with the feature).

use std::process::Command;

#[test]
fn e2e_svg_identity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .arg("e2e")
        .status()
        .expect("run gv-diff e2e");
    assert!(
        status.success(),
        "e2e SVG identity regressed; see gv-diff output above"
    );
}

/// With the math library taken out of the equation, the portable build must match C on every
/// input: `tests/golden/e2e.libm-matched.json` holds C's outputs when C runs with the port's
/// portable math functions (`scripts/dot-portable-libm.sh`). Portable builds only (Linux, wasm32,
/// or macOS with `--features graphviz-pure-core/portable-math`); on Apple builds the main check above
/// already compares against C with the same (Apple) libm.
#[test]
fn e2e_svg_identity_with_matched_libm_does_not_regress() {
    if !graphviz_pure_core::math::PORTABLE {
        return;
    }
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "e2e",
            "--golden",
            "tests/golden/e2e.libm-matched.json",
            "--baseline",
            "tests/golden/e2e-baseline.libm-matched.json",
        ])
        .status()
        .expect("run gv-diff e2e");
    assert!(
        status.success(),
        "e2e SVG identity against C with the portable libm regressed; see gv-diff output above"
    );
}

/// Issue #15 (fuzz case 006-fdp-cluster-port-sincos-fusion, now
/// `tests/fdp/fuzz_cluster_port_sincos.gv`). fdp places a cluster's boundary ports with
/// `T_Wd * cos(alpha)` and `T_Ht * sin(alpha)` (`lib/fdpgen/tlayout.c:501-502`), and C calls
/// `sin` and `cos` separately there. LLVM merged the two calls in the port into
/// `__sincos_stret`, whose sine differs from Apple's `sin()` in the last bit for some
/// arguments; the spring embedder amplified that into clusters and edges a tenth of a point
/// off. Checked against C with the same math library as the build: stock C on Apple builds,
/// C with the port's portable functions (`e2e.libm-matched.json`) on portable builds.
#[test]
fn fdp_cluster_ports_call_sin_and_cos_separately() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut args = vec!["e2e", "--filter", "fdp/fuzz_cluster_port_sincos", "--list"];
    if graphviz_pure_core::math::PORTABLE {
        args.extend([
            "--golden",
            "tests/golden/e2e.libm-matched.json",
            "--baseline",
            "tests/golden/e2e-baseline.libm-matched.json",
        ]);
    }
    let out = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args(&args)
        .output()
        .expect("run gv-diff e2e");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let total = stdout
        .lines()
        .find(|l| l.starts_with("total"))
        .expect("gv-diff e2e prints a total line");
    let counts: Vec<&str> = total.split_whitespace().skip(1).take(2).collect();
    assert_eq!(counts, ["1", "1"], "port -Tsvg differs from C:\n{stdout}");
}
