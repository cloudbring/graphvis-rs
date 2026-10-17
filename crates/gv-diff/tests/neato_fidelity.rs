//! Neato fidelity ratchet: the port's neato layouts of `tests/neato` must not move
//! further from C Graphviz than the neato baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p gv-diff -- --golden tests/golden/neato --baseline tests/golden/neato-baseline.json --update tests/neato`.

use std::process::Command;

#[test]
fn neato_corpus_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "--golden",
            "tests/golden/neato",
            "--baseline",
            "tests/golden/neato-baseline.json",
            "--worst",
            "0",
            "tests/neato",
        ])
        .status()
        .expect("run gv-diff");
    assert!(
        status.success(),
        "neato fidelity regressed; see gv-diff output above"
    );
}
