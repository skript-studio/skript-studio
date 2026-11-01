# REFACTOR_PLAN.md — what's done, what's next

> **Status:** T2 (delete the legacy regex-approximation pattern engine and
> route hover through the pattern-engine thread) is **COMPLETE**. The
> `Engine` init-option that used to switch between AST-only and merged
> diagnostics is also gone — diagnostics always merge both streams. This
> file now just records what landed and lists the remaining future work.

## What landed

### T2 — legacy regex-approximation engine deleted

- `crates/skript-core/src/pattern.rs` — **deleted** (was the regex-DSL
  compiler that treated `[craft]` as a per-char class instead of Java's
  optional literal and ignored `<regex>`, `:tag`, `¦mark`, `@time`,
  `*`/`~` flags).
- `crates/skript-core/src/matching.rs` — slimmed down to **`EntryTable`
  only**. The `SyntaxIndex` half (regex-approximation matcher) is gone;
  `EntryTable` is plain data (`Send + Sync`) and stays, because the LSP
  layer uses it for completions and for resolving engine-thread hover
  matches back to `SyntaxEntry` records.
- The single pattern engine is now `skript-patterns` + `skript-registry`
  (the faithful Java port). The `Engine` init-option and all branching
  that depended on it were removed in the diagnostic-merge refactor.

### Hover through the pattern engine thread

- `AppState::hover_via_engine` (formerly `java_hover`) sends the line to
  the dedicated engine thread, which runs
  `skript_checks::match_line_for_hover` against all compiled patterns
  and returns the best-matching entry ID + byte span.
- The LSP `hover` handler then resolves the entry ID to a `SyntaxEntry`
  via `EntryTable::get` and renders the syntax-doc markdown. AST-resolved
  hover (for local variables, functions, etc.) takes priority when both
  produce a result.

### Diagnostics always merge

- `publish_merged_diagnostics` unconditionally runs the AST validator
  (`skript-core::validation`) and the pattern-engine line checker
  (`skript_checks::check_document` via `AppState::check_document_via_engine`)
  and concatenates their results. The two streams emit disjoint codes
  (`parse-error`/`unused-variable`/`semantic` vs.
  `unparseable`/`addon-not-enabled`/`plugin-missing`/`ambiguous-addon`),
  so there is never a duplicate.
- The engine is built unconditionally at startup (compiling ~13.7k
  patterns takes a moment, so it runs on a background thread); until it
  becomes ready only AST diagnostics are published, then all open
  documents are re-validated.

## Future work

These items are intentionally **not** scheduled — pick them up when
priorities warrant.

### `java-*` identifier rename — **done in this commit**

The historical `java_*` / `Java*` naming in `crates/skript-lsp/src/state.rs`,
`backend.rs`, and the test files was a leftover from the parallel-port era.
It has been replaced with engine-agnostic names (`CheckRequest`,
`EngineState`, `check_requests`, `engine_thread_main`, `start_engine`,
`ensure_engine`, `check_document_via_engine`, `hover_via_engine`, …). No
behavior change.

### Multi-file / workspace support

The LSP currently treats every open document as an island — function and
variable indexing is per-file. Cross-file go-to-definition, references,
and rename (e.g. calling a function declared in another `.sk` file) need
a workspace-wide symbol index that updates as documents change.

### `.skriptrc.toml` hot-reload

`.skriptrc.toml` is read once during `initialize`. Watching the file
(`notify`-based) and rebuilding the engine with a new addon filter would
let users toggle addons without restarting the server.

### Publish the VSCode extension to the Marketplace

The extension is built and packaged via `bun run package`
(`extensions/vscode-skript/`) but not yet published. Marketplace
publishing requires a publisher ID, an icons set, and a stable version
bump cadence.

### Precompute the SkriptHub corpus as bincode

`Registry::embedded()` deserializes a 6.8 MB JSON corpus at startup and
then compiles ~13.7k patterns. Shipping a precompiled `bincode` blob of
the `Registry` (or at least of the per-entry `SkriptPattern`s) would
shave the bulk of the build time off startup at the cost of a build-time
codegen step.

### Parser improvements for the remaining 4 official syntax files

The official Skript test corpus under
`crates/skript-lsp/tests/fixtures/skript-official/` is parsed by the LSP
test suite. Four files still produce parse errors that the upstream Java
parser accepts; closing those gaps brings the AST validator to full
parity with Skript's grammar for the regression + misc corpora.

## Verification

`cargo test --workspace` post-refactor: **~614 tests pass**, 0 failures,
2 ignored (the WebSocket smoke tests, gated behind a running server).
`cargo clippy --workspace --all-targets -- -D warnings` is clean.
`cargo fmt --all --check` is clean.
