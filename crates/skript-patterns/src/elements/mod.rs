//! The pattern-element trait and the shared linked-list plumbing, ported
//! from Java `ch.njol.skript.patterns.PatternElement`.
//!
//! The Java class hierarchy (`PatternElement` base with `next` /
//! `originalNext` fields and 8 subclasses) becomes a trait object graph:
//! every element owns its two link fields behind `RefCell`s and implements
//! [`PatternElement`]. See `PORTING_CONTRACTS.md` for the conventions.

pub mod choice;
pub mod group;
pub mod literal;
pub mod optional;
pub mod parse_tag;
pub mod regex;
pub mod type_element;

pub use choice::ChoicePatternElement;
pub use group::GroupPatternElement;
pub use literal::LiteralPatternElement;
pub use optional::OptionalPatternElement;
pub use parse_tag::ParseTagPatternElement;
pub use regex::RegexPatternElement;
pub use type_element::TypePatternElement;

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::match_result::MatchResult;
use crate::pattern::StringificationProperties;

/// Shared handle to a pattern element (Java: `PatternElement` reference).
pub type ElementRef = Rc<RefCell<dyn PatternElement>>;

/// Port of Java `ch.njol.skript.patterns.PatternElement`.
///
/// # Matching convention
/// `true` = success and `*result` contains the winning state (write the
/// successful copy back!). `false` = failure and `*result` is unspecified.
pub trait PatternElement: std::fmt::Debug {
    /// Java `MatchResult match(String expr, MatchResult matchResult)`.
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool;

    /// Java `String toString(StringificationProperties properties)`.
    fn to_string_pretty(&self, properties: &StringificationProperties) -> String;

    /// The element to match after this element's content completes. For
    /// choice alternatives this is back-patched past the whole choice.
    fn next(&self) -> Option<ElementRef>;

    /// The syntactic successor at the same nesting level (used by keyword
    /// building and min-length computation).
    fn original_next(&self) -> Option<ElementRef>;

    /// Sets only this element's `next` field (no propagation).
    fn set_next_raw(&self, next: Option<ElementRef>);

    /// Sets only this element's `original_next` field (no propagation).
    fn set_original_next_raw(&self, next: Option<ElementRef>);

    /// Java `void setNext(PatternElement next)` — virtual; `Choice`,
    /// `Group`, `Optional` and `ParseTag` override it to propagate.
    fn set_next(&self, next: Option<ElementRef>) {
        self.set_next_raw(next);
    }

    /// Port of Java `setLastNext`: walk the `next` chain starting at
    /// `self` and set `next` on the last element found.
    fn set_last_next(&self, new_next: Option<ElementRef>) {
        if self.next().is_none() {
            self.set_next(new_next);
            return;
        }
        let mut cursor = self.next().unwrap();
        loop {
            let deeper = {
                let borrowed = cursor.borrow();
                borrowed.next()
            };
            match deeper {
                None => {
                    let element = cursor.clone();
                    element.borrow().set_next(new_next);
                    return;
                }
                Some(next) => cursor = next,
            }
        }
    }

    /// Port of Java `matchNext`: continue with `next`, or succeed when the
    /// whole input has been consumed.
    fn match_next(&self, expr: &str, result: &mut MatchResult) -> bool {
        match self.next() {
            None => result.expr_offset == expr.len(),
            Some(next) => {
                let element = next.clone();
                let matched = element.borrow().match_(expr, result);
                matched
            }
        }
    }

    /// This element plus its `original_next` chain, stringified. Used by
    /// `ChoicePatternElement` / `OptionalPatternElement` / `GroupPatternElement`
    /// `toString` implementations (Java `toFullString`).
    fn to_full_string(&self, properties: &StringificationProperties) -> String {
        let mut out = self.to_string_pretty(properties);
        let mut cursor = self.original_next();
        while let Some(element) = cursor {
            let (text, next) = {
                let borrowed = element.borrow();
                (
                    borrowed.to_string_pretty(properties),
                    borrowed.original_next(),
                )
            };
            out.push_str(&text);
            cursor = next;
        }
        out
    }

    /// For downcasting (Java `instanceof` checks in `Keyword`,
    /// `TypePatternElement`, `ParseTagPatternElement`, `PatternCompiler`).
    fn as_any(&self) -> &dyn Any;
}
