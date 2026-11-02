# AGENTS.md — skript-lsp workspace

## Repo structure

```
├── crates/
│   ├── skript-core/      ← lexer, parser, AST, semantics, hover, completion, entry table
│   ├── skript-patterns/  ← faithful Rust port of Skript's pattern-matching engine
│   ├── skript-registry/  ← SkriptHub corpus → compiled registry
│   ├── skript-checks/    ← line-based static diagnostic rules
│   └── skript-lsp/       ← LSP server binary
├── extensions/
│   └── vscode-skript/    ← the sole UI: VSCode extension hosting the LSP server
├── scripts/
└── test-fixtures/        ← .sk samples for manual testing
```

Single Cargo workspace of **5 Rust crates**, no submodules. Plain
`git clone` is sufficient.

## Crates at a glance

| Crate | Role |
|-------|------|
| `skript-core` | Lexer, parser, AST, semantic analysis, AST validator, completion, hover. `matching.rs` holds `EntryTable` only (the `Send`-safe syntax-entry lookup table; `SyntaxIndex` and `pattern.rs` were deleted — the regex-approximation engine is gone). |
| `skript-patterns` | Faithful Rust port of Skript's `ch.njol.skript.patterns` package. The **single** pattern engine; there is no second engine. |
| `skript-registry` | Loads the SkriptHub corpus into a compiled `Registry` of `SkriptPattern`s. |
| `skript-checks` | Line-based diagnostic rules (`unparseable`, `addon-not-enabled`, `plugin-missing`, `ambiguous-addon`) + `match_line_for_hover`, all on top of `skript-registry`/`skript-patterns`. |
| `skript-lsp` | `tower-lsp` server binary: stdio + WebSocket transport, 14 LSP capabilities, embedded SkriptHub cache, lifecycle, merged diagnostics. |

## Engine threading

The `!Send` `Registry` (pattern elements are `Rc<RefCell<…>>`-based, frozen
API) is built, queried, and dropped entirely on one dedicated OS thread
named `skript-pattern-engine`. The rest of the server only sees `Send`
handles: two request channels (`check_requests`, `hover_requests`) and
plain-data replies. Hover for syntax docs goes through that thread
(`AppState::hover_via_engine` → `skript_checks::match_line_for_hover`);
AST-resolved hover on local symbols takes priority when both produce a
result.

## Build & test

```bash
cargo build -p skript-lsp                # dev profile
cargo test --workspace                   # full suite (~614 tests, 2 ignored)
cargo test -p skript-lsp --test smoke -- --ignored --nocapture  # integration: needs server on ws://127.0.0.1:9876
```

## Benchmarks & fuzz

```bash
cargo bench -p skript-registry           # criterion benchmarks (corpus compile, refcell vs mutex)
cargo test -p skript-core --test fuzz_parser   # parser fuzz harness
```

## Extension (VSCode — the sole UI)

Package manager: **Bun** (`bun install`, `bun run`, `bunx`).

```bash
cd extensions/vscode-skript
bun install                              # install JS deps (vscode-languageclient, typescript, ...)
bun run compile                          # JS output → out/
bun run build:binary                     # release exe → bin/
bun run package                          # vsce package → skript-lsp.vsix
bun run install-ext                        # code --install-extension skript-lsp.vsix
bun run dev:install                      # all of the above: build → package → install
bun run prepublish                       # build:binary → compile (no install)
bun run download-cache                   # refresh SkriptHub corpus → ../../crates/skript-registry/testdata/skripthub.json
```

`extension.ts` starts the LSP with `--stdio --log-level`. No network calls
at startup — cache embedded via `include_dir!` in `state.rs`.

## `.skriptrc.toml`

Per-project config file (optional) placed at the workspace root.
Recognized keys:

```toml
# Addons the project uses, in addition to core Skript. Only these addon
# syntaxes are loaded; everything else is filtered out at startup.
addons = ["SkBee", "SkQuery"]
```

Loaded during LSP `initialize` (see `backend.rs`). When absent the server
loads only core Skript entries.

## Release build

`Cargo.toml` enables `strip = true`, `lto = true`, `codegen-units = 1` for size.
`skript-lsp` depends on `skript-core` with `default-features = false` (no `reqwest`, `tokio`, `thiserror`, `url`).

## Key constraints

- All SkriptHub corpus data is git-tracked: `crates/skript-lsp/data/skripthub/` (per-addon files, embedded via `include_dir!`) and `crates/skript-registry/testdata/skripthub.json` (aggregate corpus, embedded via `include_bytes!`). Fresh clones build with zero setup. `bun run download-cache` refreshes the aggregate corpus from the live API — commit data refreshes deliberately (the diff is large).
- `nul` in `.gitignore` works around a Windows reserved-name phantom entry that appears in every directory listing.
- LSP listens on `--port <n>` (WebSocket) or `--stdio`. The VSCode extension uses `--stdio`.
- `skript-patterns`, `skript-registry`, and `skript-checks` have FROZEN public APIs (see `PORTING_CONTRACTS.md`); the port is COMPLETE.
- Do not modify crates under `crates/skript-patterns/`, `crates/skript-registry/`, or `crates/skript-checks/` unless extending the FROZEN API deliberately.
