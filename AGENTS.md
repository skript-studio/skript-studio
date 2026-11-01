# AGENTS.md — skript-lsp workspace (monorepo)

## Repo structure

```
├── crates/
│   ├── skript-core/      ← in-tree (was github.com/skript-studio/skript-core)
│   ├── skript-lsp/       ← LSP server binary
│   ├── skript-patterns/  ← faithful Java pattern-engine port
│   ├── skript-registry/  ← SkriptHub corpus → compiled registry
│   └── skript-checks/    ← static diagnostic rules
├── extensions/
│   └── vscode-skript/    ← in-tree (was github.com/skript-studio/skript-studio-vscode)
├── scripts/
└── test-fixtures/        ← .sk samples for manual testing
```

Single Cargo workspace, no submodules. Plain `git clone` is sufficient.

## Build & test

```bash
cargo build -p skript-lsp                # dev profile
cargo test -p skript-core                 # core lib tests (345 pass, 0 failures)
cargo test -p skript-lsp --test smoke -- --ignored --nocapture  # integration: needs server on ws://127.0.0.1:9876
```

All `skript-core` tests pass (345 unit + 1 integration). Historical: the `escape_debug()` regex escaping bug was fixed in commit `2ddee04`; the previously-failing `partial_prefix_matches`, `normalize_parses_stringified_entries_for_sections`, and `deserializes_unknown_syntax_type_to_other` were fixed in commit `35737b0` (when skript-core was a submodule; now in-tree).

## Extension (VSCode)

Package manager: **Bun** (`bun install`, `bun run`, `bunx`).

```bash
cd extensions/vscode-skript
bun install                              # install JS deps (vscode-languageclient, typescript, ...)
bun run compile                          # JS output → out/
bun run build:binary                     # release exe → bin/
bun run package                          # vsce package → skript-studio-vscode.vsix
bun run install                          # code --install-extension skript-studio-vscode.vsix
bun run dev:install                      # all of the above: build → package → install
bun run prepublish                       # build:binary → compile → package (no install)
bun run download-cache                   # fetch SkriptHub data → ../../crates/skript-lsp/data/skripthub/
```

`extension.ts` starts the LSP with `--stdio --log-level`. No network calls at startup — cache embedded via `include_dir!` in `state.rs`.

## Release build

`Cargo.toml` enables `strip = true`, `lto = true`, `codegen-units = 1` for size.  
`skript-lsp` depends on `skript-core` with `default-features = false` (no `reqwest`, `tokio`, `thiserror`, `url`).

## Standalone desktop app (retired)

The Tauri-based standalone desktop IDE (`apps/skript-studio-ui/`) was retired in favor of the VSCode extension as the mainstream integration. Its history is preserved in the git log (subtree-add commit `a6b94fb` + retirement commit); restore with `git checkout <pre-retirement-sha> -- apps/skript-studio-ui/` and re-add `"apps/skript-studio-ui/src-tauri"` to workspace `members`.

## Key constraints

- `crates/skript-lsp/data/skripthub/` (per-addon cache files split by `extensions/vscode-skript/normalize-cache.mjs`) is gitignored and regenerable via `bun run download-cache`. `cargo build` fails if the directory is missing (embedded via `include_dir!`) — run that script first.
- `nul` in `.gitignore` works around a Windows reserved-name phantom entry that appears in every directory listing.
- LSP listens on `--port <n>` (WebSocket) or `--stdio`. The VSCode extension uses `--stdio`.
