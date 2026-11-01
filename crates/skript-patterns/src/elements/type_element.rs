//! Port of Java `ch.njol.skript.patterns.TypePatternElement`.
//!
//! OWNER: Agent 3. This is the hardest element: `%type%` slots with the
//! offset-iteration driven by the next literal element, plus the
//! TypeResolver seam replacing Java's recursive expression parser.
//!
//! Java source:
//! /home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/TypePatternElement.java

use std::any::Any;
use std::cell::RefCell;

use crate::elements::{ElementRef, LiteralPatternElement, PatternElement};
use crate::match_result::{MatchResult, ParseFlags, TypeSpec};
use crate::parse_context::ParseContext;
use crate::parse_utils;
use crate::pattern::StringificationProperties;
use crate::resolver::with_type_resolver;

/// A `%type%` slot, e.g. `%number%`, `%-entities%`, `%entity/location%`,
/// `%objects%@-1%` (time state).
#[derive(Debug)]
pub struct TypePatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    /// `/`-separated type alternatives, plural-resolved (Java: ClassInfo[]).
    pub classes: Vec<TypeSpec>,
    /// Leading `-` in the pattern.
    pub is_nullable: bool,
    /// `!0` bits survive; `*` clears PARSE_EXPRESSIONS, `~` clears PARSE_LITERALS.
    pub flag_mask: u32,
    /// Trailing `@n` (time state).
    pub time: i32,
    /// Which slot of `MatchResult::expressions` this element fills.
    pub expression_index: usize,
}

impl TypePatternElement {
    /// Port of Java `TypePatternElement.fromString(String, int)`.
    ///
    /// DIVERGENCE: Java resolves type names through `Classes.getClassInfo`
    /// (throws on unknown); the static port keeps the raw code name and
    /// tolerates unknowns. Use `parse_utils::is_plural` for plural
    /// detection (it returns the singularized string + plural flag).
    pub fn from_string(string: &str, expression_index: usize) -> Self {
        // ---- leading flags loop (Java lines 46-59) ----
        let mut caret = 0usize;
        let mut flag_mask: u32 = !0; // Java: ~0
        let mut is_nullable = false;
        {
            let bytes = string.as_bytes(); // '-', '*', '~' are ASCII
            loop {
                // DIVERGENCE: Java's string.charAt(caret) throws
                // StringIndexOutOfBoundsException when the slot consists
                // only of flags (e.g. "%-%"); we treat end-of-string as
                // terminating the flag loop instead of panicking.
                let Some(&c) = bytes.get(caret) else { break };
                match c {
                    b'-' => is_nullable = true,
                    b'*' => flag_mask &= !ParseFlags::EXPRESSIONS.0,
                    b'~' => flag_mask &= !ParseFlags::LITERALS.0,
                    _ => break,
                }
                caret += 1;
            }
        }

        // ---- optional trailing @<int> time (Java lines 61-68) ----
        let mut time: i32 = 0;
        // Java: string.indexOf('@', caret) — '@' is ASCII, byte scan is safe.
        let at_index = string.as_bytes()[caret..]
            .iter()
            .position(|&b| b == b'@')
            .map(|i| i + caret);
        let type_part: &str = match at_index {
            Some(at) => {
                // DIVERGENCE: Java's Integer.parseInt(string.substring(at + 1))
                // throws NumberFormatException (crashing pattern compilation)
                // for a malformed time; we default to 0 and carry on.
                time = string[at + 1..].parse().unwrap_or(0);
                // DIVERGENCE: Java keeps string.substring(0, timeStart) as
                // the type part, which RETAINS the leading flags when a slot
                // has both flags and a time (e.g. "%-number@-1%" would yield
                // the type name "-number" — the flags are never stripped in
                // that branch). That is clearly unintended; we strip the
                // flags as well. (No corpus pattern combines the two, and
                // the SkriptHub dump contains no @time slots at all.)
                &string[caret..at]
            }
            None => &string[caret..],
        };

        // ---- '/'-separated type names, each run through Utils.isPlural ----
        // Java: ClassInfo[i] = Classes.getClassInfo(p.updated()); isPlural[i] = p.plural();
        // DIVERGENCE (mandated): unknown type names are tolerated — the
        // static corpus contains addon types we do not know.
        let classes: Vec<TypeSpec> = java_split(type_part)
            .into_iter()
            .map(|name| {
                let (updated, plural) = parse_utils::is_plural(&name);
                TypeSpec {
                    code_name: updated,
                    is_plural: plural,
                }
            })
            .collect();

        TypePatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            classes,
            is_nullable,
            flag_mask,
            time,
            expression_index,
        }
    }

    /// Java `ExprInfo getExprInfo()` equivalent for the resolver seam.
    pub fn expr_specs(&self) -> &[TypeSpec] {
        &self.classes
    }

    // -- Offset computation (Java lines 221-276) --

    /// Port of Java `initOffset(String, MatchResult, OffsetState)`:
    /// computes the initial expression offset based on the next pattern
    /// element.
    fn init_offset(
        &self,
        expr: &str,
        result: &MatchResult,
        state: &mut OffsetState,
    ) -> Option<usize> {
        let Some(next) = self.next() else {
            return Some(expr.len());
        };

        // Java: if (!(next instanceof LiteralPatternElement)) { ... }
        let literal_text = {
            let borrowed = next.borrow();
            borrowed
                .as_any()
                .downcast_ref::<LiteralPatternElement>()
                .map(|literal| literal.literal().to_string())
        };
        let Some(mut next_literal) = literal_text else {
            state.next_literal = None;
            return parse_utils::next(expr, result.expr_offset, result.parse_context);
        };

        state.next_literal_is_whitespace = next_literal.trim().is_empty();

        if !state.next_literal_is_whitespace {
            // Don't do this for literal patterns that are *only* whitespace —
            // they have their own special handling.
            // Trim trailing whitespace — it can cause issues with optional
            // patterns following the literal.
            // (Java: String.stripTrailing(); DIVERGENCE: Java's strip uses
            // Character.isWhitespace while Rust's trim_end uses Unicode
            // White_Space — identical for the ASCII patterns Skript uses.)
            next_literal = next_literal.trim_end().to_string();
        }

        state.next_literal = Some(next_literal.clone());

        let offset = parse_utils::next_occurrence(
            expr,
            &next_literal,
            result.expr_offset,
            result.parse_context,
            false,
        );
        if offset.is_none() && state.next_literal_is_whitespace {
            // We need to tread more carefully here.
            // This may be because the next PatternElement is optional or an
            // empty choice (there may be other cases too).
            state.next_literal = None;
            return parse_utils::next(expr, result.expr_offset, result.parse_context);
        }
        offset
    }

    /// Port of Java `advanceOffset(String, int, ParseContext, OffsetState)`:
    /// advances the expression offset to the next candidate split point.
    fn advance_offset(
        &self,
        expr: &str,
        current_offset: usize,
        parse_context: ParseContext,
        state: &mut OffsetState,
    ) -> Option<usize> {
        let Some(next_literal) = state.next_literal.clone() else {
            return parse_utils::next(expr, current_offset, parse_context);
        };

        // Java: nextOccurrence(expr, nextLiteral, currentOffset + 1, ...).
        // DIVERGENCE: Java advances one UTF-16 code unit; we advance to the
        // next UTF-8 char boundary (identical for ASCII input).
        let search_start = parse_utils::plus_one_char(expr, current_offset);
        let new_offset = parse_utils::next_occurrence(
            expr,
            &next_literal,
            search_start,
            parse_context,
            false,
        );
        if new_offset.is_none() && state.next_literal_is_whitespace {
            // This may be because the next PatternElement is optional or an
            // empty choice (there may be other cases too).
            // So, from this point on, we're going to go character by character.
            state.next_literal = None;
            return parse_utils::next(expr, current_offset, parse_context);
        }
        new_offset
    }
}

/// Mutable state for a single match() invocation's offset iteration.
/// Kept separate from the element so the element is thread-safe.
/// (Java: `TypePatternElement.OffsetState`.)
struct OffsetState {
    /// String value of the next literal element (for finding the right
    /// boundary). `None` once we fall back to character-by-character
    /// iteration (Java: `nextLiteral == null`).
    next_literal: Option<String>,
    /// Whether that next literal is just whitespace.
    next_literal_is_whitespace: bool,
}

/// Java `String.split("/")` semantics: split on `/`, then drop ALL trailing
/// empty parts (Java's split with limit 0). "" has no separator, so it
/// stays [""], while "/" becomes [] — exactly like the Java regex split.
fn java_split(s: &str) -> Vec<String> {
    let mut parts: Vec<String> = s.split('/').map(str::to_string).collect();
    if parts.len() == 1 {
        // no separator found — Java returns the whole string as one part
        return parts;
    }
    while parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts
}

impl PatternElement for TypePatternElement {
    /// Port of Java `TypePatternElement.match(String, MatchResult)`.
    ///
    /// DIVERGENCE (mandated): the Java implementation additionally
    /// - consults the runtime parse caches (`ExpressionParseCache` /
    ///   `LiteralParseCache`) to skip substrings that already failed,
    /// - juggles `ParseLogHandler`s (loop/expr logs, backups and printing),
    /// - validates time states via `applyTimeState` (including
    ///   `ParserInstance.getHasDelayBefore()`), and
    /// - keeps a backup of the first match containing "unparsed literals"
    ///   in case no cleaner match exists (`hasUnparsedLiterals`).
    /// None of that is meaningful for static analysis (no live expression
    /// parser, no runtime state): the TypeResolver seam replaces
    /// `new SkriptParser(substring, effectiveFlags, context)
    /// .parseExpression(exprInfo)`. Note that in Java `isNullable` and
    /// `time` only flow into `ExprInfo` (the expression parser and default
    /// expression machinery); they do not change the offset iteration, so
    /// they are compile-time data here.
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        let mut state = OffsetState {
            next_literal: None,
            next_literal_is_whitespace: false,
        };
        let mut expr_offset = match self.init_offset(expr, result, &mut state) {
            Some(offset) => offset,
            None => return false,
        };

        loop {
            // Defensive UTF-8 guard: every offset produced by parse_utils is
            // a char boundary, but never slice unsafely if one somehow
            // isn't (Java's UTF-16 indexing has no such hazard).
            if expr.is_char_boundary(expr_offset) && expr_offset >= result.expr_offset {
                let substring = &expr[result.expr_offset..expr_offset];
                // Java: int effectiveFlags = matchResult.flags & flagMask;
                let effective_flags = result.flags.and_mask(self.flag_mask);

                // Match rest of pattern to determine our range to work in.
                // (Java: MatchResult copy = matchResult.copy(); copy.exprOffset =
                // exprOffset; MatchResult tailMatch = matchNext(expr, copy);)
                let mut copy = result.clone();
                copy.expr_offset = expr_offset;
                if self.match_next(expr, &mut copy) {
                    // Actually attempt to parse the substring.
                    // (Java: new SkriptParser(substring, effectiveFlags,
                    //  matchResult.parseContext).parseExpression(exprInfo);)
                    let accepted = with_type_resolver(|resolver| {
                        resolver.parse_expression(
                            substring,
                            &self.classes,
                            effective_flags,
                            result.parse_context,
                        )
                    });
                    if let Some(slot) = accepted {
                        // Java: tailMatch.expressions[expressionIndex] = expression;
                        // DIVERGENCE: Java would throw
                        // ArrayIndexOutOfBoundsException on a miscompiled
                        // pattern; we grow the vec instead of panicking.
                        if self.expression_index >= copy.expressions.len() {
                            copy.expressions.resize(self.expression_index + 1, None);
                        }
                        copy.expressions[self.expression_index] = Some(slot);
                        // write the winning state back into *result
                        *result = copy;
                        return true;
                    }
                }
            }

            // Advance the expression offset to the next candidate split point.
            expr_offset = match self.advance_offset(expr, expr_offset, result.parse_context, &mut state) {
                Some(offset) => offset,
                None => return false,
            };
        }
    }

    /// Port of Java `TypePatternElement.toString(StringificationProperties)`.
    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        let mut out = String::from("%");
        if !properties.exclude_type_flags {
            if self.is_nullable {
                out.push('-');
            }
            if self.flag_mask != !0u32 {
                if self.flag_mask & ParseFlags::LITERALS.0 == 0 {
                    out.push('~');
                } else if self.flag_mask & ParseFlags::EXPRESSIONS.0 == 0 {
                    out.push('*');
                }
            }
        }
        for (i, class) in self.classes.iter().enumerate() {
            if i != 0 {
                out.push('/');
            }
            if class.is_plural {
                // Java: Utils.toEnglishPlural(classes[i].getCodeName())
                out.push_str(&parse_utils::to_english_plural(&class.code_name));
            } else {
                out.push_str(&class.code_name);
            }
        }
        if !properties.exclude_type_flags && self.time != 0 {
            out.push('@');
            out.push_str(&self.time.to_string());
        }
        out.push('%');
        out
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
    use std::any::Any;
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::elements::LiteralPatternElement;
    use crate::match_result::SlotMatch;
    use crate::resolver::{PermissiveResolver, TypeResolver, set_type_resolver};

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    fn type_rc(spec: &str, expression_index: usize) -> ElementRef {
        Rc::new(RefCell::new(TypePatternElement::from_string(
            spec,
            expression_index,
        )))
    }

    /// Mimics what Java `PatternCompiler.appendElement` does when a
    /// TypePatternElement is followed by another element: the virtual
    /// `setNext` (plain write for Type) plus the direct `originalNext` write.
    fn link(first: &ElementRef, next: ElementRef) {
        first.borrow().set_next_raw(Some(next.clone()));
        first.borrow().set_original_next_raw(Some(next));
    }

    /// A minimal terminal tail element (NOT a LiteralPatternElement, so
    /// `init_offset` takes the `parse_utils::next` path). Succeeds only when
    /// the remaining input is exactly `expected`, then consumes it all.
    /// Lets us test the offset iteration without depending on
    /// LiteralPatternElement's own match port.
    #[derive(Debug)]
    struct TailExpect {
        next_field: RefCell<Option<ElementRef>>,
        original_next_field: RefCell<Option<ElementRef>>,
        expected: String,
    }

    impl PatternElement for TailExpect {
        fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
            if expr[result.expr_offset..] == self.expected {
                result.expr_offset = expr.len();
                self.match_next(expr, result)
            } else {
                false
            }
        }

        fn to_string_pretty(&self, _properties: &StringificationProperties) -> String {
            format!("#{}", self.expected)
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

    fn tail_expect(expected: &str) -> ElementRef {
        Rc::new(RefCell::new(TailExpect {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            expected: expected.to_string(),
        }))
    }

    /// A tail element that always succeeds without consuming anything —
    /// used to let a preceding literal's boundaries ALL reach the resolver.
    #[derive(Debug)]
    struct TailAny {
        next_field: RefCell<Option<ElementRef>>,
        original_next_field: RefCell<Option<ElementRef>>,
    }

    impl PatternElement for TailAny {
        fn match_(&self, _expr: &str, _result: &mut MatchResult) -> bool {
            true
        }

        fn to_string_pretty(&self, _properties: &StringificationProperties) -> String {
            "#any".to_string()
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

    fn tail_any() -> ElementRef {
        Rc::new(RefCell::new(TailAny {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
        }))
    }

    /// A resolver that records every call and only accepts one exact string
    /// (PermissiveResolver accepts everything non-blank).
    #[derive(Debug, Default)]
    struct RecordingResolver {
        calls: RefCell<Vec<(String, ParseFlags, ParseContext)>>,
        accept_only: String,
    }

    impl TypeResolver for RecordingResolver {
        fn parse_expression(
            &self,
            substring: &str,
            specs: &[TypeSpec],
            effective_flags: ParseFlags,
            context: ParseContext,
        ) -> Option<SlotMatch> {
            self.calls
                .borrow_mut()
                .push((substring.to_string(), effective_flags, context));
            if substring == self.accept_only {
                Some(SlotMatch {
                    text: substring.to_string(),
                    type_names: specs.iter().map(|s| s.code_name.clone()).collect(),
                })
            } else {
                None
            }
        }
    }

    // ------------------------------------------------------------------
    // from_string
    // ------------------------------------------------------------------

    #[test]
    fn from_string_plain() {
        let el = TypePatternElement::from_string("number", 0);
        assert!(!el.is_nullable);
        assert_eq!(el.flag_mask, !0u32);
        assert_eq!(el.time, 0);
        assert_eq!(el.expression_index, 0);
        assert_eq!(el.classes.len(), 1);
        assert_eq!(el.classes[0].code_name, "number");
        assert!(!el.classes[0].is_plural);
    }

    #[test]
    fn from_string_flags() {
        let el = TypePatternElement::from_string("-number", 3);
        assert!(el.is_nullable);
        assert_eq!(el.flag_mask, !0u32);
        assert_eq!(el.expression_index, 3);

        let el = TypePatternElement::from_string("*number", 0);
        // '*' clears PARSE_EXPRESSIONS (bit 1)
        assert_eq!(el.flag_mask, !0u32 & !ParseFlags::EXPRESSIONS.0);

        let el = TypePatternElement::from_string("~number", 0);
        // '~' clears PARSE_LITERALS (bit 2)
        assert_eq!(el.flag_mask, !0u32 & !ParseFlags::LITERALS.0);

        // combined flags, in any order/repetition
        let el = TypePatternElement::from_string("-*~entities", 1);
        assert!(el.is_nullable);
        assert_eq!(el.flag_mask, !0u32 & !1 & !2);
        assert_eq!(el.classes[0].code_name, "entity");
        assert!(el.classes[0].is_plural);
    }

    #[test]
    fn from_string_time() {
        let el = TypePatternElement::from_string("objects@-1", 0);
        assert_eq!(el.time, -1);
        assert_eq!(el.classes.len(), 1);
        assert_eq!(el.classes[0].code_name, "object");
        assert!(el.classes[0].is_plural);

        let el = TypePatternElement::from_string("number@2", 0);
        assert_eq!(el.time, 2);
        assert_eq!(el.classes[0].code_name, "number");

        // DIVERGENCE from Java: Java keeps substring(0, timeStart) and would
        // end up with the type name "-number" here; we strip the flags.
        let el = TypePatternElement::from_string("-number@-1", 0);
        assert!(el.is_nullable);
        assert_eq!(el.time, -1);
        assert_eq!(el.classes[0].code_name, "number");

        // malformed time: Java would throw NumberFormatException; we keep 0
        let el = TypePatternElement::from_string("number@soon", 0);
        assert_eq!(el.time, 0);
        assert_eq!(el.classes[0].code_name, "number");

        // empty time: Java would throw; we keep 0
        let el = TypePatternElement::from_string("number@", 0);
        assert_eq!(el.time, 0);
        assert_eq!(el.classes[0].code_name, "number");
    }

    #[test]
    fn from_string_multiple_types_and_plurals() {
        let el = TypePatternElement::from_string("entity/location", 0);
        assert_eq!(el.classes.len(), 2);
        assert_eq!(el.classes[0], TypeSpec { code_name: "entity".into(), is_plural: false });
        assert_eq!(el.classes[1], TypeSpec { code_name: "location".into(), is_plural: false });

        let el = TypePatternElement::from_string("entities/locations", 0);
        assert_eq!(el.classes[0], TypeSpec { code_name: "entity".into(), is_plural: true });
        assert_eq!(el.classes[1], TypeSpec { code_name: "location".into(), is_plural: true });

        let el = TypePatternElement::from_string("itemtypes", 0);
        assert_eq!(el.classes[0].code_name, "itemtype");
        assert!(el.classes[0].is_plural);

        // mixed flag + multi-type + time
        let el = TypePatternElement::from_string("~entity/location@1", 0);
        assert_eq!(el.time, 1);
        assert_eq!(el.classes.len(), 2);
        assert_eq!(el.flag_mask, !0u32 & !ParseFlags::LITERALS.0);
    }

    #[test]
    fn from_string_unknown_types_tolerated() {
        // DIVERGENCE (mandated): Java's Classes.getClassInfo would throw for
        // unregistered names; addon types in the static corpus are kept raw.
        let el = TypePatternElement::from_string("resident/disguise", 0);
        assert_eq!(el.classes[0].code_name, "resident");
        assert_eq!(el.classes[1].code_name, "disguise");
    }

    #[test]
    fn from_string_java_split_semantics() {
        // Java split("/") drops trailing empty parts
        let el = TypePatternElement::from_string("number/", 0);
        assert_eq!(el.classes.len(), 1);
        // internal empties are kept
        let el = TypePatternElement::from_string("a//b", 0);
        assert_eq!(el.classes.len(), 3);
        assert_eq!(el.classes[1].code_name, "");
    }

    // ------------------------------------------------------------------
    // to_string_pretty
    // ------------------------------------------------------------------

    #[test]
    fn to_string_pretty_cases() {
        let cases: &[(&str, &str)] = &[
            ("number", "%number%"),
            ("entities", "%entities%"),
            ("-number", "%-number%"),
            ("*number", "%*number%"),
            ("~number", "%~number%"),
            ("number@-1", "%number@-1%"),
            ("number@2", "%number@2%"),
            ("entity/location", "%entity/location%"),
            ("entities/locations", "%entities/locations%"),
            ("-entities/items", "%-entities/items%"),
            ("~entity/location@1", "%~entity/location@1%"),
        ];
        for (spec, expected) in cases {
            let el = TypePatternElement::from_string(spec, 0);
            assert_eq!(&el.to_string_pretty(&StringificationProperties::DEFAULT), expected, "spec {spec}");
        }
    }

    #[test]
    fn to_string_pretty_exclude_type_flags() {
        let el = TypePatternElement::from_string("-entities@-1", 0);
        let props = StringificationProperties {
            exclude_type_flags: true,
            ..StringificationProperties::DEFAULT
        };
        // flags and time hidden, plural form still shown
        assert_eq!(el.to_string_pretty(&props), "%entities%");
    }

    // ------------------------------------------------------------------
    // match_
    // ------------------------------------------------------------------

    #[test]
    fn match_no_tail_consumes_everything() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // pattern "%number%" — no next element: the whole input is the slot
        let el = type_rc("number", 0);
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12", &mut result));
        assert_eq!(result.expr_offset, 2);
        let slot = result.expressions[0].as_ref().expect("slot filled");
        assert_eq!(slot.text, "12");
        assert_eq!(slot.type_names, vec!["number".to_string()]);
    }

    #[test]
    fn match_no_tail_rejects_empty_and_blank() {
        set_type_resolver(Rc::new(PermissiveResolver));
        let el = type_rc("number", 0);
        let mut result = MatchResult::new(1);
        assert!(!el.borrow().match_("", &mut result));
        // whitespace-only: tail candidates exist but the resolver (like
        // Java's parseExpression on a blank) refuses them
        let mut result = MatchResult::new(1);
        assert!(!el.borrow().match_("   ", &mut result));
    }

    #[test]
    fn match_effective_flags_reach_the_resolver() {
        // '*' clears PARSE_EXPRESSIONS, so the seam must see only
        // PARSE_LITERALS surviving the flag mask
        let resolver = Rc::new(RecordingResolver {
            calls: RefCell::new(Vec::new()),
            accept_only: "12".to_string(),
        });
        set_type_resolver(resolver.clone());
        let el = type_rc("*number", 0);
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12", &mut result));
        let calls = resolver.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "12");
        assert_eq!(calls[0].1, ParseFlags(2)); // LITERALS only
        assert_eq!(calls[0].2, ParseContext::Default);
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "12");
    }

    #[test]
    fn match_rejected_substring_advances_then_fails() {
        // The resolver only accepts "12"; with no tail the single candidate
        // is the whole input "123" — rejected, the offset advances past the
        // end of input and the match fails.
        let resolver = Rc::new(RecordingResolver {
            calls: RefCell::new(Vec::new()),
            accept_only: "12".to_string(),
        });
        set_type_resolver(resolver.clone());
        let el = type_rc("number", 0);
        let mut result = MatchResult::new(1);
        assert!(!el.borrow().match_("123", &mut result));
        let calls = resolver.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "123");
    }

    #[test]
    fn match_offset_iteration_with_plain_tail() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // Type followed by a NON-literal element: initOffset uses
        // parse_utils::next (one char at a time), the tail must be " 34".
        let el = type_rc("number", 0);
        link(&el, tail_expect(" 34"));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12 34", &mut result));
        // "1" and "12 " fail the tail; "12" is the winner
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "12");
        assert_eq!(result.expr_offset, 5);
    }

    #[test]
    fn match_failing_tail_forces_offset_advancement() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // no offset produces a valid tail -> overall failure
        let el = type_rc("number", 0);
        link(&el, tail_expect(" zzz"));
        let mut result = MatchResult::new(1);
        assert!(!el.borrow().match_("12 34", &mut result));

        // and a tail that only matches at a later boundary: the earlier
        // candidates are skipped without polluting the result
        let el = type_rc("number", 0);
        link(&el, tail_expect("4"));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("1234", &mut result));
        // candidates: "1","12","123","1234" — only "123" leaves exactly "4"
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "123");
        assert_eq!(result.expr_offset, 4);
    }

    #[test]
    fn match_copy_isolation_on_failure() {
        // A failed match must not write anything back into the result.
        set_type_resolver(Rc::new(PermissiveResolver));
        let el = type_rc("number", 0);
        link(&el, tail_expect(" zzz"));
        let mut result = MatchResult::new(1);
        let snapshot = result.clone();
        assert!(!el.borrow().match_("12", &mut result));
        // On failure the contents are unspecified per the contract, but our
        // implementation leaves the result untouched.
        assert_eq!(result.expr_offset, snapshot.expr_offset);
        assert!(result.expressions[0].is_none());
    }

    // ------------------------------------------------------------------
    // match_ with a real LiteralPatternElement tail
    // (exercises initOffset's nextOccurrence path incl. stripTrailing and
    //  the whitespace-literal fallback)
    // ------------------------------------------------------------------

    #[test]
    fn match_literal_tail_not_found_fails_without_fallback() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // "%number% apples" vs "12": " apples" never occurs, and because the
        // literal is not whitespace-only there is no char-by-char fallback.
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" apples"));
        let mut result = MatchResult::new(1);
        assert!(!el.borrow().match_("12", &mut result));
    }

    #[test]
    fn match_literal_tail_happy_path() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // "%number% apples" matching "12 apples"
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" apples"));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12 apples", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "12");
        assert_eq!(result.expr_offset, 9);
    }

    #[test]
    fn match_literal_tail_case_insensitive_boundary_search() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // nextOccurrence searches case-insensitively (Java passes false)
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" apples"));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12 APPLES", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "12");
    }

    #[test]
    fn match_literal_tail_advances_past_failed_first_boundary() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // "1 apples 2 apples": the first " apples" boundary leaves
        // " 2 apples" unconsumed (literal has no next -> fails), so the
        // offset advances to the second occurrence.
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" apples"));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("1 apples 2 apples", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "1 apples 2");
        assert_eq!(result.expr_offset, 17);
    }

    #[test]
    fn match_literal_tail_trailing_input_fails() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // the literal matches once but "extra" remains -> no second
        // occurrence -> failure (a failing tail forcing advancement)
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" apples"));
        let mut result = MatchResult::new(1);
        assert!(!el.borrow().match_("12 apples extra", &mut result));
    }

    #[test]
    fn match_literal_tail_skips_quoted_needle_occurrences() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // an " apples" inside quotes must not be a boundary
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" apples"));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("\" apples\" 5 apples", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "\" apples\" 5");
    }

    #[test]
    fn match_strip_trailing_detail() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // literal " apples " with a trailing space: initOffset strips the
        // trailing space for the boundary SEARCH, while the literal's own
        // lenient space handling copes with the input ending early.
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" apples "));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12 apples", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "12");
    }

    #[test]
    fn match_whitespace_literal_fallback() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // "%number% " (whitespace-only literal): nextOccurrence(" ") finds
        // the space at 2, but the tail fails; advanceOffset falls back to
        // character-by-character iteration and the whitespace literal's own
        // lenient matching accepts the end of input.
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" "));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12 3", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "12 3");
        assert_eq!(result.expr_offset, 4);
    }

    #[test]
    fn match_whitespace_literal_fallback_in_init() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // no whitespace at all in the input: initOffset's nextOccurrence
        // fails and immediately falls back to parse_utils::next
        let el = type_rc("number", 0);
        link(&el, LiteralPatternElement::new_rc(" "));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "12");
        assert_eq!(result.expr_offset, 2);
    }

    // ------------------------------------------------------------------
    // nullable + multi-type behaviour
    // ------------------------------------------------------------------

    #[test]
    fn match_nullable_type_still_matches() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // Java: isNullable only flows into ExprInfo (the expression parser
        // / default-expression machinery) — it never blocks matching here.
        let el = type_rc("-number", 0);
        link(&el, LiteralPatternElement::new_rc(" apples"));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("12 apples", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "12");
    }

    #[test]
    fn match_nullable_type_does_not_match_empty() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // Java: parseExpression returns null for an empty substring, so even
        // a nullable slot cannot consume nothing.
        let el = type_rc("-number", 0);
        let mut result = MatchResult::new(1);
        assert!(!el.borrow().match_("", &mut result));
    }

    #[test]
    fn match_multi_type_slot() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // "%entity/location%" — the slot records both type alternatives
        let el = type_rc("entity/location", 0);
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("somewhere", &mut result));
        let slot = result.expressions[0].as_ref().unwrap();
        assert_eq!(slot.text, "somewhere");
        assert_eq!(slot.type_names, vec!["entity".to_string(), "location".to_string()]);
    }

    #[test]
    fn match_expression_index_targets_its_own_slot() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // second %...% slot of a chain: expression_index 1
        let el = type_rc("number", 1);
        let mut result = MatchResult::new(2);
        assert!(el.borrow().match_("42", &mut result));
        assert!(result.expressions[0].is_none());
        assert_eq!(result.expressions[1].as_ref().unwrap().text, "42");
    }

    #[test]
    fn match_resolver_rejection_advances_to_next_boundary() {
        // Type -> literal " apples" -> always-succeeding tail: the tail
        // succeeds at every " apples" boundary, so a rejecting resolver
        // forces the offset forward through each of them.
        let resolver = Rc::new(RecordingResolver {
            calls: RefCell::new(Vec::new()),
            accept_only: "zzz".to_string(),
        });
        set_type_resolver(resolver.clone());
        let el = type_rc("number", 0);
        let literal = LiteralPatternElement::new_rc(" apples");
        link(&el, literal.clone());
        link(&literal, tail_any());
        let mut result = MatchResult::new(1);
        assert!(!el.borrow().match_("a apples b apples", &mut result));
        let calls = resolver.calls.borrow();
        let texts: Vec<&str> = calls.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(texts, vec!["a", "a apples b"]);

        // accepting the later candidate wins with that longer substring
        let resolver = Rc::new(RecordingResolver {
            calls: RefCell::new(Vec::new()),
            accept_only: "a apples b".to_string(),
        });
        set_type_resolver(resolver.clone());
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("a apples b apples", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "a apples b");
    }

    #[test]
    fn match_quoted_needle_region_is_atomic() {
        set_type_resolver(Rc::new(PermissiveResolver));
        // "%string% and pears" vs `say "apples and pears" and pears`:
        // the " and pears" INSIDE the quotes must not be a boundary, so the
        // slot swallows the whole quoted string.
        let el = type_rc("string", 0);
        link(&el, LiteralPatternElement::new_rc(" and pears"));
        let mut result = MatchResult::new(1);
        assert!(el.borrow().match_("say \"apples and pears\" and pears", &mut result));
        assert_eq!(result.expressions[0].as_ref().unwrap().text, "say \"apples and pears\"");
        assert_eq!(result.expr_offset, 32);
    }
}
