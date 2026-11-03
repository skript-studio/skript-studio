//! Shared application state.
//!
//! Holds the compiled entry table, the open document set, a readiness
//! flag, and the effective runtime config. The SkriptHub cache is embedded
//! into the binary at compile time via `include_bytes!`, so the server is
//! ready instantly on startup with no network calls.
//!
//! Readiness model: every feature (diagnostics, completions, hover) checks
//! [`AppState::ready`] and degrades gracefully — returning empty results —
//! until the cache is loaded.
//!
//! # Pattern engine thread
//!
//! The faithful pattern engine (`skript-registry` + `skript-patterns`)
//! is owned by **one dedicated OS thread** (see [`AppState::start_engine`]).
//! This is forced by the frozen `skript-patterns` API: pattern elements
//! are `Rc<RefCell<…>>`-based (`!Send`), so the compiled `Registry` can
//! never be shared with async tasks — it is built, matched against, and
//! dropped entirely on that thread. The rest of the server only ever
//! sees `Send` handles: the request channel
//! ([`AppState::check_document_via_engine`],
//! [`AppState::hover_via_engine`]) and plain data
//! (`skript_checks::LineDiagnostic`s).

use crate::options::EffectiveConfig;
use include_dir::{include_dir, Dir};
use skript_core::matching::EntryTable;
use skript_core::parse_program;
use skript_core::parser::incremental::incremental_update;
use skript_core::parser::ParseResult;
use skript_core::semantic::{analyze, SemanticAnalysis};
use skript_core::SyntaxEntry;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex as SyncMutex, RwLock};
use tokio::sync::Mutex;

/// The cached result of parsing and analyzing a document.
pub struct DocumentSnapshot {
    pub parse_result: ParseResult,
    pub analysis: SemanticAnalysis,
}

impl DocumentSnapshot {
    /// Create a fresh snapshot by parsing + analyzing the source.
    pub fn new(source: &str) -> Self {
        let parse_result = parse_program(source);
        let analysis = analyze(&parse_result.program);
        DocumentSnapshot {
            parse_result,
            analysis,
        }
    }

    /// Incrementally update from a previous snapshot.
    pub fn update(old_source: &str, new_source: &str, old: &DocumentSnapshot) -> Self {
        let parse_result = incremental_update(old_source, new_source, &old.parse_result.program);
        let analysis = analyze(&parse_result.program);
        DocumentSnapshot {
            parse_result,
            analysis,
        }
    }
}

struct DocEntry {
    text: String,
    snapshot: Option<DocumentSnapshot>,
}

/// Shared server state, cloned cheaply into every request handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    /// `None` until the first load completes; `Some` afterwards.
    table: RwLock<Option<Arc<EntryTable>>>,
    /// `true` once the embedded cache has populated `table`.
    ready: AtomicBool,
    /// Open documents keyed by URI string.
    docs: dashmap::DashMap<String, DocEntry>,
    config: RwLock<EffectiveConfig>,
    /// Serializes refresh attempts.
    refresh_lock: Mutex<()>,
    /// Pattern-engine lifecycle + stats. `engine_state_cv` is signalled
    /// whenever the state transitions to `Ready` or `Broken` (so
    /// [`AppState::ensure_engine`] waiters wake up).
    engine_state: SyncMutex<EngineState>,
    engine_state_cv: Condvar,
    /// Request channel to the engine thread; `None` until the engine is
    /// started (always set while holding the `engine_state` lock). Checks
    /// and hovers share one FIFO channel so neither can starve the other.
    engine_requests:
        SyncMutex<Option<tokio::sync::mpsc::UnboundedSender<EngineRequest>>>,
}

/// Lifecycle + stats of the engine thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineState {
    /// Not started yet.
    Idle,
    /// Thread spawned; the registry is still compiling.
    Building,
    /// Registry compiled and serving requests.
    Ready(EngineStats),
    /// Permanently unavailable (corpus load failure or a panic — e.g. a
    /// `todo!()` stub while the engine port is still landing).
    Broken,
}

/// What the engine thread built. Mirrored out of the (single-owner,
/// `!Send`) registry so the rest of the server can report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineStats {
    /// Registry entries loaded from the embedded corpus.
    pub entries: usize,
    /// Split patterns that compiled successfully.
    pub patterns_compiled: usize,
    /// Split patterns that failed to compile.
    pub compile_failures: usize,
}

/// Everything the async side can ask the engine thread to do.
///
/// Both request kinds deliberately share ONE channel: the engine thread
/// serves requests strictly in arrival order. The previous design used
/// two channels with `blocking_recv` on the check channel first — which
/// *blocks* — so hover requests were never served while the check sender
/// was alive (the server holds it for its whole lifetime) and every hover
/// hung until the client cancelled it (VSCode: perpetual "Loading..").
enum EngineRequest {
    Check(CheckRequest),
    Hover(HoverRequest),
}

/// One document check sent to the engine thread.
struct CheckRequest {
    text: String,
    config: skript_checks::CheckConfig,
    reply: tokio::sync::oneshot::Sender<CheckReply>,
}

/// Answer from the engine thread. Plain data only (`Send`).
enum CheckReply {
    Diagnostics(Vec<skript_checks::LineDiagnostic>),
    /// The engine could not produce diagnostics (broken / unavailable).
    Unavailable,
}

/// A hover request sent to the engine thread.
struct HoverRequest {
    line: String,
    cursor: Option<usize>,
    reply: tokio::sync::oneshot::Sender<HoverReply>,
}

/// Hover reply from the engine thread. Plain data only (`Send`).
enum HoverReply {
    /// The engine matched the line; here's the entry ID + span.
    Match { entry_id: u32, span: (usize, usize) },
    /// No match, or the engine is unavailable.
    None,
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
                table: RwLock::new(None),
                ready: AtomicBool::new(false),
                docs: dashmap::DashMap::new(),
                config: RwLock::new(config),
                refresh_lock: Mutex::new(()),
                engine_state: SyncMutex::new(EngineState::Idle),
                engine_state_cv: Condvar::new(),
                engine_requests: SyncMutex::new(None),
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
        let snapshot = DocumentSnapshot::new(&text);
        self.inner.docs.insert(
            uri,
            DocEntry {
                text,
                snapshot: Some(snapshot),
            },
        );
    }

    pub fn update_doc(&self, uri: &str, new_text: String) {
        if let Some(mut entry) = self.inner.docs.get_mut(uri) {
            let old_text = std::mem::replace(&mut entry.text, new_text);
            if old_text != entry.text {
                let old_snapshot = entry.snapshot.take();
                let new_snapshot = match old_snapshot {
                    Some(old) => DocumentSnapshot::update(&old_text, &entry.text, &old),
                    None => DocumentSnapshot::new(&entry.text),
                };
                entry.snapshot = Some(new_snapshot);
            }
        } else {
            let snapshot = DocumentSnapshot::new(&new_text);
            self.inner.docs.insert(
                uri.to_owned(),
                DocEntry {
                    text: new_text,
                    snapshot: Some(snapshot),
                },
            );
        }
    }

    pub fn close_doc(&self, uri: &str) {
        self.inner.docs.remove(uri);
    }

    pub fn get_doc(&self, uri: &str) -> Option<String> {
        self.inner.docs.get(uri).map(|entry| entry.text.clone())
    }

    /// Access the cached snapshot for a document.
    pub fn with_snapshot<R>(&self, uri: &str, f: impl FnOnce(&DocumentSnapshot) -> R) -> Option<R> {
        self.inner
            .docs
            .get(uri)
            .and_then(|entry| entry.snapshot.as_ref().map(f))
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

    /// Read-only access to the entry table, if loaded.
    pub fn with_index<R>(&self, f: impl FnOnce(&EntryTable) -> R) -> Option<R> {
        let tbl_guard = self.inner.table.read().unwrap();
        tbl_guard.as_ref().map(|tbl| f(tbl))
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
            tracing::info!(
                count = entries.len(),
                "loaded syntax entries from addon cache"
            );
        }

        self.install(entries);
    }

    /// Build the entry table and mark the server ready.
    pub fn install(&self, entries: Vec<SyntaxEntry>) {
        let table = EntryTable::from_entries(entries);
        *self.inner.table.write().unwrap() = Some(Arc::new(table));
        self.inner.ready.store(true, Ordering::Release);
        tracing::info!("syntax entry table installed; server is ready");
    }

    // --- pattern engine -------------------------------------------------

    /// Stats of the built engine; `None` until it is `Ready`.
    pub fn engine_stats(&self) -> Option<EngineStats> {
        let guard = self
            .inner
            .engine_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match *guard {
            EngineState::Ready(stats) => Some(stats),
            _ => None,
        }
    }

    /// Start building the engine on its dedicated thread.
    ///
    /// Idempotent and non-blocking. Returns `true` when the engine is being
    /// built (or is already ready), `false` when it is permanently
    /// unavailable.
    ///
    /// Why a dedicated thread: `skript-patterns` elements are
    /// `Rc<RefCell<…>>`-based (frozen API), so the compiled `Registry` is
    /// `!Send` and can never be stored in this (async-shared) state or moved
    /// into `spawn_blocking` closures. The registry is built, matched
    /// against, and dropped entirely on this one thread; everything that
    /// crosses back to the async side is plain `Send` data.
    pub fn start_engine(&self) -> bool {
        let mut state = self
            .inner
            .engine_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match *state {
            EngineState::Idle => {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                {
                    // Lock order is always engine_state → engine_requests; no
                    // other path nests these two locks.
                    let mut requests = self
                        .inner
                        .engine_requests
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    *requests = Some(tx);
                }
                *state = EngineState::Building;
                // Release the state lock before spawning so the new thread
                // can immediately take it to report Ready/Broken.
                drop(state);

                let spawn_state = self.clone();
                let spawn_result = std::thread::Builder::new()
                    .name("skript-pattern-engine".to_owned())
                    .spawn(move || {
                        let engine_state = spawn_state.clone();
                        // The whole engine body is panic-contained: a panic
                        // (e.g. a `todo!()` stub while the port lands) must
                        // mark the engine Broken so `ensure_engine`
                        // waiters wake up and parked `check_document_via_engine`
                        // replies resolve (their oneshot senders drop together
                        // with the request channel) instead of hanging.
                        let outcome =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                                engine_thread_main(engine_state, rx);
                            }));
                        if outcome.is_err() {
                            tracing::error!(
                                "pattern engine thread panicked; \
                                 the pattern engine is disabled for this session"
                            );
                            spawn_state.set_engine_broken();
                        }
                    });
                match spawn_result {
                    Ok(_handle) => true,
                    Err(e) => {
                        tracing::error!(error = %e, "failed to spawn the pattern engine thread");
                        // The closure (and with it the channel receiver) was
                        // dropped by the failed spawn, so sends on the stored
                        // sender now fail and callers fall back. Wake any
                        // waiters:
                        self.set_engine_broken();
                        false
                    }
                }
            }
            EngineState::Building | EngineState::Ready(_) => true,
            EngineState::Broken => false,
        }
    }

    /// Build the engine and block until it is ready.
    ///
    /// Starts the dedicated engine thread if needed, then waits for it to
    /// finish compiling the embedded corpus. Returns the number of compile
    /// failures (`0` when every pattern compiled). Returns `usize::MAX`
    /// when the engine is permanently unavailable (corpus load failure or
    /// a panicking compile).
    ///
    /// Only call this from a blocking context (`spawn_blocking`, tests) —
    /// never directly from an async handler. Subsequent calls neither
    /// rebuild nor re-block (they return the stored failure count).
    pub fn ensure_engine(&self) -> usize {
        if !self.start_engine() {
            return usize::MAX;
        }
        let mut guard = self
            .inner
            .engine_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            match *guard {
                EngineState::Ready(stats) => return stats.compile_failures,
                EngineState::Broken => return usize::MAX,
                // `Idle` is unreachable here (start_engine ran),
                // `Building` means the engine thread is still compiling.
                EngineState::Idle | EngineState::Building => {}
            }
            guard = self
                .inner
                .engine_state_cv
                .wait(guard)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// Run `skript_checks::check_document` for one document on the engine
    /// thread.
    ///
    /// Returns `None` when the engine cannot produce diagnostics (not
    /// startable, broken, or its thread died) — callers fall back to the
    /// skript-core validator. Requests sent while the engine is still
    /// building are queued and answered once it is ready, so the first
    /// diagnostics after startup are correct rather than a flash of
    /// core-engine output.
    pub async fn check_document_via_engine(
        &self,
        text: String,
        config: skript_checks::CheckConfig,
    ) -> Option<Vec<skript_checks::LineDiagnostic>> {
        if !self.start_engine() {
            return None;
        }
        let sender = {
            let guard = self
                .inner
                .engine_requests
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.clone()
        };
        let sender = sender?;
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        sender
            .send(EngineRequest::Check(CheckRequest {
                text,
                config,
                reply: reply_tx,
            }))
            .ok()?;
        match reply_rx.await {
            Ok(CheckReply::Diagnostics(diags)) => Some(diags),
            // Unavailable, or the engine thread died before answering.
            Ok(CheckReply::Unavailable) | Err(_) => None,
        }
    }

    /// Match a line against the pattern engine for hover purposes.
    ///
    /// Sends the line to the engine thread, which matches it against all
    /// compiled patterns and returns the best-matching entry index + span.
    /// Returns `None` when the engine is unavailable or no pattern matches.
    /// The caller resolves the entry index to a `SyntaxEntry` via
    /// `with_index` / `EntryTable::get`.
    pub async fn hover_via_engine(
        &self,
        line: String,
        cursor: Option<usize>,
    ) -> Option<(u32, (usize, usize))> {
        if !self.start_engine() {
            return None;
        }
        let sender = {
            let guard = self
                .inner
                .engine_requests
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.clone()
        };
        let sender = sender?;
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        sender
            .send(EngineRequest::Hover(HoverRequest {
                line,
                cursor,
                reply: reply_tx,
            }))
            .ok()?;
        match reply_rx.await {
            Ok(HoverReply::Match { entry_id, span }) => Some((entry_id, span)),
            Ok(HoverReply::None) | Err(_) => None,
        }
    }

    fn set_engine_ready(&self, stats: EngineStats) {
        let mut guard = self
            .inner
            .engine_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = EngineState::Ready(stats);
        drop(guard);
        self.inner.engine_state_cv.notify_all();
    }

    fn set_engine_broken(&self) {
        let mut guard = self
            .inner
            .engine_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = EngineState::Broken;
        drop(guard);
        self.inner.engine_state_cv.notify_all();
    }
}

/// Body of the dedicated engine thread. Exclusively owns the `!Send`
/// compiled registry for the whole thread lifetime.
fn engine_thread_main(
    state: AppState,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<EngineRequest>,
) {
    // skript-patterns matching consults a thread-local type resolver; this
    // is the only thread that compiles or matches patterns, so install the
    // permissive resolver HERE (idempotent and cheap; this function is
    // fully synchronous, so the resolver is never held across an await).
    skript_patterns::set_type_resolver(Rc::new(skript_patterns::PermissiveResolver));

    let started = std::time::Instant::now();
    let registry = match skript_registry::Registry::embedded() {
        Ok(mut registry) => {
            let failures = registry.compile_all();
            let patterns_compiled: usize = (0..registry.entries().len())
                .map(|i| registry.compiled_for_entry(i).len())
                .sum();
            tracing::info!(
                entries = registry.entries().len(),
                compiled = patterns_compiled,
                failures = failures.len(),
                elapsed_ms = started.elapsed().as_millis() as u64,
                "pattern engine: embedded corpus compiled"
            );
            // First few failures at debug level for post-hoc inspection.
            for failure in failures.iter().take(5) {
                tracing::debug!(
                    entry = failure.entry_index,
                    pattern = failure.pattern_index,
                    raw = %failure.raw,
                    error = ?failure.error,
                    "pattern engine: pattern compile failure"
                );
            }
            state.set_engine_ready(EngineStats {
                entries: registry.entries().len(),
                patterns_compiled,
                compile_failures: failures.len(),
            });
            Some(registry)
        }
        Err(e) => {
            tracing::error!(error = %e, "pattern engine: embedded corpus failed to load");
            state.set_engine_broken();
            None
        }
    };

    // Serve requests until the channel closes (server shutdown). Checks
    // and hovers share the channel, so each is answered strictly in
    // arrival order — neither can starve the other. A panic inside
    // `check_document` or `match_line_for_hover` unwinds out of the loop
    // and is contained by the catch_unwind at the spawn site: the channel
    // (and the oneshot senders inside queued requests) drop, so awaiting
    // callers see an error and fall back gracefully.
    while let Some(request) = rx.blocking_recv() {
        match request {
            EngineRequest::Check(request) => {
                let reply = match registry.as_ref() {
                    Some(reg) => {
                        skript_patterns::set_type_resolver(Rc::new(
                            skript_patterns::PermissiveResolver,
                        ));
                        CheckReply::Diagnostics(skript_checks::check_document(
                            &request.text,
                            reg,
                            &request.config,
                        ))
                    }
                    None => CheckReply::Unavailable,
                };
                let _ = request.reply.send(reply);
            }
            EngineRequest::Hover(request) => {
                let reply = match registry.as_ref() {
                    Some(reg) => {
                        skript_patterns::set_type_resolver(Rc::new(
                            skript_patterns::PermissiveResolver,
                        ));
                        match skript_checks::match_line_for_hover(
                            &request.line,
                            request.cursor,
                            reg,
                        ) {
                            Some(m) => HoverReply::Match {
                                entry_id: m.entry_id,
                                span: m.span,
                            },
                            None => HoverReply::None,
                        }
                    }
                    None => HoverReply::None,
                };
                let _ = request.reply.send(reply);
            }
        }
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
        assert!(state.with_index(|_| ()).is_none());

        state.install(sample_entries());
        assert!(state.ready());
        let count = state.with_index(|tbl| tbl.len()).unwrap();
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

    // --- addon filtering (first plan Task 6) ---

    const TEST_CACHE: Dir = include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/skripthub");

    fn loaded_addons(extra: &[String]) -> std::collections::HashSet<String> {
        load_addon_files(&TEST_CACHE, extra)
            .into_iter()
            .map(|e| e.addon.name)
            .collect()
    }

    #[test]
    fn load_only_skript_when_extra_empty() {
        let addons = loaded_addons(&[]);
        assert_eq!(addons.len(), 1, "should only contain Skript entries");
        assert!(addons.contains("Skript"));
    }

    #[test]
    fn load_skript_plus_requested_addons() {
        let addons = loaded_addons(&["SkBee".to_owned()]);
        assert!(addons.contains("Skript"));
        assert!(addons.contains("SkBee"));
        assert!(
            !addons.contains("SkQuery"),
            "unrequested addon must be filtered out"
        );
    }

    #[test]
    fn load_all_when_skript_is_explicit() {
        let entries = load_addon_files(&TEST_CACHE, &["Skript".to_owned()]);
        assert!(!entries.is_empty(), "should have at least Skript entries");
        assert!(entries.iter().all(|e| e.addon.name == "Skript"));
    }

    // --- pattern engine wiring -----------------------------------------------

    /// `AppState` is held across `.await`s by every tower-lsp handler, so it
    /// must stay `Send + Sync`. This is exactly why the `!Send`
    /// `Registry` lives on its dedicated thread instead of inside `Inner`.
    #[test]
    fn app_state_remains_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AppState>();
    }

    #[test]
    fn engine_idle_on_fresh_state() {
        // A fresh state has no engine, and stats read `None`. (Do NOT start
        // the engine here: with the pattern-compiler stubs still `todo!()`,
        // starting would panic-build on the engine thread by design.)
        let state = AppState::new(EffectiveConfig::default());
        assert_eq!(state.engine_stats(), None);
    }
}
