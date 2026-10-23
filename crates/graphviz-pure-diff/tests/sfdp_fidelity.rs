//! Sfdp fidelity ratchet: the port's sfdp layouts of `tests/sfdp` must not move
//! further from C Graphviz than the sfdp baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p graphviz-pure-diff -- --golden tests/golden/sfdp --baseline tests/golden/sfdp-baseline.portable.json --update tests/sfdp` (or run
//! `scripts/update-baselines.sh`, which refreshes every baseline).

use std::process::Command;

#[test]
fn sfdp_corpus_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "--golden",
            "tests/golden/sfdp",
            "--baseline",
            "tests/golden/sfdp-baseline.portable.json",
            "--worst",
            "0",
            "tests/sfdp",
        ])
        .status()
        .expect("run gv-diff");
    assert!(
        status.success(),
        "sfdp fidelity regressed; see gv-diff output above"
    );
}
