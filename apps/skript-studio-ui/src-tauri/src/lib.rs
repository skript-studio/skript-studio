//! Skript Studio — Tauri 2 backend.
//!
//! Wires up the plugins, registers the IPC commands, and manages the
//! LSP sidecar process.

mod commands;
mod lsp;

use std::sync::Arc;

use lsp::LspManager;
use tauri::Manager;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .setup(|app| {
            // Initialise the LSP sidecar manager and store it in Tauri's
            // state container so commands can access it.
            let manager = Arc::new(LspManager::new());
            app.manage(manager);

            // Stash the AppHandle so the manager can reach the sidecar API.
            lsp::set_app_handle(app.handle().clone());

            tracing::info!("Skript Studio backend ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // File operations
            commands::fs::open_folder,
            commands::fs::list_dir,
            commands::fs::read_file,
            commands::fs::write_file,
            commands::fs::create_file,
            commands::fs::delete_file,
            commands::fs::rename_file,
            // LSP operations
            commands::lsp::start_lsp,
            commands::lsp::stop_lsp,
            commands::lsp::restart_lsp,
            commands::lsp::get_lsp_status,
            // Docs / settings
            commands::get_skript_docs,
            commands::read_settings,
            commands::write_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).with_ansi(true))
        .with(filter)
        .init();
}
