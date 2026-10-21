# Distribution packages

Recipes for channels that are submitted by hand once a release exists. Nothing here is
built or published by this repository's workflows. The checksums are placeholders until the
release is out; fill them in only from verified downloads, as described below.

| channel | files | builds from | submitted to |
| :--- | :--- | :--- | :--- |
| AUR `graphviz-pure-bin` | [`aur/graphviz-pure-bin/`](aur/graphviz-pure-bin) | the release's static musl archives (x86_64, aarch64) | `ssh://aur@aur.archlinux.org/graphviz-pure-bin.git` |
| AUR `graphviz-pure` | [`aur/graphviz-pure/`](aur/graphviz-pure) | the GitHub source tarball of the tag, with cargo | `ssh://aur@aur.archlinux.org/graphviz-pure.git` |
| nixpkgs | [`nixpkgs/pkgs/by-name/gr/graphviz-pure/package.nix`](nixpkgs/pkgs/by-name/gr/graphviz-pure/package.nix) | the tag, with `rustPlatform.buildRustPackage` | a pull request to `NixOS/nixpkgs` |
| conda-forge | [`conda-forge/graphviz-pure/`](conda-forge/graphviz-pure) | the GitHub source tarball of the tag, with cargo (rattler-build v1 recipe) | a pull request to `conda-forge/staged-recipes` |

All three source builds use the GitHub tarball of the tag rather than the crates.io crate: it
holds the workspace `Cargo.lock`, the release profile in `.cargo/config.toml` (thin LTO, one
codegen unit) and the test inputs that the command line's tests read. They build and test only
the `graphviz-pure-cli` package; the workspace's other binary, `gv-diff`, is the project's
differential test harness.

## Tests in the source packages

The source recipes run the command line's own tests (`cargo test -p graphviz-pure-cli`): about
twenty end-to-end runs of the built binary, offline, in seconds. They do not run
`cargo test --workspace`, whose fidelity ratchets compare the port with C Graphviz's committed
outputs on about 2,600 inputs: that works offline from the tarball too, but takes far longer than
a distribution build should, and upstream CI already runs it on every change. conda-forge runs
`graphviz-pure -V` and renders a two-node graph to SVG and PNG instead.

## The `dot` command

No package installs a `dot` command, and none provides or conflicts with `graphviz`:
graphviz-pure is the `dot` command line only, without the other Graphviz tools (`gvpr`, `neato`
and friends as separate commands, the libraries and plugins) that packages depending on
`graphviz` expect. Both can be installed side by side.

Users who want `dot` to run graphviz-pure can link it themselves, ahead of any Graphviz on their
`PATH`:

```sh
ln -s "$(command -v graphviz-pure)" ~/.local/bin/dot
```

The program's name only changes how it names itself in messages. Unlike C Graphviz, a link
called `neato` (or `fdp`, ...) still uses the `dot` layout engine; use `-Kneato` instead.

## AUR

**One-time setup:** create an account on https://aur.archlinux.org, add an SSH public key to it
(My Account → SSH Public Key), and point SSH at that key:

```
Host aur.archlinux.org
  IdentityFile ~/.ssh/aur
  User aur
```

**Each release:**

1. `packaging/aur/update.sh 0.1.0`. It downloads the two Linux musl archives and `SHA256SUMS`
   from the release, runs `gh attestation verify` on each archive (signer workflow
   `release.yml`, source ref the tag) and checks it against `SHA256SUMS`. For the source tarball,
   which has no attestation, it checks that the tag is signed by a key in
   `.github/allowed_signers` on `master` and that the tarball's contents equal the tagged tree.
   Then it writes `pkgver`, `pkgrel=1` and the checksums into both PKGBUILDs, removes the TODO
   lines, and regenerates `.SRCINFO` with `makepkg --printsrcinfo` (in an `archlinux` container
   if makepkg is not installed). It prints the source tarball's sha256 for the conda-forge recipe.
2. Test both packages on Arch, ideally in a clean chroot (`pkgctl build` from `devtools`, or
   `makepkg -sri`), and run `namcap PKGBUILD` and `namcap *.pkg.tar.zst`. Two warnings are
   expected for `graphviz-pure-bin`: "Reference to x86_64 should be changed to $CARCH" (the
   per-architecture `source_x86_64` must name its own archive) and "ELF file is unstripped"
   (`!strip`, below).
3. Publish each package. Its AUR repository holds only `PKGBUILD` and `.SRCINFO`:

   ```sh
   git clone ssh://aur@aur.archlinux.org/graphviz-pure-bin.git   # empty the first time
   cp packaging/aur/graphviz-pure-bin/{PKGBUILD,.SRCINFO} graphviz-pure-bin/
   cd graphviz-pure-bin && git add PKGBUILD .SRCINFO && git commit -m "Update to 0.1.0" && git push
   ```

   and the same for `graphviz-pure`. The first push creates the package.
4. Commit the updated `packaging/aur/` files here in a pull request.

`graphviz-pure-bin` keeps the binary unstripped (`!strip`), so the installed file is the attested
release binary byte for byte. Both packages provide `graphviz-pure` and conflict with each
other. `license` is `EPL-2.0`, `OFL-1.1` (the Tinos, Arimo and Cousine fonts compiled into the
binary for PNG and JPEG text) and `MIT` (the CORE-MATH `pow` port), the same as the conda-forge
and nixpkgs recipes; `LICENSE` and `NOTICE` are installed under `/usr/share/licenses/`.

## nixpkgs

**One-time setup:** a GitHub account is enough. In the same pull request, add a maintainer entry
for `cloudbring` to `maintainers/maintainer-list.nix` in a separate first commit
(`maintainers: add cloudbring`), then fill in `meta.maintainers` in `package.nix`.

**Submitting:**

1. In a nixpkgs checkout on a branch from `master`, copy
   `packaging/nixpkgs/pkgs/by-name/gr/graphviz-pure/package.nix` to the same path.
2. Compute the two hashes with the fake-hash trick: both start as `lib.fakeHash`. Run
   `nix-build -A graphviz-pure`; it fails with `hash mismatch ... got: sha256-...` for `src`.
   Put that value in `src.hash` and build again; the second failure gives `cargoHash`. (For `src`
   alone, `nix-prefetch-github graphviz-pure graphviz-pure --rev v0.1.0` or
   `nix flake prefetch github:graphviz-pure/graphviz-pure/v0.1.0` print the same hash.)
3. Build until it succeeds: `nix-build -A graphviz-pure`, then
   `./result/bin/graphviz-pure -V`. Format with `nix fmt` (CI requires nixfmt), and run
   `nixpkgs-review` on the branch or the PR.
4. Commit as `graphviz-pure: init at 0.1.0` and open the pull request, filling in its checklist
   (built on which platforms, tested the binary, sandboxing on).
5. Later releases: change `version`, reset both hashes to `lib.fakeHash` and repeat step 2, with
   the commit message `graphviz-pure: 0.1.0 -> 0.2.0`. `nixpkgs-update` (r-ryantm) usually opens
   these pull requests by itself.

The expression follows nixpkgs' current style: `finalAttrs`, `tag =` in `fetchFromGitHub`,
`cargoHash` (vendoring with `fetchCargoVendor` is the only mode now; `useFetchCargoVendor` has
been removed), `__structuredAttrs`, and `versionCheckHook` with `-V` as the install check.
`checkPhase` stays on but runs only `--package graphviz-pure-cli` (see above). `meta.license`
lists `epl20`, `ofl` and `mit`, as in the other recipes.

## conda-forge

conda-forge wants new recipes in the v1 format (`recipe.yaml`, built with rattler-build); the v0
`meta.yaml` is deprecated for new recipes. The recipe follows conda-forge's Rust template:
`cargo auditable install`, `cargo-bundle-licenses` writing `THIRDPARTY.yml` (the licences of all
statically linked crates, which conda-forge requires), stripped symbols and fat LTO.

**Submitting:**

1. Fork `conda-forge/staged-recipes`, branch from `main`, and copy
   `packaging/conda-forge/graphviz-pure/recipe.yaml` to `recipes/graphviz-pure/recipe.yaml`.
2. Set `source.sha256` to the source tarball's sha256 printed by `packaging/aur/update.sh`, or
   `curl -sL https://github.com/graphviz-pure/graphviz-pure/archive/refs/tags/v0.1.0.tar.gz | sha256sum`
   (only after the tag's signature has been checked, which update.sh does). Remove the TODO line.
3. Optionally build locally: `pixi run build-linux` (or `python build-locally.py`) in the
   staged-recipes checkout, and `pixi run lint`.
4. Open the pull request, complete the checklist, and comment
   `@conda-forge/help-rust, ready for review!`. Once merged, conda-forge creates
   `conda-forge/graphviz-pure-feedstock` with `cloudbring` as its maintainer; later versions
   arrive as pull requests from the autotick bot.

`license` is `EPL-2.0 AND OFL-1.1 AND MIT`: the port, the fonts compiled into the binary, and the
CORE-MATH `pow` port (see `NOTICE`); crate dependencies are listed in `THIRDPARTY.yml`.
