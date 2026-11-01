use std::collections::HashMap;

use crate::parse_context::ParseContext;

/// Port of the parse flags from Java `SkriptParser`
/// (`PARSE_EXPRESSIONS = 1`, `PARSE_LITERALS = 2`, `ALL_FLAGS = 3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParseFlags(pub u32);

impl ParseFlags {
    pub const EXPRESSIONS: ParseFlags = ParseFlags(1);
    pub const LITERALS: ParseFlags = ParseFlags(2);
    pub const ALL: ParseFlags = ParseFlags(3);

    pub fn contains(&self, other: ParseFlags) -> bool {
        self.0 & other.0 == other.0
    }

    /// Bitwise-and, used to compute effective flags through `TypePatternElement.flag_mask`.
    pub fn and_mask(&self, mask: u32) -> ParseFlags {
        ParseFlags(self.0 & mask)
    }
}

/// What a `%type%` slot consumed when a [`crate::resolver::TypeResolver`]
/// accepted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotMatch {
    /// The raw substring the resolver accepted.
    pub text: String,
    /// The type code names the slot was declared with (e.g. `["entities"]`).
    pub type_names: Vec<String>,
}

/// A type reference inside a `%type%` slot, e.g. `entities` in
/// `detonate %entities%`. Java resolves these to `ClassInfo` objects at
/// compile time; the static port keeps the raw code name (DIVERGENCE:
/// unknown type names are tolerated instead of throwing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSpec {
    /// Singular type code name, as produced by `Utils.isPleral(...).updated()`.
    pub code_name: String,
    /// Whether the pattern spelled the type in the plural.
    pub is_plural: bool,
}

/// Recorded result of one `<regex>` element match (Java stores
/// `java.util.regex.MatchResult`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexMatchInfo {
    /// The whole match (group 0).
    pub text: String,
    /// Capture groups; index 0 corresponds to regex group 1.
    pub groups: Vec<Option<String>>,
}

/// Port of Java `ch.njol.skript.patterns.MatchResult`.
///
/// DIVERGENCE: Java keeps a back-reference to the source `SkriptPattern`
/// and the full `expr` string; the port drops the source reference (it
/// would be circular) and passes `expr` by parameter instead.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub expr_offset: usize,
    /// Sized to `SkriptPattern.expression_amount`; `None` = slot not filled.
    pub expressions: Vec<Option<SlotMatch>>,
    pub mark: usize,
    pub tags: Vec<String>,
    /// Java inserts successful regex matches at the FRONT (`addFirst`).
    pub regex_results: Vec<RegexMatchInfo>,
    pub parse_context: ParseContext,
    pub flags: ParseFlags,
}

impl MatchResult {
    pub fn new(expression_amount: usize) -> Self {
        Self {
            expr_offset: 0,
            expressions: vec![None; expression_amount],
            mark: 0,
            tags: Vec::new(),
            regex_results: Vec::new(),
            parse_context: ParseContext::Default,
            flags: ParseFlags::ALL,
        }
    }
}

impl Default for MatchResult {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Extra metadata the checks crate attaches to a match (unused by the
/// engine itself, kept here so downstream code has one type to carry).
#[derive(Debug, Clone, Default)]
pub struct MatchExtras {
    pub tags_set: HashMap<String, ()>,
}
