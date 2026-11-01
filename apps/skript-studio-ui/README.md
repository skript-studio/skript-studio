# Skript Studio

A standalone desktop IDE for editing Minecraft Skript scripts. Built with Tauri 2, React 19, and the Monaco editor — connects to a Rust LSP sidecar over WebSocket for completions, hover, and validation.

## Architecture

```
skript-studio/  (this repo — TypeScript/React/Rust)
├── src/                ← React frontend (Vite + Monaco + Shadcn/UI)
└── src-tauri/          ← Rust backend (Tauri 2 shell + LSP sidecar manager)
```

Skript Studio is one of three repositories under `github.com/skript-studio`:

| Repo | Language | Purpose |
|------|----------|---------|
| `skript-core` | Rust | Library: pattern parsing, matching, completions, hover, SkriptHub API client |
| `skript-lsp` | Rust | LSP server binary (`tower-lsp` + `skript-core`), WebSocket transport |
| `skript-studio` | TS/Rust | **This repo.** Tauri 2 desktop app shell |

### Communication flow

```
Monaco Editor (React)
  ↕ WebSocket (monaco-languageclient + vscode-ws-jsonrpc)
skript-lsp sidecar (127.0.0.1:<port>, spawned by Tauri)
  ↕ uses skript-core
  ↕ fetches https://skripthub.net/api/v1/addonsyntaxlist/ at startup
```

## Tech stack

- **Runtime:** Tauri 2 (Rust backend + WebView frontend)
- **JS runtime:** Bun
- **Framework:** React 19 + TypeScript
- **Editor:** Monaco via `@monaco-editor/react`
- **LSP client:** `monaco-languageclient` + `vscode-ws-jsonrpc`
- **UI components:** Shadcn/UI (Radix primitives + Tailwind)
- **Icons:** `lucide-react`
- **State:** `zustand`
- **Layout:** `react-resizable-panels`
- **Bundler:** Vite (with `vite-plugin-monaco-editor` for web workers)

## Project structure

```
skript-studio/
├── src-tauri/                  # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json         # App config, sidecar registration, CSP
│   ├── capabilities/default.json  # Tauri 2 permission caps
│   ├── icons/                  # App icons (.ico, .png, .icns)
│   └── src/
│       ├── main.rs             # Entry point
│       ├── lib.rs              # Tauri builder + command registration
│       ├── commands/
│       │   ├── mod.rs          # Docs + settings commands
│       │   ├── fs.rs           # File I/O commands
│       │   └── lsp.rs          # LSP lifecycle commands
│       └── lsp/
│           ├── mod.rs
│           └── manager.rs      # Sidecar spawn, port discovery, restart
│
├── src/                        # React frontend
│   ├── main.tsx
│   ├── App.tsx
│   ├── index.css               # Tailwind + CSS variables (light/dark)
│   │
│   ├── components/
│   │   ├── ui/                 # Shadcn primitives (button, dialog, …, tree-view)
│   │   ├── editor/             # MonacoEditor, EditorTabs, EditorBreadcrumb, OutputPanel, StatusBar, WelcomeOverlay
│   │   ├── explorer/           # FileExplorer, NewFileDialog
│   │   ├── docs/               # DocsPanel, DocsSearch, DocsEntry, DocsCategory
│   │   ├── settings/           # SettingsDialog, AppearanceSettings, LSPSettings
│   │   ├── menu/               # MenuBar
│   │   └── layout/             # AppLayout (react-resizable-panels)
│   │
│   ├── stores/                 # Zustand stores
│   │   ├── editor-store.ts
│   │   ├── lsp-store.ts
│   │   ├── settings-store.ts
│   │   └── docs-store.ts
│   │
│   ├── hooks/
│   │   ├── use-lsp.ts          # monaco-languageclient WebSocket bridge
│   │   ├── use-tauri-fs.ts     # Tauri FS command wrappers
│   │   ├── use-tauri-lsp.ts    # Tauri LSP command wrappers
│   │   └── use-skript-theme.ts # Theme sync (Shadcn + Monaco)
│   │
│   ├── lib/
│   │   ├── monaco-skript.ts    # Custom "skript" language + Monarch tokenizer
│   │   ├── monaco-theme.ts     # Dark + light themes matching IDE shell
│   │   ├── tauri-commands.ts   # Type-safe invoke() wrappers
│   │   └── utils.ts
│   │
│   └── types/
│       ├── skript.ts           # SkriptHub entry types (mirrors Rust SyntaxEntry)
│       ├── lsp.ts              # LSP lifecycle types
│       └── settings.ts         # Settings schema + defaults
│
├── scripts/generate-icons.py   # Generate placeholder app icons
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
├── tailwind.config.js
├── postcss.config.js
├── components.json             # Shadcn/UI config
└── .gitmodules                 # Points to skript-lsp submodule
```

## Getting started

### Prerequisites

- [Bun](https://bun.sh/) ≥ 1.3
- [Rust](https://rustup.rs/) (stable, for building the Tauri shell)
- Tauri 2 prerequisites — see the [Tauri 2 prerequisites guide](https://v2.tauri.app/start/prerequisites/)

### Install dependencies

```bash
bun install
```

### Run in development (frontend only)

```bash
bun run dev
# → http://localhost:1420
```

The app will boot in "browser dev" mode: Tauri-specific commands (filesystem, LSP) are detected as unavailable and the editor falls back to the local Monarch tokenizer. The SkriptHub docs panel fetches directly from `https://skripthub.net`.

### Run as a Tauri desktop app

```bash
# Clone the LSP sidecar as a submodule
git submodule update --init --recursive

# Build the LSP binary and copy it to src-tauri/binaries/
# (the build location depends on the skript-lsp repo's CI artifacts)

# Run the full desktop app
bun tauri dev
```

### Production build

```bash
bun tauri build
```

## Tauri backend commands

The Rust backend exposes these commands via `@tauri-apps/api/core`'s `invoke()`:

### File operations (`src-tauri/src/commands/fs.rs`)

| Command | Args | Returns | Notes |
|---------|------|---------|-------|
| `open_folder` | — | `string \| null` | Native folder picker |
| `list_dir` | `path` | `DirEntry[]` | Sorted: directories first |
| `read_file` | `path` | `string` | UTF-8 only |
| `write_file` | `path, content` | `void` | Creates parent dirs |
| `create_file` | `path` | `void` | Refuses to overwrite |
| `delete_file` | `path` | `void` | Refuses directories |
| `rename_file` | `old_path, new_path` | `void` | Refuses to overwrite |

### LSP operations (`src-tauri/src/commands/lsp.rs`)

| Command | Returns | Notes |
|---------|---------|-------|
| `start_lsp` | `{ port: number }` | Spawns sidecar, waits for `LISTENING <port>` line |
| `stop_lsp` | `void` | Kills sidecar process |
| `restart_lsp` | `{ port: number }` | Stop + start |
| `get_lsp_status` | `{ status, port?, error? }` | Polled by frontend every 5s |

### Docs & settings (`src-tauri/src/commands/mod.rs`)

| Command | Returns | Notes |
|---------|---------|-------|
| `get_skript_docs` | `SkriptHubEntry[]` | Reads LSP's on-disk cache, falls back to direct HTTP |
| `read_settings` | `string \| null` | Reads `settings.json` from app-data dir |
| `write_settings` | `void` | Persists `settings.json` |

## Monaco + LSP integration

The frontend uses `monaco-languageclient` + `vscode-ws-jsonrpc` to bridge Monaco and the LSP sidecar's WebSocket. See `src/hooks/use-lsp.ts` for the full connection logic.

The Tauri webview CSP (in `tauri.conf.json`) explicitly allows:
- `connect-src 'self' https://skripthub.net http://127.0.0.1:* ws://127.0.0.1:* https://cdn.jsdelivr.net` — for the LSP socket and the Monaco CDN
- `worker-src 'self' blob:` — for Monaco's web workers (bundled as blob URLs via `vite-plugin-monaco-editor`)
- `script-src 'self' 'unsafe-eval' 'unsafe-inline'` — Monaco requires `unsafe-eval` for its AMD loader

## Skript language support

The custom `skript` Monaco language (see `src/lib/monaco-skript.ts`) provides:

- Tokenizer for keywords (`on`, `if`, `set`, `send`, …), types (`player`, `number`, …), operators (`is`, `contains`, `between`, …)
- Variables: global `{x}` and local `{_x}`
- Color codes: `&a`, `&c`, `&l`, …
- String literals (single and double quote)
- Comments (`#`)
- Auto-closing pairs, bracket matching, indentation rules

Token types are aligned with what the LSP returns as semantic tokens, so the local tokenizer and the LSP produce a consistent look.

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd + S` | Save active file |
| `Ctrl/Cmd + B` | Toggle Explorer panel |
| `Ctrl/Cmd + ,` | Open Settings |
| `Ctrl/Cmd + O` | Open folder |
| `Ctrl/Cmd + N` | New file |
| `Ctrl/Cmd + Z` / `Shift + Cmd + Z` | Undo / Redo |
| `Ctrl/Cmd + F` | Find |
| `Alt + Cmd + F` | Replace |
| Middle-click on tab | Close tab |

## Theming

Three theme modes are supported:
- **Dark** (default) — VS Code dark+ inspired palette
- **Light** — VS Code light inspired palette
- **System** — follows OS color scheme via `prefers-color-scheme`

Theme is persisted in the settings store and written to disk via Tauri's app-data directory. Both Shadcn (via `dark` class on `<html>`) and Monaco (via custom themes `skript-studio-dark` / `skript-studio-light`) stay in sync.

## Generating app icons

Placeholder icons live in `src-tauri/icons/` and were generated by:

```bash
python3 scripts/generate-icons.py
```

Replace them with a properly designed icon set before release.

## What's NOT included (by design)

- The LSP server itself — see `github.com/skript-studio/skript-lsp`
- Cross-file variable tracking / go-to-definition
- Plugin marketplace
- Script execution / live preview
- These are deferred to future versions per the implementation brief.

## License

MIT
