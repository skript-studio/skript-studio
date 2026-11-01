//! Port of Java `ch.njol.skript.patterns.TypePatternElement`.
//!
//! OWNER: Agent 3. Replace the `todo!()` bodies with the faithful port.
//! This is the hardest element: offset iteration + the TypeResolver seam.

use std::any::Any;
use std::cell::RefCell;

use crate::elements::{ElementRef, PatternElement};
use crate::match_result::{MatchResult, ParseFlags, TypeSpec};
use crate::pattern::StringificationProperties;

/// A `%type%` slot, e.g. `%number%`, `%-entities%`, `%entity/location%`,
/// `%objects%@-1%` (time state).
#[derive(Debug)]
pub struct TypePatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    /// `/`-separated type alternatives, plural-resolved (Java: ClassInfo[]).
    pub classes: Vec<TypeSpec>,
    /// Leading `-` in the pattern.
    pub is_nullable: bool,
    /// `!0` bits survive; `*` clears PARSE_EXPRESSIONS, `~` clears PARSE_LITERALS.
    pub flag_mask: u32,
    /// Trailing `@n` (time state).
    pub time: i32,
    /// Which slot of `MatchResult::expressions` this element fills.
    pub expression_index: usize,
}

impl TypePatternElement {
    /// Port of Java `TypePatternElement.fromString(String, int)`.
    ///
    /// DIVERGENCE: Java resolves type names through `Classes.getClassInfo`
    /// (throws on unknown); the static port keeps the raw code name and
    /// tolerates unknowns. Use `parse_utils::is_plural` for plural
    /// detection (it returns the singularized string + plural flag).
    pub fn from_string(string: &str, expression_index: usize) -> Self {
        todo!("Agent 3: port fromString (flags -, *, ~; @time; / split; plural)")
    }

    /// Java `ExprInfo getExprInfo()` equivalent for the resolver seam.
    pub fn expr_specs(&self) -> &[TypeSpec] {
        &self.classes
    }
}

impl PatternElement for TypePatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // TODO(Agent 3): port TypePatternElement.match, adapted:
        //  1. initOffset / advanceOffset iteration over candidate end
        //     offsets (uses the next literal element when present, else
        //     parse_utils::next). Port initOffset/advanceOffset faithfully
        //     incl. the whitespace-literal fallback handling.
        //  2. for each candidate offset:
        //       let substring = &expr[result.expr_offset..offset];
        //       let mut copy = result.clone();
        //       copy.expr_offset = offset;
        //       if !self.match_next(expr, &mut copy) { continue; }
        //       let effective = result.flags.and_mask(self.flag_mask);
        //       let accepted = with_type_resolver(|r|
        //           r.parse_expression(substring, &self.classes, effective, result.parse_context));
        //       if let Some(slot) = accepted {
        //           copy.expressions[self.expression_index] = Some(slot);
        //           *result = copy;
        //           return true;
        //       }
        //  3. false when the iteration is exhausted.
        // DIVERGENCE: parse caches, log handlers, applyTimeState and the
        // unparsed-literals backup loop are NOT ported (static analysis).
        todo!("Agent 3: port TypePatternElement.match")
    }

    fn to_string_pretty(&self, properties: &StringificationProperties) -> String {
        // Port of Java toString(StringificationProperties):
        // "%" + ["-" if is_nullable and !excludeTypeFlags] +
        // ["~" if PARSE_LITERALS cleared / "*" if PARSE_EXPRESSIONS cleared] +
        // classes joined with "/" (plural via parse_utils::to_english_plural
        // when is_plural) + ["@time" when != 0 and !excludeTypeFlags] + "%"
        todo!("Agent 3: port TypePatternElement.toString")
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
