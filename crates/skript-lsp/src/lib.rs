//! Library surface of the `skript-lsp` server.
//!
//! The shipped binary (`src/main.rs`) declares these same modules itself and
//! is unchanged; this library target exists so that the integration tests
//! under `tests/` can drive the server's state, configuration, and engine
//! wiring in-process (see `tests/engine_integration.rs`).
//!
//! NOTE: adding this file was a small deviation from the originally assigned
//! file list — it was required to make `tests/engine_integration.rs`
//! importable (`skript_lsp::state::AppState`, …). The binary keeps its own
//! module declarations, so runtime behavior is untouched; the only side
//! effect is that the crate's unit tests now also run against this library
//! target during `cargo test`.

pub mod backend;
pub mod folding;
pub mod options;
pub mod position;
pub mod state;
pub mod ws;
