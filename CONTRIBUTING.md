# Contributing to graphviz-pure

Thanks for helping. graphviz-pure is a function-by-function port of Graphviz 16.1.0, and the
bar for every change is the same: the output must stay byte-identical to C wherever it is
today, and should get closer wherever it is not.

## Reporting a problem

- **Different output from C Graphviz:** use the "Output differs from C Graphviz" issue form.
  Include the smallest DOT input you can, the command, and both outputs. Compare against C
  set up as described in [Verify it yourself](https://graphviz-pure.github.io/verify/): a stock
  `dot` measures text with Pango and the installed fonts, so its label sizes differ from both
  the reference and the port.
- **Crash, hang or wrong behaviour:** use the bug report form.
- **Security problems:** report them privately (see [SECURITY.md](SECURITY.md)), not in an
  issue.

Known differences are tracked in the [What's left](https://graphviz-pure.github.io/whats-left/)
list; check it first.

## Building and testing

You need a recent stable Rust (`rustup` installs it). Then:

```sh
cargo test --workspace                                            # everything, as CI runs it on macOS
cargo test --workspace --features graphviz-pure-core/portable-math  # the profile Linux, Windows and WASM use
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
```

`cargo test` runs the fidelity ratchets: they compare the port with C Graphviz's committed
outputs on about 2,600 inputs and fail if any file that matched C stops matching. They need
no C installation. Regenerating the C references does; see
[tests/golden/README.md](tests/golden/README.md).

## Making a change

- **Follow the C.** The port mirrors Graphviz's C sources function by function. Name the C
  function and file you are porting or fixing (`lib/dotgen/position.c:make_lrvn`) in comments
  and in the pull request, and keep the structure recognisable.
- **Prove it.** A fix for a difference from C comes with the input that showed it, ideally added
  to a corpus under `tests/`, and the ratchet baselines refreshed if more files now match
  (`scripts/update-baselines.sh`, and `--portable` for the portable profile). A change must not
  make any file that matched C stop matching.
- **No `unsafe`.** The workspace forbids it; the only exception is the test-only
  `graphviz-pure-portable-libm` crate.
- **Dependencies** are added only when necessary, and must pass `cargo deny check`
  (`deny.toml`: licences, advisories, sources).
- **Changelog:** add a line under "Unreleased" in [CHANGELOG.md](CHANGELOG.md) for anything a
  user would notice, including changes to output bytes.
- **Pull requests** need the CI checks to pass (tests in both math profiles, the supply-chain
  check, and the cross-platform check that every platform writes the same bytes) and all review
  threads resolved.

## Licence

graphviz-pure is a derivative work of Graphviz and is distributed under the
[Eclipse Public License 2.0](LICENSE). By submitting a contribution you agree that it is
licensed under the same terms.

## Conduct

Everyone taking part is expected to follow the [code of conduct](CODE_OF_CONDUCT.md).
