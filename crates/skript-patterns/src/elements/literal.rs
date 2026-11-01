//! Port of Java `ch.njol.skript.patterns.LiteralPatternElement`.
//!
//! Java source:
//! /home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/LiteralPatternElement.java

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::elements::{ElementRef, PatternElement};
use crate::match_result::MatchResult;
use crate::pattern::StringificationProperties;

/// A literal string to match, e.g. `hello world`. Spaces are handled
/// leniently (see Java source). The literal is stored lowercased.
#[derive(Debug)]
pub struct LiteralPatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    /// Lowercased literal text (Java stores `char[]`).
    pub literal: String,
}

/// Java `Character.toLowerCase(char)` — single-char Unicode lowercase
/// mapping. Rust's `char::to_lowercase` may expand to multiple chars
/// (e.g. `'İ'` → `"i̇"`); per the porting contract we compare the first
/// char of the expansion.
fn lower_char(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

impl LiteralPatternElement {
    /// Constructor returning a shared handle (used by the compiler).
    ///
    /// Java: `literal.toLowerCase(Locale.ENGLISH).toCharArray()`. Rust's
    /// `str::to_lowercase` is locale-invariant, which equals the English
    /// locale for all practical purposes (differs only under a Turkish
    /// locale, which Java's explicit `Locale.ENGLISH` also rules out).
    pub fn new_rc(literal: impl Into<String>) -> ElementRef {
        let element = LiteralPatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            literal: literal.into().to_lowercase(),
        };
        Rc::new(RefCell::new(element))
    }

    /// The lowercased literal text.
    pub fn literal(&self) -> &str {
        &self.literal
    }

    pub fn is_empty(&self) -> bool {
        self.literal.is_empty()
    }
}

impl PatternElement for LiteralPatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // Port of LiteralPatternElement.match: iterate the (already
        // lowercased) literal chars against the input starting at
        // `result.expr_offset`, mutating a LOCAL index and only writing
        // `result.expr_offset` back after the whole literal was consumed —
        // exactly like the Java, which advances `exprIndex` locally and
        // assigns `matchResult.exprOffset` once at the end.
        //
        // DIVERGENCE: Java indexes its `char[]` by UTF-16 code unit while
        // the port-wide convention for `expr_offset` is a UTF-8 byte
        // offset; we walk Unicode scalar values (char boundaries), which
        // is identical for ASCII/BMP text (~all of Skript) and well-defined
        // for astral chars, where Java would compare surrogate halves.
        let mut expr_index = result.expr_offset;
        // Defensive: a non-char-boundary (or out-of-range) offset would
        // make slicing panic; Java's UTF-16 indexing cannot panic, so we
        // fail the match gracefully instead (LSP robustness).
        if expr_index > expr.len() || !expr.is_char_boundary(expr_index) {
            return false;
        }
        for c in self.literal.chars() {
            if c == ' ' {
                // Spaces have special handling to account for extraneous
                // spaces within lines.
                // Ignore patterns leading or ending with spaces (or if we
                // have multiple leading spaces).
                if expr_index == 0 || expr_index == expr.len() {
                    continue;
                }
                let current = expr[expr_index..].chars().next().unwrap();
                if current == ' ' {
                    // Pattern is ' fly' and we were given ' fly': consume
                    // the input space.
                    expr_index += 1;
                    continue;
                }
                let previous = expr[..expr_index].chars().next_back().unwrap();
                if previous == ' ' {
                    // Pattern is ' fly' but we were given something like
                    // '  fly' or 'fly': skip the pattern space.
                    continue;
                }
                return false;
            } else {
                // Java: exprIndex == exprChars.length || toLowerCase(c) !=
                // toLowerCase(exprChars[exprIndex]) -> null (bounds check
                // first, then case-insensitive comparison).
                if expr_index == expr.len() {
                    return false;
                }
                let current = expr[expr_index..].chars().next().unwrap();
                if lower_char(c) != lower_char(current) {
                    return false;
                }
                expr_index += current.len_utf8();
            }
        }

        result.expr_offset = expr_index;
        self.match_next(expr, result)
    }

    fn to_string_pretty(&self, _properties: &StringificationProperties) -> String {
        // Java: return new String(literal); (the lowercased text)
        self.literal.clone()
    }

    fn next(&self) -> Option<ElementRef> {
        self.next_field.borrow().clone()
    }

    fn original_next(&self) -> Option<ElementRef> {
        self.original_next_field.borrow().clone()
    }

    fn set_next_raw(&self, next: Option<ElementRef>) {
        *self.next_field.borrow_mut() = next;
    }

    fn set_original_next_raw(&self, next: Option<ElementRef>) {
        *self.original_next_field.borrow_mut() = next;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a match from `offset` and return the result state on success.
    fn match_at(element: &ElementRef, expr: &str, offset: usize) -> Option<MatchResult> {
        let mut result = MatchResult::new(0);
        result.expr_offset = offset;
        if element.borrow().match_(expr, &mut result) {
            Some(result)
        } else {
            None
        }
    }

    fn lit(s: &str) -> ElementRef {
        LiteralPatternElement::new_rc(s)
    }

    // ------------------------------------------------------------------
    // basic matching
    // ------------------------------------------------------------------

    #[test]
    fn exact_match_advances_offset() {
        let element = lit("detonate");
        let result = match_at(&element, "detonate", 0).expect("should match");
        assert_eq!(result.expr_offset, 8);
    }

    #[test]
    fn case_insensitive_match() {
        let element = lit("detonate");
        let result = match_at(&element, "DETONATE", 0).expect("should match");
        assert_eq!(result.expr_offset, 8);
        assert!(match_at(&element, "DeToNaTe", 0).is_some());
    }

    #[test]
    fn unicode_case_insensitive_and_byte_offsets() {
        // Non-ASCII: chars compare case-insensitively and the offset
        // advances by the UTF-8 byte length of the consumed char (the
        // Java would advance UTF-16 code units; the port uses bytes).
        let element = lit("\u{e4}\u{f6}\u{fc}"); // äöü
        let input = "\u{c4}\u{d6}\u{dc}"; // ÄÖÜ
        let result = match_at(&element, input, 0).expect("should match");
        assert_eq!(result.expr_offset, input.len()); // 6 bytes
    }

    #[test]
    fn empty_literal_succeeds_only_at_input_end() {
        // The loop never runs; the offset is written back unchanged and
        // matchNext decides (no next element => whole input consumed).
        let element = lit("");
        assert!(match_at(&element, "", 0).is_some());
        assert!(match_at(&element, "x", 0).is_none());
    }

    #[test]
    fn matches_at_nonzero_start_offset() {
        let element = lit("fly");
        let result = match_at(&element, "the fly", 4).expect("should match");
        assert_eq!(result.expr_offset, 7);
    }

    #[test]
    fn fails_mid_string_and_on_short_input() {
        let element = lit("detonate");
        assert!(match_at(&element, "detonatx", 0).is_none());
        assert!(match_at(&element, "detona", 0).is_none());
        // leftover input fails the match when there is no next element
        assert!(match_at(&element, "detonate all", 0).is_none());
    }

    // ------------------------------------------------------------------
    // the four space rules (exact Java ordering)
    // ------------------------------------------------------------------

    #[test]
    fn space_rule_1a_pattern_space_at_input_start_is_skipped() {
        // Rule 1: pattern space while exprIndex == 0 is skipped (NOT used
        // to consume an input space — Java checks `exprIndex == 0` first).
        // Real inputs are trimmed by SkriptPattern.match, so leading input
        // spaces never reach the engine at offset 0.
        let element = lit(" fly");
        let result = match_at(&element, "fly", 0).expect("should match");
        assert_eq!(result.expr_offset, 3);

        // Consequently, an untrimmed leading input space does NOT match:
        // the pattern space was already skipped and 'f' meets ' '.
        assert!(match_at(&element, " fly", 0).is_none());

        // "(or if we have multiple leading spaces)": while exprIndex is
        // still 0, every leading pattern space is skipped.
        let element = lit("  fly");
        let result = match_at(&element, "fly", 0).expect("should match");
        assert_eq!(result.expr_offset, 3);
    }

    #[test]
    fn space_rule_1b_pattern_space_at_input_end_is_skipped() {
        let element = lit("fly ");
        let result = match_at(&element, "fly", 0).expect("should match");
        assert_eq!(result.expr_offset, 3);

        // A trailing pattern space with more input left is NOT skipped
        // for free (falls through to rules 2-4).
        assert!(match_at(&element, "flyx", 0).is_none());
    }

    #[test]
    fn space_rule_2_input_char_is_space_is_consumed() {
        // Rule 2: the input char at the current offset is a space -> the
        // pattern space consumes it.
        let element = lit("the fly");
        let result = match_at(&element, "the fly", 0).expect("should match");
        assert_eq!(result.expr_offset, 7);
        assert!(match_at(&element, "THE FLY", 0).is_some());
        // leftover input (here a trailing input space) is NOT consumed by
        // anything — matchNext requires the whole input to be used up
        assert!(match_at(&element, "the fly ", 0).is_none());

        // a MISSING space is not tolerated mid-string
        assert!(match_at(&element, "thefly", 0).is_none());
    }

    #[test]
    fn space_rule_3_previous_input_char_is_space_skips_pattern_space() {
        // Rule 3: the input char is not a space, but the PREVIOUS input
        // char is -> skip the pattern space without advancing.
        let element = lit(" fly");
        // "a  fly": offset 3 points at 'f'; the previous char is a space.
        let result = match_at(&element, "a  fly", 3).expect("should match");
        assert_eq!(result.expr_offset, 6);
    }

    #[test]
    fn doubled_input_spaces_tolerated_across_element_chain() {
        // How rule 3 is reached in practice: a previous element consumed
        // the input spaces, so the next literal's pattern space is
        // skipped (Java comment: "given something like '  fly'").
        let first = lit("a  ");
        let second = lit(" fly");
        first.borrow().set_next(Some(second.clone()));
        let mut result = MatchResult::new(0);
        assert!(first.borrow().match_("a  fly", &mut result));
        assert_eq!(result.expr_offset, 6);
    }

    #[test]
    fn space_rule_4_no_nearby_space_fails() {
        // Rule 4: neither the current nor the previous input char is a
        // space -> the pattern space is required and missing.
        let element = lit("a b");
        assert!(match_at(&element, "axb", 0).is_none());
        assert!(match_at(&element, "a-b", 0).is_none());
    }

    // ------------------------------------------------------------------
    // chaining / stringification
    // ------------------------------------------------------------------

    #[test]
    fn chains_through_next_element() {
        let first = lit("the ");
        let second = lit("fly");
        first.borrow().set_next(Some(second.clone()));

        let mut result = MatchResult::new(0);
        assert!(first.borrow().match_("the fly", &mut result));
        assert_eq!(result.expr_offset, 7);

        // the chain must consume the WHOLE input
        let mut result = MatchResult::new(0);
        assert!(!first.borrow().match_("the fly x", &mut result));
    }

    #[test]
    fn non_ascii_chain_byte_offsets() {
        let first = lit("\u{e4} "); // ä + space
        let second = lit("\u{f6}"); // ö
        first.borrow().set_next(Some(second.clone()));
        let mut result = MatchResult::new(0);
        let input = "\u{e4} \u{f6}"; // "ä ö"
        assert!(first.borrow().match_(input, &mut result));
        assert_eq!(result.expr_offset, input.len()); // 5 bytes
    }

    #[test]
    fn to_string_returns_lowercased_literal() {
        let element = lit("HeLLo World");
        assert_eq!(
            element
                .borrow()
                .to_string_pretty(&StringificationProperties::DEFAULT),
            "hello world"
        );
    }
}
