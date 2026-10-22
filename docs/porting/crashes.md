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
| **dot keeps going after a failed ranking** | Network simplex prints `trouble in init_rank` and returns unnormalised ranks; C carries on and crashes later in mincross, position or splines (#114, #232, #233). | Stop the layout where a cluster's ranking fails, and where C's `install_in_rank` finds that the ranks do not fit the rank arrays: an error and exit status 1. A failed ranking of the root alone is not stopped; see [below](#dot-after-a-failed-ranking). |
| **Extreme coordinates** | NaN, infinity or huge pinned positions flow into every engine and overflow at the first integer cast or recursion (#191, #196, #201, #227, #228, #229). | Validate positions once, where they are parsed. |
| **The ortho router** | Many internal invariants that C asserts or dereferences without checking (#186, #199, #202, #203). C falls back to straight edges when routing fails. | Ortho returns `Result` throughout; the entry point falls back to straight edges, as C does. |
| **`unwrap` and indexing on data C trusts** | The mechanism under all of the above. | The [panic-site ratchet](#the-panic-site-ratchet) maps the remaining sites and stops the count from growing; new code returns an error instead. |

The fix for each cause records here the issues it closed and the behaviour it chose.

## dot after a failed ranking

Closed: #232, #233 (and #114 earlier, by #208).

`trouble in init_rank` means the rank constraints contain a cycle, usually rank sets that
contradict each other (`rank=min` and `rank=max` on one node) or a node that is both in a
cluster and in a rank set. On its own it does not stop C, and it does not stop the port:
over 3000 generated inputs, C printed it on 182 and still drew 155 of them, the same way
on every run, and the port's output equals C's on all 155. Stopping the layout at the
message would turn those drawings into errors, so the port stops only where the failed
ranking leaves state that C cannot use:

- **A cluster's own ranking** (`collapse_cluster` → `dot1_rank`). The cluster's ranks
  normally start at 0, and `cluster_leader` asserts that some node is on rank 0
  (`rank.c:316`). After a failed ranking there may be none. The port fails the layout there
  (`cluster_leader: the ranking of cluster X failed: none of its nodes is on rank 0`), at
  any cluster depth (#233).
- **`install_in_rank`** (`mincross.c`), C's own check that the ranks fit the rank arrays
  built from them. The port now has all of C's checks, in C's order. Where C prints an
  error (lines 1158, 1179, 1184, 1190), the port prints the same error and abandons the
  layout as C does; the output is C's (#232, input A). Where C fails
  `assert(GD_rank(g)[r].n <= GD_rank(g)[r].an)` after writing past the array (line 1165),
  the port fails the layout before the write (`install_in_rank: rank R of G is full`)
  (#232, input B). Past the root's first component, C's line-1190 bound uses `>`, so one
  store can land one slot past the array unreported; the port grows the buffer there and
  goes on, as C does.

Most of C's crashes after `trouble in init_rank` come from a related cause that is not in
the ranking itself: a cluster keeps the rank range of nodes that `mark_clusters` later
removes from it (`x was already in a rankset, deleted from cluster G`), so some of its
ranks have no node. `rec_reset_vlists` then follows a stale rank leader (C segfaults in
`furthestnode`, and on some inputs crashes or not depending on the run), and mincross can
leave a node unplaced. The port reaches the same state; `make_regular_edge`'s rank check
(#208) still stops the layout there, so it stays.

Not this cause, although they often follow the same inputs: #230 (`flat_node` reads rank
-1 when no rank was made for it), #231 (a cluster left with no nodes), #234
(`conc_slope` on a one-sided merge node).

## The panic-site ratchet

`scripts/panic-sites.sh` counts, for each source file of `graphviz-pure-layout` and
`graphviz-pure-geom`, the library code (not tests) that four clippy lints flag:

| Column | Lint | Site |
|---|---|---|
| unwrap | `clippy::unwrap_used` | `.unwrap()` |
| expect | `clippy::expect_used` | `.expect(..)` |
| index | `clippy::indexing_slicing` | `v[i]` or `v[a..b]` that can go out of bounds |
| arith | `clippy::arithmetic_side_effects` | integer `+ - * / % << >>` that can overflow or divide by zero |

The lints are passed to clippy for this one run; they are not turned on for the workspace,
because most of these sites are faithful C and correct for every input C handles. The counts are
checked in as `tests/panic-sites.tsv`. CI runs the script after clippy, and it fails when one
file's count of one kind goes up, or when a file that is not listed has any site. A count going
down passes; `scripts/panic-sites.sh --update` rewrites the baseline to lock it in. A pull request
that raises a count on purpose (a new port of C code that indexes the way C does, a new clippy
release that counts differently) refreshes the baseline with `--update` and says why.

When the count goes up, prefer what C does at that point: if C checks a bound or a NULL, port the
check; if C reads past the end or dereferences NULL, return an error from the cause.

At the start (2026-10-22) the two crates had 10,612 sites: 278 unwrap, 44 expect, 8,144 index and
2,146 arith. These ten files have the most, and are where to look for the next causes:

| File (under `crates/graphviz-pure-`) | unwrap | expect | index | arith | total |
|---|--:|--:|--:|--:|--:|
| `layout/src/dotgen/dotsplines.rs` | 32 | 5 | 478 | 61 | 576 |
| `layout/src/dotgen/mincross.rs` | 32 | 0 | 389 | 102 | 523 |
| `layout/src/dotgen/position.rs` | 33 | 0 | 302 | 39 | 374 |
| `layout/src/cutil.rs` | 1 | 0 | 185 | 179 | 365 |
| `layout/src/neatogen/digcola.rs` | 3 | 0 | 265 | 65 | 333 |
| `layout/src/neatogen/voronoi.rs` | 22 | 0 | 250 | 41 | 313 |
| `layout/src/ortho/trapezoid.rs` | 0 | 0 | 276 | 10 | 286 |
| `layout/src/dotgen/rank.rs` | 11 | 1 | 243 | 19 | 274 |
| `layout/src/neatogen/multispline.rs` | 4 | 0 | 179 | 81 | 264 |
| `layout/src/sfdpgen/spring_electrical.rs` | 0 | 3 | 154 | 103 | 260 |

The script prints the current table, most sites first. An unwrap is the cheapest to trace: it
names the value C assumed was there.
