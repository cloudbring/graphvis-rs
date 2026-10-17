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
| **GV-BUG-011** | `lib/common/textspan.c` & `textspan_lut.c` (`estimate_textspan_size`) | Font Alias Ignored in LUT Width Lookup | **Medium** (`"Times-Bold"` / `"Helvetica-Bold"` ignores `postscript_alias` and falls back to non-bold Times width) | **Reproduced** (output fidelity wins: `estimate_text_width_1pt` looks the name up in the LUT families only, as C does; `Times-Bold` measures as regular Times) |
| **GV-BUG-012** | `lib/common/labels.c` (`storeline` empty line height truncation) | Integer Truncation Bug | **Low** (Empty lines cast `(int)(fontsize * 1.20)` losing `.8` pt vs non-empty lines) | **Reproduced** (`storeline` truncates empty-line height to `(int)(fontsize * 1.2)` exactly like C; node/label sizes depend on it) |
| **GV-BUG-013** | `lib/common/textspan_lut.c` (`estimate_text_width_1pt` UTF-8 byte loop) | Multi-byte UTF-8 Mis-measurement | **Medium** (Iterates `const char *` byte-by-byte, counting 1 UTF-8 codepoint as 3–4 spaces) | **Reproduced** (measures `text.bytes()`, each byte >= 0x80 as a space, matching C widths) |
| **GV-BUG-014** | `lib/common/colxlate.c` (`colorxlate` permissive `sscanf` & `_Thread_local` leak) | Memory Leak & Malformed Hex Parsing | **Low** (`setColorScheme` leaks `gv_strdup` buffer; `sscanf` accepts 5-digit hex strings) | **Fixed** (Exact-length hex slice validation & zero-allocation scheme lookup) |
| **GV-BUG-015** | `lib/common/shapes.c` (`parse_reclbl` shared text/port buffer) | Aliased Buffers | **Low** (`abc <p> def` writes the port name over the field text, so the field label becomes empty; text typed while a port is open lands in the text buffer) | **Reproduced** (byte-level port in `gv_geom::node_size` keeps one buffer with C's pointer arithmetic) |
| **GV-BUG-016** | `lib/common/shapes.c` (`parse_reclbl` `ishardspace`) | Sticky Flag | **Low** (after the first `\ ` hard space, every later space in the same field list is kept, and a leading hard space is dropped) | **Reproduced** |
| **GV-BUG-017** | `lib/common/htmltable.c` (`size_html_tbl` `static textfont_t savef`) | Shared Static State | **Low** (nested tables with `<FONT>` overwrite the outer table's saved font, so the restored font can leak to later cells) | **Reproduced** (one save slot shared by every nesting level) |
| **GV-BUG-018** | `lib/common/htmltable.c` (`pos_html_tbl` fixed-size `ALIGN="LEFT"`) | Wrong Variable | **Low** (uses the table *height* as the width when left-aligning a `FIXEDSIZE` table) | **Reproduced** |
| **GV-BUG-019** | `lib/common/htmltable.c` (`make_html_label` on parse error) | Silent Label Loss | **Low** (any expat/grammar error, e.g. `<b>x</B>`, leaves the label with `dimen = 0` and nothing drawn instead of falling back to text) | **Reproduced** |
