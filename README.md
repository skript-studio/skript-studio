# skript-studio

A [Language Server Protocol] 3.17 implementation for the [Skript] scripting
language (Minecraft server scripting). skript-studio ships as a standalone
LSP server binary plus a [VSCode extension](extensions/vscode-skript/) that
hosts it.

Skript syntax data is sourced live from the [SkriptHub] API and embedded
into the server binary at build time, so the server starts instantly with
no network calls.

[Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
[Skript]: https://github.com/SkriptLang/Skript
[SkriptHub]: https://skripthub.net/

## Architecture

This is a Cargo workspace with five crates:

| Crate | Type | Responsibility |
|-------|------|----------------|
| [`skript-core`](crates/skript-core) | lib | Pattern parsing, entry table, completion, hover, validation, semantic tokens. Pure logic, no I/O. |
| [`skript-patterns`](crates/skript-patterns) | lib | Faithful Rust port of Skript's pattern-matching engine. The single source of truth for pattern matching. |
| [`skript-registry`](crates/skript-registry) | lib | Loads the SkriptHub corpus into a compiled pattern registry. |
| [`skript-checks`](crates/skript-checks) | lib | Line-based static diagnostic rules (`unparseable`, `addon-not-enabled`, `plugin-missing`, `ambiguous-addon`). |
| [`skript-lsp`](crates/skript-lsp) | bin | The LSP server binary: stdio + WebSocket transport, `tower-lsp` wiring, embedded SkriptHub cache, lifecycle. |

Plus one TypeScript subdirectory:

- [`extensions/vscode-skript`](extensions/vscode-skript/) — VSCode extension that launches the LSP server in `--stdio` mode and provides the `skript` language configuration + TextMate grammar.

## Diagnostics

The server merges two complementary diagnostic streams for maximum Skript
parity. **Both always run** — there is no engine selection; the two
streams are unconditional and their results are concatenated:

1. **AST validator** (`skript-core`): `parse-error`, `unused-variable`,
   `semantic`. Language-level checks that don't depend on the pattern
   engine. The AST parser is deliberately partial (it powers folding,
   symbols, completion); the corpus line-checker is the authority for
   "is this valid Skript", so `parse-error` is demoted to **Hint** by
   default — set `parser_diagnostics = "error"` (or `"off"`) in
   `.skriptrc.toml` to change that.
2. **Pattern engine line checker** (`skript-checks` over `skript-registry`
   + `skript-patterns`, on a dedicated thread): `unparseable`,
   `addon-not-enabled`, `plugin-missing`, `ambiguous-addon`.

   Line-checker semantics (tuned against the official SkriptLang example
   scripts + real-world production scripts — see
   `crates/skript-checks/tests/fixtures_corpus.rs`):
   - `plugin-missing` fires only when **every** matching entry requires a
         missing plugin — if any match (usually base Skript) is available
         as-is, the line is fine.
   - `ambiguous-addon` (Info) fires only when ≥ 2 addons match and the
         base **Skript** plugin is **not** among them (addon overlap on
         `send`/`set`/`broadcast` is the ecosystem's normal state).
   - Command structure directives (`trigger:`, `permission:`,
     `executable by:`, …), `options:` block entries, `### ###`
     block comments, `using <feature>` directives and bare calls to
     functions declared in the same document are recognized and skipped
     — they carry no corpus syntax.

The two streams produce disjoint diagnostic codes and never duplicate.
The pattern engine is built unconditionally at startup (compiling ~13.7k
patterns takes a moment, so it runs on a background thread); until it
becomes ready only AST diagnostics are published, then all open documents
are re-validated. Hover for syntax docs also goes through the engine
thread (`hover_via_engine` → `match_line_for_hover`); AST-resolved hover
on local symbols takes priority when both are available.

## Transport

The server speaks two transports:

- **stdio** (`--stdio`) — standard LSP with `Content-Length` framing. This
  is the conventional transport used by VSCode and Neovim.
- **WebSocket** (default, `--port <N>`) — one JSON-RPC message per text or
  binary frame, with no `Content-Length` framing on the wire. A small
  framing adapter ([`ws.rs`](crates/skript-lsp/src/ws.rs)) translates
  between the message-oriented WebSocket and the byte-stream codec that
  `tower-lsp::Server` expects.

### Lifecycle (the sidecar contract — WebSocket mode)

```
Launcher                          skript-lsp
─────────                         ───────────
spawn binary ──────────────►  parses args, binds 127.0.0.1:{port}
                              prints `LISTENING {port}` to stdout ◄─────┐
reads `LISTENING {port}` ──────────────────────────────────────────────┘
opens WS to ws://127.0.0.1:{port}
LSP initialize / initialized ──► spawns background pattern-engine build
…                                (engine compiles; merged diagnostics ready)
LSP session runs (completions, hover, diagnostics, semantic tokens)
…
launcher kills process ◄──── on app exit (hard kill is safe)
```

**Stdout discipline:** the launcher parses stdout to discover the port, so
the *only* permitted stdout output is the single `LISTENING {port}` line.
All `tracing` logs go to **stderr**.

## Usage

### Build

```sh
cargo build --release -p skript-lsp
```

The binary is emitted at `target/release/skript-lsp[.exe]`.

### Run

```sh
# stdio mode (used by VSCode, Neovim, etc.)
skript-lsp --stdio

# Bind an ephemeral WebSocket port (launcher reads it from stdout)
skript-lsp --log-level info

# Bind a fixed WebSocket port
skript-lsp --port 9876
```

### CLI flags

| Flag | Default | Description |
|------|---------|-------------|
| `--stdio` | off | Use stdio transport (standard LSP). When set, `--port` is ignored. |
| `--port <N>` | `0` (ephemeral) | TCP port to listen on for WebSocket transport. `0` asks the OS for a free port. |
| `--log-level <LEVEL>` | `info` | One of `trace`, `debug`, `info`, `warn`, `error`. Overridable via `RUST_LOG`. Written to **stderr**. |

## VSCode extension

The VSCode extension at [`extensions/vscode-skript`](extensions/vscode-skript/)
hosts the LSP server as a child process in `--stdio` mode.

Package manager: **Bun**.

```sh
cd extensions/vscode-skript
bun install                              # install JS deps
bun run compile                          # JS output → out/
bun run build:binary                     # Windows x64 only: release binary → bin/skript-lsp-windows-x64.exe
bun run package                          # vsce package → skript-lsp.vsix
bun run install-ext                      # code --install-extension skript-lsp.vsix
bun run dev:install                      # Windows x64: build → package → install
bun run download-cache                   # refresh SkriptHub corpus → ../../crates/skript-registry/testdata/skripthub.json
```

Binary distribution is **Windows-only**. The vsix bundles exactly one server
binary, `bin/skript-lsp-windows-x64.exe`: CI's `binaries` job builds it from
the merge commit and the packaging job downloads it, while a Windows x64 dev
machine can rebuild it locally with `bun run build:binary`. It is a build
artifact (gitignored) and is never committed. Linux and macOS users provide
the server themselves via `PATH` or `skript-lsp.serverPath`, building it with
`cargo build --release -p skript-lsp`.

All SkriptHub corpus data is **committed to the repo** — a fresh clone
builds and tests with zero setup:

- `crates/skript-lsp/data/skripthub/` — per-addon JSON files, embedded via
  `include_dir!` in `state.rs`.
- `crates/skript-registry/testdata/skripthub.json` — the aggregate corpus
  (8712 entries), embedded via `include_bytes!` in `registry.rs`.

`bun run download-cache` re-projects the live SkriptHub API onto the
aggregate corpus (via `normalize-cache.mjs`); the resulting git diff is
reviewable, and data refreshes should be committed deliberately.

### VSCode configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `skript-lsp.serverPath` | `""` | Path to the `skript-lsp` binary. Leave empty to use the bundled binary, or set a custom path. |
| `skript-lsp.logLevel` | `info` | Log level for the LSP server (written to stderr). |
| `skript-lsp.maxCompletions` | `100` | Maximum number of completion items returned per request. |

## `.skriptrc.toml`

A per-project config file placed at the workspace root. Recognized keys:

```toml
# Addons the project uses, in addition to core Skript. Only these addon
# syntaxes are loaded; everything else is filtered out at startup.
addons = ["SkBee", "FaweSK"]

# How the AST parser's errors are surfaced: "error", "hint" (default) or
# "off". The parser is deliberately partial; the corpus line-checker is
# the validity authority, so hints are the default.
parser_diagnostics = "hint"
```

The file is optional; when absent the server loads only core Skript
entries.

## LSP capabilities advertised

The server implements LSP 3.17 with full document sync (`textDocumentSync`
kind = `Full`) plus 14 capabilities:

1. `completion` — Skript syntax completion (trigger char: space)
2. `hover` — syntax docs (via the pattern engine thread) + AST-resolved hover rendered as Markdown
3. `semanticTokens/full` — 9 token types (keyword, type, variable, string, comment, operator, event, function, number)
4. `publishDiagnostics` — pushed on open/change, when the entry table becomes ready, and when the pattern engine becomes ready (merged AST + line-check streams)
5. `documentSymbol` — top-level functions, commands, events, options blocks
6. `foldingRange` — block structure (functions, events, commands, if/else, loops)
7. `definition` — go-to-definition for variables and functions
8. `references` — find all references for a symbol under the cursor
9. `codeAction` — quick-fix scaffolding (placeholder for future rules)
10. `signatureHelp` — function call signatures (trigger chars: `(`, `,`)
11. `inlayHint` — type hints inferred from semantic analysis
12. `rename` — rename variables and functions across the document
13. `documentHighlight` — highlight occurrences of the symbol under the cursor
14. `selectionRange` — expand/shrink selection by syntactic structure

All features degrade gracefully before the SkriptHub cache loads and
before the pattern engine becomes ready (empty completions, `null` hover,
AST-only diagnostics).

## Development

```sh
# Type-check
cargo check

# Run unit + integration tests across the workspace (~614 tests)
cargo test --workspace

# Run a specific test suite
cargo test -p skript-lsp --test diagnostic_specs
cargo test -p skript-lsp --test skript_official_suite

# Benchmarks (criterion)
cargo bench -p skript-registry

# Parser fuzz harness
cargo test -p skript-core --test fuzz_parser

# End-to-end smoke test (requires a running server on :9876)
cargo run -p skript-lsp -- --port 9876 --log-level info &
cargo test -p skript-lsp --test smoke -- --ignored --nocapture
```

CI (`.github/workflows/ci.yml`) runs three jobs:

- **Rust** (`ubuntu-latest`) — `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check --workspace --all-targets`, and
  `cargo test --workspace --all-targets`.
- **Windows server binary** (`windows-latest`, after Rust) — builds
  `skript-lsp.exe` from the merge commit, stages it as
  `bin/skript-lsp-windows-x64.exe`, and uploads it as the
  `skript-lsp-windows-x64` artifact.
- **VSCode extension** (`ubuntu-latest`, after the binary job) —
  `bun install --frozen-lockfile`, `bun run typecheck`, `bun run compile`,
  downloads the Windows binary into `bin/`, runs `bun run package` (vsce
  under the bun runtime), verifies the vsix contains exactly one server
  binary, and uploads `skript-lsp.vsix` as a workflow artifact.

## SkriptHub corpus

The SkriptHub corpus is embedded into the server binary at build time
from git-tracked files (see [VSCode extension](#vscode-extension) for the
file list), so no network access or pre-build step is required.

To refresh the corpus from the live API:

```sh
cd extensions/vscode-skript
bun run download-cache    # → ../../crates/skript-registry/testdata/skripthub.json
```

The output is git-tracked; review the diff (`git diff
--stat crates/skript-registry/testdata/`) and commit deliberate data
refreshes. The per-addon files in `crates/skript-lsp/data/skripthub/` are
committed as-is and are not regenerated by this script.

## License

MIT.
