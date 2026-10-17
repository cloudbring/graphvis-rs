//! Fidelity ratchet: the port must not move further from C Graphviz than the baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p gv-diff -- --baseline tests/golden/baseline.json --update`.

use std::process::Command;

#[test]
fn corpus_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args(["--baseline", "tests/golden/baseline.json", "--worst", "0"])
        .status()
        .expect("run gv-diff");
    assert!(
        status.success(),
        "fidelity regressed; see gv-diff output above"
    );
}
