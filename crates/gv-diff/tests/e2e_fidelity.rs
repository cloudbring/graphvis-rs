//! Full-pipeline ratchet: every input whose `-Tsvg` matched C byte for byte in
//! `tests/golden/e2e-baseline.json` must still match.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p gv-diff -- e2e --update`.

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
