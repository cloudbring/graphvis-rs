# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's
[private vulnerability reporting](../../security/advisories/new) ("Report a vulnerability" on
the Security tab), not in a public issue. You should get a reply within a week. Once a fix is
released, the advisory is published with credit to the reporter unless you prefer otherwise.

Relevant reports include crashes or hangs on crafted input (the port parses untrusted DOT and
runs in browsers), memory-safety problems (the port forbids `unsafe` outside the test-only
`gv-portable-libm` crate), and anything in the release and distribution pipeline.

## Supported versions

Only the latest release receives fixes.

## How releases are built

Release artifacts are built on GitHub-hosted runners from a tagged commit, with
`cargo build --locked`, and each carries a signed build-provenance attestation. Verify a
download with:

```sh
gh attestation verify <file> --repo graphviz-pure/graphviz-pure
```

Packages on crates.io and npm are published from the same workflow through trusted publishing
(OpenID Connect), without long-lived tokens.
