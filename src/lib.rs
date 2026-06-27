//! skript-core: Core library for Skript language tooling.
//!
//! Provides reusable logic for parsing Skript syntax patterns (as published by
//! SkriptHub), matching user code against those patterns, and producing
//! language-intelligence artifacts (completions, diagnostics, hover info,
//! semantic tokens).
//!
//! This crate is deliberately free of LSP concerns so it can be reused by a
//! CLI validator, tests, or any other embedding.

#[cfg(feature = "fetch")]
pub mod client;
pub mod completion;
pub mod default_syntax;
pub mod hover;
pub mod matching;
pub mod pattern;
pub mod syntax;
pub mod tokens;
pub mod types;
pub mod validation;

pub use syntax::{Addon, Plugin, SyntaxEntry, SyntaxType};
