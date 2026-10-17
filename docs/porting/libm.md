# Transcendental functions off macOS (`gv_core::math`)

The native aarch64-apple-darwin build matches C Graphviz byte for byte on the whole corpus.
The wasm32 build did not: 279 of the 2526 corpus files rendered differently (counting
multi-graph files; 197 without them). The cause is libm. On macOS, Rust's `f64::hypot`,
`powf`, `sin`, ... call the system libm (`libsystem_m`), the same one C Graphviz uses. On
wasm32 they come from compiler-builtins' musl port, and on Linux from glibc. These disagree
with Apple in the last bit on some inputs, and one differing bit is enough to change a
force-directed layout.

## Measurements

Arguments came from a native run over every corpus file with the `math-trace` feature
(`GV_MATH_TRACE=dir`, distinct argument tuples, capped at 4,000,000 per function), plus
1,000,000 random inputs per function. Correctly rounded (CR) results were computed with MPFR
4.2.2. Apple means macOS 27 arm64 `libsystem_m`, and wasm means Rust's wasm32 `f64` methods
run under node. Each cell counts calls whose result differs from Apple's.

| function | corpus args | Apple ≠ CR | wasm ≠ CR | wasm ≠ Apple | random: Apple ≠ CR | random: wasm ≠ Apple |
|---|---:|---:|---:|---:|---:|---:|
| hypot | 4,000,000 | 637,069 (15.9%) | 0 | 637,069 | 141,280 | 141,280 |
| pow | 4,000,000 | 5,224 (0.13%) | 106,530 | 110,356 | 1,303 | 88,984 |
| atan2 | 361,216 | 5,771 (1.6%) | 68,614 | 69,391 | 2,900 | 159,839 |
| sin | 17,303 | 495 (2.9%) | 467 | 566 | 33,390 | 36,977 |
| cos | 1,061,293 | 49,777 (4.7%) | 34,160 | 45,709 | 35,761 | 38,432 |
| tan | 205 | 63 | 5 | 62 | 392,982 | 390,921 |
| asin | 6 | 0 | 0 | 0 | 30,984 | 49,874 |
| acos | 47,255 | 5,074 | 2,785 | 6,097 | 206,546 | 220,971 |
| exp | 135 | 0 | 14 | 14 | 1,719 | 66,536 |
| log | 4 | 0 | 0 | 0 | 71 | 11,527 |
| log2 | 113 | 0 | 0 | 0 | 34 | 431 |
| cbrt | 2,273,583 | 177,250 (7.8%) | 0 | 177,250 | 79,233 | 79,233 |

Findings:

- Apple's libm is not correctly rounded. The wasm `hypot` and `cbrt` are (they are now
  CORE-MATH-derived), so a CR port would not have helped. Apple's `pow` is close: its largest
  error is 0.5106 ulp, and it differs from CR only on near-halfway cases. Its `sin` and `cos`
  are off by up to 0.89 ulp.
- `sqrt` is IEEE everywhere. `mul_add` on wasm32 (compiler-builtins' software `fma`) matched
  the hardware `fma` on 3,000,000 random, edge and subnormal-producing inputs.

### Which function moves which corpus

On macOS with `--features gv-core/portable-math`, one function at a time was swapped for the
`libm` crate's version, which is the code wasm32 runs. With every function swapped, the native
build failed on the same 135 files the wasm build does. Swapping one function at a time:

| function swapped | files that change |
|---|---|
| `hypot` | fdp 143, main 1 (fixed by the port below) |
| `pow` | sfdp 127, neato 3, main 1 |
| `atan2` | fdp 3 |
| `sin` | fdp 2 |
| `cos` | circo 1, fdp 3 |
| `tan`, `asin`, `acos`, `exp`, `log`, `log2` | none |

## What Apple's functions are

Candidates were compared with the system results on the inputs above:

- **`hypot`**: equals `sqrt(fma(small, small, big * big))` (with `big = max(|x|, |y|)`) on
  every corpus and random input, and on extreme, subnormal and special-value pairs. The
  variants `sqrt(x*x + y*y)`, `sqrt(fma(big, big, small²))`, `x*sqrt(1 + r²)` and musl all
  differ on 4–40% of inputs.
- **`cbrt`**: equals Apple's open-source `Libm-2026/Source/ARM/cbrt.c` compiled with clang's
  default FMA contraction, with 0 differences. Without contraction it differs on 1,038 of the
  corpus arguments.
- **`pow`, `sin`, `cos`, `tan`, `atan2`, `acos`, `exp`, `log`**: no match. The published
  Libm's `Source/ARM/*.s` files are empty, and the arm64 `libsystem_m` routines are
  hand-written assembly. Tried and rejected:
  - musl/fdlibm, with and without FMA. The fdlibm `__sin`/`__cos` kernels differ even for
    `|x| < π/4`.
  - musl's pow, which is ARM optimized-routines (6,218 corpus mismatches). Apple's `pow`
    indexes a 256-entry log table where ARM's uses 128.
  - Apple's Intel `sincostan.c`, which is close for `tan` (2/205) and differs for `sin`/`cos`.
  - CR implementations. CR `atan2` happens to fix the 3 fdp files.

## Approach

`gv_core::math` (`crates/gv-core/src/math/`) is the single entry point. Every `.hypot(`,
`.powf(`, `.sin(`, ... call in the workspace became `math::hypot(a, b)` and so on, the way C
writes `hypot(a, b)`. The rewrite was mechanical, done with a `syn`-based rewriter.

- On Apple targets the functions call the `f64` methods, so native output is unchanged.
- On every other target, or with `--features gv-core/portable-math` on macOS, they use
  `math::portable`:
  - `hypot`: the formula above, with exact power-of-two scaling outside `[2^-500, 2^500]`.
  - `cbrt`: a port of Apple's ARM `cbrt.c`, with `mul_add` where clang contracts.
  - everything else: the `libm` crate 0.2.16. This is the same musl-derived code compiler-builtins
    uses for wasm32, so the wasm results do not change. Linux now gets the same bits as wasm
    (glibc's `pow`/`exp` pick FMA or non-FMA paths by CPU), and `Cargo.lock` pins the results
    rather than the toolchain.
- `crates/gv-core/src/math/tests.rs` (macOS only) checks the portable `hypot` and `cbrt`
  against the system libm on 1,000,000 random and edge inputs each.

## Results

`node scripts/wasm-e2e.mjs` (see its header for the build steps) checks the wasm build against
C's hashes and renders every graph in multi-graph files through the new `renderAll` binding:

| corpus | files | before | after | native |
|---|---:|---:|---:|---:|
| main | 514 | 512 | 513 | 514 |
| neato | 438 | 435 | 435 | 438 |
| sfdp | 297 | 170 | 170 | 297 |
| circo | 280 | 279 | 279 | 280 |
| twopi | 279 | 279 | 279 | 279 |
| fdp | 379 | 233 | 376 | 379 |
| osage | 178 | 178 | 178 | 178 |
| patchwork | 140 | 140 | 140 | 140 |
| render | 21 | 21 | 21 | 21 |
| **all** | 2526 | 2247 | 2391 | 2526 |

`gv-diff e2e` on macOS with `portable-math` fails on exactly the same 135 files as wasm. That
means the remaining gap is entirely `pow`, `sin`, `cos` and `atan2`.

Closing the gap would take one of these:

1. **Reproduce Apple's `pow`, `sin` and `cos` (and `atan2`) from the binary.** This is the only
   way to get bit-identical output. It means reverse-engineering closed Apple code, which
   raises a licensing question for the project.
2. **Correctly rounded `pow` and `atan2`** (e.g. ports of CORE-MATH, MIT licence). Measured on
   macOS with `portable-math`: CR `pow` plus CR `atan2` reaches 2407/2526 (sfdp 183, neato
   438, fdp 376). CR `sin`/`cos` is worse than musl (circo 274). CORE-MATH's `pow` is about
   4,000 lines of C.

### Linux

CI (`.github/workflows/ci.yml`) runs `cargo test --workspace` on `ubuntu-latest`. That
includes the corpus ratchets (`crates/gv-diff/tests/*_fidelity.rs`), which compare against
C hashes and need no C install. Their baselines record macOS results, so they fail on Linux
for the same libm reason. Before this change Linux also used glibc's `hypot`, so fdp was much
worse there. A macOS run with `portable-math` stands in for Linux and wasm, and these ratchets
fail:

| ratchet | change |
|---|---|
| e2e | 2526 → 2391 |
| sfdp | exact 297 → 197 |
| fdp | exact 383 → 379 |
| neato | exact 438 → 436 |
| main/dot | 514 → 513 |
| circo | 280 → 279 |
| twopi | 280 → 279 |

osage and patchwork are unchanged. No Linux machine was available to run this directly. To
make Linux CI meaningful, either close the gap above or add per-target ("portable") baselines.

## Provenance and licences

- `portable/hypot.rs`: an original formula, identified by black-box comparison. No code was
  copied.
- `portable/cbrt.rs`: a Rust port of `Source/ARM/cbrt.c` from Apple's Libm-2026
  (`apple-oss-distributions/Libm`). The file is "Copyright 2008 Apple Inc. All rights
  reserved" (Jeff Kidder; polynomial by Steve Canon) and carries no licence header. The rest
  of Apple's Libm release is under the Apple Public Source License, a file-level copyleft.
  The port keeps the algorithm, the polynomial coefficients and the table's cube roots, with
  `z` and `1/z` recomputed. **A maintainer should review the licence before release.** If
  APSL terms are unacceptable, the libm crate's (CR) `cbrt` is the fallback. It differs from
  Apple on 7.8% of calls, but only pathplan's cubic solver uses `cbrt`, and the corpus did not
  change with it (cbrt is not among the functions that move the corpus above).
- `libm` crate 0.2.16: MIT, from rust-lang/compiler-builtins (a musl port).
- Measurement tools: `scripts/libm-probe.c` (Apple vs MPFR) and the `math-trace` feature.
