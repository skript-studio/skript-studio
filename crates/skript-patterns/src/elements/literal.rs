//! Port of Java `ch.njol.skript.patterns.LiteralPatternElement`.
//!
//! OWNER: Agent 1. Replace the `todo!()` bodies with the faithful port.
//! Keep every existing signature (frozen contract, see PORTING_CONTRACTS.md).

use std::any::Any;
use std::cell::RefCell;

use crate::elements::{ElementRef, PatternElement};
use crate::match_result::MatchResult;
use crate::pattern::StringificationProperties;

/// A literal string to match, e.g. `hello world`. Spaces are handled
/// leniently (see Java source). The literal is stored lowercased.
#[derive(Debug)]
pub struct LiteralPatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    /// Lowercased literal text (Java stores `char[]`).
    pub literal: String,
}

impl LiteralPatternElement {
    /// Constructor returning a shared handle (used by the compiler).
    pub fn new_rc(literal: impl Into<String>) -> ElementRef {
        let element = LiteralPatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            literal: literal.into().to_lowercase(),
        };
        Rc::new(RefCell::new(element))
    }

    /// The lowercased literal text.
    pub fn literal(&self) -> &str {
        &self.literal
    }

    pub fn is_empty(&self) -> bool {
        self.literal.is_empty()
    }
}

use std::rc::Rc;

impl PatternElement for LiteralPatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // TODO(Agent 1): port LiteralPatternElement.match — char-by-char,
        // case-insensitive, with the Java space-handling rules:
        //   pattern ' ' at input index 0 or input end  -> skip (continue)
        //   input char is ' '                          -> consume it
        //   previous input char is ' '                 -> skip (continue)
        //   otherwise                                  -> fail
        // Non-space chars must match case-insensitively (Unicode lowercase).
        // On full consumption set result.expr_offset, then match_next.
        todo!("Agent 1: port LiteralPatternElement.match")
    }

    fn to_string_pretty(&self, _properties: &StringificationProperties) -> String {
        // Java: return new String(literal); (the lowercased text)
        self.literal.clone()
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

    // TODO(Agent 1): port the Java behaviour as tests:
    // - exact literal match succeeds and advances expr_offset
    // - case-insensitive matching ("DETTONATE" vs "detonate")
    // - space rules: " fly" matches " fly", "  fly" and "fly";
    //   "fly " does NOT match "flyx"; trailing space at input end is skipped
    // - failure leaves the overall match failing
}
