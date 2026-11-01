//! Port of Java `ch.njol.skript.patterns.ParseTagPatternElement`.
//!
//! OWNER: Agent 2 (same agent as choice.rs — the setNext logic here
//! manipulates ChoicePatternElement alternatives). Replace the `todo!()`
//! bodies with the faithful port.

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

impl ParseTagPatternElement {
    /// Java `ParseTagPatternElement(String tag)` — also parses an integer
    /// mark out of the tag when possible.
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
        // TODO(Agent 2): port ParseTagPatternElement.match:
        //   if let Some(tag) = self.tag.borrow().as_ref() {
        //       if !tag.is_empty() { result.tags.push(tag.clone()); }
        //   }
        //   result.mark ^= self.mark;
        //   self.match_next(expr, result)
        todo!("Agent 2: port ParseTagPatternElement.match")
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // TODO(Agent 2): port ParseTagPatternElement.toString:
        //   properties.exclude_parse_tags -> ""
        //   tag None     -> format!("{}:", self.mark)
        //   tag Some("") -> ""
        //   tag Some(t)  -> format!("{}:", t)
        todo!("Agent 2: port ParseTagPatternElement.toString")
    }

    /// Java override — READ THE JAVA FILE CAREFULLY. When the tag is
    /// `Some("")` this derives tags from the FOLLOWING element:
    ///   - next is a LiteralPatternElement  -> tag = next.toString().trim()
    ///     (the `(:a)` case)
    ///   - next is a Group/Optional whose inner element is a Choice ->
    ///     rewrite each non-empty Literal alternative in place into a new
    ///     ParseTagPatternElement(tag = literal.trim()) with
    ///     set_next(literal) and original_next = literal (the `:(a|b)` /
    ///     `:[a|b]` case). Use ChoicePatternElement's interior mutability;
    ///     you will need to replace the alternative inside the choice's
    ///     RefCell<Vec<...>>.
    /// Finally calls super.setNext(next).
    fn set_next(&self, next: Option<ElementRef>) {
        todo!("Agent 2: port ParseTagPatternElement.setNext tag derivation")
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
