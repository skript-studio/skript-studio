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
| [`skript-core`](crates/skript-core) | lib | Pattern parsing, syntax index, completion, hover, validation, semantic tokens. Pure logic, no I/O. |
| [`skript-patterns`](crates/skript-patterns) | lib | Faithful Rust port of Skript's Java pattern-matching engine. |
| [`skript-registry`](crates/skript-registry) | lib | Loads the SkriptHub corpus into a compiled pattern registry. |
| [`skript-checks`](crates/skript-checks) | lib | Line-based static diagnostic rules (`unparseable`, `addon-not-enabled`, `plugin-missing`, `ambiguous-addon`). |
| [`skript-lsp`](crates/skript-lsp) | bin | The LSP server binary: stdio + WebSocket transport, `tower-lsp` wiring, embedded SkriptHub cache, lifecycle. |

Plus one TypeScript subdirectory:

- [`extensions/vscode-skript`](extensions/vscode-skript/) — VSCode extension that launches the LSP server in `--stdio` mode and provides the `skript` language configuration + TextMate grammar.

## Diagnostics

The server merges two complementary diagnostic streams for maximum Skript
parity:

1. **AST validator** (`skript-core`): `parse-error`, `unused-variable`,
   `semantic`. These are the language-level checks that don't depend on
   the pattern engine.
2. **Java-port line checker** (`skript-checks` over `skript-registry` +
   `skript-patterns`, on a dedicated thread): `unparseable`,
   `addon-not-enabled`, `plugin-missing`, `ambiguous-addon`.

The two streams produce disjoint diagnostic codes and never duplicate.
The pattern engine is built unconditionally at startup (compiling ~13.7k
patterns takes a moment, so it runs on a background thread); until it
becomes ready only AST diagnostics are published, then all open documents
are re-validated.

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
bun run build:binary                     # release exe → bin/  (cargo build --release -p skript-lsp)
bun run package                          # vsce package → skript-studio-vscode.vsix
bun run install                          # code --install-extension skript-studio-vscode.vsix
bun run dev:install                      # all of the above: build → package → install
bun run download-cache                   # fetch SkriptHub data → ../../crates/skript-lsp/data/skripthub/
```

The cache files in `crates/skript-lsp/data/skripthub/` (per-addon JSON
files split by `extensions/vscode-skript/normalize-cache.mjs`) are
gitignored and regenerable via `bun run download-cache`. `cargo build`
fails if the directory is missing (embedded via `include_dir!`) — run that
script first.

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
addons = ["SkBee", "SkQuery"]
```

The file is optional; when absent the server loads only core Skript
entries.

## LSP capabilities advertised

The server implements LSP 3.17 with the following capabilities:

1. `textDocumentSync` — full document sync
2. `completion` — Skript syntax completion (trigger char: space)
3. `hover` — syntax docs + AST-resolved hover rendered as Markdown
4. `semanticTokens/full` — 9 token types (keyword, type, variable, string, comment, operator, event, function, number)
5. `publishDiagnostics` — pushed on open/change, when the index becomes ready, and when the pattern engine becomes ready
6. `documentSymbol` — top-level functions, commands, events, options blocks
7. `foldingRange` — block structure (functions, events, commands, if/else, loops)
8. `definition` — go-to-definition for variables and functions
9. `references` — find all references for a symbol under the cursor
10. `codeAction` — quick-fix scaffolding (placeholder for future rules)
11. `signatureHelp` — function call signatures (trigger chars: `(`, `,`)
12. `inlayHint` — type hints inferred from semantic analysis

All features degrade gracefully before the SkriptHub cache loads and
before the pattern engine becomes ready (empty completions, `null` hover,
AST-only diagnostics).

## Development

```sh
# Type-check
cargo check

# Run unit + integration tests across the workspace
cargo test

# Run a specific test suite
cargo test -p skript-lsp --test diagnostic_specs
cargo test -p skript-lsp --test skript_official_suite

# End-to-end smoke test (requires a running server on :9876)
cargo run -p skript-lsp -- --port 9876 --log-level info &
cargo test -p skript-lsp --test smoke -- --ignored --nocapture
```

## SkriptHub cache

The SkriptHub cache (`crates/skript-lsp/data/skripthub/`) is embedded into
the server binary at build time via `include_dir!`. The directory is
gitignored and must be regenerated locally:

```sh
cd extensions/vscode-skript
bun run download-cache    # → ../../crates/skript-lsp/data/skripthub/
```

`cargo build` will fail with a clear error if the directory is missing.

## License

MIT.
