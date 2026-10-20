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
