//! Port of Java `ch.njol.skript.patterns.ChoicePatternElement`.
//!
//! OWNER: Agent 2. Replace the `todo!()` bodies with the faithful port.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::elements::{ElementRef, PatternElement};
use crate::match_result::MatchResult;
use crate::pattern::StringificationProperties;

/// Multiple options, e.g. `hello|world`. Alternatives live behind a
/// `RefCell` because the compiler builds them incrementally (`add`,
/// `set_last`).
#[derive(Debug)]
pub struct ChoicePatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    pub pattern_elements: RefCell<Vec<ElementRef>>,
}

impl ChoicePatternElement {
    /// Constructor returning a shared handle.
    pub fn new_rc() -> ElementRef {
        let element = ChoicePatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            pattern_elements: RefCell::new(Vec::new()),
        };
        Rc::new(RefCell::new(element))
    }

    /// Java `void add(PatternElement)`.
    pub fn add(&self, element: ElementRef) {
        todo!("Agent 2: port add")
    }

    /// Java `PatternElement getLast()`.
    pub fn get_last(&self) -> Option<ElementRef> {
        todo!("Agent 2: port getLast")
    }

    /// Java `void setLast(PatternElement)` — replaces the last alternative.
    pub fn set_last(&self, element: ElementRef) {
        todo!("Agent 2: port setLast")
    }

    /// Snapshot of the alternatives (cloned Vec — avoids RefCell hazards).
    pub fn alternatives(&self) -> Vec<ElementRef> {
        self.pattern_elements.borrow().clone()
    }
}

impl PatternElement for ChoicePatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // TODO(Agent 2): port ChoicePatternElement.match — try each
        // alternative IN ORDER on a fresh clone; first success wins and is
        // written back into *result:
        //   for alt in self.alternatives() {
        //       let mut copy = result.clone();
        //       if alt.borrow().match_(expr, &mut copy) { *result = copy; return true; }
        //   }
        //   false
        todo!("Agent 2: port ChoicePatternElement.match")
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // Java: patternElements.stream().map(e -> e.toFullString(properties))
        //      .collect(joining("|"))
        todo!("Agent 2: port ChoicePatternElement.toString")
    }

    /// Java override: `setNext` also propagates `setLastNext(next)` to
    /// EVERY alternative.
    fn set_next(&self, next: Option<ElementRef>) {
        // TODO(Agent 2):
        //   self.set_next_raw(next.clone());
        //   for alt in self.alternatives() { alt.borrow().set_last_next(next.clone()); }
        todo!("Agent 2: port ChoicePatternElement.setNext propagation")
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
