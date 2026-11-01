//! LSP sidecar process management.
//!
//! Owns the `skript-lsp` sidecar process: spawns it on demand with the
//! agreed CLI flags (`--cache-dir`, `--log-level`, `--skripthub-url`),
//! discovers the WebSocket port it's listening on by parsing
//! `LISTENING <port>` from stdout, monitors its health, and restarts
//! it on crash if requested by the frontend.

pub mod manager;

pub use manager::{set_app_handle, LspManager, LspSpawnOptions, LspStatus};
