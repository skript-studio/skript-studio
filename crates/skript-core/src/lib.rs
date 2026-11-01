//! skript-core: Core library for Skript language tooling.
//!
//! Provides reusable logic for parsing Skript syntax patterns (as published by
//! SkriptHub), matching user code against those patterns, and producing
//! language-intelligence artifacts (completions, diagnostics, hover info,
//! semantic tokens).
//!
//! This crate is deliberately free of LSP concerns so it can be reused by a
//! CLI validator, tests, or any other embedding.

pub mod ast;
#[cfg(feature = "fetch")]
pub mod client;
pub mod completion;
pub mod hover;
pub mod lexer;
pub mod matching;
pub mod parser;
pub mod references;
pub mod semantic;
pub mod syntax;
pub mod tokens;
pub mod types;
pub mod validation;

pub use parser::parse_program;
pub use syntax::{Addon, Plugin, SyntaxEntry, SyntaxType};

pub use completion::{complete_functions, complete_keywords, complete_variables};
