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

These causes account for most crashes found so far. A new crash in one of these areas is
fixed in that cause's shared mechanism, not with a new local guard.

| Cause | How it fails | Fix in the port |
|---|---|---|
| **dot keeps going after a failed ranking** | Network simplex prints `trouble in init_rank` and returns unnormalised ranks; C carries on and crashes later in mincross, position or splines (#114, #232, #233). | Stop the layout where a cluster's ranking fails, and where C's `install_in_rank` finds that the ranks do not fit the rank arrays: an error and exit status 1. A failed ranking of the root alone is not stopped; see [below](#dot-after-a-failed-ranking). |
| **dot's cluster membership** | A rank set or an earlier cluster takes a node from a cluster after the cluster was ranked (`x was already in a rankset, deleted from cluster G`). The cluster keeps its rank range, so it can have ranks with no node, or no nodes at all; C crashes later in `build_ranks`, `furthestnode`, `rank_box` (#114, #231). | Stop the layout where C dereferences an emptied cluster's NULL node list. Ranks with no node, which C often draws through, are kept; see [below](#dot-cluster-membership). |
| **Extreme coordinates** | NaN, infinity or huge pinned positions flow into every engine and overflow at the first integer cast or recursion (#191, #196, #201, #227, #228, #229, #239, #240). | Keep C's positions, which C lays out wherever it finishes; bound the algorithms whose limits depend on magnitude (see [below](#extreme-coordinates)). |
| **The ortho router** | Many internal invariants that C asserts or dereferences without checking (#186, #199, #202, #203). C falls back to straight edges when routing fails. | Ortho returns `Result` throughout; the entry point falls back to straight edges, as C does. |
| **`unwrap` and indexing on data C trusts** | The mechanism under all of the above. | The [panic-site ratchet](#the-panic-site-ratchet) maps the remaining sites and stops the count from growing; new code returns an error instead. |

The fix for each cause records here the issues it closed and the behaviour it chose. Narrower
causes, each confined to one function, have their own sections after these.

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

Not this cause, although they often follow the same inputs: #230 and #241 (`flat_node`
reads the rank above the top one when no rank was made for it), #231 (a cluster left with
no nodes), #234 (`conc_slope` on a one-sided merge node).

## dot cluster membership

Closed: #231 (with its `clust_ht` and draw-instead-of-crash variants). #114's remaining
routes are this cause too.

dot ranks each cluster before the rest of the graph. A node can be taken from a cluster
after that: `mark_clusters` (at the parent's `class1`) deletes a cluster node that a later
rank set also claims, and newrank's `set_parent` registers a cluster before `node_induce`
deletes the nodes an earlier sibling cluster has. The cluster's rank range and rank
leaders still describe the nodes it had. C does not recompute them, and its drawing
depends on that: on a rank with no node left, `save_vlist` saves the root's node just right
of the cluster, and the cluster box reaches out to it. That output is C's on every run, so
the port keeps the stale range too. It stops only where C's behaviour cannot depend on
anything but a NULL pointer:

- **A cluster with no nodes left.** C removes one only among the root's clusters (the check
  at the top of `dot_mincross`, `removing empty cluster`). A nested one is kept: its rank
  leaders are installed in the parent's ranks, which can fail first with
  `install_in_rank`'s own `an = 0` error (line 1158, as in C), and otherwise
  `expand_cluster` → `build_ranks` dereferences its NULL node list (`mincross.c:1227`).
  The port fails the layout there:
  `expand_cluster: cluster X has no nodes left (a rank set or an earlier cluster took them)`.
  Dropping the cluster earlier would change the 60 inputs in the generated corpus where C
  stops cleanly at line 1158.

Two later points look like better places to stop but are not, because C gets past them
on some inputs with output that is the same on every run:

- **A saved leader that was freed.** When the node `save_vlist` saves for a rank with no
  node is the rank leader of a cluster expanded later, `remove_rankleaders` frees it, and
  `rec_reset_vlists` reads it in `furthestnode`. That is a use after free. C segfaults on
  most such inputs (#114's repro), but on some it finishes with the same output on every
  run: the freed node's fields are still intact, or its memory has been reused by a live
  node. The port keeps the freed leader's fields, as macOS's allocator does, and goes on;
  it matches C where those fields are what C reads, and still differs where the memory
  was reused (two generated inputs, where C draws and the port stops with
  `trouble in init_rank`, as before this change).
- **A node `merge2` drops.** `allocate_ranks` counts a slot per node and one spare per
  rank. A cluster rank with no node still gets a skeleton rank leader, which uses the spare;
  with two of them, a later component stores one slot past the rank array, which C's
  line-1190 check (`>`) lets through, and `merge2` cuts the rank at `an`, so the node there is
  never placed. C draws 7 generated inputs that pass through this, the same way on every
  run.

Where the unplaced node or the freed leader's stale state is read as a rank,
`make_regular_edge`'s rank check (#208) stops the layout, where C reads `GD_rank` out of
bounds in `rank_box`. Two inputs where C reads out of bounds there without crashing fail
in the port.

Measured against C and against the port before this change, on 33,686 inputs (Thread A's
corpora, #208's review hits and 4,000 new generated ones, none with `pack`): no output
changes on an input C draws or stops cleanly on, the same way on two runs. Only inputs
where C crashes change:
804 now stop at the `expand_cluster` error, of which the port drew 502 before, panicked on
54, and failed later on 248. The remaining panics on these corpora belong to #230 and
#234. With `pack=true`, where the port's components already differ from C's (#101), the
check also fires on some inputs where C stops cleanly at `install_in_rank` line 1158;
the port differed from C there before too.

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
file's count of one kind goes up, or when a file that is not listed has any site. It also fails
when the count itself looks broken: a kind with no site at all, or a lint clippy does not know.
The lints are forced on, so an `allow` in the source does not hide a site. A count going down
passes; `scripts/panic-sites.sh --update` rewrites the baseline to lock it in. A pull request that
raises a count on purpose (a new port of C code that indexes the way C does, a new clippy release
that counts differently) refreshes the baseline with `--update` and says why.

When the count goes up, prefer what C does at that point: if C checks a bound or a NULL, port the
check; if C reads past the end or dereferences NULL, return an error from the cause.

At the start (2026-10-22) the two crates had 10,615 sites: 278 unwrap, 43 expect, 8,146 index and
2,148 arith. These ten files have the most, and are where to look for the next causes:

| File (under `crates/graphviz-pure-`) | unwrap | expect | index | arith | total |
|---|--:|--:|--:|--:|--:|
| `layout/src/dotgen/dotsplines.rs` | 32 | 5 | 478 | 61 | 576 |
| `layout/src/dotgen/mincross.rs` | 32 | 0 | 390 | 104 | 526 |
| `layout/src/dotgen/position.rs` | 33 | 0 | 302 | 39 | 374 |
| `layout/src/cutil.rs` | 1 | 0 | 185 | 179 | 365 |
| `layout/src/neatogen/digcola.rs` | 3 | 0 | 265 | 65 | 333 |
| `layout/src/neatogen/voronoi.rs` | 22 | 0 | 250 | 41 | 313 |
| `layout/src/ortho/trapezoid.rs` | 0 | 0 | 276 | 10 | 286 |
| `layout/src/dotgen/rank.rs` | 11 | 0 | 244 | 19 | 274 |
| `layout/src/neatogen/multispline.rs` | 4 | 0 | 179 | 81 | 264 |
| `layout/src/sfdpgen/spring_electrical.rs` | 0 | 3 | 154 | 103 | 260 |

The script prints the current table, most sites first. An unwrap is the cheapest to trace: it
names the value C assumed was there.

## The ortho router: one failure boundary

Closed: #186, #199, #202 and #203 (round 1, fixed one site at a time, now covered by the boundary), and #228 and #229.

`lib/ortho` asserts many of its invariants: `LIST_GET` bounds-checks every index, and `chanSearch` asserts its lookups. It trusts the rest without checking: raw `N_DAD` and `cells[]` pointers, and `int` distances. In the port, every function in `crates/graphviz-pure-layout/src/ortho/` returns `Result<_, Fail>` (`ortho/fail.rs`), and every index that comes from data goes through a checked accessor. `ortho_edges_of` (`orthoEdges`) handles each failure in one place, the way C handles the failure it does detect at that stage:

| Stage | C | Port |
|---|---|---|
| Building the maze (`mkMaze`: partition, trapezoidation, `markSmall`, `chkSgraph`) | Returns NULL where it detects a failure; elsewhere it asserts or segfaults | Behaves as if C had returned NULL: `orthoEdges` returns -1. dot abandons the layout and draws only the nodes (#203, #200). The neato-family callers draw straight edges. |
| Routing (`shortPath`, `convertSPtoRoute`, channels, tracks) | `goto orthofinish` where it detects a failure (heap overflow, incomparable segments, each with C's message); elsewhere it asserts, segfaults or loops | Behaves as `goto orthofinish`: no edge is routed and `orthoEdges` returns 0. A warning names the failure (`... - edges not routed orthogonally`). The routes are installed only once every edge has been routed. |

Inputs that C routes never reach a failure, so their output is unchanged.

- **#229:** pins 1e12 inches apart make `shortPath`'s distance `-(N_VAL + E_WT)` too large for an `int`. That is undefined behaviour in C, which then fails `PQcheck`. The port rejects any distance outside `(INT_MIN, 0]`. A positive distance would make C's `PQupheap` climb past its guard forever.
- **#228:** NaN pins leave no trapezoid inside the polygon, and C's `monotonate_trapezoids` reads one past the end of `tr`. The port fails the maze at that read.
- Loops that C would run forever on broken links are bounded by the size of the structure: stale parent chains, trapezoid walks and maze walks. The three recursions (`locate_endpoint`, `traverse_polygon` and `DFS_visit`) run on an explicit stack, in C's order.
- The boundary does not depend on positions being validated where they are parsed (the "Extreme coordinates" cause): it holds whatever the input.

## Flat edge labels on the top rank

Closed: #230, #241.

A labelled flat edge that is not adjacent to its head gets a label node on the rank above
(`flat_node`). On rank 0, `flat_edges` first makes a rank -1 for it (`abomination`), but C's
test looks only at `ND_flat_in`. A labelled edge that is not its class's representative
sits in `ND_other` instead: a parallel `a -> b` behind an unlabelled one (`class2`'s
`merge_oneway`), or a `b -> a` merged into `a -> b` by `flat_rev`. The `ND_other` loop
then calls `flat_node` for it, and C reads `GD_rank(g)[-1]` and segfaults. The port makes
rank -1 for those edges as well, so the edge is handled as it is on any lower rank: C
itself, on the same edges one rank down, places the label node and then fails to route
the edge (`lost a b edge`, exit 1). On rank 0 the port prints the same errors. When the
labelled edge is the representative, C makes rank -1 itself, and nothing changes.

`flat_edges` also assumes the top rank is rank 0, which it is once ranking has
normalised the ranks (`abomination` asserts it). After a failed ranking of the root
(`trouble in init_rank`, which does not stop the layout; see above), nothing normalised
them and the top rank is `GD_minrank(g)` > 0. C's ranks above it were never allocated, so
`flat_node` on it reads a NULL `v` and segfaults (#241). The port runs the test, and makes
the new rank, at the top rank. When the top rank is 0 this is C's code.

## Concentrated cluster edges

Closed: #234.

With `concentrate=true`, an edge into or out of a cluster whose ports differ from a
parallel edge's shares that edge's virtual chain (`interclrep`) but is routed on its own,
through `make_regular_edge`'s `hackflag` branch. That branch follows the chain as it was
before `dot_concentrate` rewired it, and it goes wrong in two ways:

- **The chain now ends at a concentrator.** The walk stops there, but C
  (`dotsplines.c:1845`) passes the concentrator's merge flag to `endpath` together with the
  original edge, whose head is the real node further on. `conc_slope` then averages the real
  node's in- and out-edges. If the node has both, C draws, and the port draws the same way.
  If one side is empty, C dereferences NULL. The port then ends the path with the real
  node's own merge flag (false), as for an edge that was not concentrated.
- **`mergevirtual` deleted the node the chain ends at.** In the up pass, the main edge's
  `ED_to_virt` chain can still end at a deleted node, which has no edges left, and C segfaults in `straight_len`.
  `mergevirtual` now records the node it merged each deleted node into, and the port routes
  the edge from there, as the chain's own edges are.

Both change output only where C crashes. (With `splines=line`, C survives the deleted-node
case because `makeLineEdge` never reads the head; the redirect changes nothing there either.)

## Extreme coordinates

The plan was to validate positions once, where they are parsed. Running C showed this would
break the first rule. C 16.1.0 and the port were run on pins at NaN, ±inf and 1e6–1e308 inches,
across neato, fdp, `-n` and `-n2`, with every `splines` mode, comparing `-Tdot`.

- **C produces output for most non-finite pins.** C reads `pos` with `sscanf("%lf")`, which
  accepts `nan` and `inf`. It then lays the whole component out at NaN (`pos="nan,nan"`), or
  prints `inf` for a finite pin that overflows once multiplied by 72. The port already
  matched those outputs byte for byte, in neato, `-n` and `-n2`. C crashes only on some
  routes: ortho asserts (#228), and `update_bb_bz` overflows its stack on #201's shape. Which
  route an input takes depends on `splines`, the components and the other pins, so no check at
  parse time can reject exactly the inputs C crashes on. Positions are therefore kept as C
  reads them, and the routes where C crashes are made safe downstream. #187's finiteness test
  in `update_bb_bz` covers #201; ortho's failure boundary covers #228 (see above).
- **Finite positions are laid out exactly by C up to 1e306 inches**, except in algorithms that
  have a limit of their own:
  - **Packing** (#174) was budgeted in 0.1.1. A non-finite pin gives a component an infinite
    box; where its width or height is NaN (`inf - inf`), `computeStep`'s discriminant is NaN
    and C fails `assert(d >= 0)` (#239). The port returns step 0 there, which takes C's own
    `stepSize <= 0` exit: packing is skipped with a warning.
  - **`update_bb_bz`** (#227) refines a curve until its control points are within 2pt of the
    chord, which takes about `sqrt(length)` splits at a given magnitude, and more as the
    coordinates' rounding nears 2pt: 1.3e5 for pins 1e10 inches apart, 5.7e7 at 1.5e15.
    Further out, 2pt is below the rounding and C never finishes. The count grows
    continuously, so the port uses a cutoff, as with packing: 2^28 splits per segment, about
    5 s of C's work. A segment under it is refined as in C. Past it, the remaining pieces
    are taken whole, and so is every later segment of that layout. Their control points
    enclose the curve, so the bb may be a little larger than C's, on a segment that takes C
    seconds.
  - **Integer conversions that C lets wrap.** Where C converts a far position to `int` and
    goes on, the port reproduces what the reference build does. fdp's `gridRepulse` (#196)
    saturates the cell index and wraps `i + 1`. neato's `initItem` for `overlap=*ortho*`
    (#240) converts `x` and `y` through 64 bits and keeps the low 32, so they wrap
    from about 3e6 inches, and its `int` arithmetic after that wraps too. The output stays
    C's.
  - **VPSC overlap removal** (`overlap=vpsc`). C's `Rectangle` and `Node` assert `x <= X`
    and `width < 1e40`, which a pin at NaN, infinity or 1e300 fails (1e300 is infinite as a
    `float`). The port skips overlap removal there with a warning. C's solver loop
    `while (fabs(lastcost - cost) > 0.0001)` ends on a NaN cost; the port's had the test
    inverted and never ended. It now ends where C's does.
  - **Ortho's integer path costs** overflow once coordinates span about 3e7 inches (INT_MAX
    points), and C fails `PQcheck` (#229). The ortho router's failure boundary rejects such
    distances (see above).

Closed: #227, #239, #240.
The round-1 guards stay, because they match C and finite or NaN positions still reach them:
#206 (the Hilbert order and shift for a bb above 2^32 points, or an infinite one), #207 (the
wrapping grid cell index at 1e17), and #187's finiteness test in `update_bb_bz` (#173, #201).
