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
- **Port C's own checks, don't replace them.** Where C detects a bad state itself (an overflow
  check such as `sadd_overflow` or `INT_MAX / t < w`, an internal-invariant message such as
  `add_tree_edge: missing tree edge`) and stops or returns an error, the port does the same, with
  C's message. `wrapping_*` arithmetic is right only where C's arithmetic has no check of its own.
- **A limit C doesn't have needs a measurement.** A budget the port adds against a hang or
  running out of memory sits at or beyond the point where C stops being practical, measured in
  time and memory on the reference build, and its comment gives the measurement (#250: packing's
  cell budget refuses an input C packs in 1 s).

## Known causes

These causes account for most crashes found so far. A new crash in one of these areas is
fixed in that cause's shared mechanism, not with a new local guard.

| Cause | How it fails | Fix in the port |
|---|---|---|
| **dot keeps going after a failed ranking** | Network simplex prints `trouble in init_rank` and returns unnormalised ranks; C carries on and crashes later in mincross, position or splines (#114, #232, #233). | Stop the layout where a cluster's ranking fails, and where C's `install_in_rank` finds that the ranks do not fit the rank arrays: an error and exit status 1. A failed ranking of the root alone is not stopped; see [below](#dot-after-a-failed-ranking). |
| **dot's cluster membership** | A rank set or an earlier cluster takes a node from a cluster after the cluster was ranked (`x was already in a rankset, deleted from cluster G`). The cluster keeps its rank range, so it can have ranks with no node, or no nodes at all; C crashes later in `build_ranks`, `furthestnode`, `rank_box` (#114, #231). | Stop the layout where C dereferences an emptied cluster's NULL node list. Ranks with no node, which C often draws through, are kept; see [below](#dot-cluster-membership). |
| **Extreme coordinates** | NaN, infinity or huge pinned positions flow into every engine and overflow at the first integer cast or recursion (#191, #196, #201, #227, #228, #229, #239, #240). | Keep C's positions, which C lays out wherever it finishes; bound the algorithms whose limits depend on magnitude (see [below](#extreme-coordinates)). |
| **The ortho router** | Many internal invariants that C asserts or dereferences without checking (#186, #199, #202, #203). C falls back to straight edges when routing fails. | Ortho returns `Result` throughout; the entry point falls back to straight edges, as C does. |
| **C's own overflow checks dropped** | C checks an integer sum for overflow and exits with an error (`virtual_weight`, `x_cutval`); the port multiplied unchecked or wrapped, so it drew a layout from wrapped weights, or panicked in a build with overflow checks (#252). Unchecked `int` sums on huge `weight`, `minlen`, `ranksep`, `nodesep` or `margin` panicked the same way (#271). | Each check is ported with C's message as a layout error. Where C has no check the port wraps, and where the wrapped weights crash C (`update` with no entering edge) the port fails the layout; see [below](#cs-own-overflow-checks). |
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

### What the unwraps assume

Grouped by the value C assumed was there, most of the unwraps in the three densest dot files fall
into four classes. Each class is one candidate cause: a fuzzer that targets the assumption finds its
crashes together. Counts are from master on 2026-10-22, when the whole baseline was 10,184 sites.

| Class | Sites | C assumes | Broken by |
|---|--:|---|---|
| `rv(…).unwrap()` (`GD_rank(g)[r].v[i]`) | 57 (mincross, position, dotsplines) | every rank has a node in that slot | ranks left empty by a failed root ranking, an emptied cluster (#231), a flat label on the top rank (#241) |
| `boxes.last().unwrap()` (`P->boxes[P->nbox - 1]`) | 14 (dotsplines) | `beginpath`/`endpath` made a box | ports and clipping on zero-size or shapeless nodes, samehead/sametail, self-loops |
| `to_orig.unwrap()`, `label.expect`, `rep.unwrap()` | 19 | a virtual edge chain leads back to its real edge | chains rewired by `class2`, `flat_rev`, `concentrate` (#234) |
| `.ln/.rn.unwrap()` (`GD_ln`, `GD_rn`) | 11 (position) | `make_lrvn` ran for every cluster | clusters emptied after ranking ([cluster membership](#dot-cluster-membership)) |

Two causes don't show up as unwraps. One is the arith column: integer attributes at extreme values
(`weight`, `minlen`, `nslimit`, `mclimit`, `searchsize`, `peripheries`, `sides`), the
[extreme coordinates](#extreme-coordinates) cause for attributes that are not positions. The other
is C's checks that the port dropped, as in #252. C prints a message where it notices a bad state,
so a C diagnostic with no counterpart in the port is where to look.

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
    seconds. The budget does not bound the depth: at 1e300 points the pieces stop shrinking
    (a piece recurs two splits down) while `ptToLine2` overflows to NaN, and C recurses until its stack
    overflows (#269: `ratio` or `ranksep` at 1e300). The port refines on an explicit stack in
    C's order, and a piece deeper than C's stack holds ends the segment as the budget does.
  - **Path boxes `add_box` drops** (#270). Huge sizes or scales (`quantum` or `ratio` at
    1e300, cluster `fontsize` at 1e20) make some of an edge's boxes empty. `add_box` drops them,
    but `completeregularpath` still counts them, so C's `adjustregularpath` goes past `P->nbox`.
    Those slots are stale and nothing reads them before the next `add_box` writes them, so the
    port stops at the last box and draws what C draws. Where every box is dropped, C segfaults
    in `Pshortestpath`; the port's `checkpath` loses the edge.
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
  - **Exterior labels** (`addXLabels`). `xlhdxload` asserts that each label area's centre
    fits an `int`, which an infinite or 400-digit `fontsize`/`labelfontsize` (or an object at
    infinity, #191) fails; past it, the placement slide never ends. The port stops there as
    `xlhdxload`'s own failure return does: no exterior label is placed, with a warning (#301).

Closed: #227, #239, #240, #269, #270, #301.
The round-1 guards stay, because they match C and finite or NaN positions still reach them:
#206 (the Hilbert order and shift for a bb above 2^32 points, or an infinite one), #207 (the
wrapping grid cell index at 1e17), and #187's finiteness test in `update_bb_bz` (#173, #201).

## C's own overflow checks

Closed: #252, #271.

C checks two integer sums for overflow. Where a check fails, C prints an error and exits 1,
after writing the graphs before the failing one. The port fails the layout at the same point,
with C's message, and writes nothing more:

| C | Message | Port |
|---|---|---|
| `virtual_weight` (mincross.c:1725), `INT_MAX / t < ED_weight(e)` | `overflow when calculating virtual weight of edge` | `dotgen/mincross.rs` `virtual_weight` |
| `x_cutval` (ns.c:1060), `sadd_overflow` | `overflow when computing edge weight sum` | `common/ns.rs` `x_cutval`; `rank2` returns the error to every caller |

The stderr line has the port's usual prefix for a failed layout
(`Error: <file>: Layout computation error in dot: …`) where C prints `Error: …`.

**Where C has no check, the port wraps.** Everywhere else, C's `int` arithmetic has undefined
behaviour on overflow, and the reference build wraps. That covers ns.c's arithmetic on
weights, cut values, ranks and minimum lengths, and dotgen's weight sums (`group`'s ×100,
merged parallel and virtual edges, `CL_BACK` cluster edges, newrank's merged constraint
edges, `contain_clustnodes`' +128). It also covers the sums on `minlen`, `ranksep` and
`nodesep` (`edgelabel_ranks`, `make_LR_constraints`, `make_edge_pairs`, #271). The port
wraps there too (`wrapping_*`). Before, a build with overflow checks panicked at these
sites, and the panics stopped some inputs before they reached C's two checks: `x_val`'s
`rv -= weight` runs before `x_cutval`'s sum. A comment at each dotgen site says so, and so
does the module note in `common/ns.rs`. One site doesn't wrap in the reference build.
`make_edge_pairs` passes `m + 1` to a `double` parameter, and clang computes it as
`(double)m + 1`, so `INT_MAX + 1` arrives as 2147483648 and `largeMinlen` reports it. The
port does the same.

**What wrapped weights do in C.** A wrapped sum can come back positive (three parallel
edges of weight 2e9). C draws those inputs, and so does the port, byte for byte. A sum that
wraps negative leaves a tree edge whose cut value is negative with no edge to replace it.
`enter_edge` returns NULL, and `rank2` passes it to `update`, which dereferences it
(`SLACK(f)`, ns.c:710) and crashes. Before, the port returned 2 there and went on, so it
drew from broken ranks or panicked later in class2 or mincross. Now it fails the layout at
that point (`network simplex: no edge can enter the tree for a tree edge with a negative
cut value`). All 14 of C's crashes that were traced in a debug build stopped there: 7 of
1500 generated inputs with extreme weights, and 7 hand-written ones. Other negative cut
values make network simplex cycle until `maxiter`, which is INT_MAX for ranking. Measured
at 1e5 to 7e7 iterations, that takes about 0.27 µs per iteration in C and 0.19 µs in the
port, so about 10 and 7 minutes, and with a lower `nslimit1` the two outputs are identical.
That is C's own limit, so the port adds none.

**Also reported now:** ns.c's internal errors, `add_tree_edge: missing tree edge`,
`add_tree_edge: empty outedge list` and `add_tree_edge: empty inedge list`,
`update: mismatched lca in treeupdates` and `invalidate_path: skipped over LCA`. The port
already returned where C does. Only the messages were missing. dot's `largeMinlen`
(position.c:64) is reported too: `Edge length … larger than maximum 2147483647 allowed`.
The port already clamped the edge as C does, and the error makes dot exit 1, as in C
(part of #262).

**Checked and left as they are:**

- `RectArea`'s `UINT64_MAX / dim < area` (label/rectangle.c:73). Both sides of an rtree
  rectangle must pass 2^32 points. Every engine has put the drawing's lower-left corner
  near the origin before `addXLabels`, so such a rectangle has an object centre past
  `INT_MAX`, and C fails `xlhdxload`'s assertion first. That assertion belongs to
  [extreme coordinates](#extreme-coordinates).
- Checks the port already has: `late_int`'s `rv > INT_MAX`, `scale_clamp`, dijkstra's
  `INT_MAX - prevClosestDist < 10`, and `make_aux_edge`'s clamp.
- Checks that cannot fail in the port: `Pobsopen`'s `n > INT_MAX`, and the `SIZE_MAX`
  checks in `gv_calloc`/`gv_recalloc`/`LIST`, which Rust's allocator makes.
- neato's overlap constraint graph (`constraint.c`) ranks with the same network simplex.
  Its edges weigh 1 or 100, so `x_cutval`'s sum cannot overflow for a graph that fits in
  memory, and the port ignores the error there.

## C's numeric idioms

Closed: #192, #194, #216, #272, #274, #292.

Some C idioms look like a Rust operation but differ at NaN, at the ends of the integer
ranges, or in how they print. A port of one of them uses the helper in
`graphviz_pure_core::cnum`, which behaves as the reference build (clang on arm64, the macOS
libc) does, not the Rust operation that looks the same:

| C | Differs from Rust's look-alike in | Helper |
|---|---|---|
| `MIN(a,b)`, `MAX(a,b)` (`arith.h` ternaries) | the result is `b` whenever the test is false, so a NaN `b` is kept; `f64::min`/`max` drop it. `fmin`/`fmax` are Rust's. | `c_min`, `c_max` |
| `is_exactly_zero`, `is_exactly_equal` (`gv_math.h`) | they compare bits: `-0.0` is not zero, and a NaN equals itself | `is_exactly_zero`, `is_exactly_equal` |
| `ROUND(f)`, `POINTS(f)` | the result is an `int` | `c_round` |
| `ROUND` in a pair that clang vectorizes | converts through 64 bits and keeps the low 32 | `c_round_low32` |
| `strtol`, `sscanf("%ld")` | saturates at `LONG_MIN`/`LONG_MAX`, reads a prefix after white space | `strtol` |
| `atoi`, `sscanf("%d")` | the saturated `long`, truncated to 32 bits | `atoi`, `scan_d` |
| `late_int`, `mapBool` (`utils.c`) | built on these | `late_int`, `map_bool` |
| `printf("%.Nf")`, `%g` | `nan`, not `NaN` | `fmt_f`, `fmt_g` |

**Float-to-int conversions.** arm64's `fcvtzs`/`fcvtzu` into a 32-bit register saturate and
give 0 for a NaN, as Rust's `as i32`/`as u32` do, so a plain `(int)d` is `d as i32`. The
port has to convert to C's type (`int`, not `i64`), and then do C's `int` arithmetic, which
wraps (fdp's `T_pass1 = T_unscaled * T_maxIters / 100` with `maxiter=INT_MAX` is 0 in C,
where the 64-bit product ran 2^31 iterations, #272). Clang converts through 64 bits only
where it vectorizes or widens. In the reference dylibs (`otool -tv`, looking for
`fcvtzs x`, `fcvtzs.2d` and `fcvtzu`) that is: `gvRenderJobs` (`init_job_pagination`'s
`job->width`/`height` and `setup_page`'s `pageBoundingBox`, #194), `cAdjust` (`initItem`,
#240), `colorxlate` and `attach_edge_colors`, plus three conversions to 64-bit unsigned
types (`RectArea`, `generateRandomOrdering`, `StressMajorizationSmoother_smooth`). A new
conversion site follows the default unless its function is on this list.

**C's font dictionary** (#274). C interns every label font in `gvc->textfont_dt`, a CDT
splay tree whose comparison (`textfont_comparf`) orders sizes with `<` and `>`. A NaN
`fontsize` compares equal to any size, so the label gets the font that the search reaches
first: the most recently used font of that name, or, when a NaN font was interned first,
later labels of that name get the NaN one. The port interns fonts in the same splay
tree (`core::cdt`, moved from layout), one per thread and run, at C's sites: `storeline`,
and for HTML labels `mkFont` (when the parser reads the token), `pushFont` and
`size_html_txt`. C measures each label once and keeps the interned font in each span;
the port measures some objects again (dot's node pre-pass, port lookups, patchwork, the
root label after `graph_init`) and the renderer measures them all. So the geometry
functions measure through `font::textfont_object` with the object's identity (`node:`,
`edge:` with its id, `graph:`, `root:`): the first measurement in a layout interns and
keeps the sizes, and later ones get the same sizes and leave the tree alone. Where the
port's first measurement comes in a different order from C's (fdp's nested cluster
labels, which C makes one level at a time in `deriveGraph`; C14), a NaN size can still
meet a different font. The render-time `_ldraw_` fonts of `emit.c` are not interned.

**Not this cause:** #249 (`overlap=prism` with pins near 1e7 inches). The divergence
starts in the prism smoother's first iteration and only at that magnitude, C's residual
there is 8.5e14, and the path has no conversion that differs from Rust, but its
`SparseMatrix_multiply_vector` and `vector_product` are FMA-contracted: C11.
