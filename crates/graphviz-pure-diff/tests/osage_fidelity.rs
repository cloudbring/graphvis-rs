//! Osage fidelity ratchet: the port's osage layouts of `tests/osage` must not move
//! further from C Graphviz than the osage baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p graphviz-pure-diff -- --golden tests/golden/osage --baseline tests/golden/osage-baseline.json --update tests/osage`. Add
//! `--features graphviz-pure-core/portable-math` to refresh the portable baseline (`*.portable.json`), which
//! gv-diff selects on every portable-math build (Linux, wasm32, or macOS with the feature).

use std::process::Command;

#[test]
fn osage_corpus_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "--golden",
            "tests/golden/osage",
            "--baseline",
            "tests/golden/osage-baseline.json",
            "--worst",
            "0",
            "tests/osage",
        ])
        .status()
        .expect("run gv-diff");
    assert!(
        status.success(),
        "osage fidelity regressed; see gv-diff output above"
    );
}
