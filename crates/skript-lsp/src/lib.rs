//! Library surface of the `skript-lsp` server.
//!
//! The shipped binary (`src/main.rs`) declares these same modules itself and
//! is unchanged; this library target exists so that the integration tests
//! under `tests/` can drive the server's state, configuration, and engine
//! wiring in-process (see `tests/java_engine_integration.rs`).
//!
//! NOTE (Agent 10): adding this file is a small deviation from the assigned
//! file list — it was required to make `tests/java_engine_integration.rs`
//! importable (`skript_lsp::state::AppState`, …). The binary keeps its own
//! module declarations, so runtime behavior is untouched; the only side
//! effect is that the crate's unit tests now also run against this library
//! target during `cargo test`.

pub mod backend;
pub mod folding;
pub mod options;
pub mod state;
pub mod ws;
