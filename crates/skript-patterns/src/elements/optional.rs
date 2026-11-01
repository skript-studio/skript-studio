//! Port of Java `ch.njol.skript.patterns.OptionalPatternElement`.
//!
//! Java source:
//! /home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/OptionalPatternElement.java

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use super::GroupPatternElement;
use crate::elements::{ElementRef, PatternElement};
use crate::match_result::MatchResult;
use crate::pattern::StringificationProperties;

/// An optional part, e.g. `[hello world]`.
#[derive(Debug)]
pub struct OptionalPatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    pub pattern_element: ElementRef,
}

impl OptionalPatternElement {
    /// Constructor returning a shared handle.
    ///
    /// Java: converts `[(...)]` to `[...]` (e.g. `[(a|b)]` to `[a|b]`) —
    /// when the child is a `GroupPatternElement` whose `next == null`,
    /// the optional wraps the group's inner element instead of the group.
    pub fn new_rc(child: ElementRef) -> ElementRef {
        let child = {
            let inner = {
                let borrowed = child.borrow();
                match borrowed.as_any().downcast_ref::<GroupPatternElement>() {
                    Some(group) if group.next().is_none() => {
                        Some(group.pattern_element.clone())
                    }
                    _ => None,
                }
            };
            match inner {
                Some(inner) => inner,
                None => child,
            }
        };
        let element = OptionalPatternElement {
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

impl PatternElement for OptionalPatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // Java: MatchResult newMatchResult =
        //         patternElement.match(expr, matchResult.copy());
        //       if (newMatchResult != null) return newMatchResult;
        //       return matchNext(expr, matchResult);
        // Copy-on-success convention: the child works on a clone and only
        // the winning state is written back into `*result`.
        let mut copy = result.clone();
        let child = self.pattern_element.clone();
        if child.borrow().match_(expr, &mut copy) {
            *result = copy;
            return true;
        }
        self.match_next(expr, result)
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // Java: "[" + patternElement.toFullString(properties) + "]".
        // NOTE: this Skript version's StringificationProperties only has
        // excludeParseTags / excludeTypeFlags — it has no
        // excludeOptionalParts flag, so none is consulted here. (The extra
        // fields on the Rust struct are simply unused by this element,
        // exactly matching the Java.)
        let child = self.pattern_element.clone();
        let inner = child.borrow().to_full_string(properties);
        format!("[{inner}]")
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

    fn lit(s: &str) -> ElementRef {
        LiteralPatternElement::new_rc(s)
    }

    fn opt(child: ElementRef) -> ElementRef {
        OptionalPatternElement::new_rc(child)
    }

    fn as_optional(el: &ElementRef) -> std::cell::Ref<'_, OptionalPatternElement> {
        std::cell::Ref::map(el.borrow(), |p| {
            p.as_any().downcast_ref::<OptionalPatternElement>().unwrap()
        })
    }

    // ------------------------------------------------------------------
    // match: child path vs skip path
    // ------------------------------------------------------------------

    #[test]
    fn child_match_path_wins_and_writes_back() {
        let optional = opt(lit("fly"));
        let next = lit("away");
        optional.borrow().set_next(Some(next.clone()));

        let mut result = MatchResult::new(0);
        assert!(optional.borrow().match_("flyaway", &mut result));
        assert_eq!(result.expr_offset, 7);
    }

    #[test]
    fn skip_path_matches_when_child_fails() {
        let optional = opt(lit("fly"));
        let next = lit("away");
        optional.borrow().set_next(Some(next.clone()));

        let mut result = MatchResult::new(0);
        assert!(optional.borrow().match_("away", &mut result));
        assert_eq!(result.expr_offset, 4);
    }

    #[test]
    fn child_success_then_chain_failure_falls_back_to_skip() {
        // If the child matched but the REST of the chain failed, the
        // child's match returns null, so the optional falls back to
        // matchNext (the skip path) — here both paths fail.
        let optional = opt(lit("ab"));
        let next = lit("cd");
        optional.borrow().set_next(Some(next.clone()));

        let mut result = MatchResult::new(0);
        assert!(!optional.borrow().match_("ab", &mut result));

        // "abcd" succeeds via the child path
        let mut result = MatchResult::new(0);
        assert!(optional.borrow().match_("abcd", &mut result));
        assert_eq!(result.expr_offset, 4);
    }

    #[test]
    fn failed_child_does_not_corrupt_result_for_skip_path() {
        // Copy-on-success: the child mutates only its own clone; when it
        // fails, the original result is handed to matchNext untouched.
        let optional = opt(lit("fly"));
        let next = lit("fire");
        optional.borrow().set_next(Some(next.clone()));

        let mut result = MatchResult::new(0);
        result.mark = 42; // arbitrary state that must survive the child
        assert!(optional.borrow().match_("fire", &mut result));
        assert_eq!(result.expr_offset, 4);
        assert_eq!(result.mark, 42);
    }

    #[test]
    fn nested_optionals_both_paths() {
        // Also exercises the recursive setNext propagation and the
        // copy-on-success write-back through two levels.
        let inner = opt(lit("fly"));
        let outer = OptionalPatternElement::new_rc(inner);
        let next = lit("away");
        outer.borrow().set_next(Some(next.clone()));

        let mut result = MatchResult::new(0);
        assert!(outer.borrow().match_("flyaway", &mut result));
        assert_eq!(result.expr_offset, 7);

        let mut result = MatchResult::new(0);
        assert!(outer.borrow().match_("away", &mut result));
        assert_eq!(result.expr_offset, 4);
    }

    // ------------------------------------------------------------------
    // constructor: [(a|b)] -> [a|b] unwrapping
    // ------------------------------------------------------------------

    #[test]
    fn constructor_unwraps_group_without_next() {
        let inner = lit("fly");
        let group = GroupPatternElement::new_rc(inner.clone());
        let optional = opt(group);

        let child = as_optional(&optional).pattern_element();
        assert!(
            Rc::ptr_eq(&child, &inner),
            "the optional should wrap the group's inner element"
        );
    }

    #[test]
    fn constructor_keeps_group_with_next_set() {
        let group = GroupPatternElement::new_rc(lit("fly"));
        let other = lit("x");
        group.borrow().set_next(Some(other.clone())); // group.next != null
        let optional = opt(group.clone());

        let child = as_optional(&optional).pattern_element();
        assert!(
            Rc::ptr_eq(&child, &group),
            "a group with a next must NOT be unwrapped"
        );
    }

    // ------------------------------------------------------------------
    // setNext propagation
    // ------------------------------------------------------------------

    #[test]
    fn set_next_propagates_into_child_chain() {
        let child = lit("a");
        let tail = lit("b");
        child.borrow().set_next(Some(tail.clone()));
        let optional = OptionalPatternElement::new_rc(child.clone());
        let next = lit("z");
        optional.borrow().set_next(Some(next.clone()));

        // super.setNext: the optional itself got the next...
        let opt_next = as_optional(&optional).next();
        assert!(Rc::ptr_eq(&opt_next.unwrap(), &next));
        // ...and setLastNext walked the child chain to its end: the tail
        // literal (not the head) received the next.
        let tail_next = tail.borrow().next();
        assert!(Rc::ptr_eq(&tail_next.unwrap(), &next));
        // the head's own next is still the tail
        let child_next = child.borrow().next();
        assert!(Rc::ptr_eq(&child_next.unwrap(), &tail));
    }

    // ------------------------------------------------------------------
    // stringification
    // ------------------------------------------------------------------

    #[test]
    fn to_string_wraps_child_full_string_in_brackets() {
        let child = lit("fly");
        // toFullString follows the originalNext chain
        let tail = lit(" away");
        child.borrow().set_original_next_raw(Some(tail.clone()));
        let optional = opt(child);

        assert_eq!(
            optional
                .borrow()
                .to_string_pretty(&StringificationProperties::DEFAULT),
            "[fly away]"
        );
    }

    #[test]
    fn to_string_of_unwrapped_group_has_no_parens() {
        // [(fly)] stringifies as [fly], not [(fly)]
        let optional = opt(GroupPatternElement::new_rc(lit("fly")));
        assert_eq!(
            optional
                .borrow()
                .to_string_pretty(&StringificationProperties::DEFAULT),
            "[fly]"
        );
    }
}
