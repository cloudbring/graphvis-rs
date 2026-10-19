# Reference outputs from C Graphviz 16.1.0

Everything here was produced by C Graphviz 16.1.0 built without Pango, so that C measures text
with its built-in font metric tables exactly as the port does (`GVBINDIR` pointing at a plugin
directory with only the core, dot and neato plugins; see `scripts/gen-golden.sh`). The
`gv-diff` harness compares the port against these files, and `cargo test --workspace` runs it
as ratchets that fail on any regression.

| path | what | written by |
| :--- | :--- | :--- |
| `*.dot` | C's `-Tdot` layouts of `tests/wild` and `tests/fixtures` | `scripts/gen-golden.sh` |
| `baseline.json` | per-file layout scores the port must not fall below | `scripts/update-baselines.sh` |
| `<engine>/`, `<engine>-baseline.json` | the same for each engine corpus (`tests/<engine>/`) | `scripts/gen-golden.sh`, `scripts/update-baselines.sh` |
| `e2e.json` | hashes of C's `-Tsvg` for every corpus input, plus inputs whose C output is unstable | `gv-diff e2e --generate` |
| `e2e-baseline.json` | the inputs whose port `-Tsvg` equals C's byte for byte | `gv-diff e2e --update` |
| `render.tar.xz` | the render corpus, compressed (see below) | `scripts/gen-render-golden.sh` |
| `render/baseline.json` | render ratchet | `gv-diff render --update` |

Every baseline has a `*.portable.json` sibling, used by builds with the portable math profile
(Linux, WASM, or macOS with `--features gv-core/portable-math`); see `docs/porting/libm.md`.

## The render corpus is compressed

`render/` holds C's layout of each main-corpus graph (`render/laid/`) and C's rendering of that
layout in eight formats (`render/<format>/`). Unpacked that is about 50 MB of text, so git
tracks only `render.tar.xz` (about 3 MB) and the baselines. The unpacked directories are
gitignored.

- **Nothing to do to run the tests.** `gv-diff render` (and so `cargo test`) unpacks the
  archive with the system `tar` on first use, and again whenever the archive changes; the
  stamp file `render/.unpacked` records which archive is unpacked.
- **To unpack by hand:** `tar -xJf tests/golden/render.tar.xz -C tests/golden`.
- **To regenerate the references:** `scripts/gen-render-golden.sh` (needs the pango-free C
  build). It ends by running `scripts/pack-render-golden.py`, which rewrites `render.tar.xz`
  deterministically; commit the archive, not the directories.
- **If you edit unpacked files by hand**, run `scripts/pack-render-golden.py` afterwards.
  Otherwise the next `gv-diff render` sees the archive unchanged and keeps your edits, but
  they are not in git.
