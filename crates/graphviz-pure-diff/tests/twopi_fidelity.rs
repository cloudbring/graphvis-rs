//! Twopi fidelity ratchet: the port's twopi layouts of `tests/twopi` must not move
//! further from C Graphviz than the twopi baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p graphviz-pure-diff -- --golden tests/golden/twopi --baseline tests/golden/twopi-baseline.portable.json --update tests/twopi` (or run
//! `scripts/update-baselines.sh`, which refreshes every baseline).

use std::process::Command;

#[test]
fn twopi_corpus_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "--golden",
            "tests/golden/twopi",
            "--baseline",
            "tests/golden/twopi-baseline.portable.json",
            "--worst",
            "0",
            "tests/twopi",
        ])
        .status()
        .expect("run gv-diff");
    assert!(
        status.success(),
        "twopi fidelity regressed; see gv-diff output above"
    );
}
