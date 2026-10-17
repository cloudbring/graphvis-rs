//! Patchwork fidelity ratchet: the port's patchwork layouts of `tests/patchwork` must not move
//! further from C Graphviz than the patchwork baseline.
//!
//! Refresh the baseline after an improvement with
//! `cargo run --release -p gv-diff -- --golden tests/golden/patchwork --baseline tests/golden/patchwork-baseline.json --update tests/patchwork`. Add
//! `--features gv-core/portable-math` to refresh the portable baseline (`*.portable.json`), which
//! gv-diff selects on every portable-math build (Linux, wasm32, or macOS with the feature).

use std::process::Command;

#[test]
fn patchwork_corpus_fidelity_does_not_regress() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .args([
            "--golden",
            "tests/golden/patchwork",
            "--baseline",
            "tests/golden/patchwork-baseline.json",
            "--worst",
            "0",
            "tests/patchwork",
        ])
        .status()
        .expect("run gv-diff");
    assert!(
        status.success(),
        "patchwork fidelity regressed; see gv-diff output above"
    );
}
