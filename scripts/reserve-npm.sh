#!/usr/bin/env bash
# One-time: publishes and deprecates a 0.0.0 placeholder of @graphviz-pure/wasm, so that a
# trusted publisher can be configured for it. Run after `npm login`; npm asks for your 2FA
# code. See docs/releasing.md.
set -euo pipefail
work=$(mktemp -d)
cat > "$work/package.json" <<JSON
{
  "name": "@graphviz-pure/wasm",
  "version": "0.0.0",
  "description": "Placeholder for graphviz-pure, a Rust + WebAssembly port of Graphviz; the first release is 0.1.0.",
  "license": "EPL-2.0",
  "repository": { "type": "git", "url": "https://github.com/graphviz-pure/graphviz-pure.git" }
}
JSON
printf '# @graphviz-pure/wasm\n\nPlaceholder; the first release is 0.1.0. See https://github.com/graphviz-pure/graphviz-pure.\n' > "$work/README.md"
(cd "$work" && npm publish --access public)
npm deprecate "@graphviz-pure/wasm@0.0.0" "Placeholder; use 0.1.0 or later."
echo "Now add the trusted publisher and disallow tokens (docs/releasing.md); then npm logout."
