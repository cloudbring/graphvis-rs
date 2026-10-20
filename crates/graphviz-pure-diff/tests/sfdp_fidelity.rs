//! Sfdp fidelity ratchet: the port's sfdp layouts of `tests/sfdp` must not move
//! further from C Graphviz than the sfdp baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p graphviz-pure-diff -- --golden tests/golden/sfdp --baseline tests/golden/sfdp-baseline.json --update tests/sfdp`. Add
//! `--features graphviz-pure-core/portable-math` to refresh the portable baseline (`*.portable.json`), which
//! gv-diff selects on every portable-math build (Linux, wasm32, or macOS with the feature).

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
            "tests/golden/sfdp-baseline.json",
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
