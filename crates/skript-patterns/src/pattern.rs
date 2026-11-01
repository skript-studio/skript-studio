//! Port of Java `ch.njol.skript.patterns.SkriptPattern`.
//!
//! OWNER: Agent 6 (same agent as keyword.rs).
//! Java source: /home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/SkriptPattern.java

use std::fmt;

use crate::elements::{
    ChoicePatternElement, ElementRef, GroupPatternElement, OptionalPatternElement,
    TypePatternElement,
};
use crate::keyword::Keyword;
use crate::match_result::{MatchResult, ParseFlags};
use crate::parse_context::ParseContext;

/// Properties controlling pattern stringification. Port of the Java nested
/// class `SkriptPattern.StringificationProperties`. Field set FROZEN (other
/// agents already reference these names); fields may be ADDED but never
/// renamed or removed.
///
/// Java (v2.17.1) defines exactly TWO properties — `excludeParseTags()` and
/// `excludeTypeFlags()` — consulted by `ParseTagPatternElement.toString` and
/// `TypePatternElement.toString` respectively; the builder
/// (`StringificationPropertiesImpl.BuilderImpl`) defaults both to `false`.
/// The remaining three fields (`exclude_optional_parts`, `exclude_tags`,
/// `use_angle_brackets`) have NO Java counterpart in this version — they are
/// port-only extras kept for the frozen cross-crate contract, default to
/// `false`, and are never set by the faithful Java semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StringificationProperties {
    /// Port-only extra (no Java counterpart in v2.17.1). Default `false`.
    pub exclude_optional_parts: bool,
    /// Port-only extra (no Java counterpart in v2.17.1). Default `false`.
    pub exclude_tags: bool,
    /// Java `excludeTypeFlags()` — hide `%type%` flags (`-`, `*`, `~`,
    /// `@time`) when stringifying. Default `false`.
    pub exclude_type_flags: bool,
    /// Port-only extra (no Java counterpart in v2.17.1). Default `false`.
    pub use_angle_brackets: bool,
    /// Java `excludeParseTags()` — hide `:tag` / `¦mark` elements when
    /// stringifying. Default `false`.
    pub exclude_parse_tags: bool,
}

impl StringificationProperties {
    /// Java `StringificationProperties.DEFAULT = builder().build()` —
    /// every property `false` (verified against the BuilderImpl defaults:
    /// `excludeParseTags = false`, `excludeTypeFlags = false`).
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
    /// Port of the Java constructor: `keywords` and `minLength` are
    /// computed once, eagerly, from the element chain.
    pub fn new(first: ElementRef, expression_amount: usize) -> Self {
        let keywords = Keyword::build_keywords(&first);
        let min_length = Keyword::compute_min_length(&first);
        Self {
            first,
            expression_amount,
            keywords,
            min_length,
        }
    }

    /// The keyword prefilter (Java `SkriptPattern.keywords`).
    ///
    /// Patterns with NO keywords have no required literal text — every
    /// non-type element is optional, a regex, or a tag — so they can match
    /// almost any input (e.g. `<.+>`, `%booleans%`, `[on] [all] %javatype%`).
    /// Downstream consumers that need discriminative line matching
    /// (diagnostics) treat these as catch-alls and skip them; see
    /// `skript-checks`. This mirrors the `is_bare_type` heuristic the
    /// skript-core regex approximation uses for the same reason.
    pub fn keywords(&self) -> &[Keyword] {
        &self.keywords
    }

    /// Whether this pattern has no required literal text (no keywords) and
    /// can therefore match nearly any input. See [`SkriptPattern::keywords`].
    pub fn is_catch_all(&self) -> bool {
        self.keywords.is_empty()
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
        // Matching shortcut (Java lines 32-39).
        //
        // DIVERGENCE: Java lowercases with `expr.toLowerCase(Locale.ENGLISH)`;
        // Rust `str::to_lowercase` is locale-independent Unicode lowercase
        // (identical for the ASCII text Skript patterns are written in).
        let lower = expr.to_lowercase();
        // DIVERGENCE: Java compares `lowerExpr.length()` (UTF-16 code units)
        // against the int `minLength`; we compare `chars().count()` (Unicode
        // scalar values). Identical for BMP input; for supplementary-plane
        // characters Java counts 2 units per character and would reject the
        // input slightly later than we do.
        if lower.chars().count() < self.min_length {
            return None;
        }
        for keyword in &self.keywords {
            if !keyword.is_present(&lower) {
                return None;
            }
        }

        // Java: `expr = expr.trim();` — the trimmed string is what gets
        // matched. (DIVERGENCE: Java `trim()` strips chars <= U+0020; Rust
        // `trim()` strips Unicode whitespace.)
        let trimmed = expr.trim();

        // Java: new MatchResult() + source/expr/expressions/parseContext/
        // flags. DIVERGENCE: the port's MatchResult carries no back-reference
        // to the pattern and no `expr` field (see match_result.rs); the
        // expressions array is sized by `MatchResult::new`.
        let mut result = MatchResult::new(self.expression_amount);
        result.parse_context = context;
        result.flags = flags;

        let first = self.first.clone();
        if first.borrow().match_(trimmed, &mut result) {
            Some(result)
        } else {
            None
        }
    }

    /// Java `countTypes()`: the size of the `MatchResult::expressions`
    /// array from a match.
    pub fn count_types(&self) -> usize {
        self.expression_amount
    }

    /// Java `countNonNullTypes()` — the maximum number of non-null values
    /// in the expressions array from a match.
    pub fn count_non_null_types(&self) -> usize {
        Self::count_non_null_types_of(&self.first)
    }

    /// Port of the private recursive
    /// `countNonNullTypes(PatternElement patternElement)`.
    fn count_non_null_types_of(first: &ElementRef) -> usize {
        /// What one chain element contributes (extracted under a single
        /// short-lived borrow; recursion happens after it ends).
        enum Step {
            Nothing,
            /// Group / optional: recurse into the inner chain.
            Inner(ElementRef),
            /// Choice: recurse into every alternative, keep the max.
            Alternatives(Vec<ElementRef>),
        }

        let mut count = 0usize;
        let mut cursor = Some(first.clone());

        while let Some(el) = cursor {
            let step = {
                let borrowed = el.borrow();
                let any = borrowed.as_any();
                if let Some(choice_element) = any.downcast_ref::<ChoicePatternElement>() {
                    Step::Alternatives(choice_element.alternatives())
                } else if let Some(group_element) = any.downcast_ref::<GroupPatternElement>() {
                    // For groups and optionals, simply recurse.
                    Step::Inner(group_element.pattern_element())
                } else if let Some(optional_element) = any.downcast_ref::<OptionalPatternElement>() {
                    Step::Inner(optional_element.pattern_element())
                } else if any.downcast_ref::<TypePatternElement>().is_some() {
                    // Increment when seeing a type.
                    count += 1;
                    Step::Nothing
                } else {
                    // Literal, regex and parse-tag elements: nothing.
                    Step::Nothing
                }
            };

            match step {
                Step::Nothing => {}
                Step::Inner(inner) => {
                    count += Self::count_non_null_types_of(&inner);
                }
                Step::Alternatives(alternatives) => {
                    // Keep track of the max type count of each component:
                    // only one of the components will be used, so only the
                    // max needs adding (Java comment).
                    let mut max = 0usize;
                    for alternative in &alternatives {
                        let component_count = Self::count_non_null_types_of(alternative);
                        if component_count > max {
                            max = component_count;
                        }
                    }
                    count += max;
                }
            }

            // Move on to the next pattern element.
            cursor = el.borrow().original_next();
        }

        count
    }

    /// Java `toString(StringificationProperties)`:
    /// `first.toFullString(properties)` — the first element plus its whole
    /// `original_next` chain, stringified.
    pub fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        let first = self.first.clone();
        let borrowed = first.borrow();
        borrowed.to_full_string(properties)
    }
}

/// Java `SkriptPattern.toString()` — stringifies with the default
/// properties.
impl fmt::Display for SkriptPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_pretty(&StringificationProperties::DEFAULT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::{ChoicePatternElement, LiteralPatternElement, OptionalPatternElement};
    use crate::match_result::TypeSpec;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Link `a -> b` on the same nesting level the way the compiler's
    /// `appendElement` does for plain elements: sets BOTH `next` and
    /// `originalNext` without propagation. Tests build the compiler's
    /// post-propagation shape by hand so they never call other agents'
    /// in-flight `set_next` overrides.
    fn link(a: &ElementRef, b: ElementRef) {
        a.borrow().set_next_raw(Some(b.clone()));
        a.borrow().set_original_next_raw(Some(b));
    }

    /// Set only the `next` field — what `setLastNext` propagation leaves
    /// behind on choice alternatives / optional + group children.
    fn link_next_only(a: &ElementRef, b: ElementRef) {
        a.borrow().set_next_raw(Some(b));
    }

    /// Java `ChoicePatternElement.add(PatternElement)` is a plain list add.
    fn add_alternative(choice: &ElementRef, alternative: ElementRef) {
        let borrowed = choice.borrow();
        let choice_element = borrowed
            .as_any()
            .downcast_ref::<ChoicePatternElement>()
            .unwrap();
        choice_element.pattern_elements.borrow_mut().push(alternative);
    }

    /// A `%object%`-style type element. `TypePatternElement::from_string`
    /// is Agent 3's in-flight code, so tests build the struct directly
    /// (all fields are pub and the count walk only checks the TYPE).
    fn type_element(expression_index: usize) -> ElementRef {
        Rc::new(RefCell::new(TypePatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            classes: vec![TypeSpec {
                code_name: "object".to_string(),
                is_plural: false,
            }],
            is_nullable: false,
            flag_mask: !0,
            time: 0,
            expression_index,
        }))
    }

    #[test]
    fn match_prefilter_rejects_short_input() {
        // "detonate" — 8 mandatory characters, no further keywords.
        let lit = LiteralPatternElement::new_rc("detonate");
        let pattern = SkriptPattern::new(lit, 0);
        assert_eq!(pattern.count_types(), 0);

        // Rejected by the min-length prefilter BEFORE any element matching
        // happens (the element `match_` impls are other agents' in-flight
        // code — these prefilter tests must not reach them).
        assert!(pattern
            .match_("det", ParseFlags::ALL, ParseContext::Default)
            .is_none());
        assert!(pattern
            .match_("", ParseFlags::ALL, ParseContext::Default)
            .is_none());
    }

    #[test]
    fn match_prefilter_rejects_missing_keyword() {
        // "[the] name" — keyword "name" must be present (endsWith).
        let lit_the = LiteralPatternElement::new_rc("the");
        let optional = OptionalPatternElement::new_rc(lit_the.clone());
        let lit_name = LiteralPatternElement::new_rc(" name");
        link(&optional, lit_name.clone());
        link_next_only(&lit_the, lit_name.clone()); // setNext propagation

        let pattern = SkriptPattern::new(optional, 0);

        // Long enough (10 chars >= 4) but "the colour" does not end with
        // "name" — rejected by the keyword prefilter.
        assert!(pattern
            .match_("the colour", ParseFlags::ALL, ParseContext::Default)
            .is_none());
        // The min-length check runs on the LOWERCASED input.
        assert!(pattern
            .match_("THE", ParseFlags::ALL, ParseContext::Default)
            .is_none());
    }

    #[test]
    fn count_non_null_types_walks_original_next_chain() {
        // "give %object% to %object%" — 2 types in the top-level chain.
        let lit1 = LiteralPatternElement::new_rc("give ");
        let t1 = type_element(0);
        let lit2 = LiteralPatternElement::new_rc(" to ");
        let t2 = type_element(1);
        link(&lit1, t1.clone());
        link(&t1, lit2.clone());
        link(&lit2, t2.clone());

        let pattern = SkriptPattern::new(lit1, 2);
        assert_eq!(pattern.count_types(), 2);
        assert_eq!(pattern.count_non_null_types(), 2);
    }

    #[test]
    fn count_non_null_types_choice_takes_max() {
        // "(%object%|%object% and %object%)[the ]" — the choice contributes
        // max(1, 2) = 2; the trailing optional contributes 0.
        let alt1 = type_element(0);
        let alt2a = type_element(1);
        let alt2b = type_element(2);
        link(&alt2a, alt2b.clone());
        let choice = ChoicePatternElement::new_rc();
        add_alternative(&choice, alt1);
        add_alternative(&choice, alt2a);
        let group = GroupPatternElement::new_rc(choice);
        let optional = OptionalPatternElement::new_rc(LiteralPatternElement::new_rc("the "));
        link(&group, optional.clone());

        let pattern = SkriptPattern::new(group, 3);
        assert_eq!(pattern.count_non_null_types(), 2);
    }

    #[test]
    fn count_non_null_types_optional_still_counts() {
        // "[%object%]" — an optional-wrapped type still counts toward the
        // maximum (Java recurses into optionals).
        let optional = OptionalPatternElement::new_rc(type_element(0));
        let pattern = SkriptPattern::new(optional, 1);
        assert_eq!(pattern.count_non_null_types(), 1);
    }

    #[test]
    fn count_non_null_types_without_types_is_zero() {
        let lit = LiteralPatternElement::new_rc("just words");
        let pattern = SkriptPattern::new(lit, 0);
        assert_eq!(pattern.count_types(), 0);
        assert_eq!(pattern.count_non_null_types(), 0);
    }

    #[test]
    fn to_string_pretty_literal_chain() {
        // Java: first.toFullString(DEFAULT) — literals concatenate.
        let a = LiteralPatternElement::new_rc("hello ");
        let b = LiteralPatternElement::new_rc("world");
        link(&a, b.clone());
        let pattern = SkriptPattern::new(a, 0);
        assert_eq!(
            pattern.to_string_pretty(&StringificationProperties::DEFAULT),
            "hello world"
        );
        // Display mirrors Java toString() == toString(DEFAULT).
        assert_eq!(pattern.to_string(), "hello world");
    }

    #[test]
    fn stringification_default_matches_java_builder_defaults() {
        // Java: builder() starts with excludeParseTags = false and
        // excludeTypeFlags = false; DEFAULT = builder().build().
        assert!(!StringificationProperties::DEFAULT.exclude_parse_tags);
        assert!(!StringificationProperties::DEFAULT.exclude_type_flags);
        // Port-only extras (no v2.17.1 Java counterpart) also default to
        // false so the DEFAULT is a plain `Default::default()`.
        assert_eq!(StringificationProperties::DEFAULT, StringificationProperties::default());
    }

    // ------------------------------------------------------------------
    // End-to-end tests — gated on the compiler (Agent 5) and the element
    // `match_` impls (Agents 1-4). Run with
    // `cargo test -p skript-patterns --lib -- --ignored` after the whole
    // engine is integrated.
    // ------------------------------------------------------------------

    #[test]
    fn end_to_end_match_the_name() {
        let pattern = crate::compile_pattern("[the] name").unwrap();
        assert_eq!(pattern.count_types(), 0);
        assert_eq!(pattern.count_non_null_types(), 0);
        // Stringification round-trips.
        assert_eq!(pattern.to_string(), "[the] name");

        let result = pattern
            .match_("the name", ParseFlags::ALL, ParseContext::Default)
            .expect("'the name' matches '[the] name'");
        assert_eq!(result.expressions.len(), 0);
        assert_eq!(result.tags.len(), 0);

        // The optional part can be skipped entirely.
        assert!(pattern
            .match_("name", ParseFlags::ALL, ParseContext::Default)
            .is_some());
        // The keyword prefilter PASSES ("the nickname" ends with "name"),
        // but the literal match must reject it.
        assert!(pattern
            .match_("the nickname", ParseFlags::ALL, ParseContext::Default)
            .is_none());
        // Prefilter rejects: no "name" keyword at all.
        assert!(pattern
            .match_("the colour", ParseFlags::ALL, ParseContext::Default)
            .is_none());
    }

    #[test]
    fn end_to_end_match_choice_and_type() {
        let pattern = crate::compile_pattern("give %object% (a|an) apple").unwrap();
        assert_eq!(pattern.count_types(), 1);
        assert_eq!(pattern.count_non_null_types(), 1);

        let result = pattern
            .match_("give stick an apple", ParseFlags::ALL, ParseContext::Default)
            .expect("'give stick an apple' matches");
        assert_eq!(result.expressions.len(), 1);
        assert!(result.expressions[0].is_some());

        assert!(pattern
            .match_("give stick a apple", ParseFlags::ALL, ParseContext::Default)
            .is_some());
        // Keyword prefilter: "give" is a required prefix.
        assert!(pattern
            .match_("take stick an apple", ParseFlags::ALL, ParseContext::Default)
            .is_none());
    }
}
