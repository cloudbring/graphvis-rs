# Versioning

graphviz-pure follows [Semantic Versioning](https://semver.org/). All crates and the npm package
share one version number, set in `[workspace.package]` in `Cargo.toml` and in
`web/package.json`, and are released together.

## What the version covers

- **The Rust API** of the published crates (`graphviz-pure` and `graphviz-pure-*`), and
  **the JavaScript API** of `@graphviz-pure/wasm`.
- **The command line:** options, accepted `-T` formats, exit status.
- **Output.** Because the project exists to reproduce Graphviz's output exactly, every change
  to the bytes the port writes for an existing input is listed in the changelog, whatever its
  size.

## What each kind of change bumps

While the version is `0.x`, a breaking change bumps the minor version (`0.1` → `0.2`) and
everything else bumps the patch version (`0.1.0` → `0.1.1`), as Cargo treats it.

From `1.0`:

| change | bump |
| :--- | :--- |
| Removing or changing a public Rust or JavaScript API, a command-line option or a format | major |
| A fix that makes output closer to C Graphviz (bytes change for some inputs) | minor |
| A new format, engine option or API | minor |
| Raising the minimum supported Rust version | minor |
| A fix that does not change output for any input in the test corpora; dependency updates; documentation | patch |
| Updating the reference to a newer Graphviz release | major |

Output that moves *away* from C Graphviz is a bug, never a release decision: the fidelity
ratchets in CI refuse it.

## Making a release

`scripts/bump-version.sh X.Y.Z` updates the version everywhere, moves the "Unreleased"
changelog entries under the new version, and prepares the release pull request. After it is
merged, a maintainer tags the merge commit; see [releasing.md](releasing.md).
