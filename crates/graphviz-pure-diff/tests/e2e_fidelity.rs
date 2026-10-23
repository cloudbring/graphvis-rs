//! Full-pipeline ratchet: every input whose `-Tsvg` matched C byte for byte in
//! `tests/golden/e2e-baseline.portable.json` must still match.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p graphviz-pure-diff -- e2e --update` (or run
//! `scripts/update-baselines.sh`, which refreshes every baseline).

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
/// portable math functions (`scripts/dot-portable-libm.sh`).
#[test]
fn e2e_svg_identity_with_matched_libm_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "e2e",
            "--golden",
            "tests/golden/e2e.libm-matched.json",
            "--baseline",
            "tests/golden/e2e-baseline.libm-matched.portable.json",
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
/// off. Checked against C with the port's portable functions (`e2e.libm-matched.json`).
#[test]
fn fdp_cluster_ports_call_sin_and_cos_separately() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let args = [
        "e2e",
        "--filter",
        "fdp/fuzz_cluster_port_sincos",
        "--list",
        "--golden",
        "tests/golden/e2e.libm-matched.json",
        "--baseline",
        "tests/golden/e2e-baseline.libm-matched.portable.json",
    ];
    let out = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args(args)
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
