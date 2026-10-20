# Releasing graphviz-pure

## How a release works

1. Run `scripts/bump-version.sh X.Y.Z --pr`: it sets the version in `Cargo.toml`,
   `Cargo.lock` and `web/package.json`, dates the changelog's "Unreleased" entries, and opens the
   release pull request. Merge it. The version follows [the versioning policy](versioning.md).
2. A maintainer pushes the tag: `git tag -s v0.1.0 && git push origin v0.1.0`. Only maintainers
   can create `v*` tags (tag ruleset).
3. `.github/workflows/release.yml` runs on GitHub-hosted runners, with no caches, the pinned
   Rust toolchain and `cargo build --locked`:
   - builds `graphviz-pure` for six targets (Linux x86-64 and arm64 as static musl binaries,
     macOS arm64 and x86-64, Windows x86-64 and arm64) and the npm package;
   - requires every binary and the npm package to reproduce the gallery samples byte for byte
     (`scripts/release-smoke.py`, `scripts/release-smoke-wasm.mjs`);
   - packages deterministic archives (`scripts/release-package.py`), writes `SHA256SUMS` and
     attests the build provenance of every file (Sigstore, stored on GitHub).
4. The publishing jobs wait for a maintainer to approve the `release` environment, then:
   - create the GitHub release with all files;
   - publish the eight crates to crates.io (trusted publishing, `cargo publish --workspace`);
   - stage `@graphviz-pure/wasm` on npm (trusted publishing, with npm provenance). The trusted
     publisher may only stage, so the version goes live only when a maintainer approves it with
     2FA: the package's **Staged Packages** tab on npmjs.com, or `npm stage approve <stage-id>`.
5. Within hours, `graphviz-pure/homebrew-tap` and `graphviz-pure/scoop-bucket` pull the
   release, verify each asset's attestation and checksum, and commit the new formula and
   manifest. `graphviz-pure/graphviz-pure.github.io` deploys the site from `master` builds the
   same way. No repository holds a credential for another.

Run the workflow by hand (Actions → Release → Run workflow) to build, smoke-test and attest
everything without publishing anything.

## Verifying a download

```sh
gh attestation verify graphviz-pure-0.1.0-x86_64-unknown-linux-musl.tar.gz \
  --repo graphviz-pure/graphviz-pure --signer-workflow graphviz-pure/graphviz-pure/.github/workflows/release.yml
sha256sum --check --ignore-missing SHA256SUMS
npm audit signatures   # in a project that depends on @graphviz-pure/wasm
```

## One-time setup

Done once per channel by a maintainer. Nothing here needs a long-lived token in GitHub.

### GitHub (organization `graphviz-pure`)

- Organization settings → Authentication security → **Require two-factor authentication**
  (the only setting the API cannot change).
- Install the **Blacksmith** GitHub app on the organization (app.blacksmith.sh) before CI jobs
  move to Blacksmith runners. Release builds stay on GitHub-hosted runners: npm trusted
  publishing rejects self-hosted runners, and GitHub-hosted builders are the documented basis
  for SLSA Build Level 3.
- Already configured by script (see the transfer PR): Actions restricted to GitHub-owned
  actions plus `Swatinem/rust-cache` and `rust-lang/crates-io-auth-action`, full-SHA pinning
  required, read-only default workflow token, members cannot create repositories.

### crates.io

Trusted publishing can only be configured for a crate that exists, so the names are reserved
once with placeholder releases:

1. Log in to crates.io with GitHub; verify your email address.
2. Account settings → API tokens → new token, scope **publish-new**, crates `graphviz-pure*`,
   expiry 1 day. Run `cargo login` and paste it there (not in chat).
3. `scripts/reserve-crates.sh` publishes `0.0.0` placeholders of the eight crate names.
4. For each crate: Settings → Trusted Publishing → add GitHub: owner `graphviz-pure`,
   repository `graphviz-pure`, workflow `release.yml`, environment `release`. Then enable
   **"Require trusted publishing"** for the crate.
5. Delete the API token; `cargo logout`.

### npm (organization `graphviz-pure`)

1. Two-factor authentication on your account (done).
2. `npm login`, then `scripts/reserve-npm.sh` publishes a `0.0.0` placeholder of
   `@graphviz-pure/wasm`, and deprecates it.
3. Package settings → Trusted publishers → GitHub Actions: organization `graphviz-pure`,
   repository `graphviz-pure`, workflow `release.yml`, environment `release`. Under **Allowed
   actions**, leave both `npm publish` and `npm dist-tag` unchecked: the workflow can then only
   stage a release, and each one waits for your 2FA approval.
4. Package settings → Publishing access → **"Require two-factor authentication and disallow
   tokens"**. `npm logout`.

### Homebrew and Scoop

Nothing to set up: the tap and bucket repositories update themselves after each release.
Users run `brew install graphviz-pure/tap/graphviz-pure` and
`scoop bucket add graphviz-pure https://github.com/graphviz-pure/scoop-bucket` then
`scoop install graphviz-pure/graphviz-pure`. Submitting to homebrew-core and the Scoop main
bucket comes later; both require a project with some adoption, and homebrew-core builds from
source.

### Later channels

| channel | what it needs |
| :--- | :--- |
| winget | a manifest PR to `microsoft/winget-pkgs` per release (`wingetcreate` or `komac`); automating it needs a GitHub token able to open PRs there, kept in the `release` environment |
| Debian/Ubuntu and Fedora repositories | Cloudsmith open-source plan (supports GitHub OIDC, so no stored token), or Fedora COPR and a Launchpad PPA, which build from source |
| AUR | an AUR account and SSH key; `graphviz-pure-bin` (release binaries) and `graphviz-pure` (from source) PKGBUILDs |
| nixpkgs, conda-forge | pull requests to their repositories; no account beyond GitHub |
| Docker | an image on `ghcr.io/graphviz-pure/graphviz-pure`, signed and attested from the release workflow |
| macOS notarization, Windows code signing | an Apple Developer ID (99 USD/year) and Azure Trusted Signing or an OV certificate; until then, binaries downloaded through a browser show a Gatekeeper or SmartScreen warning (Homebrew and Scoop installs do not) |
