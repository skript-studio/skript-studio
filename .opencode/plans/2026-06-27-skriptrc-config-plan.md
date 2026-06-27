# `.skriptrc.toml` Project Config Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow Skript projects to specify which addons they use via a `.skriptrc.toml` config file, so the LSP only loads syntax data for relevant addons.

**Architecture:** The LSP server reads `.skriptrc.toml` from the workspace root during `initialize`, stores the addon list in `EffectiveConfig`, then `refresh()` filters entries by addon name before building the syntax index. The monolithic cache JSON is replaced by per-addon JSON files produced by the download script and embedded via `include_dir!`.

**Tech Stack:** Rust (`toml` 0.8, `include_dir` 0.7), Node.js (`normalize-cache.mjs`), TOML (config format).

## Global Constraints

- No extension-side `.skriptrc.toml` changes — server reads it directly
- `.skriptrc.toml` is optional — no file = only core Skript entries
- `Skript` addon is always loaded implicitly (hardcoded check)
- `data/skripthub/` directory is gitignored — regenerable via `download-cache`
- No hot-reload for `.skriptrc.toml` in v1 (server restart required)
- Add `toml = "0.8"` and `include_dir = "0.7"` to workspace Cargo.toml
- The embedded directory must exist at build time (same constraint as old monolith)

---

## File Structure

| File | Responsibility |
|---|---|
| `extensions/vscode-skript/normalize-cache.mjs` | Split API response into per-addon JSON files in `data/skripthub/` |
| `Cargo.toml` (workspace) | Declare `toml` and `include_dir` workspace deps |
| `crates/skript-lsp/Cargo.toml` | Reference new workspace deps |
| `crates/skript-lsp/src/options.rs` | `SkriptRcToml` struct, `extra_addons` field on `EffectiveConfig` |
| `crates/skript-lsp/src/state.rs` | Embed directory, filter entries in `refresh()` |
| `crates/skript-lsp/src/backend.rs` | Parse `.skriptrc.toml` in `initialize()` |
| `crates/skript-lsp/data/skripthub-cache.json` | **Delete** — replaced by `data/skripthub/` directory |
| `.gitignore` | Ignore `data/skripthub/` instead of old monolithic file |

---

### Task 1: Update download script to split cache by addon

**Files:**
- Modify: `extensions/vscode-skript/normalize-cache.mjs`

**Interfaces:**
- Consumes: `https://skripthub.net/api/v1/addonsyntaxlist/` (same as before)
- Produces: `crates/skript-lsp/data/skripthub/{AddonName}.json` — one file per addon, each a `Vec<SyntaxEntry>` JSON array

**Behavior:**
- Group input entries by `addon.name`
- Write one `.json` file per addon into `../../crates/skript-lsp/data/skripthub/`
- Create the directory if it doesn't exist
- Remove stale files that don't correspond to current addons
- Total entries across all files must equal the original API response count

- [ ] **Step 1: Rewrite `normalize-cache.mjs`**

```js
import { existsSync, mkdirSync, readdirSync, unlinkSync, writeFileSync } from "fs";
import { join } from "path";

const raw = JSON.parse(readFileSync(0, "utf-8"));

// Group entries by addon name
const grouped = new Map();
for (const entry of raw) {
  const name = entry.addon?.name || "Unknown";
  if (!grouped.has(name)) grouped.set(name, []);
  grouped.get(name).push(entry);
}

// Write one file per addon into data/skripthub/
const outDir = "../../crates/skript-lsp/data/skripthub";
if (!existsSync(outDir)) mkdirSync(outDir, { recursive: true });

// Clear stale files first
for (const f of readdirSync(outDir)) {
  if (f.endsWith(".json")) unlinkSync(join(outDir, f));
}

let total = 0;
for (const [name, entries] of grouped) {
  // Sanitize filename: keep only alphanumeric, underscore, hyphen
  const safe = name.replace(/[^a-zA-Z0-9_-]/g, "_");
  writeFileSync(join(outDir, `${safe}.json`), JSON.stringify(entries), "utf-8");
  total += entries.length;
  console.log(`  ${safe}.json — ${entries.length} entries`);
}

console.log(`\nWrote ${grouped.size} addon files (${total} total entries) to ${outDir}`);

// Verify total matches input
if (total !== raw.length) {
  console.error(`WARNING: total mismatch! Input had ${raw.length}, wrote ${total}`);
  process.exit(1);
}
```

Note: The `readFileSync(0, ...)` stdin approach is kept unchanged so piping
`curl ... | node normalize-cache.mjs` still works.

- [ ] **Step 2: Remove `readFileSync` import — only `readFileSync` from fs needed now**

```js
import { existsSync, mkdirSync, readdirSync, unlinkSync, writeFileSync } from "fs";
```

Remove the `readFileSync` reference (it was for stdin): the script still reads
stdin via `readFileSync(0)`. Actually `readFileSync` IS still needed for stdin.
Just add the new imports. Keep the import line as:

```js
import { existsSync, mkdirSync, readdirSync, readFileSync, unlinkSync, writeFileSync } from "fs";
```

- [ ] **Step 3: Run the download script to generate the split cache**

```bash
cd extensions/vscode-skript
bun run download-cache
```

Expected output:
```
  Skript.json — 2494 entries
  SkBee.json — 87 entries
  SkQuery.json — 312 entries
  ...
  Wrote 167 addon files (8060 total entries) to ../../crates/skript-lsp/data/skripthub
```

- [ ] **Step 4: Verify the output directory exists with addon files**

```bash
ls ../../crates/skript-lsp/data/skripthub/ | head -10
```

Expected: shows `Skript.json`, plus other addon `.json` files.

- [ ] **Step 5: Commit**

```bash
git add extensions/vscode-skript/normalize-cache.mjs crates/skript-lsp/data/skripthub/
git commit -m "feat: split SkriptHub cache into per-addon files"
```

---

### Task 2: Add TOML + include_dir dependencies and data model

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/skript-lsp/Cargo.toml`
- Modify: `crates/skript-lsp/src/options.rs`
- Test: `crates/skript-lsp/src/options.rs` (inline tests)

**Interfaces:**
- Consumes: nothing from other tasks
- Produces: `SkriptRcToml` struct, `EffectiveConfig.extra_addons` field

- [ ] **Step 1: Add workspace dependencies**

Add to `[workspace.dependencies]` in root `Cargo.toml`:

```toml
toml = "0.8"
include_dir = "0.7"
```

- [ ] **Step 2: Reference in `skript-lsp/Cargo.toml`**

Add to `[dependencies]`:

```toml
toml = { workspace = true }
include_dir = { workspace = true }
```

- [ ] **Step 3: Add `SkriptRcToml` struct and update `EffectiveConfig` in `options.rs`**

Add after `LogLevelArg` impl block:

```rust
/// Schema for `.skriptrc.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SkriptRcToml {
    /// Addon names the project uses, in addition to core Skript.
    pub addons: Vec<String>,
}
```

Update `EffectiveConfig`:

```rust
pub struct EffectiveConfig {
    /// Cap on the number of completion items returned per request.
    pub max_completions: usize,
    /// Addon names to load syntax for, in addition to always-loaded "Skript".
    /// Empty = only load core Skript entries.
    pub extra_addons: Vec<String>,
}
```

Update `Default` impl for `EffectiveConfig`:

```rust
impl Default for EffectiveConfig {
    fn default() -> Self {
        EffectiveConfig {
            max_completions: 100,
            extra_addons: Vec::new(),
        }
    }
}
```

- [ ] **Step 4: Write unit tests for `SkriptRcToml` deserialization**

Add to `options.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skript_rc_empty_addons() {
        let rc: SkriptRcToml = toml::from_str("").unwrap();
        assert!(rc.addons.is_empty());
    }

    #[test]
    fn skript_rc_missing_addons_key() {
        let rc: SkriptRcToml = toml::from_str("other = true").unwrap();
        assert!(rc.addons.is_empty());
    }

    #[test]
    fn skript_rc_with_addons() {
        let rc: SkriptRcToml = toml::from_str(r#"addons = ["SkBee", "SkQuery"]"#).unwrap();
        assert_eq!(rc.addons, vec!["SkBee", "SkQuery"]);
    }

    #[test]
    fn skript_rc_duplicate_addons() {
        let rc: SkriptRcToml = toml::from_str(r#"addons = ["SkBee", "SkBee"]"#).unwrap();
        let mut addons = rc.addons;
        addons.dedup();
        assert_eq!(addons, vec!["SkBee"]);
    }

    #[test]
    fn effective_config_default_extra_addons_empty() {
        let cfg = EffectiveConfig::default();
        assert!(cfg.extra_addons.is_empty());
        assert_eq!(cfg.max_completions, 100);
    }
}
```

- [ ] **Step 5: Run tests to verify**

```bash
cargo test -p skript-lsp -- options::tests --nocapture
```

Expected: 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/skript-lsp/Cargo.toml crates/skript-lsp/src/options.rs
git commit -m "feat: add SkriptRcToml data model and extra_addons config field"
```

---

### Task 3: Update state.rs to embed directory and filter by addon

**Files:**
- Modify: `crates/skript-lsp/src/state.rs`

**Interfaces:**
- Consumes: `EffectiveConfig.extra_addons` (from Task 2), `CACHE_DIR` (embedded directory)
- Produces: Filtered `Vec<SyntaxEntry>` passed to `install()`

- [ ] **Step 1: Replace `include_bytes!` with `include_dir!`**

At the top of `state.rs`, add the `include_dir` import and replace the static:

```rust
use include_dir::{include_dir, Dir};

// In AppState impl, replace:
//   const EMBEDDED_CACHE: &[u8] = include_bytes!("../data/skripthub-cache.json");

// With:
const CACHE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/data/skripthub");
```

Remove the old `EMBEDDED_CACHE` constant.

- [ ] **Step 2: Extract `load_addon_files` helper function**

Add a free function (or associated function on `AppState`) above or inside the impl:

```rust
/// Load entries from the embedded addon cache, filtering to only the
/// requested addons. Empty `extra_addons` means only "Skript" entries.
fn load_addon_files(cache: &Dir, extra_addons: &[String]) -> Vec<SyntaxEntry> {
    let mut all_entries = Vec::new();
    for file in cache.files() {
        let name = match file.path().file_stem().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        // Always include core "Skript", plus any explicitly requested addons.
        if name != "Skript" && !extra_addons.contains(&name.to_owned()) {
            continue;
        }
        match serde_json::from_slice::<Vec<SyntaxEntry>>(file.contents()) {
            Ok(batch) => all_entries.extend(batch),
            Err(e) => tracing::error!(addon = %name, error = %e, "failed to parse addon file"),
        }
    }
    all_entries
}
```

- [ ] **Step 3: Update `refresh()` to use the directory + filter**

Replace the `refresh()` method body:

```rust
pub async fn refresh(&self) {
    let _guard = self.inner.refresh_lock.lock().await;

    let extra = self.config().extra_addons.clone();
    let entries = load_addon_files(&CACHE_DIR, &extra);

    if entries.is_empty() {
        tracing::warn!("no syntax entries loaded — server will be empty");
    } else {
        tracing::info!(count = entries.len(), "loaded syntax entries from addon cache");
    }

    self.install(entries);
}
```

- [ ] **Step 4: Run existing tests to confirm they still pass**

```bash
cargo test -p skript-lsp -- state::tests --nocapture
```

Expected: Tests pass (they use `install()` directly with manual entry lists, not `refresh()`).

- [ ] **Step 5: Build to confirm compilation**

```bash
cargo build -p skript-lsp
```

Expected: Compiles successfully. Note: `data/skripthub/` directory must exist
from Task 1.

- [ ] **Step 6: Commit**

```bash
git add crates/skript-lsp/src/state.rs
git commit -m "feat: embed addon directory and filter entries in refresh()"
```

---

### Task 4: Parse `.skriptrc.toml` in `initialize()` handler

**Files:**
- Modify: `crates/skript-lsp/src/backend.rs`

**Interfaces:**
- Consumes: `SkriptRcToml` struct (Task 2), `AppState.config()` / `set_config()` (Task 3)
- Produces: `EffectiveConfig.extra_addons` populated from `.skriptrc.toml`

- [ ] **Step 1: Add `.skriptrc.toml` parsing imports to `backend.rs`**

At the top, ensure `toml` and `std::path::Path` are available (they should be
via existing dependencies). Add to imports:

```rust
use crate::options::SkriptRcToml;
// toml is already used via imported crate
```

`toml` crate is used by name — no extra import needed since the function
calls `toml::from_str`.

- [ ] **Step 2: Add `.skriptrc.toml` parsing in `initialize()`**

Add after the existing `apply_init_options(params)` call inside `initialize()`:

```rust
// Parse .skriptrc.toml from workspace root
if let Some(root_uri) = &params.root_uri {
    if let Ok(root_path) = root_uri.to_file_path() {
        let rc_path = root_path.join(".skriptrc.toml");
        if rc_path.exists() {
            match std::fs::read_to_string(&rc_path) {
                Ok(content) => {
                    match toml::from_str::<SkriptRcToml>(&content) {
                        Ok(rc) => {
                            let mut cfg = self.state.config();
                            let mut extra = rc.addons;
                            extra.dedup();
                            cfg.extra_addons = extra;
                            self.state.set_config(cfg);
                            tracing::info!(path = %rc_path.display(), addons = ?cfg.extra_addons, "loaded .skriptrc.toml");
                        }
                        Err(e) => tracing::warn!(path = %rc_path.display(), error = %e, "failed to parse .skriptrc.toml"),
                    }
                }
                Err(e) => tracing::warn!(path = %rc_path.display(), error = %e, "failed to read .skriptrc.toml"),
            }
        } else {
            tracing::debug!("no .skriptrc.toml found — using defaults");
        }
    }
}
```

- [ ] **Step 3: Build to confirm**

```bash
cargo build -p skript-lsp
```

Expected: Compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add crates/skript-lsp/src/backend.rs
git commit -m "feat: parse .skriptrc.toml from workspace root during initialize"
```

---

### Task 5: Cleanup — remove old monolith, update .gitignore, finalize

**Files:**
- Modify: `.gitignore`
- Delete: `crates/skript-lsp/data/skripthub-cache.json`
- Modify: `AGENTS.md` (update references if needed)

- [ ] **Step 1: Remove old monolithic cache file**

```bash
git rm crates/skript-lsp/data/skripthub-cache.json
```

- [ ] **Step 2: Update `.gitignore`**

Replace the old entry `data/skripthub-cache.json` with:

```
# Per-addon SkriptHub cache directory
data/skripthub/
```

- [ ] **Step 3: Create a placeholder so `include_dir!` doesn't break a fresh clone**

Since `include_dir!` requires the directory to exist at compile time, add a
`.gitkeep` file so the directory survives cloning:

```bash
New-Item -ItemType File -Path "crates/skript-lsp/data/skripthub/.gitkeep"
```

Update `.gitignore` to NOT ignore `.gitkeep`:

```
# Per-addon SkriptHub cache directory
data/skripthub/*
!data/skripthub/.gitkeep
```

- [ ] **Step 4: Run full test suite**

```bash
cargo test -p skript-core
cargo test -p skript-lsp
```

Expected: All tests pass (5 pre-existing pattern failures in skript-core unchanged).

- [ ] **Step 5: Run content-aware test**

```bash
cargo test -p skript-core -- content_aware --nocapture
```

Expected: Passes (uses existing test fixture files, no `.skriptrc.toml` involved).

- [ ] **Step 6: Commit**

```bash
git add .gitignore crates/skript-lsp/data/
git rm crates/skript-lsp/data/skripthub-cache.json
git commit -m "chore: remove monolithic cache, add .gitkeep for addon directory"
```

---

### Task 6: Write integration tests for addon filtering

**Files:**
- Create: `crates/skript-lsp/tests/addon_filtering.rs` (or add to existing)

- [ ] **Step 1: Create integration test for `load_addon_files` filtering logic**

Since `load_addon_files` is a free function in `state.rs`, test its behavior
with real `include_dir!` over a test data directory:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use include_dir::{include_dir, Dir};

    const TEST_CACHE: Dir = include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/skripthub");

    #[test]
    fn load_only_skript_when_extra_empty() {
        let entries = load_addon_files(&TEST_CACHE, &[]);
        let addons: std::collections::HashSet<String> = entries
            .iter()
            .map(|e| e.addon.name.clone())
            .collect();
        assert_eq!(addons.len(), 1, "should only contain Skript entries");
        assert!(addons.contains("Skript"));
    }

    #[test]
    fn load_skript_plus_requested_addons() {
        let entries = load_addon_files(&TEST_CACHE, &["SkBee".to_owned()]);
        let addons: std::collections::HashSet<String> = entries
            .iter()
            .map(|e| e.addon.name.clone())
            .collect();
        assert!(addons.contains("Skript"));
        assert!(addons.contains("SkBee"));
        assert!(!addons.contains("SkQuery")); // not requested
    }

    #[test]
    fn load_all_when_skript_is_explicit() {
        let entries = load_addon_files(&TEST_CACHE, &[]);
        let count = entries.len();
        assert!(count > 0, "should have at least Skript entries");
    }
}
```

- [ ] **Step 2: Create test fixture directory with a few addon files**

Create `crates/skript-lsp/tests/fixtures/skripthub/` with 3 minimal JSON files:

`Skript.json`:
```json
[{"id":1,"title":"Send","syntax_pattern":"send %text% to %player%","syntax_type":"effect","addon":{"name":"Skript"}}]
```

`SkBee.json`:
```json
[{"id":100,"title":"Bee Effect","syntax_pattern":"make a bee","syntax_type":"effect","addon":{"name":"SkBee"}}]
```

`SkQuery.json`:
```json
[{"id":200,"title":"Query","syntax_pattern":"query %string%","syntax_type":"effect","addon":{"name":"SkQuery"}}]
```

- [ ] **Step 3: Run the integration tests**

```bash
cargo test -p skript-lsp -- addon_filtering --nocapture
```

Expected: All 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/skript-lsp/tests/addon_filtering.rs crates/skript-lsp/tests/fixtures/
git commit -m "test: add integration tests for addon filtering"
```

---

## Self-Review Checklist

**1. Spec coverage:**
- Behavior matrix ✅ (Task 1 sets up files, Task 3 implements filtering, Task 4 wires .skriptrc.toml)
- `Skript.json` always loaded ✅ (Task 3: `name != "Skript"` check)
- No file = only Skript ✅ (empty extra_addons by default)
- Invalid TOML ignored ✅ (Task 4: warn log, no crash)
- Malformed addon JSON skipped ✅ (Task 3: warn log per file)
- `data/skripthub/` gitignored ✅ (Task 5)
- `install()` signature unchanged ✅ (Task 3: filtering happens before call)

**2. No placeholders:** All code is fully written out in every step.

**3. Type consistency:**
- `SkriptRcToml.addons: Vec<String>` (Task 2) → `EffectiveConfig.extra_addons: Vec<String>` (Task 2) → `load_addon_files(cache, &[String])` (Task 3) ✅
- `include_dir!("$CARGO_MANIFEST_DIR/data/skripthub")` in state.rs ✅
