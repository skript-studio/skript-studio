//! Shared application state.
//!
//! Holds the compiled syntax index, the entry table, the open document set,
//! a readiness flag, and the effective runtime config. The SkriptHub cache
//! is embedded into the binary at compile time via `include_bytes!`, so the
//! server is ready instantly on startup with no network calls.
//!
//! Readiness model: every feature (diagnostics, completions, hover) checks
//! [`AppState::ready`] and degrades gracefully — returning empty results —
//! until the cache is loaded.

use crate::options::EffectiveConfig;
use include_dir::{include_dir, Dir};
use skript_core::matching::{EntryTable, SyntaxIndex};
use skript_core::SyntaxEntry;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

/// Shared server state, cloned cheaply into every request handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    /// `None` until the first load completes; `Some` afterwards.
    index: RwLock<Option<Arc<SyntaxIndex>>>,
    table: RwLock<Option<Arc<EntryTable>>>,
    /// `true` once the embedded cache has populated `index`/`table`.
    ready: AtomicBool,
    /// Open documents keyed by URI string.
    docs: dashmap::DashMap<String, String>,
    config: RwLock<EffectiveConfig>,
    /// Serializes refresh attempts.
    refresh_lock: Mutex<()>,
}

const CACHE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/data/skripthub");

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

impl AppState {
    /// Create an empty state. Call [`Self::spawn_fetch`] to populate it.
    pub fn new(config: EffectiveConfig) -> Self {
        AppState {
            inner: Arc::new(Inner {
                index: RwLock::new(None),
                table: RwLock::new(None),
                ready: AtomicBool::new(false),
                docs: dashmap::DashMap::new(),
                config: RwLock::new(config),
                refresh_lock: Mutex::new(()),
            }),
        }
    }

    // --- config ------------------------------------------------------------

    pub fn config(&self) -> EffectiveConfig {
        self.inner.config.read().unwrap().clone()
    }

    pub fn set_config(&self, config: EffectiveConfig) {
        *self.inner.config.write().unwrap() = config;
    }

    // --- documents ---------------------------------------------------------

    pub fn open_doc(&self, uri: String, text: String) {
        self.inner.docs.insert(uri, text);
    }

    pub fn update_doc(&self, uri: &str, text: String) {
        self.inner.docs.insert(uri.to_owned(), text);
    }

    pub fn close_doc(&self, uri: &str) {
        self.inner.docs.remove(uri);
    }

    pub fn get_doc(&self, uri: &str) -> Option<String> {
        self.inner.docs.get(uri).map(|s| s.clone())
    }

    /// URIs of all currently open documents. Useful for re-publishing
    /// diagnostics across the whole open set (e.g. when the index becomes
    /// ready).
    pub fn doc_uris(&self) -> Vec<String> {
        self.inner
            .docs
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    // --- readiness / data --------------------------------------------------

    pub fn ready(&self) -> bool {
        self.inner.ready.load(Ordering::Acquire)
    }

    /// Read-only access to the compiled index, if loaded.
    pub fn with_index<R>(&self, f: impl FnOnce(&SyntaxIndex, &EntryTable) -> R) -> Option<R> {
        let idx_guard = self.inner.index.read().unwrap();
        let tbl_guard = self.inner.table.read().unwrap();
        match (idx_guard.as_ref(), tbl_guard.as_ref()) {
            (Some(idx), Some(tbl)) => Some(f(idx, tbl)),
            _ => None,
        }
    }

    /// Spawn the background load of the embedded syntax data. Safe to call
    /// multiple times — each call spawns a new task, but `refresh` is guarded
    /// by `refresh_lock`.
    pub fn spawn_fetch(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            state.refresh().await;
        });
    }

    /// Load syntax data from the embedded per-addon cache directory, filtering
    /// to only the requested addons. Guarded by `refresh_lock` so concurrent
    /// callers coalesce.
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

    /// Compile entries into the index/table and mark the server ready.
    pub fn install(&self, entries: Vec<SyntaxEntry>) {
        let index = SyntaxIndex::build(&entries);
        let table = EntryTable::from_entries(entries);
        *self.inner.index.write().unwrap() = Some(Arc::new(index));
        *self.inner.table.write().unwrap() = Some(Arc::new(table));
        self.inner.ready.store(true, Ordering::Release);
        tracing::info!("syntax index installed; server is ready");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skript_core::syntax::{Addon, SyntaxType};

    fn sample_entries() -> Vec<SyntaxEntry> {
        vec![SyntaxEntry {
            id: 1,
            title: "Send".to_owned(),
            description: String::new(),
            syntax_pattern: "send %text% to %player%".to_owned(),
            syntax_type: SyntaxType::Effect,
            addon: Addon::default(),
            return_type: None,
            required_plugins: Vec::new(),
            event_values: None,
            type_usage: None,
            entries: None,
            compatible_addon_version: String::new(),
            compatible_minecraft_version: String::new(),
            json_id: None,
            event_cancellable: false,
            mark_as_removed: false,
            removed_since: None,
        }]
    }

    #[test]
    fn install_makes_state_ready_and_queryable() {
        let state = AppState::new(EffectiveConfig::default());
        assert!(!state.ready());
        assert!(state
            .with_index(|_, _| ())
            .is_none());

        state.install(sample_entries());
        assert!(state.ready());
        let count = state.with_index(|_idx, tbl| tbl.len()).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn documents_round_trip() {
        let state = AppState::new(EffectiveConfig::default());
        state.open_doc("file:///x.sk".to_owned(), "hello".to_owned());
        assert_eq!(state.get_doc("file:///x.sk").as_deref(), Some("hello"));
        state.update_doc("file:///x.sk", "world".to_owned());
        assert_eq!(state.get_doc("file:///x.sk").as_deref(), Some("world"));
        state.close_doc("file:///x.sk");
        assert!(state.get_doc("file:///x.sk").is_none());
    }
}
