# Graphviz 16.1.0 C Bug & Hazard Ledger (`docs/c-bugs-ledger.md`)

As we port Graphviz 16.1.0 (`graphviz-16.1.0.tar.xz`) to pure, safe Rust using test-driven differential verification against `/opt/homebrew/bin/dot`, we audit the C reference implementation and document every bug, memory hazard, concurrency defect, and numerical instability we encounter and fix before/during translation.

---

## Ledger Summary

| Bug ID | Upstream C Location | Category | Severity | Status in `graphvis-rs` |
| :--- | :--- | :--- | :--- | :--- |
| **GV-BUG-001** | `lib/cgraph/scan.l` (`line_num`, `InputFile`, `Sbuf`, `lexbuf`) | Re-entrancy / Global Mutable State | **High** (Corrupts concurrent parses) | **Fixed** (State encapsulated in `Lexer<'a>`) |
| **GV-BUG-002** | `lib/common/ns.c` (`Tree_node`, `Tree_edge`, `Low`, `Lim`, `Slack`) | Re-entrancy / Global Mutable State | **Critical** (Crashes concurrent Network Simplex layouts) | **Fixed** (Encapsulated in `SimplexSolver`) |
| **GV-BUG-003** | `lib/dotgen/mincross.c` (`Root`, `GlobalMinRank`, `GlobalMaxRank`, `TE_list`, `TI_list`) | Re-entrancy / Global Mutable State | **High** (Corrupts crossing minimization across threads/workers) | **Fixed** (Encapsulated in `MincrossWorkspace`) |
| **GV-BUG-004** | `lib/util/exit.h` (`graphviz_exit`), `lib/cgraph/agerror.c` (`AGMAX`) | Process Abort on Recoverable Error | **Critical** (Traps WASM module / kills host process) | **Fixed** (Replaced with `Result<T, GraphvizError>`) |
| **GV-BUG-005** | `lib/cgraph/grammar.y` & `lib/common/htmlparse.y` | Memory Leak on Syntax Error | **Medium** (Leaks AST/HTML strings on malformed input) | **Fixed** (RAII drop + arena-scoped allocations) |
| **GV-BUG-006** | `lib/common/splines.c` & `lib/common/arrows.c` | Numerical (`NaN` propagation on zero-length edge vectors) | **Medium** (Produces `NaN` in SVG path `d="..."` attributes) | **Fixed** (Explicit epsilon / zero-length vector guards) |
| **GV-BUG-007** | `lib/common/ns.c` (`tight_tree` / `enter_edge` floating-point slack tie-breaking) | Non-determinism / Infinite Pivot Loop | **Medium** (Pointer-address order & float drift across architectures) | **Fixed** (Deterministic `EdgeId` tie-breaking & pivot cap) |
| **GV-BUG-008** | `lib/dotgen/compound.c` & `lib/dotgen/fastgr.c` | Dangling Virtual Edge / Node Pointers | **High** (Use-after-free when deleting cloned virtual edges) | **Fixed** (Typed generational/arena IDs `NodeId`, `EdgeId`) |
| **GV-BUG-009** | `lib/pathplan/shortest.c` & `lib/pathplan/route.c` | Unchecked Stack/Heap Scratch Growth & Degenerate Triangle Collinearity | **Medium** (Collinear polygon vertices can fail Pshortestpath) | **Fixed** (Checked collinearity & safe `Vec` buffers) |
| **GV-BUG-010** | `lib/cgraph/refstr.c` (`Refstr_t` bitfield counter overflow & global dict) | Reference Count / Thread-Safety | **Medium** (Shared static string dict is not thread-safe) | **Fixed** (Per-graph or `Arc<str>` / `StringInterner` with `u32` IDs) |
