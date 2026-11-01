//! LSP lifecycle commands. Forwards to the `LspManager` in Tauri state.
//!
//! `start_lsp` accepts an optional `LspSpawnOptions` payload from the
//! frontend so settings (SkriptHub URL, log level, max completions)
//! flow into both the CLI flags and the LSP `initializationOptions`.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};
use tracing::info;

use crate::lsp::{LspManager, LspSpawnOptions, LspStatus};

#[derive(Debug, Serialize)]
pub struct LspStartResult {
    pub port: u16,
}

#[derive(Debug, Serialize)]
pub struct LspStatusResult {
    pub status: String,
    pub port: Option<u16>,
    pub error: Option<String>,
}

impl From<LspStatus> for LspStatusResult {
    fn from(s: LspStatus) -> Self {
        match s {
            LspStatus::Connected(port) => Self {
                status: "connected".into(),
                port: Some(port),
                error: None,
            },
            LspStatus::Starting => Self {
                status: "starting".into(),
                port: None,
                error: None,
            },
            LspStatus::Disconnected => Self {
                status: "disconnected".into(),
                port: None,
                error: None,
            },
            LspStatus::Error(msg) => Self {
                status: "error".into(),
                port: None,
                error: Some(msg),
            },
        }
    }
}

/// Spawn the LSP sidecar. The optional `options` payload is forwarded
/// both to the sidecar's CLI flags (cache-dir, log-level, skripthub-url)
/// and remembered by the manager for use in `restart_lsp`.
#[tauri::command]
pub async fn start_lsp(
    _app: AppHandle,
    manager: State<'_, Arc<LspManager>>,
    options: Option<LspSpawnOptions>,
) -> Result<LspStartResult, String> {
    info!("start_lsp command received (options={:?})", options);
    let opts = options.unwrap_or_default();
    let port = manager.start_with_options(opts).await.map_err(|e| e.to_string())?;
    Ok(LspStartResult { port })
}

#[tauri::command]
pub async fn stop_lsp(manager: State<'_, Arc<LspManager>>) -> Result<(), String> {
    info!("stop_lsp command received");
    manager.stop().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn restart_lsp(
    manager: State<'_, Arc<LspManager>>,
) -> Result<LspStartResult, String> {
    info!("restart_lsp command received");
    let port = manager.restart().await.map_err(|e| e.to_string())?;
    Ok(LspStartResult { port })
}

#[tauri::command]
pub async fn get_lsp_status(
    manager: State<'_, Arc<LspManager>>,
) -> Result<LspStatusResult, String> {
    Ok(manager.status().into())
}
