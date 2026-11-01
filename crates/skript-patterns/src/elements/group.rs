//! Port of Java `ch.njol.skript.patterns.GroupPatternElement`.
//!
//! Java source:
//! /home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/GroupPatternElement.java

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::elements::{ElementRef, PatternElement};
use crate::match_result::MatchResult;
use crate::pattern::StringificationProperties;

/// A group, e.g. `(test)`.
#[derive(Debug)]
pub struct GroupPatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    pub pattern_element: ElementRef,
}

impl GroupPatternElement {
    /// Constructor returning a shared handle.
    pub fn new_rc(child: ElementRef) -> ElementRef {
        let element = GroupPatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            pattern_element: child,
        };
        Rc::new(RefCell::new(element))
    }

    pub fn pattern_element(&self) -> ElementRef {
        self.pattern_element.clone()
    }
}

impl PatternElement for GroupPatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // Java: return patternElement.match(expr, matchResult);  (NO copy —
        // the child works directly on the caller's result).
        let child = self.pattern_element.clone();
        let matched = child.borrow().match_(expr, result);
        matched
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // Java: "(" + patternElement.toFullString(properties) + ")".
        // NOTE: this Skript version's StringificationProperties only has
        // excludeParseTags / excludeTypeFlags — the Java uses neither
        // useAngleBrackets nor excludeOptionalParts here (those flags do
        // not exist in this version), so the parentheses are unconditional.
        let child = self.pattern_element.clone();
        let inner = child.borrow().to_full_string(properties);
        format!("({inner})")
    }

    /// Java override: `setNext` also propagates `setLastNext(next)` into
    /// the child chain.
    fn set_next(&self, next: Option<ElementRef>) {
        // Java: super.setNext(next); patternElement.setLastNext(next);
        self.set_next_raw(next.clone());
        let child = self.pattern_element.clone();
        child.borrow().set_last_next(next);
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
    use crate::elements::literal::LiteralPatternElement;
    use crate::elements::optional::OptionalPatternElement;

    fn lit(s: &str) -> ElementRef {
        LiteralPatternElement::new_rc(s)
    }

    fn as_group(el: &ElementRef) -> std::cell::Ref<'_, GroupPatternElement> {
        std::cell::Ref::map(el.borrow(), |p| {
            p.as_any().downcast_ref::<GroupPatternElement>().unwrap()
        })
    }

    // ------------------------------------------------------------------
    // match: pure delegation to the child
    // ------------------------------------------------------------------

    #[test]
    fn delegates_matching_to_child() {
        let group = GroupPatternElement::new_rc(lit("abc"));

        let mut result = MatchResult::new(0);
        assert!(group.borrow().match_("abc", &mut result));
        assert_eq!(result.expr_offset, 3);

        // case-insensitive through the group
        let mut result = MatchResult::new(0);
        assert!(group.borrow().match_("ABC", &mut result));
        assert_eq!(result.expr_offset, 3);

        // child failure propagates
        let mut result = MatchResult::new(0);
        assert!(!group.borrow().match_("abd", &mut result));
    }

    #[test]
    fn group_passes_result_through_without_copy() {
        // Java: return patternElement.match(expr, matchResult) — no copy.
        // State changes made by the child (here: an optional either
        // matching or being skipped) are visible to the caller directly.
        let group = GroupPatternElement::new_rc(OptionalPatternElement::new_rc(lit("ab")));
        let next = lit("cd");
        group.borrow().set_next(Some(next.clone()));

        let mut result = MatchResult::new(0);
        assert!(group.borrow().match_("abcd", &mut result));
        assert_eq!(result.expr_offset, 4);

        // optional skipped inside the group
        let mut result = MatchResult::new(0);
        assert!(group.borrow().match_("cd", &mut result));
        assert_eq!(result.expr_offset, 2);
    }

    // ------------------------------------------------------------------
    // setNext propagation
    // ------------------------------------------------------------------

    #[test]
    fn set_next_propagates_into_child_chain() {
        let child = lit("abc");
        let group = GroupPatternElement::new_rc(child.clone());
        let next = lit("def");
        group.borrow().set_next(Some(next.clone()));

        // the group itself got the next (super.setNext)...
        let group_next = as_group(&group).next();
        assert!(Rc::ptr_eq(&group_next.unwrap(), &next));
        // ...and so did the child (patternElement.setLastNext(next))
        let child_next = child.borrow().next();
        assert!(Rc::ptr_eq(&child_next.unwrap(), &next));

        // matching flows through: group -> child -> next
        let mut result = MatchResult::new(0);
        assert!(group.borrow().match_("abcdef", &mut result));
        assert_eq!(result.expr_offset, 6);
    }

    #[test]
    fn set_last_next_walks_child_chain_to_its_end() {
        // Child chain a -> b; the group's setNext must land on the chain's
        // last element (b), not on the head.
        let a = lit("a");
        let b = lit("b");
        a.borrow().set_next(Some(b.clone()));
        let group = GroupPatternElement::new_rc(a.clone());
        let next = lit("z");
        group.borrow().set_next(Some(next.clone()));

        let b_next = b.borrow().next();
        assert!(Rc::ptr_eq(&b_next.unwrap(), &next));
    }

    // ------------------------------------------------------------------
    // stringification
    // ------------------------------------------------------------------

    #[test]
    fn to_string_wraps_child_full_string_in_parens() {
        let child = lit("abc");
        // toFullString follows the originalNext chain
        let tail = lit(" def");
        child.borrow().set_original_next_raw(Some(tail.clone()));
        let group = GroupPatternElement::new_rc(child);

        assert_eq!(
            group
                .borrow()
                .to_string_pretty(&StringificationProperties::DEFAULT),
            "(abc def)"
        );
    }

    #[test]
    fn to_string_nested() {
        let inner = OptionalPatternElement::new_rc(lit("x"));
        let group = GroupPatternElement::new_rc(inner);

        assert_eq!(
            group
                .borrow()
                .to_string_pretty(&StringificationProperties::DEFAULT),
            "([x])"
        );
    }
}
