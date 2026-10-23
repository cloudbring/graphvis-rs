//! Circo fidelity ratchet: the port's circo layouts of `tests/circo` must not move
//! further from C Graphviz than the circo baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p graphviz-pure-diff -- --golden tests/golden/circo --baseline tests/golden/circo-baseline.portable.json --update tests/circo` (or run
//! `scripts/update-baselines.sh`, which refreshes every baseline).

use std::process::Command;

#[test]
fn circo_corpus_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "--golden",
            "tests/golden/circo",
            "--baseline",
            "tests/golden/circo-baseline.portable.json",
            "--worst",
            "0",
            "tests/circo",
        ])
        .status()
        .expect("run gv-diff");
    assert!(
        status.success(),
        "circo fidelity regressed; see gv-diff output above"
    );
}
