//! Fdp fidelity ratchet: the port's fdp layouts of `tests/fdp` must not move
//! further from C Graphviz than the fdp baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p graphviz-pure-diff -- --golden tests/golden/fdp --baseline tests/golden/fdp-baseline.portable.json --update tests/fdp` (or run
//! `scripts/update-baselines.sh`, which refreshes every baseline).

use std::process::Command;

#[test]
fn fdp_corpus_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "--golden",
            "tests/golden/fdp",
            "--baseline",
            "tests/golden/fdp-baseline.portable.json",
            "--worst",
            "0",
            "tests/fdp",
        ])
        .status()
        .expect("run gv-diff");
    assert!(
        status.success(),
        "fdp fidelity regressed; see gv-diff output above"
    );
}
