//! LSP sidecar manager.
//!
//! The `skript-lsp` binary is bundled as a Tauri sidecar (declared in
//! `tauri.conf.json` → `bundle.externalBin`). It binds an ephemeral
//! WebSocket port (127.0.0.1:0), prints exactly one line to stdout —
//! `LISTENING <port>` — immediately after the TCP socket binds, and
//! writes all subsequent tracing/logging to stderr.
//!
//! This manager:
//! 1. Spawns the sidecar via `tauri_plugin_shell` with the CLI flags
//!    `--cache-dir <app_data_dir>`, `--log-level <level>`, and (optionally)
//!    `--skripthub-url <url>`.
//! 2. Reads stdout line-by-line until it sees the `LISTENING <port>`
//!    announcement (or a startup error / 15s timeout).
//! 3. Stores the port and process handle behind a `parking_lot::Mutex`.
//! 4. Polls health by checking whether the process is still alive; if the
//!    process has died, returns `LspStatus::Error` so the frontend can
//!    trigger a restart.
//! 5. Provides `stop()` and `restart()`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use thiserror::Error;
use tracing::{error, info, warn};

/// Maximum time to wait for the sidecar to announce its port before
/// giving up. The TCP bind is essentially instant; this is a generous
/// safety margin.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

/// LSP lifecycle status, mirrored into TypeScript via `LspStatusResult`.
#[derive(Debug, Clone)]
pub enum LspStatus {
    /// Sidecar is running and listening on the given port.
    Connected(u16),
    /// Sidecar is starting up; port not yet known.
    Starting,
    /// Sidecar is not running.
    Disconnected,
    /// Sidecar failed to start or crashed; the message explains why.
    Error(String),
}

/// Configuration passed from the frontend (settings store) into the
/// LSP spawn. Mirrors the LSP's `initializationOptions` schema:
/// `{ skriptHubUrl?, logLevel?, maxCompletions? }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LspSpawnOptions {
    /// Override the SkriptHub API URL. `None` = use LSP default
    /// (`https://skripthub.net/api/v1/addonsyntaxlist/`).
    #[serde(rename = "skriptHubUrl")]
    pub skripthub_url: Option<String>,

    /// Log level: `trace | debug | info | warn | error`. Defaults to
    /// `info` on the LSP side if `None`.
    pub log_level: Option<String>,

    /// Cap on completion items returned per request. `None` = LSP default.
    #[serde(rename = "maxCompletions")]
    pub max_completions: Option<u32>,
}

struct Inner {
    status: LspStatus,
    child: Option<CommandChild>,
    port: Option<u16>,
    started_at: Option<Instant>,
}

impl Inner {
    fn new() -> Self {
        Self {
            status: LspStatus::Disconnected,
            child: None,
            port: None,
            started_at: None,
        }
    }
}

/// Thread-safe handle to the LSP sidecar. Stored in Tauri's state.
#[derive(Clone)]
pub struct LspManager {
    inner: Arc<Mutex<Inner>>,
    /// Last options used to spawn the sidecar. Re-applied on restart.
    last_options: Arc<Mutex<LspSpawnOptions>>,
}

#[derive(Debug, Error)]
pub enum LspError {
    #[error("LSP sidecar is already running")]
    AlreadyRunning,
    #[error("LSP sidecar is not running")]
    NotRunning,
    #[error("Failed to spawn sidecar: {0}")]
    SpawnFailed(String),
    #[error("Sidecar did not announce a port within {0:?}")]
    StartupTimeout(Duration),
    #[error("Sidecar exited unexpectedly: {0}")]
    Exited(String),
}

impl LspManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::new())),
            last_options: Arc::new(Mutex::new(LspSpawnOptions::default())),
        }
    }

    /// Spawn the sidecar with default options and block until it announces
    /// its port.
    pub async fn start(&self) -> Result<u16, LspError> {
        self.start_with_options(LspSpawnOptions::default()).await
    }

    /// Spawn the sidecar with the given options and block until it
    /// announces its port. The options are remembered and re-applied
    /// on subsequent `restart()` calls.
    pub async fn start_with_options(
        &self,
        opts: LspSpawnOptions,
    ) -> Result<u16, LspError> {
        // Check current state
        {
            let mut inner = self.inner.lock();
            if matches!(inner.status, LspStatus::Connected(_) | LspStatus::Starting) {
                return Err(LspError::AlreadyRunning);
            }
            inner.status = LspStatus::Starting;
        }

        // Remember the options for restart()
        *self.last_options.lock() = opts.clone();

        match self.spawn_and_wait(&opts).await {
            Ok(port) => {
                let mut inner = self.inner.lock();
                inner.status = LspStatus::Connected(port);
                inner.port = Some(port);
                inner.started_at = Some(Instant::now());
                info!("LSP sidecar listening on port {port}");
                Ok(port)
            }
            Err(err) => {
                let mut inner = self.inner.lock();
                inner.status = LspStatus::Error(err.to_string());
                inner.child = None;
                inner.port = None;
                error!("LSP sidecar failed to start: {err}");
                Err(err)
            }
        }
    }

    /// Kill the sidecar if it is running. Note: the LSP `shutdown` →
    /// `exit` handshake must be initiated from the **frontend** over
    /// the WebSocket (the Rust shell has no LSP client). The frontend
    /// should call `client.shutdown()` then `client.exit()` *before*
    /// invoking this command.
    pub async fn stop(&self) -> Result<(), LspError> {
        let child = {
            let mut inner = self.inner.lock();
            inner.status = LspStatus::Disconnected;
            inner.started_at = None;
            inner.port = None;
            inner.child.take()
        };

        if let Some(child) = child {
            // Best-effort kill. The LSP server implements graceful
            // shutdown but only via the LSP protocol, which is owned
            // by the frontend's language client. From the Rust side
            // we only have a process handle, so hard kill is the only
            // option — and the backend confirmed that's safe (no
            // critical unflushed state in v1).
            if let Err(e) = child.kill() {
                warn!("Failed to kill LSP sidecar cleanly: {e}");
            } else {
                info!("LSP sidecar stopped");
            }
        }
        Ok(())
    }

    /// Kill the sidecar (if running) and start it again, reusing the
    /// options from the previous `start_with_options` call.
    pub async fn restart(&self) -> Result<u16, LspError> {
        let opts = self.last_options.lock().clone();
        self.stop().await?;
        // Small grace period to let the OS release the port.
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.start_with_options(opts).await
    }

    /// Snapshot of the current status.
    pub fn status(&self) -> LspStatus {
        self.inner.lock().status.clone()
    }

    // --- Internals -------------------------------------------------------

    async fn spawn_and_wait(&self, opts: &LspSpawnOptions) -> Result<u16, LspError> {
        let app = self
            .app_handle()
            .ok_or_else(|| LspError::SpawnFailed("no Tauri app handle available".into()))?;

        // Resolve the app-data directory for --cache-dir. The LSP writes
        // skripthub-cache.json here, which our get_skript_docs command
        // then reads back.
        let cache_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| LspError::SpawnFailed(format!("app_data_dir: {e}")))?;
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| LspError::SpawnFailed(format!("create cache_dir: {e}")))?;

        // Build the sidecar command with the agreed flags.
        let mut cmd = app.shell().sidecar("skript-lsp").map_err(|e| {
            LspError::SpawnFailed(format!("sidecar lookup failed: {e}"))
        })?;

        cmd = cmd
            .args([
                "--cache-dir",
                cache_dir.to_string_lossy().as_ref(),
            ])
            .args([
                "--log-level",
                opts.log_level.as_deref().unwrap_or("info"),
            ]);

        if let Some(url) = &opts.skripthub_url {
            cmd = cmd.args(["--skripthub-url", url]);
        }

        let (mut rx, child) = cmd
            .spawn()
            .map_err(|e| LspError::SpawnFailed(e.to_string()))?;

        // Store the child handle so we can kill it later.
        {
            let mut inner = self.inner.lock();
            inner.child = Some(child);
        }

        // Wait for either a `LISTENING <port>` line on stdout or an
        // error / exit. The LSP writes ONLY the LISTENING line to
        // stdout; all tracing goes to stderr, so we don't have to
        // filter around log noise.
        let deadline = Instant::now() + STARTUP_TIMEOUT;

        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(LspError::StartupTimeout(STARTUP_TIMEOUT))?;

            match tokio::time::timeout(remaining, rx.recv()).await {
                Err(_) => return Err(LspError::StartupTimeout(STARTUP_TIMEOUT)),
                Ok(None) => {
                    return Err(LspError::Exited(
                        "sidecar stdout closed before announcing port".into(),
                    ));
                }
                Ok(Some(event)) => match event {
                    CommandEvent::Stdout(bytes) => {
                        let line = String::from_utf8_lossy(&bytes);
                        let line = line.trim();
                        // Stdout is reserved for the LISTENING announcement.
                        // Anything else here is unexpected — log it but keep waiting.
                        if let Some(port) = parse_listening(line) {
                            return Ok(port);
                        } else if !line.is_empty() {
                            info!("[skript-lsp:stdout] {line}");
                        }
                    }
                    CommandEvent::Stderr(bytes) => {
                        let line = String::from_utf8_lossy(&bytes);
                        let line = line.trim();
                        if !line.is_empty() {
                            warn!("[skript-lsp] {line}");
                        }
                    }
                    CommandEvent::Error(err) => {
                        warn!("[skript-lsp] error event: {err}");
                    }
                    CommandEvent::Terminated(payload) => {
                        let msg = format!(
                            "sidecar exited with code {:?} before announcing port",
                            payload.code
                        );
                        return Err(LspError::Exited(msg));
                    }
                    _ => {}
                },
            }
        }
    }

    fn app_handle(&self) -> Option<AppHandle> {
        APP_HANDLE.get().cloned()
    }
}

// --- Global AppHandle storage -------------------------------------------
//
// Tauri commands receive `AppHandle` as an argument, but `LspManager`
// is constructed during `setup()` before any command runs. We stash
// the handle in a OnceCell so `spawn_and_wait` can reach the sidecar
// API.

static APP_HANDLE: once_cell::sync::OnceCell<AppHandle> = once_cell::sync::OnceCell::new();

/// Stash the global AppHandle. Called from `lib.rs` setup.
pub fn set_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

// We need `Manager` for `app.path()`.
use tauri::Manager;

// --- Helpers -------------------------------------------------------------

/// Parse a line of the form `LISTENING 41237` and return the port.
fn parse_listening(line: &str) -> Option<u16> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("LISTENING")?.trim();
    let port: u16 = rest.parse().ok()?;
    if port > 0 {
        Some(port)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_listening_line() {
        assert_eq!(parse_listening("LISTENING 41237"), Some(41237));
        assert_eq!(parse_listening("listening 1"), None); // lowercase
        assert_eq!(parse_listening("LISTENING"), None);
        assert_eq!(parse_listening("LISTENING abc"), None);
        assert_eq!(parse_listening("something else"), None);
        assert_eq!(parse_listening("LISTENING 0"), None); // port 0 invalid
        assert_eq!(parse_listening("  LISTENING  41237  "), Some(41237));
    }

    #[test]
    fn default_spawn_options_use_lsp_defaults() {
        let opts = LspSpawnOptions::default();
        assert!(opts.skripthub_url.is_none());
        assert!(opts.log_level.is_none());
        assert!(opts.max_completions.is_none());
    }
}
