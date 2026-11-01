//! Port of Java `ch.njol.skript.patterns.ParseTagPatternElement`.
//!
//! OWNER: Agent 2 (same agent as choice.rs — the setNext logic here
//! manipulates ChoicePatternElement alternatives). Faithful port of:
//! `/home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/ParseTagPatternElement.java`

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::elements::{
    ChoicePatternElement, ElementRef, GroupPatternElement, LiteralPatternElement,
    OptionalPatternElement, PatternElement,
};
use crate::match_result::MatchResult;
use crate::pattern::StringificationProperties;

/// Applies a parse tag (or integer parse mark) when matched. Created by
/// `:(tag)` or `¦mark` in patterns.
#[derive(Debug)]
pub struct ParseTagPatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    /// `None` = created from an integer parse mark (`¦`).
    /// Mutable: the `setNext` override derives tags for `:(a|b)` shapes.
    pub tag: RefCell<Option<String>>,
    /// `0` when constructed from a non-numeric tag.
    pub mark: usize,
}

/// Java `String.trim()` — removes leading and trailing characters with
/// code point <= U+0020. (Rust's `str::trim` uses the Unicode
/// `White_Space` property, which is a slightly different set.)
fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

impl ParseTagPatternElement {
    /// Java `ParseTagPatternElement(String tag)` — also parses an integer
    /// mark out of the tag when possible.
    ///
    /// DIVERGENCE: Java's `Integer.parseInt` accepts negative marks (e.g.
    /// `"-1"`); the frozen contract fixes `mark: usize`, so negative (and
    /// > `u64`-range) tags fall back to `0`. Real-world `¦` marks are
    /// non-negative.
    pub fn new_rc_tag(tag: impl Into<String>) -> ElementRef {
        let tag = tag.into();
        let mark = tag.parse::<usize>().unwrap_or(0);
        let element = ParseTagPatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            tag: RefCell::new(Some(tag)),
            mark,
        };
        Rc::new(RefCell::new(element))
    }

    /// Java `ParseTagPatternElement(int mark)` (old `¦` parse marks).
    pub fn new_rc_mark(mark: usize) -> ElementRef {
        let element = ParseTagPatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            tag: RefCell::new(None),
            mark,
        };
        Rc::new(RefCell::new(element))
    }
}

impl PatternElement for ParseTagPatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // Java:
        //   if (tag != null && !tag.isEmpty()) matchResult.tags.add(tag);
        //   matchResult.mark ^= mark;
        //   return matchNext(expr, matchResult);
        // (the result is mutated in place — no copy — so a later failure
        //  leaves the tag/mark behind; that is the Java behaviour too)
        {
            let tag = self.tag.borrow();
            if let Some(tag) = tag.as_ref() {
                if !tag.is_empty() {
                    result.tags.push(tag.clone());
                }
            }
        }
        result.mark ^= self.mark;
        self.match_next(expr, result)
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // Java:
        //   if (properties.excludeParseTags()) return "";
        //   if (tag == null) return mark + ":";
        //   if (tag.isEmpty()) return "";
        //   return tag + ":";
        if properties.exclude_parse_tags {
            return String::new();
        }
        let tag = self.tag.borrow();
        match tag.as_deref() {
            None => format!("{}:", self.mark),
            Some("") => String::new(),
            Some(tag) => format!("{}:", tag),
        }
    }

    /// Java override — READ THE JAVA FILE CAREFULLY. When the tag is
    /// `Some("")` this derives tags from the FOLLOWING element:
    ///   - next is a LiteralPatternElement  -> tag = next.toString().trim()
    ///     (the `(:a)` case)
    ///   - next is a Group/Optional whose inner element is a Choice ->
    ///     rewrite each non-empty Literal alternative in place into a new
    ///     ParseTagPatternElement(tag = literal.trim()) with
    ///     set_next(literal) and original_next = literal (the `:(a|b)` /
    ///     `:[a|b]` case).
    /// Finally calls super.setNext(next).
    fn set_next(&self, next: Option<ElementRef>) {
        // Java: if (tag != null && tag.isEmpty()) — only an EMPTY STRING tag
        // (not `None`, not a real tag) triggers the derivation.
        let is_empty_tag = {
            let tag = self.tag.borrow();
            matches!(tag.as_deref(), Some(""))
        };
        if is_empty_tag {
            // Java `next instanceof LiteralPatternElement` is false for a
            // null next; `if let Some` mirrors that.
            if let Some(next_el) = next.clone() {
                let literal_text: Option<String> = {
                    let borrowed = next_el.borrow();
                    if borrowed.as_any().is::<LiteralPatternElement>() {
                        // Java: next.toString() == toString(DEFAULT)
                        Some(borrowed.to_string_pretty(&StringificationProperties::DEFAULT))
                    } else {
                        None
                    }
                };
                if let Some(text) = literal_text {
                    // (:a)
                    // Java: tag = next.toString().trim();
                    *self.tag.borrow_mut() = Some(java_trim(&text).to_string());
                } else {
                    // Get the inner element from either a group or optional
                    // pattern element.
                    let inner: Option<ElementRef> = {
                        let borrowed = next_el.borrow();
                        if let Some(group) =
                            borrowed.as_any().downcast_ref::<GroupPatternElement>()
                        {
                            Some(group.pattern_element())
                        } else if let Some(optional) = borrowed
                            .as_any()
                            .downcast_ref::<OptionalPatternElement>()
                        {
                            Some(optional.pattern_element())
                        } else {
                            None
                        }
                    };
                    if let Some(inner) = inner {
                        let is_choice = inner.borrow().as_any().is::<ChoicePatternElement>();
                        if is_choice {
                            // :(a|b) or :[a|b] — rewrite the choice's
                            // non-empty literal alternatives in place. We
                            // hold the outer borrow of `inner` (the choice)
                            // but only mutate its INNER RefCell<Vec>, and we
                            // never re-borrow `inner` while it is held.
                            let choice_ref = inner.borrow();
                            let choice = choice_ref
                                .as_any()
                                .downcast_ref::<ChoicePatternElement>()
                                .expect("checked is_choice above");
                            let mut elements = choice.pattern_elements.borrow_mut();
                            for i in 0..elements.len() {
                                // Clone the Rc and end all borrows at
                                // statement boundaries before building the
                                // replacement element.
                                let el = elements[i].clone();
                                let literal_text: Option<String> = {
                                    let borrowed = el.borrow();
                                    if borrowed.as_any().is::<LiteralPatternElement>() {
                                        Some(
                                            borrowed.to_string_pretty(
                                                &StringificationProperties::DEFAULT,
                                            ),
                                        )
                                    } else {
                                        None
                                    }
                                };
                                // Prevent a pattern such as :(a|b|) from being
                                // turned into (a:a|b:b|:), instead (a:a|b:b|)
                                // — empty literals are left alone.
                                let text = match literal_text.as_deref() {
                                    Some(text) if !text.is_empty() => text,
                                    _ => continue,
                                };
                                let new_tag =
                                    ParseTagPatternElement::new_rc_tag(java_trim(text));
                                // Java: newTag.setNext(patternElement); — the
                                // override runs (a no-op for non-empty derived
                                // tags; for whitespace-only literals it
                                // re-derives the same empty tag, like Java).
                                new_tag.borrow().set_next(Some(el.clone()));
                                // Java: newTag.originalNext = patternElement;
                                // (direct field write, bypassing the override)
                                new_tag.borrow().set_original_next_raw(Some(el.clone()));
                                elements[i] = new_tag;
                            }
                        }
                    }
                }
            }
        }
        // Java: super.setNext(next);
        self.set_next_raw(next);
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
    use crate::elements::{ChoicePatternElement, GroupPatternElement, LiteralPatternElement};
    use std::cell::Ref;
    use std::rc::Rc;

    fn as_parse_tag(element: &ElementRef) -> Ref<'_, ParseTagPatternElement> {
        Ref::map(element.borrow(), |borrowed| {
            borrowed
                .as_any()
                .downcast_ref::<ParseTagPatternElement>()
                .expect("not a ParseTagPatternElement")
        })
    }

    fn as_choice(element: &ElementRef) -> Ref<'_, ChoicePatternElement> {
        Ref::map(element.borrow(), |borrowed| {
            borrowed
                .as_any()
                .downcast_ref::<ChoicePatternElement>()
                .expect("not a ChoicePatternElement")
        })
    }

    fn tag_of(element: &ElementRef) -> Option<String> {
        as_parse_tag(element).tag.borrow().clone()
    }

    #[test]
    fn match_records_tag_then_succeeds_at_end_of_input() {
        // No `next` element and the whole (empty) input consumed -> the
        // match succeeds right after applying the tag.
        let tag = ParseTagPatternElement::new_rc_tag("hello");
        let mut result = MatchResult::new(0);
        assert!(tag.borrow().match_("", &mut result));
        assert_eq!(result.tags, vec!["hello".to_string()]);
        // "hello" is not numeric -> mark stays 0.
        assert_eq!(result.mark, 0);
    }

    #[test]
    fn match_does_not_record_empty_or_missing_tags() {
        let empty = ParseTagPatternElement::new_rc_tag("");
        let mut result = MatchResult::new(0);
        assert!(empty.borrow().match_("", &mut result));
        assert!(result.tags.is_empty());
        assert_eq!(result.mark, 0);

        let mark_el = ParseTagPatternElement::new_rc_mark(4);
        let mut result = MatchResult::new(0);
        assert!(mark_el.borrow().match_("", &mut result));
        assert!(result.tags.is_empty());
        assert_eq!(result.mark, 4);
    }

    #[test]
    fn match_mark_xor_returns_to_zero_after_two_matches() {
        let tag = ParseTagPatternElement::new_rc_mark(5);
        let mut result = MatchResult::new(0);
        assert!(tag.borrow().match_("", &mut result));
        assert_eq!(result.mark, 5);
        // Matching the same mark a second time flips it back (XOR).
        assert!(tag.borrow().match_("", &mut result));
        assert_eq!(result.mark, 5 ^ 5);
    }

    #[test]
    fn numeric_tag_also_carries_a_mark() {
        // Java: ParseTagPatternElement(String) parses Integer.parseInt(tag).
        let tag = ParseTagPatternElement::new_rc_tag("3");
        let mut result = MatchResult::new(0);
        assert!(tag.borrow().match_("", &mut result));
        assert_eq!(result.tags, vec!["3".to_string()]);
        assert_eq!(result.mark, 3);
        assert_eq!(
            tag.borrow().to_string_pretty(&StringificationProperties::DEFAULT),
            "3:"
        );
    }

    #[test]
    fn to_string_pretty_variants() {
        let mark_el = ParseTagPatternElement::new_rc_mark(7);
        let tag_el = ParseTagPatternElement::new_rc_tag("tagged");
        let empty_el = ParseTagPatternElement::new_rc_tag("");
        let props = StringificationProperties::DEFAULT;
        assert_eq!(mark_el.borrow().to_string_pretty(&props), "7:");
        assert_eq!(tag_el.borrow().to_string_pretty(&props), "tagged:");
        assert_eq!(empty_el.borrow().to_string_pretty(&props), "");

        // exclude_parse_tags -> "" for every variant.
        let excluded = StringificationProperties {
            exclude_parse_tags: true,
            ..StringificationProperties::DEFAULT
        };
        assert_eq!(mark_el.borrow().to_string_pretty(&excluded), "");
        assert_eq!(tag_el.borrow().to_string_pretty(&excluded), "");
        assert_eq!(empty_el.borrow().to_string_pretty(&excluded), "");
    }

    #[test]
    fn set_next_derives_tag_from_literal_the_colon_a_shape() {
        // (:a) — the parse tag is followed directly by a literal.
        let tag = ParseTagPatternElement::new_rc_tag("");
        let lit = LiteralPatternElement::new_rc("a");
        tag.borrow().set_next(Some(lit.clone()));

        assert_eq!(tag_of(&tag), Some("a".to_string()));
        // super.setNext(next) still ran:
        assert!(Rc::ptr_eq(&tag.borrow().next().unwrap(), &lit));
    }

    #[test]
    fn set_next_literal_tag_is_trimmed() {
        let tag = ParseTagPatternElement::new_rc_tag("");
        let lit = LiteralPatternElement::new_rc("  spaced  ");
        tag.borrow().set_next(Some(lit));
        assert_eq!(tag_of(&tag), Some("spaced".to_string()));
    }

    #[test]
    fn set_next_derives_tags_inside_choice_the_colon_group_choice_shape() {
        // :(a|b) — group wrapping a choice of literals.
        let tag = ParseTagPatternElement::new_rc_tag("");
        let lit_a = LiteralPatternElement::new_rc("a");
        let lit_b = LiteralPatternElement::new_rc("b");
        let choice = ChoicePatternElement::new_rc();
        {
            let c = as_choice(&choice);
            c.add(lit_a.clone());
            c.add(lit_b.clone());
        }
        let group = GroupPatternElement::new_rc(choice.clone());
        tag.borrow().set_next(Some(group.clone()));

        // The parse tag itself keeps its empty tag and links to the group.
        assert_eq!(tag_of(&tag), Some(String::new()));
        assert!(Rc::ptr_eq(&tag.borrow().next().unwrap(), &group));

        // Both alternatives were REPLACED by parse tags wrapping their
        // original literals (both `next` and `originalNext`).
        let alts = as_choice(&choice).alternatives();
        assert_eq!(alts.len(), 2);
        let expected = [(&lit_a, "a"), (&lit_b, "b")];
        for (i, (lit, text)) in expected.iter().enumerate() {
            let alt = &alts[i];
            assert!(
                alt.borrow().as_any().is::<ParseTagPatternElement>(),
                "alternative {} was not rewritten into a parse tag",
                i
            );
            let parsed = as_parse_tag(alt);
            assert_eq!(*parsed.tag.borrow(), Some(text.to_string()));
            assert!(Rc::ptr_eq(&parsed.next().unwrap(), lit));
            assert!(Rc::ptr_eq(&parsed.original_next().unwrap(), lit));
        }
    }

    #[test]
    fn set_next_leaves_empty_alternative_alone() {
        // :(a|b|) — the trailing empty literal must NOT become a parse tag:
        // Java prevents (a:a|b:b|:) in favour of (a:a|b:b|).
        let tag = ParseTagPatternElement::new_rc_tag("");
        let lit_a = LiteralPatternElement::new_rc("a");
        let empty = LiteralPatternElement::new_rc("");
        let choice = ChoicePatternElement::new_rc();
        {
            let c = as_choice(&choice);
            c.add(lit_a.clone());
            c.add(empty.clone());
        }
        let group = GroupPatternElement::new_rc(choice.clone());
        tag.borrow().set_next(Some(group));

        let alts = as_choice(&choice).alternatives();
        assert_eq!(alts.len(), 2);
        // First alternative rewritten...
        assert!(alts[0].borrow().as_any().is::<ParseTagPatternElement>());
        assert_eq!(tag_of(&alts[0]), Some("a".to_string()));
        // ...but the empty literal alternative is untouched.
        assert!(alts[1].borrow().as_any().is::<LiteralPatternElement>());
        assert!(Rc::ptr_eq(&alts[1], &empty));
    }

    #[test]
    fn set_next_derives_tags_inside_optional_the_colon_optional_choice_shape() {
        // :[a|b] — optional wrapping a choice of literals.
        let tag = ParseTagPatternElement::new_rc_tag("");
        let lit_a = LiteralPatternElement::new_rc("a");
        let lit_b = LiteralPatternElement::new_rc("b");
        let choice = ChoicePatternElement::new_rc();
        {
            let c = as_choice(&choice);
            c.add(lit_a.clone());
            c.add(lit_b.clone());
        }
        let optional = OptionalPatternElement::new_rc(choice.clone());
        tag.borrow().set_next(Some(optional.clone()));

        assert!(Rc::ptr_eq(&tag.borrow().next().unwrap(), &optional));
        let alts = as_choice(&choice).alternatives();
        assert_eq!(alts.len(), 2);
        assert_eq!(tag_of(&alts[0]), Some("a".to_string()));
        assert_eq!(tag_of(&alts[1]), Some("b".to_string()));
        assert!(Rc::ptr_eq(&as_parse_tag(&alts[0]).next().unwrap(), &lit_a));
        assert!(Rc::ptr_eq(&as_parse_tag(&alts[1]).next().unwrap(), &lit_b));
    }

    #[test]
    fn set_next_does_not_derive_for_non_empty_or_missing_tags() {
        // A real tag stays as-is...
        let tagged = ParseTagPatternElement::new_rc_tag("existing");
        let lit = LiteralPatternElement::new_rc("a");
        tagged.borrow().set_next(Some(lit));
        assert_eq!(tag_of(&tagged), Some("existing".to_string()));

        // ...and so does an integer mark (`¦` -> tag == None).
        let marked = ParseTagPatternElement::new_rc_mark(2);
        let lit = LiteralPatternElement::new_rc("a");
        marked.borrow().set_next(Some(lit));
        assert!(tag_of(&marked).is_none());
    }

    #[test]
    fn set_next_none_with_empty_tag_only_links() {
        // Java: `null instanceof LiteralPatternElement` is false, the
        // group/optional branch finds no inner element, so nothing is
        // derived — only super.setNext(null) runs.
        let tag = ParseTagPatternElement::new_rc_tag("");
        tag.borrow().set_next(None);
        assert_eq!(tag_of(&tag), Some(String::new()));
        assert!(tag.borrow().next().is_none());
    }

    #[test]
    fn set_next_whitespace_only_literal_becomes_empty_tag_wrapper() {
        // A whitespace-only literal passes Java's non-empty check but trims
        // to "" — the alternative is STILL replaced by a parse tag whose
        // tag stays empty (newTag.setNext re-derives "" from the literal,
        // terminating instead of recursing).
        let tag = ParseTagPatternElement::new_rc_tag("");
        let spaced = LiteralPatternElement::new_rc(" ");
        let choice = ChoicePatternElement::new_rc();
        as_choice(&choice).add(spaced.clone());
        let group = GroupPatternElement::new_rc(choice.clone());
        tag.borrow().set_next(Some(group));

        let alts = as_choice(&choice).alternatives();
        assert_eq!(alts.len(), 1);
        assert!(alts[0].borrow().as_any().is::<ParseTagPatternElement>());
        assert_eq!(tag_of(&alts[0]), Some(String::new()));
        assert!(Rc::ptr_eq(&as_parse_tag(&alts[0]).next().unwrap(), &spaced));
    }

    #[test]
    fn set_next_ignores_group_of_literal() {
        // :(a) — a group wrapping a single literal is NOT rewritten (the
        // inner element must be a Choice); the tag keeps waiting for a
        // literal that never comes, exactly like Java.
        let tag = ParseTagPatternElement::new_rc_tag("");
        let lit = LiteralPatternElement::new_rc("a");
        let group = GroupPatternElement::new_rc(lit);
        tag.borrow().set_next(Some(group.clone()));
        assert_eq!(tag_of(&tag), Some(String::new()));
        assert!(Rc::ptr_eq(&tag.borrow().next().unwrap(), &group));
    }
}
