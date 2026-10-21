# Releasing graphviz-pure

## How a release works

1. Run `scripts/bump-version.sh X.Y.Z --pr`: it sets the version in `Cargo.toml`,
   `Cargo.lock`, `web/package.json` and `web/jsr.json`, dates the changelog's "Unreleased" entries,
   and opens the release pull request. Merge it. The version follows
   [the versioning policy](versioning.md).
2. A maintainer pushes a signed tag: `git tag -s v0.1.0 -m v0.1.0 && git push origin v0.1.0`.
   Only maintainers can create `v*` tags (tag ruleset), and the workflow refuses a tag that is
   not on `master` or not signed with a key in [`.github/allowed_signers`](../.github/allowed_signers)
   (read from `master`, not from the tagged commit). To sign with SSH: `git config gpg.format ssh`,
   `git config user.signingkey ~/.ssh/id_ed25519.pub`; add the key to GitHub as a signing key
   and, in a reviewed pull request, to `.github/allowed_signers`.
   **First release only:** npm drops a trusted-publisher configuration that has not published
   within 48 hours of its creation. If it is older than that, delete and recreate it (same
   fields, see [npm](#npm-organization-graphviz-pure)) just before pushing the tag.
3. `.github/workflows/release.yml` runs on GitHub-hosted runners, with no caches, the pinned
   Rust toolchain and `cargo build --locked`:
   - builds `graphviz-pure` for six targets (Linux x86-64 and arm64 as static musl binaries,
     macOS arm64 and x86-64, Windows x86-64 and arm64) and the npm package;
   - requires every binary and the npm package to reproduce the gallery samples byte for byte
     (`scripts/release-smoke.py`, `scripts/release-smoke-wasm.mjs`);
   - packages deterministic archives (`scripts/release-package.py`), writes `SHA256SUMS` and
     attests the build provenance of every file (Sigstore, stored on GitHub);
   - builds the container image for linux/amd64 and linux/arm64 from the two musl archives
     (`scripts/release-image.sh`, no second compile; with the same BuildKit, commit time and
     revision, the same archives give the same digest) and runs the version check and the
     gallery smoke test through it on both platforms;
   - installs the x86-64 Windows zip with Scoop, from a manifest written like the bucket's, and
     runs the same checks through Scoop's shim;
   - dry-runs the JSR publish of the npm tarball.
4. The publishing jobs wait for every smoke test and for a maintainer to approve the `release`
   environment, then:
   - create the GitHub release with all files;
   - publish the eight crates to crates.io (trusted publishing, `cargo publish --workspace`);
   - stage `@graphviz-pure/wasm` on npm (trusted publishing, with npm provenance). The trusted
     publisher may only stage, so the version goes live only when a maintainer approves it with
     2FA: the package's **Staged Packages** tab on npmjs.com, or `npm stage approve <stage-id>`;
   - publish the files of the npm tarball (which carries `web/jsr.json`) to JSR with a pinned
     Deno (`deno publish`, OIDC). JSR has no staging: the version is live as soon as the job
     finishes, so the `release` approval is its only gate;
   - push the smoke-tested image to `ghcr.io/graphviz-pure/graphviz-pure` as `0.1.0`, attest its
     digest (the attestation is stored next to the image in the registry), then point `0.1`
     and `latest` at it if it is the newest stable `0.1.*` and the newest stable release. A
     pre-release gets only its own tag.
5. Within hours, `graphviz-pure/homebrew-tap` and `graphviz-pure/scoop-bucket` pull the
   release, verify each asset's attestation and checksum, and commit the new formula and
   manifest. `graphviz-pure/graphviz-pure.github.io` deploys the site from `master` builds the
   same way. No repository holds a credential for another.

Run the workflow by hand (Actions → Release → Run workflow) to build, smoke-test and attest
everything, the container image included, without publishing anything.

### If a publishing job fails

The publishing jobs are independent once the GitHub release exists, so a failure leaves the
other channels published. Fix the cause (an expired trusted-publisher configuration, a
registry outage), then **Re-run failed jobs** on the same run: never push a new tag for the
same version.

- `crates-io`: crates.io refuses a version that already exists. If no crate was published,
  re-run the job. If it failed partway, the remaining crates need a `cargo publish -p` run
  from `release.yml` in the `release` environment (trusted publishing accepts nothing else).
- `npm`, `jsr`: a re-run is safe if the job failed before publishing. If the version went up,
  there is nothing to re-run.
- `ghcr`: safe to re-run. Pushing the same image again changes nothing, and `0.1`/`latest`
  only ever move forward.
- `github-release`: the release is immutable once published. If the job failed before
  publishing, delete the draft release, then re-run.

## Verifying a download

```sh
gh attestation verify graphviz-pure-0.1.0-x86_64-unknown-linux-musl.tar.gz \
  --repo graphviz-pure/graphviz-pure --signer-workflow graphviz-pure/graphviz-pure/.github/workflows/release.yml
sha256sum --check --ignore-missing SHA256SUMS
npm audit signatures   # in a project that depends on @graphviz-pure/wasm
gh attestation verify oci://ghcr.io/graphviz-pure/graphviz-pure:0.1.0 \
  --repo graphviz-pure/graphviz-pure --signer-workflow graphviz-pure/graphviz-pure/.github/workflows/release.yml
```

The image verification checks the multi-platform index digest that the tag points to, so
verify by tag and then pull by digest (`docker pull ghcr.io/graphviz-pure/graphviz-pure@sha256:...`)
if you need the check and the pull to refer to the same bytes.

## Using the container image

The image holds the static binary and its licence, nothing else (no shell), and runs as
`nobody`:

```sh
docker run --rm -i ghcr.io/graphviz-pure/graphviz-pure -Tsvg < graph.dot > graph.svg
docker run --rm -v "$PWD:/data" --user "$(id -u):$(id -g)" ghcr.io/graphviz-pure/graphviz-pure \
  -Kneato -Tpng -o graph.png graph.dot
```

## One-time setup

Done once per channel by a maintainer. Nothing here needs a long-lived token in GitHub.

### GitHub (organization `graphviz-pure`)

- Organization settings → Authentication security → **Require two-factor authentication**
  (the only setting the API cannot change).
- CI and release builds run on GitHub-hosted runners, which are free for a public repository.
  Releases must stay there: npm trusted publishing rejects self-hosted runners, and
  GitHub-hosted builders are the documented basis for SLSA Build Level 3.
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

Since 2026-10-02 npm expires a trusted-publisher configuration 48 hours after it is created
unless it has published by then; the first successful (staged) publish validates it for good
([changelog](https://github.blog/changelog/2026-10-02-unvalidated-npm-trusted-publishing-configurations-now-expire/)).
If the configuration was created well before the first release, delete it and recreate it with
the same fields just before pushing the first tag, then release within 48 hours.

### JSR (scope `@graphviz-pure`)

1. Log in to jsr.io with GitHub and create the scope `@graphviz-pure`, then the package `wasm`.
2. Package settings → **GitHub repository**: link `graphviz-pure/graphviz-pure`. JSR accepts an
   OIDC publish only from a workflow in the linked repository.
3. Scope settings: keep publishing from GitHub Actions restricted to scope members (on by
   default), so a release publishes only when a member pushed the tag. Once the first version
   is out, also consider requiring every version to be published from CI.

JSR has no staged-approval step like npm's: `deno publish` makes the version live, and the
`release` environment's approval is the only gate. The JSR package holds the npm tarball's
files as listed in `web/jsr.json` (README.md, LICENSE, NOTICE and `pkg/`), so both registries
show `web/README.md`; npm's `package.json` is not part of it.
In Deno, `init()` fetches the `.wasm` next to the module, so a script importing
`jsr:@graphviz-pure/wasm` needs `--allow-net=jsr.io` (or passes the bytes to `init`).

### Container image (`ghcr.io/graphviz-pure/graphviz-pure`)

No token: the `ghcr` job pushes with its own `GITHUB_TOKEN` (`packages: write` in that job
only). GitHub's documentation gives two answers for the visibility of a package first pushed
from a workflow: private, like every new package, or inherited from the repository. Check
after the first release:

1. `docker logout ghcr.io && docker pull ghcr.io/graphviz-pure/graphviz-pure:0.1.0`. If that
   fails as unauthorized, the package is private: Organization → Packages → `graphviz-pure` →
   Package settings → Danger Zone → **Change visibility** → Public. This cannot be undone. If
   Public is not offered, allow it under Organization settings → Packages → Package creation.
2. In the same settings, **Manage Actions access** should list only
   `graphviz-pure/graphviz-pure` (a publishing repository gets the Admin role automatically), and
   the package should show this repository as its source (the image's
   `org.opencontainers.image.source` label).

### Homebrew and Scoop

Nothing to set up: the tap and bucket repositories update themselves after each release.
Users run `brew install graphviz-pure/tap/graphviz-pure` and
`scoop bucket add graphviz-pure https://github.com/graphviz-pure/scoop-bucket` then
`scoop install graphviz-pure/graphviz-pure`. Submitting to homebrew-core and the Scoop main
bucket comes later; both require a project with some adoption, and homebrew-core builds from
source.

The release workflow's `scoop-smoke` job installs each release's x86-64 zip with a manifest
written the way the bucket's `scripts/update.sh` writes it, using Scoop v0.6.0 at a pinned
commit. If the bucket's manifest format changes, change that job to match.

### Later channels

| channel | what it needs |
| :--- | :--- |
| winget | a manifest pull request to `microsoft/winget-pkgs` per release, made with komac. The first version is submitted by hand (`komac new`); later ones can be automated with `komac update --submit` from the release workflow. komac needs a **classic** token with the `public_repo` scope (fine-grained tokens can commit but cannot open the pull request, [komac#310](https://github.com/russellbanks/Komac/issues/310)). The plan: a dedicated bot account, not a member of the organization, that owns a fork of `winget-pkgs`, its token stored as a `release`-environment secret. Each Windows zip holds one portable `graphviz-pure.exe`, which komac detects as the nested installer |
| Debian/Ubuntu and Fedora repositories | Cloudsmith open-source plan (supports GitHub OIDC, so no stored token), or Fedora COPR and a Launchpad PPA, which build from source |
| AUR | an AUR account and SSH key; the `graphviz-pure-bin` (release binaries) and `graphviz-pure` (from source) PKGBUILDs are in [`packaging/aur/`](../packaging/aur), filled in from a verified release by `packaging/aur/update.sh VERSION` ([steps](../packaging/README.md#aur)) |
| nixpkgs, conda-forge | pull requests to their repositories; no account beyond GitHub. The recipes are in [`packaging/nixpkgs/`](../packaging/nixpkgs) and [`packaging/conda-forge/`](../packaging/conda-forge) ([steps](../packaging/README.md#nixpkgs)) |
| macOS notarization, Windows code signing | an Apple Developer ID (99 USD/year) and Azure Trusted Signing or an OV certificate; until then, binaries downloaded through a browser show a Gatekeeper or SmartScreen warning (Homebrew and Scoop installs do not) |
