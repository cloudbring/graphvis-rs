#!/usr/bin/env bash
# Fills in pkgver and the checksums of both AUR packages for a published release, after
# verifying what it downloads, and regenerates their .SRCINFO files.
#
# Usage: packaging/aur/update.sh VERSION      (e.g. 0.1.0)
#
# - The two Linux musl archives (graphviz-pure-bin) must have a build-provenance attestation
#   from this repository's release workflow for the tag, and match the release's SHA256SUMS.
# - The GitHub source tarball (graphviz-pure) is not a release asset and has no attestation, so
#   the tag must be signed by a key in .github/allowed_signers (read from master, as the release
#   workflow does) and the tarball's contents must equal the tagged tree.
#
# Needs gh (logged in), git, curl, tar and sha256sum or shasum. .SRCINFO is written with
# `makepkg --printsrcinfo`, run in an archlinux container when makepkg is not installed and
# docker is; otherwise the script says what to run.
set -euo pipefail

version=${1:?usage: $0 VERSION}
version=${version#v}
tag=v$version
repo=graphviz-pure/graphviz-pure
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
# sed -i differs between GNU and BSD; write through a temporary file instead.
subst() {
  local file=$1; shift
  sed "$@" "$file" > "$file.tmp" && mv "$file.tmp" "$file"
}

echo "== release assets of $tag" >&2
mkdir "$work/dist"
targets=(x86_64-unknown-linux-musl aarch64-unknown-linux-musl)
patterns=(--pattern SHA256SUMS)
for t in "${targets[@]}"; do patterns+=(--pattern "graphviz-pure-$version-$t.tar.gz"); done
gh release download "$tag" --repo "$repo" --dir "$work/dist" "${patterns[@]}"
for t in "${targets[@]}"; do
  f=graphviz-pure-$version-$t.tar.gz
  gh attestation verify "$work/dist/$f" --repo "$repo" \
    --signer-workflow "$repo/.github/workflows/release.yml" --source-ref "refs/tags/$tag" >&2
  sum=$(sha256 "$work/dist/$f")
  listed=$(awk -v f="$f" '$2 == f || $2 == "*" f {print $1}' "$work/dist/SHA256SUMS")
  [ "$sum" = "$listed" ] || { echo "error: $f does not match SHA256SUMS" >&2; exit 1; }
  case $t in x86_64*) sum_x86_64=$sum ;; aarch64*) sum_aarch64=$sum ;; esac
done

echo "== source tarball of $tag" >&2
curl -fsSL -o "$work/src.tar.gz" "https://github.com/$repo/archive/refs/tags/$tag.tar.gz"
git clone --quiet --depth 1 --branch "$tag" "https://github.com/$repo.git" "$work/repo"
curl -fsSL -o "$work/allowed_signers" \
  "https://raw.githubusercontent.com/$repo/master/.github/allowed_signers"
[ "$(git -C "$work/repo" cat-file -t "refs/tags/$tag")" = tag ] \
  || { echo "error: $tag is not an annotated tag" >&2; exit 1; }
git -C "$work/repo" -c gpg.format=ssh -c gpg.ssh.allowedSignersFile="$work/allowed_signers" \
  verify-tag "$tag" >&2
mkdir "$work/a" "$work/b"
tar -xzf "$work/src.tar.gz" -C "$work/a"
git -C "$work/repo" archive --prefix="graphviz-pure-$version/" "$tag" | tar -x -C "$work/b"
diff -r "$work/a" "$work/b" >&2 || { echo "error: the source tarball differs from $tag" >&2; exit 1; }
src_sum=$(sha256 "$work/src.tar.gz")

echo "== PKGBUILDs" >&2
for p in graphviz-pure-bin graphviz-pure; do
  subst "$here/$p/PKGBUILD" -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/"
done
subst "$here/graphviz-pure-bin/PKGBUILD" \
  -e "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('$sum_x86_64')/" \
  -e "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('$sum_aarch64')/" \
  -e "/^# TODO(release): placeholders/d"
subst "$here/graphviz-pure/PKGBUILD" \
  -e "s/^sha256sums=.*/sha256sums=('$src_sum')/" \
  -e "/^# TODO(release): placeholder/d"

echo "== .SRCINFO" >&2
for p in graphviz-pure-bin graphviz-pure; do
  if command -v makepkg >/dev/null; then
    (cd "$here/$p" && makepkg --printsrcinfo > .SRCINFO)
  elif command -v docker >/dev/null; then
    # makepkg refuses to run as root, even for --printsrcinfo.
    docker run --rm -i --platform linux/amd64 archlinux:latest bash -c \
      'useradd -m b && mkdir /p && cat > /p/PKGBUILD && chown -R b /p && cd /p && su b -c "makepkg --printsrcinfo"' \
      < "$here/$p/PKGBUILD" > "$here/$p/.SRCINFO.tmp"
    mv "$here/$p/.SRCINFO.tmp" "$here/$p/.SRCINFO"
  else
    echo "warning: no makepkg or docker; run 'makepkg --printsrcinfo > .SRCINFO' in $here/$p" >&2
  fi
done

cat >&2 <<EOF

Updated packaging/aur for $version (attestations, SHA256SUMS and the signed tag verified).
source tarball sha256: $src_sum
  (the same file conda-forge's recipe.yaml uses: put this value in its source.sha256)
EOF
