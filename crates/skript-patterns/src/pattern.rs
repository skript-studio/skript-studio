//! Port of Java `ch.njol.skript.patterns.SkriptPattern`.
//!
//! OWNER: Agent 6 (same agent as keyword.rs).
//! Java source: /home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/SkriptPattern.java

use crate::elements::ElementRef;
use crate::keyword::Keyword;
use crate::match_result::{MatchResult, ParseFlags};
use crate::parse_context::ParseContext;

/// Properties controlling pattern stringification. Port of the Java nested
/// class `SkriptPattern.StringificationProperties`. Field set FROZEN (other
/// agents already reference these names); you may ADD fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StringificationProperties {
    pub exclude_optional_parts: bool,
    pub exclude_tags: bool,
    pub exclude_type_flags: bool,
    pub use_angle_brackets: bool,
    pub exclude_parse_tags: bool,
}

impl StringificationProperties {
    /// Java `StringificationProperties.DEFAULT` — VERIFY against the Java
    /// source and set the actual defaults it uses.
    pub const DEFAULT: StringificationProperties = StringificationProperties {
        exclude_optional_parts: false,
        exclude_tags: false,
        exclude_type_flags: false,
        use_angle_brackets: false,
        exclude_parse_tags: false,
    };
}

/// A compiled pattern. Constructed by [`crate::compiler::compile_pattern`].
#[derive(Debug)]
pub struct SkriptPattern {
    pub(crate) first: ElementRef,
    pub(crate) expression_amount: usize,
    pub(crate) keywords: Vec<Keyword>,
    pub(crate) min_length: usize,
}

impl SkriptPattern {
    /// Port of the Java constructor: builds keywords and min length from
    /// the element chain (`Keyword::build_keywords`, `Keyword::compute_min_length`).
    pub fn new(first: ElementRef, expression_amount: usize) -> Self {
        todo!("Agent 6: port SkriptPattern constructor")
    }

    /// Port of `SkriptPattern.match(String, int, ParseContext)`:
    /// lowercase prefilter (every keyword `isPresent`), min-length check,
    /// `trim()`, fresh `MatchResult` sized to `expression_amount`, then
    /// `first.borrow().match_(...)`.
    pub fn match_(
        &self,
        expr: &str,
        flags: ParseFlags,
        context: ParseContext,
    ) -> Option<MatchResult> {
        todo!("Agent 6: port SkriptPattern.match")
    }

    /// Java `countTypes()`.
    pub fn count_types(&self) -> usize {
        self.expression_amount
    }

    /// Java `countNonNullTypes()` — maximum number of non-null values in
    /// the expressions array from a match.
    pub fn count_non_null_types(&self) -> usize {
        todo!("Agent 6: port countNonNullTypes")
    }

    /// Java `toString(StringificationProperties)`.
    pub fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        todo!("Agent 6: port toString")
    }
}
