//! Render-fidelity ratchet: the port's `-n2` renderings must not move further from C than
//! the baseline (`tests/golden/render/baseline.json`).
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p gv-diff -- render --baseline tests/golden/render/baseline.json --update`.

use std::process::Command;

#[test]
fn render_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args(["render", "--baseline", "tests/golden/render/baseline.json"])
        .status()
        .expect("run gv-diff render");
    assert!(
        status.success(),
        "render fidelity regressed; see gv-diff output above"
    );
}
