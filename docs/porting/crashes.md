# Where C crashes: fixing panics at the cause

The port follows Graphviz's C function by function. Where the C has undefined behaviour (a
NULL dereference, an out-of-bounds read, a failed `assert`, an unbounded loop), the literal
translation becomes a Rust panic at the same spot: an `.unwrap()` on a pointer C never
checked, a `[i]` on an index C never bounds, an integer cast or add that overflows. The fuzzers
find these sites one at a time, and fixing them one at a time does not converge: in the
October 2026 p1 round, 14 single-site fixes turned up 8 new crashes next to them.

So a crash is fixed at its cause, not at the line that panics.

## Rules

- **The port must not panic, hang or run out of memory**, whatever C does. Where C produces
  output, the port matches it. Where C crashes, asserts or hangs, there is no output to match,
  and the port picks the smallest defensible behaviour: what C does on the nearest input it
  handles, or an error or warning and a clean exit. The comment names the C function and says
  that C crashes there.
- **Find the first point where the state goes wrong.** Before guarding the panicking line, ask
  which earlier step produced the bad state, and whether C reported it there (a warning such as
  `trouble in init_rank`, a failed return value it ignored). Fix it at that point, so every later
  site that depends on it is covered.
- **Group crashes by cause, not by panic site.** When filing or triaging, check whether open
  issues share the cause (the same diagnostic before the crash, the same input feature, the same
  function). One fix and one pull request closes the group.
- **Compare how C and Rust fail.** C keeps running on corrupt state and crashes later, or not at
  all; Rust stops at the first unchecked access. A Rust panic that C "survives" is usually C
  reading garbage, and the fix is still at the cause.
- **Report C bugs upstream.** If the cause is a bug in C (it crashes, hangs or reads out of
  bounds), label the issue `upstream-c` and note the C location, so it can be reported to
  Graphviz.

## Known causes

These four causes account for most crashes found so far. A new crash in one of these areas is
fixed in that cause's shared mechanism, not with a new local guard.

| Cause | How it fails | Fix in the port |
|---|---|---|
| **dot keeps going after a failed ranking** | Network simplex prints `trouble in init_rank` and returns unnormalised ranks; C carries on and crashes later in mincross, position or splines (#114, #232, #233). | Stop the layout where the ranking fails: an error and exit status 1. |
| **Extreme coordinates** | NaN, infinity or huge pinned positions flow into every engine and overflow at the first integer cast or recursion (#191, #196, #201, #227, #228, #229). | Validate positions once, where they are parsed. |
| **The ortho router** | Many internal invariants that C asserts or dereferences without checking (#186, #199, #202, #203). C falls back to straight edges when routing fails. | Ortho returns `Result` throughout; the entry point falls back to straight edges, as C does. |
| **`unwrap` and indexing on data C trusts** | The mechanism under all of the above. | `clippy::unwrap_used` and `clippy::indexing_slicing` as warnings in the layout crates map the remaining sites; new code returns an error instead. |

The fix for each cause records here the issues it closed and the behaviour it chose.
