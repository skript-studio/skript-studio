//! Port of Skript's pattern-matching engine
//! (`ch.njol.skript.patterns`, Skript v2.17.1) for static analysis.
//!
//! See `PORTING_CONTRACTS.md` at the workspace root for the frozen API.

pub mod compiler;
pub mod elements;
pub mod error;
pub mod keyword;
pub mod match_result;
pub mod parse_context;
pub mod parse_utils;
pub mod pattern;
pub mod resolver;

pub use compiler::compile_pattern;
pub use error::MalformedPatternError;
pub use match_result::{MatchResult, ParseFlags, RegexMatchInfo, SlotMatch, TypeSpec};
pub use parse_context::ParseContext;
pub use pattern::{SkriptPattern, StringificationProperties};
pub use resolver::{set_type_resolver, with_type_resolver, PermissiveResolver, TypeResolver};
