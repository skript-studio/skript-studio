//! Port of Java `ch.njol.skript.patterns.RegexPatternElement`.
//!
//! OWNER: Agent 4.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::elements::{ElementRef, PatternElement};
use crate::error::MalformedPatternError;
use crate::match_result::{MatchResult, RegexMatchInfo};
use crate::parse_utils;
use crate::pattern::StringificationProperties;

/// A regex, e.g. `<.+>`. Compiled with `fancy_regex` (Java uses
/// `java.util.regex`; DIVERGENCE: fancy-regex covers lookarounds and
/// backreferences, the common Java features used in Skript patterns).
#[derive(Debug)]
pub struct RegexPatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    /// The regex source as written in the pattern (for toString).
    pub source: String,
    /// The compiled regex. `fancy_regex::Regex` implements Debug.
    pub pattern: fancy_regex::Regex,
}

impl RegexPatternElement {
    /// Compiles the regex source; errors become `MalformedPatternError`
    /// (Java: `PatternSyntaxException` wrapped in MalformedPatternException
    /// by the compiler — here the constructor itself reports it).
    pub fn new_rc(source: &str) -> Result<ElementRef, MalformedPatternError> {
        let pattern = fancy_regex::Regex::new(source).map_err(|e| {
            MalformedPatternError::new(source, format!("invalid regex <{source}>: {e}"))
        })?;
        let element = RegexPatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            source: source.to_string(),
            pattern,
        };
        Ok(Rc::new(RefCell::new(element)))
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Port of Java `matcher.region(exprIndex, nextExprOffset).matches()`:
    /// the regex must match the ENTIRE slice. Returns the recorded match
    /// info on a whole-slice match, `None` otherwise.
    ///
    /// DIVERGENCE (Java regex vs fancy-regex): fancy-regex has no region
    /// API, so we run `captures` on the slice and require the returned
    /// match to span all of it (start == 0 && end == slice.len()). Two gaps
    /// versus Java's anchored region `matches()`:
    ///  * Java backtracks across top-level alternatives to find a
    ///    whole-region match (e.g. `a|ab` fully matches "ab"), while
    ///    leftmost-first `captures` would return the "a" alternative and we
    ///    would reject it. Skript's `<...>` regexes (`.+`, `\w+`, ...) do
    ///    not rely on that.
    ///  * fancy-regex reports runtime failures (e.g. its backtracking
    ///    limit) as `Err`; Java has no such limit. We treat `Err` as "no
    ///    match" instead of panicking.
    fn full_match_info(&self, slice: &str) -> Option<RegexMatchInfo> {
        let caps = match self.pattern.captures(slice) {
            Ok(Some(caps)) => caps,
            // No match, or fancy-regex gave up (see DIVERGENCE above).
            Ok(None) | Err(_) => return None,
        };
        let whole = caps.get(0)?;
        if whole.start() != 0 || whole.end() != slice.len() {
            return None;
        }
        // fancy-regex group 0 is the whole match; `RegexMatchInfo.groups`
        // maps regex groups 1..n (index 0 = regex group 1). Groups that did
        // not participate stay `None` (Java: null group).
        let groups = (1..caps.len())
            .map(|i| caps.get(i).map(|m| m.as_str().to_string()))
            .collect();
        Some(RegexMatchInfo {
            text: slice.to_string(),
            groups,
        })
    }
}

impl PatternElement for RegexPatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        let expr_index = result.expr_offset;

        // DIVERGENCE: Java wraps this loop in a ParseLogHandler (log.clear()
        // per candidate, printLog() on success, printError() on failure);
        // parse-log handlers are not ported for the static LSP.

        // Java walks UTF-16 indices; we slice by byte offset. Guard the
        // bounds so slicing can never panic even if an offset ever lands
        // inside a multi-byte character.
        if expr_index > expr.len() || !expr.is_char_boundary(expr_index) {
            return false;
        }

        let mut next_offset = parse_utils::next(expr, expr_index, result.parse_context);
        while let Some(offset) = next_offset {
            // Skip candidates we cannot slice (Java's UTF-16 regions always
            // form valid slice bounds there); iteration continues normally.
            if offset >= expr_index && offset <= expr.len() && expr.is_char_boundary(offset) {
                let slice = &expr[expr_index..offset];
                if let Some(info) = self.full_match_info(slice) {
                    let mut copy = result.clone();
                    copy.expr_offset = offset;
                    if self.match_next(expr, &mut copy) {
                        // Java: "at this point, all other regex results will
                        // have been added thus, we append first to insert at
                        // the correct position" — i.e. addFirst: results of
                        // elements further down the chain are already in the
                        // list, so this element's result goes to index 0.
                        copy.regex_results.insert(0, info);
                        *result = copy;
                        return true;
                    }
                }
            }
            next_offset = parse_utils::next(expr, offset, result.parse_context);
        }
        false
    }

    fn to_string_pretty(&self, _properties: &StringificationProperties) -> String {
        // Java: "<" + pattern + ">", and java.util.regex.Pattern.toString()
        // is the regex source string.
        format!("<{}>", self.source)
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

    /// Builds the concrete element (bypassing `new_rc`'s `ElementRef`
    /// handle) so the private `full_match_info` helper can be exercised
    /// without going through `match_` (and thus without `parse_utils`).
    fn element(source: &str) -> RegexPatternElement {
        RegexPatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            source: source.to_string(),
            pattern: fancy_regex::Regex::new(source).unwrap(),
        }
    }

    // ==================================================================
    // These tests do NOT depend on `parse_utils` (Agent 3) — they run now.
    // ==================================================================

    #[test]
    fn full_match_requires_entire_slice() {
        // Java: matcher.region(a, b).matches() — the whole slice must be
        // consumed by the match.
        let el = element(".+");
        assert_eq!(el.full_match_info("hello").unwrap().text, "hello");
        assert!(el.full_match_info("").is_none()); // `.+` needs >= 1 char

        let el = element("ab");
        assert_eq!(el.full_match_info("ab").unwrap().text, "ab");

        // An empty regex matches the empty slice whole (0..0).
        let el = element("");
        let info = el.full_match_info("").unwrap();
        assert_eq!(info.text, "");
        assert!(info.groups.is_empty());

        // A match that does not START at the beginning is rejected...
        let el = element("b+");
        assert!(el.full_match_info("ab").is_none()); // finds "b" at 1..2
        let el = element("ell");
        assert!(el.full_match_info("hello").is_none()); // 1..4
        let el = element("l+");
        assert!(el.full_match_info("hello").is_none()); // "ll" at 2..4

        // ...and so is one that starts at 0 but ends early.
        let el = element("h");
        assert!(el.full_match_info("hello").is_none()); // spans 0..1 only
        let el = element("hel");
        assert!(el.full_match_info("hello").is_none()); // spans 0..3 only
    }

    #[test]
    fn full_match_extracts_capture_groups() {
        // fancy-regex group 0 (whole match) becomes `text`; groups 1..n
        // become `groups` (index 0 = regex group 1).
        let el = element(r"(\w+)-(\d+)");
        let info = el.full_match_info("abc-123").unwrap();
        assert_eq!(info.text, "abc-123");
        assert_eq!(
            info.groups,
            vec![Some("abc".to_string()), Some("123".to_string())]
        );

        // Unmatched optional groups stay None (Java: null group).
        let el = element("a(b)?c");
        assert_eq!(el.full_match_info("ac").unwrap().groups, vec![None]);
        assert_eq!(
            el.full_match_info("abc").unwrap().groups,
            vec![Some("b".to_string())]
        );

        // No capture groups at all -> empty vec.
        let el = element(".+");
        assert_eq!(
            el.full_match_info("x").unwrap().groups,
            Vec::<Option<String>>::new()
        );
    }

    #[test]
    fn to_string_pretty_wraps_source_in_angle_brackets() {
        // Java: "<" + pattern + ">" where Pattern.toString() = the source.
        let el = element(".+");
        assert_eq!(
            el.to_string_pretty(&StringificationProperties::DEFAULT),
            "<.+>"
        );
        let el = element(r"\w+");
        assert_eq!(
            el.to_string_pretty(&StringificationProperties::DEFAULT),
            r"<\w+>"
        );
    }

    #[test]
    fn new_rc_rejects_invalid_regex() {
        let err = RegexPatternElement::new_rc("[").unwrap_err();
        assert!(err.message.contains("invalid regex"), "{err:?}");
    }

    #[test]
    fn new_rc_compiles_and_exposes_source() {
        let el = RegexPatternElement::new_rc(".+").unwrap();
        let borrowed = el.borrow();
        let regex_el = borrowed
            .as_any()
            .downcast_ref::<RegexPatternElement>()
            .unwrap();
        assert_eq!(regex_el.source(), ".+");
        // fancy-regex's is_match returns Result; use the whole-slice helper.
        assert!(regex_el.full_match_info("anything").is_some());
    }

    // ==================================================================
    // End-to-end tests of `match_` below. Candidate offsets come from
    // `parse_utils::next` (Agent 3, ported in parallel; its stub
    // `todo!()`s), so they are `#[ignore]`d — run with
    // `cargo test -p skript-patterns -- --ignored` once Agent 3 lands.
    //
    // They assume the faithful `next` semantics for ParseContext::Default
    // on plain ASCII input (no quotes/braces/parens): the candidate offsets
    // are start+1, start+2, ..., expr.len().
    // ==================================================================

    #[test]
    #[ignore = "gated on parse_utils"]
    fn match_requires_full_input_consumption() {
        // `<.+>` full-matches EVERY candidate prefix ("h", "he", ...); only
        // the candidate whose offset lets match_next finish (offset ==
        // expr.len(), no next element) wins — the failing tail forces the
        // loop to continue past earlier regex successes.
        let el = RegexPatternElement::new_rc(".+").unwrap();
        let mut result = MatchResult::new(0);
        assert!(el.borrow().match_("hello world", &mut result));
        assert_eq!(result.expr_offset, "hello world".len());
        assert_eq!(result.regex_results.len(), 1);
        assert_eq!(result.regex_results[0].text, "hello world");
        assert!(result.regex_results[0].groups.is_empty());
    }

    #[test]
    #[ignore = "gated on parse_utils"]
    fn match_fails_when_tail_cannot_consume_rest() {
        // The regex full-matches the slice "hello" (candidate offset 5),
        // but with no next element match_next requires expr_offset ==
        // expr.len(); every candidate fails, so the element fails.
        let el = RegexPatternElement::new_rc("hello").unwrap();
        let mut result = MatchResult::new(0);
        assert!(!el.borrow().match_("hello world", &mut result));
    }

    #[test]
    #[ignore = "gated on parse_utils"]
    fn match_rejects_unanchored_substring_match() {
        // `l+` finds "ll" inside "hello" but never spans a whole candidate
        // slice, so no candidate wins.
        let el = RegexPatternElement::new_rc("l+").unwrap();
        let mut result = MatchResult::new(0);
        assert!(!el.borrow().match_("hello", &mut result));
    }

    #[test]
    #[ignore = "gated on parse_utils"]
    fn match_respects_nonzero_expr_offset() {
        let el = RegexPatternElement::new_rc(".+").unwrap();
        let mut result = MatchResult::new(0);
        result.expr_offset = 2;
        assert!(el.borrow().match_("hello", &mut result));
        assert_eq!(result.expr_offset, 5);
        assert_eq!(result.regex_results[0].text, "llo");
    }

    #[test]
    #[ignore = "gated on parse_utils"]
    fn failing_tail_forces_loop_to_continue() {
        // `h.+` full-matches as early as offset 2 ("he"), but the tail only
        // completes at offset 5; the element must keep iterating instead of
        // committing to the first regex hit.
        let el = RegexPatternElement::new_rc("h.+").unwrap();
        let mut result = MatchResult::new(0);
        assert!(el.borrow().match_("hello", &mut result));
        assert_eq!(result.expr_offset, 5);
        assert_eq!(result.regex_results[0].text, "hello");
    }

    #[test]
    #[ignore = "gated on parse_utils"]
    fn regex_results_are_inserted_at_the_front_add_first() {
        // Chain <a+><b+><c+> and match "aabbcc". Java comment: "at this
        // point, all other regex results will have been added thus, we
        // append first to insert at the correct position" — the deeper
        // element's result is inserted first (in time), then this element
        // PREPENDS its own, so regex_results ends up in element order: the
        // temporally-second insert ("aa", by <a+>) sits at index 0.
        let r1 = RegexPatternElement::new_rc("a+").unwrap();
        let r2 = RegexPatternElement::new_rc("b+").unwrap();
        let r3 = RegexPatternElement::new_rc("c+").unwrap();
        r1.borrow().set_next(Some(r2.clone()));
        r2.borrow().set_next(Some(r3.clone()));
        let mut result = MatchResult::new(0);
        assert!(r1.borrow().match_("aabbcc", &mut result));
        assert_eq!(result.expr_offset, 6);
        assert_eq!(result.regex_results.len(), 3);
        assert_eq!(result.regex_results[0].text, "aa"); // <a+>, added last
        assert_eq!(result.regex_results[1].text, "bb"); // <b+>
        assert_eq!(result.regex_results[2].text, "cc"); // <c+>, added first
    }

    #[test]
    #[ignore = "gated on parse_utils"]
    fn match_extracts_capture_groups_end_to_end() {
        let el = RegexPatternElement::new_rc("a(b)?c").unwrap();
        let mut result = MatchResult::new(0);
        assert!(el.borrow().match_("abc", &mut result));
        assert_eq!(result.regex_results[0].text, "abc");
        assert_eq!(result.regex_results[0].groups, vec![Some("b".to_string())]);

        let mut result = MatchResult::new(0);
        assert!(el.borrow().match_("ac", &mut result));
        assert_eq!(result.regex_results[0].text, "ac");
        assert_eq!(result.regex_results[0].groups, vec![None]);
    }
}
