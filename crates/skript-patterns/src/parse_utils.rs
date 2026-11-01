//! Parse helpers ported from `SkriptParser` and `Utils`.
//!
//! OWNER: Agent 3 (same agent as type_element.rs — these exist to serve
//! `TypePatternElement` and `RegexPatternElement`).

use crate::parse_context::ParseContext;

/// Port of `SkriptParser.next(String, int, ParseContext)` (SkriptParser.java:1449).
/// Returns the next index (after `start`) at which an expression may end,
/// honoring quotes and brackets per the parse context. Java returns `-1`
/// for "end of input" — here `Some(expr.len())` means the end is a valid
/// boundary and `None` means no further boundary exists. PORT THE EXACT
/// SEMANTICS FROM THE JAVA SOURCE.
pub(crate) fn next(expr: &str, start: usize, context: ParseContext) -> Option<usize> {
    todo!("Agent 3: port SkriptParser.next")
}

/// Port of `SkriptParser.nextOccurrence(String, String, int, ParseContext, boolean)`
/// (SkriptParser.java:1495). Finds the next occurrence of `needle` in
/// `haystack` at or after `start_index`, case-insensitively unless
/// `case_sensitive`. `Some(expr.len())` = end boundary, `None` = not found.
pub(crate) fn next_occurrence(
    haystack: &str,
    needle: &str,
    start: usize,
    context: ParseContext,
    case_sensitive: bool,
) -> Option<usize> {
    todo!("Agent 3: port SkriptParser.nextOccurrence")
}

/// Port of `Utils.isPlural(String)` (Utils.java:272) returning
/// `(updated_word, is_plural)` — Java's `PluralResult`.
pub fn is_plural(word: &str) -> (String, bool) {
    todo!("Agent 3: port Utils.isPlural")
}

/// Port of `Utils.toEnglishPlural(String)` (Utils.java:344).
pub fn to_english_plural(word: &str) -> String {
    todo!("Agent 3: port Utils.toEnglishPlural")
}
