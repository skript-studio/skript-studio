# `.skriptrc.toml` Project Config Design

> **Status:** Approved design spec
> **Date:** 2026-06-27

**Goal:** Allow Skript projects to specify which addons they use via a `.skriptrc.toml` config file, so the LSP only loads syntax data for relevant addons.

**Architecture:** The LSP server discovers `.skriptrc.toml` from the workspace root during `initialize`, reads the addon list, and filters entries before building the syntax index. The monolithic SkriptHub cache is split into per-addon JSON files at download time and embedded as a directory tree.

**Tech Stack:** Rust (`toml` 0.8, `include_dir` 0.7), Node.js (`normalize-cache.mjs` updated to split files), TOML (config format).

---

## Behavior Matrix

| Scenario                                        | Loaded addon data                                   |
| ----------------------------------------------- | --------------------------------------------------- |
| No `.skriptrc.toml` exists                      | `Skript.json` only (core syntax)                    |
| `.skriptrc.toml` exists, `addons` missing/empty | `Skript.json` only                                  |
| `addons = ["SkBee", "TownyAdvanced"]`           | `Skript.json` + `SkBee.json` + `TownyAdvanced.json` |

`Skript.json` is **always** loaded — it is the core language and does not require
opt-in. The config only controls which additional addons are enabled.

---

## File Structure

### `.skriptrc.toml` (workspace root)

```toml
addons = ["SkBee", "Skript", "TownyAdvanced"]
```

`addons` is optional. When present, entries named "Skript" in the array are
harmless duplicates — the server always includes Skript data regardless.

### Per-addon cache files (`crates/skript-lsp/data/skripthub/`)

Created by the updated download/normalize script:

```text
data/skripthub/
├── Skript.json              ← entries whose addon.name == "Skript"
├── SkBee.json
├── SkQuery.json
├── TownyAdvanced.json
└── ... (one JSON array file per addon)
```

Each file is a small `SyntaxEntry` JSON array (serde-deserializable to
`Vec<SyntaxEntry>`). The monolithic `skripthub-cache.json` is replaced by
this directory at build time.

---

## Component Changes

### 1. `normalize-cache.mjs` → split by addon

**File:** `extensions/vscode-skript/normalize-cache.mjs`

**Change:** Instead of writing one `skripthub-cache.json`, group entries by
`addon.name` and write one file per addon into
`../../crates/skript-lsp/data/skripthub/`.

Output:

- `data/skripthub/Skript.json`
- `data/skripthub/SkBee.json`
- etc. (one file per addon)

**Note:** The `data/skripthub/` directory is gitignored (regenerable via
`download-cache`), same as the old monolithic file was.

### 2. Add dependencies to `Cargo.toml`

**File:** `Cargo.toml` (workspace root)

Add to `[workspace.dependencies]`:

```toml
toml = "0.8"
include_dir = "0.7"
```

**File:** `crates/skript-lsp/Cargo.toml`

Add to `[dependencies]`:

```toml
toml = { workspace = true }
include_dir = { workspace = true }
```

### 3. New `SkriptRcToml` struct

**File:** `crates/skript-lsp/src/options.rs`

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkriptRcToml {
    #[serde(default)]
    pub addons: Vec<String>,
}
```

### 4. `EffectiveConfig` gains `extra_addons`

**File:** `crates/skript-lsp/src/options.rs`

```rust
pub struct EffectiveConfig {
    pub max_completions: usize,
    /// Addon names in addition to "Skript" to load syntax for.
    /// Empty = only load core Skript entries.
    pub extra_addons: Vec<String>,
}
```

Default: `Vec::new()` (backward compatible — new servers only load Skript
by default, which is the explicit desired behavior).

### 5. `AppState` embeds directory instead of single file

**File:** `crates/skript-lsp/src/state.rs`

**Replace:**

```rust
const EMBEDDED_CACHE: &[u8] = include_bytes!("../data/skripthub-cache.json");
```

**With:**

```rust
use include_dir::{include_dir, Dir};
static CACHE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/data/skripthub");
```

### 6. `refresh()` loads only selected addon files

**File:** `crates/skript-lsp/src/state.rs`

```rust
pub async fn refresh(&self) {
    let _guard = self.inner.refresh_lock.lock().await;
    let extra = self.config().extra_addons.clone();

    let mut all_entries = Vec::new();
    for file in CACHE_DIR.files() {
        let name = file.path().file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if name != "Skript" && !extra.contains(&name.to_owned()) {
            continue;
        }
        match serde_json::from_slice::<Vec<SyntaxEntry>>(file.contents()) {
            Ok(batch) => all_entries.extend(batch),
            Err(e) => tracing::error!(addon = %name, error = %e, "failed to parse addon file"),
        }
    }

    self.install(all_entries);
}
```

### 7. `initialize` reads `.skriptrc.toml`

**File:** `crates/skript-lsp/src/backend.rs`

In `Backend::initialize()`, after processing `initializationOptions`:

```rust
// Parse .skriptrc.toml from workspace root
if let Some(root_uri) = &params.root_uri {
    if let Ok(root_path) = root_uri.to_file_path() {
        let rc_path = root_path.join(".skriptrc.toml");
        if rc_path.exists() {
            match std::fs::read_to_string(&rc_path) {
                Ok(content) => {
                    if let Ok(rc) = toml::from_str::<SkriptRcToml>(&content) {
                        let mut cfg = self.state.config();
                        let mut extra: Vec<String> = rc.addons;
                        extra.dedup();
                        cfg.extra_addons = extra;
                        self.state.set_config(cfg);
                    }
                }
                Err(e) => tracing::warn!(path = %rc_path.display(), error = %e, "failed to read .skriptrc.toml"),
            }
        }
    }
}
```

Note: Uses `std::fs::read_to_string` (sync) since `initialize` is already
handling I/O and the file read is a one-time startup operation.

### 8. `install()` signature unchanged

**File:** `crates/skript-lsp/src/state.rs`

`install()` still takes `Vec<SyntaxEntry>` — the filtering happens before
the call. No signature change needed.

---

## Error Handling

| Scenario                             | Behavior                                            |
| ------------------------------------ | --------------------------------------------------- |
| `.skriptrc.toml` has invalid TOML    | Ignored (warn log), Skript-only loaded              |
| An addon JSON file is malformed      | Warn log, that addon's entries are skipped          |
| `CACHE_DIR` has no files             | Server starts with empty index (warn log)           |
| `.skriptrc.toml` lists unknown addon | Silently ignored — no matching file = no entries    |
| `Skript.json` is missing             | No core entries loaded (server degrades gracefully) |

No user-facing diagnostics for config errors in v1 — warnings go to stderr/logs
only.

---

## Testing

### Unit tests

1. **`SkriptRcToml` deserialization:**
   - Empty input → `addons: []`
   - `addons = ["SkBee", "SkQuery"]` → `["SkBee", "SkQuery"]`
   - Missing `addons` key → `[]`

2. **`extra_addons` dedup:**
   - `addons = ["SkBee", "SkBee"]` → `extra_addons = ["SkBee"]`

3. **`refresh()` filtering:**
   - Mock `CACHE_DIR` with 3 files (`Skript.json`, `SkBee.json`, `SkQuery.json`)
   - `extra_addons = ["SkBee"]` → loads Skript + SkBee entries
   - `extra_addons = []` → loads only Skript entries

4. **Existing `state.rs` tests** continue to pass (they use `install()` directly
   with filtered entries, not `refresh()`).

### Integration

- Create a temp workspace with `.skriptrc.toml`, open a `.sk` file, verify
  completions don't include addon that wasn't listed.

---

## Files Changed Summary

| File                                           | Change                                                |
| ---------------------------------------------- | ----------------------------------------------------- |
| `extensions/vscode-skript/normalize-cache.mjs` | Split output into per-addon files                     |
| `Cargo.toml` (workspace)                       | Add `toml`, `include_dir` deps                        |
| `crates/skript-lsp/Cargo.toml`                 | Reference new workspace deps                          |
| `crates/skript-lsp/src/options.rs`             | Add `SkriptRcToml` struct, `extra_addons` field       |
| `crates/skript-lsp/src/state.rs`               | Embed directory, filter in `refresh()`                |
| `crates/skript-lsp/src/backend.rs`             | Read `.skriptrc.toml` in `initialize()`               |
| `crates/skript-lsp/data/skripthub-cache.json`  | **Deleted** (replaced by `data/skripthub/` directory) |

## What's NOT in v1

- Watching `.skriptrc.toml` for hot-reload (server restart required)
- Diagnostic warnings for unknown addon names
- Schema validation on `.skriptrc.toml`
- Extension-side `.skriptrc.toml` reading (server reads it directly)
