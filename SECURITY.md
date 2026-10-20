# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's
[private vulnerability reporting](../../security/advisories/new) ("Report a vulnerability" on
the Security tab), not in a public issue. You should get a reply within a week. Once a fix is
released, the advisory is published with credit to the reporter unless you prefer otherwise.

Relevant reports include crashes or hangs on crafted input (the port parses untrusted DOT and
runs in browsers), memory-safety problems (the port forbids `unsafe` outside the test-only
`graphviz-pure-portable-libm` crate), and anything in the build and distribution pipeline.

## Supported versions

There has been no release yet. Once there is, only the latest release receives fixes.

## Supply chain

- Every GitHub Action is pinned to a full commit SHA; only GitHub-owned actions and
  `Swatinem/rust-cache` are used. Workflows run with read-only tokens by default and are linted
  with zizmor in CI.
- Rust dependencies come from crates.io only, through `Cargo.lock`, and are checked in CI by
  `cargo deny` (RustSec advisories, licences, sources). npm dependencies are installed from
  `package-lock.json` without lifecycle scripts until their registry signatures are verified.
- Dependabot proposes updates with a 7-day cooldown.

The release process (artifacts built on GitHub-hosted runners with build-provenance
attestations, and trusted publishing to crates.io and npm) will be documented here when the
first release workflow lands.
