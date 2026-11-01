//! Tauri IPC commands.

pub mod fs;
pub mod lsp;

use std::path::PathBuf;

use serde::Deserialize;
use tauri::{AppHandle, Manager};

/// Fetch the SkriptHub syntax database, caching it on disk for offline use.
///
/// The LSP sidecar already does this at startup; we forward the request
/// to its cached file if present, otherwise fetch directly.
#[tauri::command]
pub async fn get_skript_docs(app: AppHandle) -> Result<serde_json::Value, String> {
    // 1. Try the LSP cache file in the app-data dir.
    let cache_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("skripthub-cache.json");

    if cache_path.exists() {
        let contents = std::fs::read_to_string(&cache_path)
            .map_err(|e| format!("Failed to read SkriptHub cache: {e}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&contents).map_err(|e| format!("Invalid cache JSON: {e}"))?;
        return Ok(value);
    }

    // 2. Fall back to a direct HTTP fetch.
    let url = "https://skripthub.net/api/v1/addonsyntaxlist/";
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("SkriptHub returned HTTP {}", resp.status()));
    }

    let value: serde_json::Value = resp.json().await.map_err(|e| format!("JSON decode failed: {e}"))?;

    // 3. Persist the response for next time.
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cache_path, serde_json::to_string(&value).unwrap_or_default());

    Ok(value)
}

#[derive(Debug, Deserialize)]
pub struct WriteSettingsArgs {
    pub settings: String,
}

#[tauri::command]
pub async fn write_settings(
    app: AppHandle,
    args: WriteSettingsArgs,
) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, args.settings).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn read_settings(app: AppHandle) -> Result<Option<String>, String> {
    let path = settings_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(Some(contents))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json"))
}
