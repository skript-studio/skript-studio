//! Port of Java `ch.njol.skript.patterns.GroupPatternElement`.
//!
//! OWNER: Agent 1. Replace the `todo!()` bodies with the faithful port.

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
        // Java: return patternElement.match(expr, matchResult);  (NO copy)
        let child = self.pattern_element.clone();
        let matched = child.borrow().match_(expr, result);
        matched
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // Java: "(" + patternElement.toFullString(properties) + ")"
        // (respect properties.exclude_optional_parts / use_angle_brackets
        //  if the Java source does — READ the Java file)
        todo!("Agent 1: port GroupPatternElement.toString")
    }

    /// Java override: `setNext` also propagates `setLastNext(next)` into
    /// the child chain.
    fn set_next(&self, next: Option<ElementRef>) {
        // TODO(Agent 1): super.setNext(next); patternElement.setLastNext(next);
        todo!("Agent 1: port GroupPatternElement.setNext propagation")
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
