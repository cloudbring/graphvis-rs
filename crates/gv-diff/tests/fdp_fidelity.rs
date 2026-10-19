//! Fdp fidelity ratchet: the port's fdp layouts of `tests/fdp` must not move
//! further from C Graphviz than the fdp baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p gv-diff -- --golden tests/golden/fdp --baseline tests/golden/fdp-baseline.json --update tests/fdp`. Add
//! `--features gv-core/portable-math` to refresh the portable baseline (`*.portable.json`), which
//! gv-diff selects on every portable-math build (Linux, wasm32, or macOS with the feature).

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
            "tests/golden/fdp-baseline.json",
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
