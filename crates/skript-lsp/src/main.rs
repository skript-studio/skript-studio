//! `skript-lsp` entry point.
//!
//! Two transport modes are supported:
//!
//! **WebSocket mode** (default, used by `--port`):
//!   - Bind `127.0.0.1:{--port}` (0 = ephemeral), print `LISTENING <port>`,
//!     and accept WebSocket-upgraded TCP connections. Each connection gets a
//!     `tower-lsp` session over the [`ws::WsStream`] framing adapter. This is
//!     the contract the Tauri sidecar launcher depends on.
//!
//! **stdio mode** (`--stdio`):
//!   - Read LSP messages from stdin and write responses to stdout using the
//!     standard `Content-Length` framing. This is the conventional transport
//!     used by editors like VSCode and Neovim.
//!
//! Shared startup:
//!   1. Parse CLI flags (clap).
//!   2. Install a `tracing` subscriber that writes to **stderr only**.
//!   3. Populate the initial state.
//!   4. Enter the selected transport loop.

mod backend;
mod folding;
mod options;
mod position;
mod state;
mod ws;

use crate::options::{CliOptions, EffectiveConfig};
use crate::state::AppState;
use clap::Parser;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = CliOptions::parse();
    init_tracing(cli.log_level.0);

    let config = EffectiveConfig::default();
    let state = AppState::new(config);

    if cli.r#stdio {
        serve_stdio(state).await
    } else {
        serve_websocket(state, cli.port).await
    }
}

/// Serve a single LSP session over stdio with standard Content-Length framing.
/// This is the conventional transport used by editors (VSCode, Neovim, etc.).
async fn serve_stdio(state: AppState) -> anyhow::Result<()> {
    tracing::info!("starting stdio LSP server");

    let (service, socket) =
        tower_lsp::LspService::new(|client| backend::Backend::new(client, state));

    tower_lsp::Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
    Ok(())
}

/// Serve LSP sessions over WebSocket on a TCP port.
async fn serve_websocket(state: AppState, port: u16) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let actual_port = listener
        .local_addr()
        .map(|addr| addr.port())
        .unwrap_or(port);

    announce_listening(actual_port);

    // Shared shutdown flag flipped by the `exit` notification (via the
    // Backend) — but the simplest robust approach is to let each session end
    // when its socket closes, and have the process exit when no session is
    // alive. For v1 we accept loop forever; the sidecar is hard-killed by the
    // launcher on shutdown (see the contract: hard kill is safe).
    let shutdown = Arc::new(AtomicBool::new(false));

    tracing::info!(port = actual_port, "skript-lsp listening on 127.0.0.1");

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!(error = %e, "accept failed; continuing");
                continue;
            }
        };
        tracing::info!(%peer, "incoming WebSocket connection");

        let state = state.clone();
        let session_shutdown = shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_connection(stream, state).await {
                tracing::warn!(%peer, error = %e, "session ended with error");
            } else {
                tracing::info!(%peer, "session ended cleanly");
            }
            // For v1, treat the first session's end as a request to exit
            // (single-client server, like a sidecar).
            session_shutdown.store(true, Ordering::Release);
        });

        // If any session signalled shutdown, stop accepting.
        if shutdown.load(Ordering::Acquire) {
            tracing::info!("shutdown signalled; exiting accept loop");
            break;
        }
    }

    Ok(())
}

/// Run one LSP session over a WebSocket-upgraded TCP stream.
async fn serve_connection(stream: tokio::net::TcpStream, state: AppState) -> anyhow::Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| anyhow::anyhow!("websocket upgrade failed: {e}"))?;
    let ws = ws::WsStream::new(ws_stream);

    let (service, socket) = tower_lsp::LspService::new(|client| {
        // Kick off the background fetch for this server instance if it hasn't
        // been started yet (idempotent — spawn_fetch itself just spawns).
        backend::Backend::new(client, state.clone())
    });

    // `tower-lsp::Server::new` takes separate read and write halves. Split the
    // combined `WsStream` with tokio's `io::split`, which coordinates the two
    // halves via a bi-lock.
    let (read, write) = tokio::io::split(ws);

    tower_lsp::Server::new(read, write, socket)
        .serve(service)
        .await;
    Ok(())
}

fn init_tracing(level: tracing::Level) {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level.as_str().to_lowercase()));

    // CRITICAL: logs go to stderr only. Any stdout output other than the
    // single LISTENING line corrupts the sidecar launcher's port discovery.
    // ANSI colors are disabled when stderr is piped (e.g., into VSCode's
    // output channel) to avoid raw escape codes in plain-text viewers.
    let stderr = std::io::stderr;
    fmt()
        .with_env_filter(filter)
        .with_writer(stderr)
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}

/// Print the single `LISTENING <port>` line to stdout and flush. This is the
/// only permitted stdout output.
fn announce_listening(port: u16) {
    // Detect piped/redirected stdout and flush explicitly; on Windows a
    // console-attached stdout is line-buffered but we flush anyway.
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "LISTENING {port}");
    let _ = handle.flush();
    drop(handle);
    // If stdout is a terminal (interactive dev use), also print a friendly
    // hint to stderr so the user isn't left wondering what to do next.
    if std::io::stdout().is_terminal() {
        eprintln!("skript-lsp: connect a client to ws://127.0.0.1:{port}");
    }
}
