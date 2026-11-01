# REFACTOR_PLAN.md — Diagnostic merge + Engine-enum removal

> **Status:** the diagnostic merge is **done**; the `Engine` enum is
> **removed**. This document records the work and what remains.

## 1. Goal

Collapse the binary choice between the AST validator (skript-core) and the
java-port line checker (skript-checks) into a single merged pipeline that
runs both unconditionally and combines their results. Remove the
`Engine` init-option and all branching that depended on it.

The motivation: the two streams produce **disjoint** diagnostic codes
(`parse-error`/`unused-variable`/`semantic` vs.
`unparseable`/`addon-not-enabled`/`plugin-missing`/`ambiguous-addon`), so
they can never duplicate. Running both yields the strict superset of
either alone, with no downside.

## 2. What was done

### 2.1 `Engine` enum removed (`crates/skript-lsp/src/options.rs`)

- Deleted the `Engine` enum (Core / JavaPort variants, `as_str`,
  `Display`, `Deserialize` impl — ~50 lines).
- Removed the `engine: Option<Engine>` field from `InitOptions`.
- Removed the `engine: Engine` field from `EffectiveConfig` and its
  default.
- Removed the `use serde::de::Error as _;` import (only the `Engine`
  `Deserialize` impl needed it).
- Replaced the engine-specific tests with three new tests:
  `effective_config_defaults`, `init_options_parse_checker_lists`,
  `init_options_fields_are_optional`.

### 2.2 Merged diagnostics (`crates/skript-lsp/src/backend.rs`)

- Replaced the module-level "Diagnostics engine selection" comment with a
  "Diagnostics" section that describes the merged two-stream pipeline.
- Removed the `if config.engine == Engine::JavaPort { … } else { … }`
  branch in `publish_diagnostics`. The method body is now a single call
  to a new free function `publish_merged_diagnostics(&state, &client, uri)`.
- The free function:
  1. Runs `core_lsp_diagnostics` (AST validator).
  2. Calls `state.java_check(...)`; if the engine responds, maps the
     `LineDiagnostic`s to LSP `Diagnostic`s and **extends** the AST list.
  3. Publishes the merged list via `client.publish_diagnostics`.
  If the engine is still building or permanently broken, `java_check`
  returns `None` and only AST diagnostics are published — exactly the
  desired graceful-degradation behavior.
- `initialize()` now spawns the engine-build task **unconditionally**
  (the `if self.state.config().engine == Engine::JavaPort` guard is
  gone). Log messages say "pattern engine" rather than "java-port
  engine". After the engine becomes ready, the spawned task re-publishes
  merged diagnostics for every open document via
  `publish_merged_diagnostics(&state, &client, &uri)`.

### 2.3 Test updates

- `crates/skript-lsp/tests/java_engine_integration.rs`: removed `Engine`
  imports and assertions; renamed the two plumbing tests to
  `init_options_expose_checker_lists` and
  `effective_config_carries_checker_fields`; the
  `java_port_flags_unparseable_line` ignored test now constructs the
  state from `EffectiveConfig::default()` directly.
- `crates/skript-lsp/tests/diagnostic_specs.rs` (new): 29 per-rule
  contract tests covering both halves of the pipeline. The java-port
  tests share a single `AppState` via `OnceLock` so the ~13.7k-pattern
  corpus only compiles once per test run.
- `crates/skript-lsp/tests/skript_official_suite.rs` (new): 4 tests over
  the official Skript test corpus (573 `.sk` files, copied from
  `Skript/src/test/skript/{tests,junit}`). The corpus is embedded via
  `include_dir!`. Tests: no-panic over the whole corpus, AST baseline
  diagnostic counts, ≥90% parse rate for `syntaxes/`, no-crash for
  `regressions/`.

## 3. What remains

### 3.1 Delete `crates/skript-core/src/pattern.rs` + `matching.rs` — **blocked**

The old regex-approximation pattern engine
(`skript-core::pattern` + `skript-core::matching`) is now redundant: the
java-port engine (`skript-patterns` + `skript-registry`) is the source of
truth for pattern matching, and the AST validator
(`skript-core::validation`) doesn't use the pattern index for
diagnostics.

**Why blocked:** the `Registry` type in `skript-registry` is `!Send`
(its pattern elements are `Rc<RefCell<…>>`-based, frozen API). The
`SyntaxIndex` and `EntryTable` types in `skript-core::matching` are
`Send + Sync` and are used by `AppState` to serve completions and hover
info from async tower-lsp handlers. Until either:

- the completion/hover paths are rewritten to query the java-port
  `Registry` through the dedicated engine thread (the same channel
  pattern `java_check` already uses), or
- `skript-patterns`' API is changed to make its elements `Send` (a much
  larger undertaking),

…the `Send`-safe `EntryTable` needs to stay. The old `pattern.rs`
(regex-approximation DSL parser) could in principle be deleted today,
but its only caller is `matching.rs` (which `EntryTable` depends on),
so both have to go together.

**Recommended next step:** port the completion and hover paths to the
java-port engine thread. The pattern is established by
`AppState::java_check`; a similar `java_complete(prefix, ctx) -> Vec<CompletionItem>`
and `java_hover(line, col) -> Option<HoverInfo>` over the same channel
would unblock the deletion.

### 3.2 Keep `EntryTable` (it is `Send`-safe)

`skript-core::matching::EntryTable` is the lightweight lookup table
built from the deserialized SkriptHub entries. It is plain data
(`Vec<SyntaxEntry>`, no `Rc`, no `RefCell`) and trivially `Send + Sync`.
**It should be kept** even after the deletion in §3.1: the LSP layer
uses it for completions and hover, and there is no benefit to routing
those through the java-port engine thread (the entry metadata is the
same on both sides; only the *pattern-matching* differs).

In other words: the end state is
- `skript-core::matching::EntryTable` — **kept** (data only);
- `skript-core::matching::SyntaxIndex` — **deleted** (regex-approximation
  matcher, no longer needed);
- `skript-core::pattern` — **deleted** (regex-approximation DSL parser).

### 3.3 Out of scope

- **`skript-patterns` API rework.** The `!Send` `Rc<RefCell<…>>`-based
  pattern elements are a faithful port of the Java API. Making them
  `Send` would require either `Arc<Mutex<…>>` everywhere (perf hit) or a
  fundamentally different ownership model. Not planned.
- **`CheckConfig` versioning.** `skript-checks::CheckConfig` has
  `skript_version` and `minecraft_version` fields that the LSP layer
  always sets to `None`. Plumbing those through `initializationOptions`
  is a separate feature.

## 4. Verification

`cargo test --workspace` post-refactor: **all tests pass**, no
regressions. The diagnostic-merge tests
(`crates/skript-lsp/tests/diagnostic_specs.rs`) and the official-corpus
suite (`crates/skript-lsp/tests/skript_official_suite.rs`) are the
contract that prevents the merge from silently regressing.
