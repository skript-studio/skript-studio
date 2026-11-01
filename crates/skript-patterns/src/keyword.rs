//! Port of Java `ch.njol.skript.patterns.Keyword`.
//!
//! OWNER: Agent 6 (same agent as pattern.rs).
//! Java source: /home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/Keyword.java

use crate::elements::{
    ChoicePatternElement, ElementRef, GroupPatternElement, LiteralPatternElement,
    ParseTagPatternElement,
};

/// A required component of a pattern, used as a cheap prefilter before the
/// full match (see `SkriptPattern.match`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyword {
    /// Java `SimpleKeyword` — requires a specific string to be present.
    Simple {
        keyword: String,
        starting: bool,
        ending: bool,
    },
    /// Java `ChoiceKeyword` — at least one alternative's keyword set must
    /// be fully present.
    ///
    /// DIVERGENCE: Java collects the alternative keyword sets in a
    /// `Set<Set<Keyword>>` (deduplicated, unordered); the port keeps a
    /// `Vec<Vec<Keyword>>` in alternative order. `is_present` semantics
    /// (`any` alternative / `all` of its keywords) are unaffected.
    Choice { choices: Vec<Vec<Keyword>> },
}

/// What one element of an `original_next`-chain walk contributes. All data
/// is extracted (cloned) under a single short-lived borrow so the recursive
/// calls below never hold a `RefCell` borrow (see PORTING_CONTRACTS.md,
/// "RefCell discipline").
enum ChainStep {
    /// Nothing to recurse into (the element either already contributed
    /// inline, or contributes nothing at all).
    Nothing,
    /// A group: recurse into the inner element chain.
    Group(ElementRef),
    /// A choice: recurse into each alternative chain.
    Choice(Vec<ElementRef>),
}

impl Keyword {
    /// Java `Keyword.isPresent(String)` — dispatches on the keyword kind.
    pub fn is_present(&self, expr: &str) -> bool {
        match self {
            Keyword::Simple {
                keyword,
                starting,
                ending,
            } => {
                if *starting {
                    expr.starts_with(keyword.as_str())
                } else if *ending {
                    expr.ends_with(keyword.as_str())
                } else {
                    expr.contains(keyword.as_str())
                }
            }
            Keyword::Choice { choices } => choices
                .iter()
                .any(|keywords| keywords.iter().all(|k| k.is_present(expr))),
        }
    }

    /// Port of `Keyword.buildKeywords(PatternElement)` — walks the
    /// `original_next` chain. Choice recursion only at depth <= 1; groups
    /// recurse with depth + 1; ParseTag elements do not affect `starting`.
    pub fn build_keywords(first: &ElementRef) -> Vec<Keyword> {
        Self::build_keywords_inner(first, /* starting = */ true, /* depth = */ 0)
    }

    /// Port of the private
    /// `buildKeywords(PatternElement first, boolean starting, int depth)`.
    fn build_keywords_inner(first: &ElementRef, starting: bool, depth: u32) -> Vec<Keyword> {
        let mut keywords = Vec::new();
        let mut starting = starting;
        let mut cursor = Some(first.clone());

        while let Some(el) = cursor {
            let step = {
                let borrowed = el.borrow();
                let any = borrowed.as_any();
                if let Some(literal_element) = any.downcast_ref::<LiteralPatternElement>() {
                    // Java:
                    //   String literal = next.toString().trim();
                    //   while (literal.contains("  "))
                    //       literal = literal.replace("  ", " ");
                    //   if (!literal.isEmpty()) // empty string is not useful
                    //       keywords.add(new SimpleKeyword(literal, starting, next.next == null));
                    //
                    // DIVERGENCE: Java `String.trim()` strips chars <= U+0020;
                    // Rust `str::trim()` strips Unicode whitespace (equivalent
                    // for the near-ASCII text patterns consist of).
                    let mut literal = literal_element.literal().trim().to_string();
                    while literal.contains("  ") {
                        literal = literal.replace("  ", " ");
                    }
                    if !literal.is_empty() {
                        keywords.push(Keyword::Simple {
                            keyword: literal,
                            starting,
                            // Java checks the `next` FIELD here (not
                            // `originalNext`): the keyword must end the input
                            // when nothing is matched after this element.
                            ending: borrowed.next().is_none(),
                        });
                    }
                    ChainStep::Nothing
                } else if let Some(choice_element) = any.downcast_ref::<ChoicePatternElement>() {
                    // Java `case ChoicePatternElement ... when depth <= 1`:
                    // a deeper choice falls through to `default` and
                    // contributes NOTHING (not even its alternatives'
                    // literals).
                    if depth <= 1 {
                        ChainStep::Choice(choice_element.alternatives())
                    } else {
                        ChainStep::Nothing
                    }
                } else if let Some(group_element) = any.downcast_ref::<GroupPatternElement>() {
                    ChainStep::Group(group_element.pattern_element())
                } else {
                    // OptionalPatternElement, TypePatternElement,
                    // RegexPatternElement, ParseTagPatternElement:
                    // do not contribute keywords.
                    ChainStep::Nothing
                }
            };

            match step {
                ChainStep::Nothing => {}
                ChainStep::Group(inner) => {
                    // Java: Collections.addAll(keywords, buildKeywords(
                    //     groupPatternElement.getPatternElement(), starting, depth + 1));
                    // The group's inner elements see the CURRENT `starting`.
                    keywords.extend(Self::build_keywords_inner(&inner, starting, depth + 1));
                }
                ChainStep::Choice(alternatives) => {
                    // Java captures finalStarting/finalDepth and passes the
                    // SAME starting and depth to every alternative (NOT
                    // depth + 1 — a choice directly nested in an alternative
                    // would recurse the same way Java does).
                    let mut choices = Vec::with_capacity(alternatives.len());
                    for alternative in &alternatives {
                        choices.push(Self::build_keywords_inner(alternative, starting, depth));
                    }
                    // Java: each choice must have a keyword for this to work —
                    // if ANY alternative's set is empty, no ChoiceKeyword.
                    if choices.iter().all(|set| !set.is_empty()) {
                        keywords.push(Keyword::Choice { choices });
                    }
                }
            }

            // Java: a parse tag does not represent actual content in a
            // pattern, therefore it should not affect `starting`.
            let is_parse_tag = el.borrow().as_any().is::<ParseTagPatternElement>();
            if !is_parse_tag {
                starting = false;
            }

            cursor = el.borrow().original_next();
        }

        keywords
    }

    /// Port of `Keyword.computeMinLength(PatternElement)` — walks the
    /// `original_next` chain summing mandatory characters (literals count
    /// non-space chars; choices take the minimum alternative; optional,
    /// type, regex and parse-tag elements count 0).
    pub fn compute_min_length(first: &ElementRef) -> usize {
        let mut length = 0usize;
        let mut cursor = Some(first.clone());

        while let Some(el) = cursor {
            let step = {
                let borrowed = el.borrow();
                let any = borrowed.as_any();
                if let Some(literal_element) = any.downcast_ref::<LiteralPatternElement>() {
                    // Java: only count non-space characters since spaces are
                    // somewhat flexible — underestimation is safe, over is not.
                    // DIVERGENCE: Java counts UTF-16 code units
                    // (`literal.length()`); we count `chars()` (Unicode
                    // scalar values). Identical for the BMP text patterns
                    // are written in.
                    length += literal_element
                        .literal()
                        .chars()
                        .filter(|&c| c != ' ')
                        .count();
                    ChainStep::Nothing
                } else if let Some(choice_element) = any.downcast_ref::<ChoicePatternElement>() {
                    // Get the min length of the options.
                    ChainStep::Choice(choice_element.alternatives())
                } else if let Some(group_element) = any.downcast_ref::<GroupPatternElement>() {
                    ChainStep::Group(group_element.pattern_element())
                } else {
                    // OptionalPatternElement, TypePatternElement,
                    // RegexPatternElement, ParseTagPatternElement:
                    // 0 min length.
                    ChainStep::Nothing
                }
            };

            match step {
                ChainStep::Nothing => {}
                ChainStep::Group(inner) => {
                    length += Self::compute_min_length(&inner);
                }
                ChainStep::Choice(alternatives) => {
                    // Java: min over the alternatives; when the choice has no
                    // alternatives (min stays Integer.MAX_VALUE) nothing is
                    // added.
                    let mut min: Option<usize> = None;
                    for alternative in &alternatives {
                        let choice_len = Self::compute_min_length(alternative);
                        min = Some(match min {
                            None => choice_len,
                            Some(current) => current.min(choice_len),
                        });
                    }
                    if let Some(min) = min {
                        length += min;
                    }
                }
            }

            cursor = el.borrow().original_next();
        }

        length
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::{OptionalPatternElement, RegexPatternElement};

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
        choice_element
            .pattern_elements
            .borrow_mut()
            .push(alternative);
    }

    fn simple(keyword: &str, starting: bool, ending: bool) -> Keyword {
        Keyword::Simple {
            keyword: keyword.to_string(),
            starting,
            ending,
        }
    }

    /// The `SkriptPattern.match` prefilter: every keyword must be present.
    fn all_present(keywords: &[Keyword], expr: &str) -> bool {
        keywords.iter().all(|k| k.is_present(expr))
    }

    #[test]
    fn simple_keyword_is_present_semantics() {
        // starting → must be a prefix (takes priority over `ending`, as in
        // the Java SimpleKeyword.isPresent).
        let k = simple("foo", true, false);
        assert!(k.is_present("foo bar"));
        assert!(!k.is_present("bar foo"));
        let k = simple("foo", true, true);
        assert!(k.is_present("foo"));
        assert!(!k.is_present("bar foo")); // starting wins over ending

        // ending → must be a suffix.
        let k = simple("foo", false, true);
        assert!(k.is_present("bar foo"));
        assert!(!k.is_present("foo bar"));

        // neither → containment.
        let k = simple("foo", false, false);
        assert!(k.is_present("xfoox"));
        assert!(!k.is_present("fxox"));
    }

    #[test]
    fn choice_keyword_is_present_semantics() {
        // At least ONE alternative's keyword set must be FULLY present.
        let choice = Keyword::Choice {
            choices: vec![
                vec![simple("red", false, false), simple("box", false, true)],
                vec![simple("blue", false, false)],
            ],
        };
        assert!(choice.is_present("a red box")); // alternative 1 complete
        assert!(choice.is_present("blue thing")); // alternative 2
        assert!(!choice.is_present("red chair")); // alternative 1 incomplete, 2 absent
        assert!(!choice.is_present("green box")); // "box" alone is not enough
        assert!(!choice.is_present("green chair"));
    }

    #[test]
    fn keywords_of_optional_the_name() {
        // Pattern "[the] name" — the example from the Java class javadoc
        // ("the pattern '[the] name' has the keyword ' name'"). NOTE: the
        // Java DOC is stale — the CODE trims literals, so the actual
        // keyword is "name". Compiler shape: Optional(lit "the") -> " name".
        let lit_the = LiteralPatternElement::new_rc("the");
        let optional = OptionalPatternElement::new_rc(lit_the.clone());
        let lit_name = LiteralPatternElement::new_rc(" name");
        link(&optional, lit_name.clone());
        // Optional's setNext propagates setLastNext into the child:
        link_next_only(&lit_the, lit_name.clone());

        // The optional part contributes nothing; " name" trims to "name"
        // and is the LAST element, so `ending` is true.
        let keywords = Keyword::build_keywords(&optional);
        assert_eq!(keywords, vec![simple("name", false, true)]);
        assert!(all_present(&keywords, "the name"));
        assert!(all_present(&keywords, "name"));
        // Prefilter-only semantics: "the nickname" ENDS with "name", so the
        // keyword prefilter passes (the full match later rejects it).
        assert!(all_present(&keywords, "the nickname"));
        assert!(!all_present(&keywords, "the colour"));

        // Optional contributes 0; " name" has 4 non-space chars.
        assert_eq!(Keyword::compute_min_length(&optional), 4);
    }

    #[test]
    fn min_length_and_keywords_of_group_choice() {
        // Pattern "(a|bb)": Group(Choice[alt "a", alt "bb"]).
        let alt_a = LiteralPatternElement::new_rc("a");
        let alt_bb = LiteralPatternElement::new_rc("bb");
        let choice = ChoicePatternElement::new_rc();
        add_alternative(&choice, alt_a);
        add_alternative(&choice, alt_bb);
        let group = GroupPatternElement::new_rc(choice);

        // The group bumps the depth to 1, so the choice still builds a
        // ChoiceKeyword; its alternatives inherit starting = true and
        // (nothing follows the pattern) ending = true.
        let keywords = Keyword::build_keywords(&group);
        assert_eq!(
            keywords,
            vec![Keyword::Choice {
                choices: vec![
                    vec![simple("a", true, true)],
                    vec![simple("bb", true, true)]
                ],
            }]
        );
        assert!(keywords[0].is_present("a"));
        assert!(keywords[0].is_present("bb"));
        assert!(keywords[0].is_present("aa")); // startsWith("a") — first alternative
        assert!(!keywords[0].is_present("c"));

        // min over the alternatives: "a" is 1 char.
        assert_eq!(Keyword::compute_min_length(&group), 1);
    }

    #[test]
    fn choice_keyword_starting_ending_contains() {
        // Pattern "give (a|an) apple". Compiler shape:
        //   lit "give " -> Group(Choice[alt "a", alt "an"]) -> lit " apple"
        // with the choice's alternatives back-patched (`next`) to " apple".
        let lit_give = LiteralPatternElement::new_rc("give ");
        let alt_a = LiteralPatternElement::new_rc("a");
        let alt_an = LiteralPatternElement::new_rc("an");
        let choice = ChoicePatternElement::new_rc();
        add_alternative(&choice, alt_a.clone());
        add_alternative(&choice, alt_an.clone());
        let group = GroupPatternElement::new_rc(choice.clone());
        let lit_apple = LiteralPatternElement::new_rc(" apple");

        link(&lit_give, group.clone());
        link(&group, lit_apple.clone());
        // Group::setNext propagates setLastNext into the choice, whose own
        // override back-patches every alternative:
        link_next_only(&choice, lit_apple.clone());
        link_next_only(&alt_a, lit_apple.clone());
        link_next_only(&alt_an, lit_apple.clone());

        let keywords = Keyword::build_keywords(&lit_give);
        assert_eq!(
            keywords,
            vec![
                simple("give", true, false), // startsWith; `next` is the group
                Keyword::Choice {
                    choices: vec![
                        vec![simple("a", false, false)],  // contains (back-patched)
                        vec![simple("an", false, false)], // contains
                    ]
                },
                simple("apple", false, true), // endsWith (last element)
            ]
        );
        assert!(all_present(&keywords, "give an apple"));
        assert!(all_present(&keywords, "give a apple"));
        assert!(!all_present(&keywords, "take an apple")); // prefix "give" missing
        assert!(!all_present(&keywords, "give an")); // suffix "apple" missing
                                                     // Neither "a" nor "an" occurs anywhere (note: a single-letter
                                                     // alternative is `contains`-matched, so "green chair" WOULD pass —
                                                     // the prefilter is a cheap heuristic, not exact).
        assert!(!keywords[1].is_present("green cheese"));

        // 4 ("give") + min(1, 2) + 5 ("apple").
        assert_eq!(Keyword::compute_min_length(&lit_give), 10);
    }

    #[test]
    fn group_keyword_inherits_starting() {
        // Pattern "(very )big": Group(lit "very ") -> lit "big".
        let lit_very = LiteralPatternElement::new_rc("very ");
        let group = GroupPatternElement::new_rc(lit_very.clone());
        let lit_big = LiteralPatternElement::new_rc("big");
        link(&group, lit_big.clone());
        link_next_only(&lit_very, lit_big.clone());

        // The group's inner literal inherits starting = true; the literal
        // AFTER the group does not (the group itself clears `starting`).
        let keywords = Keyword::build_keywords(&group);
        assert_eq!(
            keywords,
            vec![simple("very", true, false), simple("big", false, true)]
        );
        assert!(all_present(&keywords, "very big"));
        assert!(!all_present(&keywords, "not very big")); // "very" must be a prefix
        assert_eq!(Keyword::compute_min_length(&group), 7);
    }

    #[test]
    fn parse_tag_does_not_affect_starting() {
        // A "tag then literal" chain (e.g. pattern "1: cat" style): the
        // parse tag contributes no keyword and keeps `starting` true for
        // the element that follows it.
        let tag = ParseTagPatternElement::new_rc_tag("1");
        let lit_cat = LiteralPatternElement::new_rc(" cat");
        link(&tag, lit_cat.clone());

        let keywords = Keyword::build_keywords(&tag);
        assert_eq!(keywords, vec![simple("cat", true, true)]);
        assert!(keywords[0].is_present("cat"));
        assert!(!keywords[0].is_present("the cat")); // starting → prefix required
                                                     // Parse tags have zero min length.
        assert_eq!(Keyword::compute_min_length(&tag), 3);
    }

    #[test]
    fn empty_alternative_suppresses_choice_keyword() {
        // Pattern "a|" (trailing pipe) leaves an empty literal alternative:
        // Choice[alt "a", alt ""]. One empty alternative set means NO
        // ChoiceKeyword at all.
        let alt_a = LiteralPatternElement::new_rc("a");
        let alt_empty = LiteralPatternElement::new_rc("");
        let choice = ChoicePatternElement::new_rc();
        add_alternative(&choice, alt_a);
        add_alternative(&choice, alt_empty);

        assert!(Keyword::build_keywords(&choice).is_empty());
        // The empty alternative also wins the min: 0.
        assert_eq!(Keyword::compute_min_length(&choice), 0);
    }

    #[test]
    fn choice_at_depth_2_contributes_no_keywords() {
        // Pattern "((a|b))c": two groups put the choice at depth 2, past
        // the `depth <= 1` guard — it contributes no keywords (and neither
        // do its alternatives' literals).
        let alt_a = LiteralPatternElement::new_rc("a");
        let alt_b = LiteralPatternElement::new_rc("b");
        let inner_choice = ChoicePatternElement::new_rc();
        add_alternative(&inner_choice, alt_a);
        add_alternative(&inner_choice, alt_b);
        let inner_group = GroupPatternElement::new_rc(inner_choice);
        let outer_group = GroupPatternElement::new_rc(inner_group);
        let lit_c = LiteralPatternElement::new_rc("c");
        link(&outer_group, lit_c.clone());

        assert_eq!(
            Keyword::build_keywords(&outer_group),
            vec![simple("c", false, true)]
        );
        // computeMinLength has NO depth guard: (a|b) still counts (min 1).
        assert_eq!(Keyword::compute_min_length(&outer_group), 2);
    }

    #[test]
    fn literal_trim_and_space_collapse() {
        // Leading/trailing whitespace is trimmed, runs of spaces collapsed
        // ("hello   world" needs two replace passes in Java and here).
        let lit = LiteralPatternElement::new_rc("  hello   world  ");
        let keywords = Keyword::build_keywords(&lit);
        assert_eq!(keywords, vec![simple("hello world", true, true)]);

        // Min length counts NON-SPACE characters of the UNTRIMMED literal:
        // "  hello   world  " has 10 of them.
        assert_eq!(Keyword::compute_min_length(&lit), 10);
    }

    #[test]
    fn regex_and_optional_contribute_nothing() {
        // "<[a-z]+>[maybe ]end": regex and optional contribute neither
        // keywords nor min length. The optional's child is never reached —
        // keyword building follows `original_next`, which bypasses it.
        let regex_element = RegexPatternElement::new_rc("[a-z]+").unwrap();
        let optional = OptionalPatternElement::new_rc(LiteralPatternElement::new_rc("maybe "));
        let lit_end = LiteralPatternElement::new_rc("end");
        link(&regex_element, optional.clone());
        link(&optional, lit_end.clone());

        assert_eq!(
            Keyword::build_keywords(&regex_element),
            vec![simple("end", false, true)]
        );
        assert_eq!(Keyword::compute_min_length(&regex_element), 3);
    }

    // ------------------------------------------------------------------
    // End-to-end tests — gated on the compiler (Agent 5). Run with
    // `cargo test -p skript-patterns --lib -- --ignored` after the whole
    // engine is integrated.
    // ------------------------------------------------------------------

    #[test]
    fn compiled_keywords_of_the_name() {
        let pattern = crate::compile_pattern("[the] name").unwrap();
        assert_eq!(pattern.keywords, vec![simple("name", false, true)]);
        assert_eq!(pattern.min_length, 4);
    }

    #[test]
    fn compiled_min_length_of_group_choice() {
        let pattern = crate::compile_pattern("(a|bb)").unwrap();
        assert_eq!(pattern.min_length, 1);
        assert_eq!(
            pattern.keywords,
            vec![Keyword::Choice {
                choices: vec![
                    vec![simple("a", true, true)],
                    vec![simple("bb", true, true)]
                ],
            }]
        );
    }
}
