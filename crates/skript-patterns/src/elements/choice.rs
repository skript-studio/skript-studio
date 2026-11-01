//! Port of Java `ch.njol.skript.patterns.ChoicePatternElement`.
//!
//! OWNER: Agent 2. Faithful port of:
//! `/home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/ChoicePatternElement.java`

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

    /// Java `void add(PatternElement patternElement)`.
    pub fn add(&self, element: ElementRef) {
        self.pattern_elements.borrow_mut().push(element);
    }

    /// Java `PatternElement getLast()`.
    ///
    /// DIVERGENCE: Java's `List.getLast()` throws when the list is empty;
    /// the frozen contract mandates `Option<ElementRef>` instead
    /// (`None` = no alternatives yet).
    pub fn get_last(&self) -> Option<ElementRef> {
        self.pattern_elements.borrow().last().cloned()
    }

    /// Java `void setLast(PatternElement patternElement)` — replaces the
    /// last alternative (`removeLast()` + `add(...)`).
    ///
    /// DIVERGENCE: Java's `removeLast()` throws when the list is empty;
    /// here the element is simply pushed (the compiler only calls this
    /// after a successful `get_last`).
    pub fn set_last(&self, element: ElementRef) {
        let mut elements = self.pattern_elements.borrow_mut();
        elements.pop();
        elements.push(element);
    }

    /// Snapshot of the alternatives (cloned Vec — avoids RefCell hazards).
    ///
    /// DIVERGENCE: Java's `getPatternElements()` returns a live view of
    /// the list; the port returns a cloned snapshot (the compiler mutates
    /// the list through `add`/`set_last`, and `parse_tag::set_next`
    /// mutates it through `pattern_elements` directly).
    pub fn alternatives(&self) -> Vec<ElementRef> {
        self.pattern_elements.borrow().clone()
    }
}

impl PatternElement for ChoicePatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // Java: for each alternative, match on a COPY of the result; the
        // first success wins and its (mutated) copy becomes the result.
        // A failed attempt must not leak state into the next attempt.
        for alt in self.alternatives() {
            let mut copy = result.clone();
            if alt.borrow().match_(expr, &mut copy) {
                *result = copy;
                return true;
            }
            // Failure: discard the copy, try the next alternative on a
            // fresh clone of the ORIGINAL result.
        }
        false
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // Java: patternElements.stream()
        //         .map(element -> element.toFullString(properties))
        //         .collect(Collectors.joining("|"))
        self.alternatives()
            .iter()
            .map(|element| element.borrow().to_full_string(properties))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Java override: `setNext` also propagates `setLastNext(next)` to
    /// EVERY alternative.
    fn set_next(&self, next: Option<ElementRef>) {
        // Java: super.setNext(next);
        self.set_next_raw(next.clone());
        // Java: for (PatternElement patternElement : patternElements)
        //          patternElement.setLastNext(next);
        for alt in self.alternatives() {
            alt.borrow().set_last_next(next.clone());
        }
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
    use crate::elements::LiteralPatternElement;
    use std::any::Any;
    use std::cell::Ref;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Test-only element: records each match attempt in a shared log, mutates
    /// the result (so state restoration between alternatives is observable),
    /// then fails or succeeds.
    #[derive(Debug)]
    struct Probe {
        next_field: RefCell<Option<ElementRef>>,
        original_next_field: RefCell<Option<ElementRef>>,
        id: usize,
        fail: bool,
        log: Rc<RefCell<Vec<usize>>>,
    }

    impl Probe {
        fn new_rc(id: usize, fail: bool, log: Rc<RefCell<Vec<usize>>>) -> ElementRef {
            Rc::new(RefCell::new(Probe {
                next_field: RefCell::new(None),
                original_next_field: RefCell::new(None),
                id,
                fail,
                log,
            }))
        }
    }

    impl PatternElement for Probe {
        fn match_(&self, _expr: &str, result: &mut MatchResult) -> bool {
            self.log.borrow_mut().push(self.id);
            // Mutate so a failed attempt would be visible if it leaked.
            result.tags.push(format!("probe{}", self.id));
            result.mark ^= self.id + 1;
            !self.fail
        }

        fn to_string_pretty(&self, _properties: &StringificationProperties) -> String {
            format!("<probe{}>", self.id)
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

    fn as_choice(element: &ElementRef) -> Ref<'_, ChoicePatternElement> {
        Ref::map(element.borrow(), |borrowed| {
            borrowed
                .as_any()
                .downcast_ref::<ChoicePatternElement>()
                .expect("not a ChoicePatternElement")
        })
    }

    #[test]
    fn add_get_last_set_last() {
        let choice = ChoicePatternElement::new_rc();
        let c = as_choice(&choice);
        assert!(c.get_last().is_none());

        let a = LiteralPatternElement::new_rc("a");
        let b = LiteralPatternElement::new_rc("b");
        c.add(a);
        c.add(b.clone());
        assert_eq!(c.alternatives().len(), 2);
        assert!(Rc::ptr_eq(&c.get_last().unwrap(), &b));

        // set_last REPLACES the last alternative instead of appending.
        let replacement = LiteralPatternElement::new_rc("z");
        c.set_last(replacement.clone());
        assert_eq!(c.alternatives().len(), 2);
        assert!(Rc::ptr_eq(&c.get_last().unwrap(), &replacement));
    }

    #[test]
    fn match_tries_alternatives_in_order_and_restores_state_between_failures() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let choice = ChoicePatternElement::new_rc();
        {
            let c = as_choice(&choice);
            c.add(Probe::new_rc(1, true, log.clone())); // fails
            c.add(Probe::new_rc(2, false, log.clone())); // succeeds
        }

        let mut result = MatchResult::new(0);
        result.mark = 7;
        result.tags.push("pre".to_string());

        assert!(choice.borrow().match_("anything", &mut result));

        // Alternatives were tried IN ORDER (1 before 2).
        assert_eq!(*log.borrow(), vec![1, 2]);
        // The failed first attempt must not leak into the winning state:
        // probe1's tag/mark mutations are gone, probe2's survive.
        assert_eq!(result.tags, vec!["pre".to_string(), "probe2".to_string()]);
        assert_eq!(result.mark, 7 ^ (2 + 1));
    }

    #[test]
    fn match_fails_when_all_alternatives_fail() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let choice = ChoicePatternElement::new_rc();
        {
            let c = as_choice(&choice);
            c.add(Probe::new_rc(1, true, log.clone()));
            c.add(Probe::new_rc(2, true, log.clone()));
        }

        let mut result = MatchResult::new(0);
        assert!(!choice.borrow().match_("x", &mut result));
        // Every alternative got a chance.
        assert_eq!(*log.borrow(), vec![1, 2]);
        // Result contents after a failure are unspecified — no assertions.
    }

    #[test]
    fn match_first_success_wins_over_later_alternatives() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let choice = ChoicePatternElement::new_rc();
        {
            let c = as_choice(&choice);
            c.add(Probe::new_rc(1, false, log.clone())); // succeeds
            c.add(Probe::new_rc(2, false, log.clone())); // never tried
        }

        let mut result = MatchResult::new(0);
        assert!(choice.borrow().match_("x", &mut result));
        assert_eq!(*log.borrow(), vec![1]);
        assert_eq!(result.tags, vec!["probe1".to_string()]);
    }

    #[test]
    fn set_next_propagates_to_every_alternative_chain_end() {
        let choice = ChoicePatternElement::new_rc();
        // Alternative chains of depth 1 and 2.
        let p1 = Probe::new_rc(1, false, Rc::new(RefCell::new(Vec::new())));
        let p2 = Probe::new_rc(2, false, Rc::new(RefCell::new(Vec::new())));
        let p3 = Probe::new_rc(3, false, Rc::new(RefCell::new(Vec::new())));
        p1.borrow().set_next_raw(Some(p2.clone())); // p1 -> p2
        {
            let c = as_choice(&choice);
            c.add(p1.clone());
            c.add(p3.clone());
        }

        let tail = Probe::new_rc(9, false, Rc::new(RefCell::new(Vec::new())));
        choice.borrow().set_next(Some(tail.clone()));

        // The choice itself points at the new next...
        assert!(Rc::ptr_eq(&choice.borrow().next().unwrap(), &tail));
        // ...and setLastNext reached the END of each alternative's chain
        // without disturbing the chain's inner links.
        assert!(Rc::ptr_eq(&p1.borrow().next().unwrap(), &p2));
        assert!(Rc::ptr_eq(&p2.borrow().next().unwrap(), &tail));
        assert!(Rc::ptr_eq(&p3.borrow().next().unwrap(), &tail));
    }

    #[test]
    fn to_string_pretty_joins_alternatives_with_pipe() {
        // Mirrors how PatternCompiler builds `a|b|` (trailing `|` appends an
        // empty literal alternative).
        let choice = ChoicePatternElement::new_rc();
        {
            let c = as_choice(&choice);
            c.add(LiteralPatternElement::new_rc("hello"));
            c.add(LiteralPatternElement::new_rc("world"));
            c.add(LiteralPatternElement::new_rc(""));
        }
        assert_eq!(
            choice
                .borrow()
                .to_string_pretty(&StringificationProperties::DEFAULT),
            "hello|world|"
        );

        // Empty choice stringifies to the empty string (Java: joining an
        // empty stream).
        let empty = ChoicePatternElement::new_rc();
        assert_eq!(
            empty
                .borrow()
                .to_string_pretty(&StringificationProperties::DEFAULT),
            ""
        );
    }

    #[test]
    fn to_string_pretty_uses_each_alternatives_full_string() {
        // An alternative's to_full_string includes its originalNext chain,
        // e.g. the `bc` in `a|bc` (appendElement appends to the LAST
        // alternative, so the choice is "a" | "b" + "c").
        let choice = ChoicePatternElement::new_rc();
        let b = LiteralPatternElement::new_rc("b");
        let c = LiteralPatternElement::new_rc("c");
        b.borrow().set_original_next_raw(Some(c.clone()));
        {
            let ch = as_choice(&choice);
            ch.add(LiteralPatternElement::new_rc("a"));
            ch.add(b);
        }
        assert_eq!(
            choice
                .borrow()
                .to_string_pretty(&StringificationProperties::DEFAULT),
            "a|bc"
        );
    }
}
