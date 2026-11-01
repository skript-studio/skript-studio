//! Port of Java `ch.njol.skript.patterns.OptionalPatternElement`.
//!
//! OWNER: Agent 1. Replace the `todo!()` bodies with the faithful port.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

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
    /// Constructor returning a shared handle. DIVERGENCE-critical: the Java
    /// constructor converts `[(a|b)]` into `[a|b]` when the child is a
    /// `GroupPatternElement` whose `next == null` — port that here.
    pub fn new_rc(child: ElementRef) -> ElementRef {
        let child = {
            // TODO(Agent 1): unwrap [(...)] to [...] per the Java constructor.
            child
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
        // TODO(Agent 1): port OptionalPatternElement.match:
        //   let mut copy = result.clone();
        //   if self.pattern_element.borrow().match_(expr, &mut copy) {
        //       *result = copy; return true;
        //   }
        //   self.match_next(expr, result)
        todo!("Agent 1: port OptionalPatternElement.match")
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // Java: "[" + patternElement.toFullString(properties) + "]"
        // (respect properties.exclude_optional_parts if the Java source does)
        todo!("Agent 1: port OptionalPatternElement.toString")
    }

    /// Java override: `setNext` also propagates `setLastNext(next)` into
    /// the child chain.
    fn set_next(&self, next: Option<ElementRef>) {
        // TODO(Agent 1): super.setNext(next); patternElement.setLastNext(next);
        todo!("Agent 1: port OptionalPatternElement.setNext propagation")
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
