//! Structural ratchet: on every corpus input, the port's `-Tsvg` must contain what C's does
//! (nodes, edges, clusters, label text), stay on its canvas with finite coordinates, and keep
//! C's canvas size and node overlaps within tolerance (`gv-diff structure`, see
//! `docs/testing/structure-checks.md`). Its reference is stock C on macOS, whose layouts
//! differ from the port's on the inputs where Apple's libm moves them.

use std::process::Command;

#[test]
fn structure_matches_c() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let status = Command::new(env!("CARGO_BIN_EXE_gv-diff"))
        .current_dir(root)
        .arg("structure")
        .status()
        .expect("run gv-diff structure");
    assert!(
        status.success(),
        "structural check failed; see gv-diff output above"
    );
}
