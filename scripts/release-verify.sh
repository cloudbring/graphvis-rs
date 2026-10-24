#!/usr/bin/env bash
# Checks that VERSION is out on every channel, and prints pass or fail per channel:
#
#   github  the GitHub release: every asset's build-provenance attestation (from release.yml) and
#           SHA256SUMS
#   crates  every publishable workspace crate at VERSION on crates.io
#   jsr     @graphviz-pure/wasm VERSION in JSR's meta.json, not yanked
#   ghcr    the image's VERSION, X.Y and latest tags point at one digest, and VERSION's
#           attestation verifies (a pre-release has only its own tag)
#   npm     @graphviz-pure/wasm VERSION is the `latest` dist-tag, and `npm audit signatures`
#           passes for it (only after the owner approved the staged version)
#   taps    the Homebrew tap's formula and the Scoop bucket's manifest carry VERSION, and their
#           newest update.yml run succeeded
#
# --ci checks github, crates, jsr and ghcr only: release.yml's verify job runs it before npm is
# approved and before the tap and bucket update. --wait SECONDS retries a failing registry check
# (crates.io, JSR, ghcr) until it passes or the time is up. Reads only.
#
# Usage: scripts/release-verify.sh VERSION [--ci] [--wait SECONDS]
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/release-lib.sh

version="" ci=0 wait=0
while [ $# -gt 0 ]; do
  case "$1" in
    --ci) ci=1 ;;
    --wait) wait=${2:?--wait needs seconds}; shift ;;
    -*) die "unknown option $1" ;;
    *) version=$1 ;;
  esac
  shift
done
[ -n "$version" ] || die "usage: scripts/release-verify.sh VERSION [--ci] [--wait SECONDS]"
check_version "$version"
tag=v$version
signer=$REPO/.github/workflows/release.yml
image=ghcr.io/$REPO
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
ua="graphviz-pure release check (https://github.com/$REPO)"
fails=0

result() { # result NAME PASS DETAIL
  if [ "$2" = 1 ]; then echo "- [x] $1: $3"; else echo "- [ ] $1: FAIL — $3"; fails=$((fails + 1)); fi
}
# retry CMD...: runs CMD until it succeeds or $wait seconds have passed.
retry() {
  local start=$SECONDS
  until "$@"; do
    [ $((SECONDS - start)) -lt "$wait" ] || return 1
    sleep 30
  done
}

echo "## Release $version: channels"
echo

# GitHub release and its attestations.
github() {
  local n=0 bad=() f
  [ "$(gh release view "$tag" --repo "$REPO" --json isDraft -q .isDraft 2> /dev/null)" = false ] ||
    { result github 0 "no published release $tag"; return; }
  gh release download "$tag" --repo "$REPO" --dir "$work/release" > /dev/null
  for f in "$work"/release/*; do
    n=$((n + 1))
    gh attestation verify "$f" --repo "$REPO" --signer-workflow "$signer" > /dev/null 2>&1 || bad+=("$(basename "$f")")
  done
  if ! (cd "$work/release" && if command -v sha256sum > /dev/null; then sha256sum --check --quiet SHA256SUMS; else shasum -a 256 --check --quiet SHA256SUMS; fi) > /dev/null 2>&1; then
    bad+=(SHA256SUMS)
  fi
  if [ ${#bad[@]} = 0 ]; then result github 1 "$n assets attested, SHA256SUMS matches"
  else result github 0 "not verified: ${bad[*]}"; fi
}

# The workspace crates without publish = false, by package name.
crates() {
  local f
  for f in crates/*/Cargo.toml; do
    grep -qE '^publish *= *false' "$f" && continue
    awk -F'"' '/^\[package\]/{p=1} p && /^name *=/{print $2; exit}' "$f"
  done
}
crates_missing() {
  local c
  : > "$work/crates-missing"
  for c in $(crates); do
    curl -fsS -A "$ua" -o /dev/null "https://crates.io/api/v1/crates/$c/$version" 2> /dev/null || echo "$c" >> "$work/crates-missing"
  done
  [ ! -s "$work/crates-missing" ]
}
crates_io() {
  local n
  n=$(crates | wc -l | tr -d ' ')
  if retry crates_missing; then result crates 1 "$n crates at $version"
  else result crates 0 "not on crates.io: $(tr '\n' ' ' < "$work/crates-missing")"; fi
}

jsr_ok() {
  curl -fsS "https://jsr.io/@graphviz-pure/wasm/meta.json" -o "$work/jsr.json" 2> /dev/null &&
    jq -e --arg v "$version" '.versions[$v] != null and (.versions[$v].yanked // false) == false' "$work/jsr.json" > /dev/null
}
jsr() {
  if retry jsr_ok; then result jsr 1 "@graphviz-pure/wasm $version (latest: $(jq -r .latest "$work/jsr.json"))"
  else result jsr 0 "@graphviz-pure/wasm $version is not in meta.json"; fi
}

# The digest a tag points to, from the registry's API with an anonymous pull token.
digest() {
  local token
  token=$(curl -fsS "https://ghcr.io/token?scope=repository:$REPO:pull" | jq -r .token)
  curl -fsSI -H "Authorization: Bearer $token" \
    -H "Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.oci.image.manifest.v1+json" \
    "https://ghcr.io/v2/$REPO/manifests/$1" 2> /dev/null | tr -d '\r' | awk -F': ' 'tolower($1) == "docker-content-digest" {print $2}'
}
ghcr_tags() {
  local t
  : > "$work/ghcr"
  for t in "$@"; do echo "$t $(digest "$t")" >> "$work/ghcr"; done
  [ "$(awk '{print $2}' "$work/ghcr" | sort -u | wc -l | tr -d ' ')" = 1 ] && [ -n "$(awk 'NR == 1 {print $2}' "$work/ghcr")" ]
}
ghcr() {
  local tags=("$version")
  case "$version" in *-*) ;; *) tags+=("${version%.*}" latest) ;; esac
  if ! retry ghcr_tags "${tags[@]}"; then
    result ghcr 0 "tags differ: $(awk '{printf "%s=%s ", $1, ($2 == "" ? "missing" : substr($2, 1, 19))}' "$work/ghcr")"
  elif ! gh attestation verify "oci://$image:$version" --repo "$REPO" --signer-workflow "$signer" > /dev/null 2>&1; then
    result ghcr 0 "$image:$version's attestation does not verify"
  else
    result ghcr 1 "${tags[*]} → $(awk 'NR == 1 {print substr($2, 1, 19)}' "$work/ghcr")…, attested"
  fi
}

npm_channel() {
  local latest
  latest=$(npm view @graphviz-pure/wasm dist-tags.latest 2> /dev/null || true)
  if [ "$latest" != "$version" ]; then
    result npm 0 "latest is ${latest:-unknown}, not $version (approve the staged version first)"
    return
  fi
  if (cd "$work" && mkdir npm && cd npm && npm init -y > /dev/null &&
    npm install --ignore-scripts --no-audit --no-fund "@graphviz-pure/wasm@$version" > /dev/null 2>&1 &&
    npm audit signatures > "$work/npm-audit" 2>&1); then
    result npm 1 "latest is $version; $(grep -m1 -i 'verified registry signature' "$work/npm-audit" || echo 'npm audit signatures passed')"
  else
    result npm 0 "npm audit signatures failed: $(tail -n 3 "$work/npm-audit" 2> /dev/null | tr '\n' ' ')"
  fi
}

# The tap's formula and the bucket's manifest, and their newest update.yml run.
taps() {
  local repo path run content
  for spec in homebrew-tap:Formula/graphviz-pure.rb scoop-bucket:bucket/graphviz-pure.json; do
    repo=graphviz-pure/${spec%%:*} path=${spec#*:}
    run=$(gh run list --repo "$repo" --workflow update.yml --limit 1 --json status,conclusion,url \
      -q '.[0] | "\(.status) \(if (.conclusion // "") == "" then "-" else .conclusion end) \(.url)"')
    content=$(gh api "repos/$repo/contents/$path" -q .content | base64 --decode)
    if ! grep -qF "$version" <<< "$content"; then
      result "${spec%%:*}" 0 "$path does not carry $version (newest update.yml run: $run)"
    elif [ "${run%% *}" != completed ] || [ "$(cut -d' ' -f2 <<< "$run")" != success ]; then
      result "${spec%%:*}" 0 "$path carries $version but the newest update.yml run is $run"
    else
      result "${spec%%:*}" 1 "$path carries $version; update.yml ${run##* }"
    fi
  done
}

github
crates_io
jsr
ghcr
if [ "$ci" = 0 ]; then
  npm_channel
  taps
fi
echo
if [ "$fails" = 0 ]; then echo "**All channels pass.**"; else echo "**$fails channel(s) fail.**"; fi
[ "$fails" = 0 ]
