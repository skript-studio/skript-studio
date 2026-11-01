//! Port of Java `ch.njol.skript.patterns.RegexPatternElement`.
//!
//! OWNER: Agent 4. Replace the `todo!()` bodies with the faithful port.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::elements::{ElementRef, PatternElement};
use crate::error::MalformedPatternError;
use crate::match_result::{MatchResult, RegexMatchInfo};
use crate::pattern::StringificationProperties;

/// A regex, e.g. `<.+>`. Compiled with `fancy_regex` (Java uses
/// `java.util.regex`; DIVERGENCE: fancy-regex covers lookarounds and
/// backreferences, the common Java features used in Skript patterns).
#[derive(Debug)]
pub struct RegexPatternElement {
    pub(crate) next_field: RefCell<Option<ElementRef>>,
    pub(crate) original_next_field: RefCell<Option<ElementRef>>,
    /// The regex source as written in the pattern (for toString).
    pub source: String,
    /// The compiled regex. `fancy_regex::Regex` implements Debug.
    pub pattern: fancy_regex::Regex,
}

impl RegexPatternElement {
    /// Compiles the regex source; errors become `MalformedPatternError`
    /// (Java: `PatternSyntaxException` wrapped in MalformedPatternException
    /// by the compiler — here the constructor itself reports it).
    pub fn new_rc(source: &str) -> Result<ElementRef, MalformedPatternError> {
        let pattern = fancy_regex::Regex::new(source).map_err(|e| {
            MalformedPatternError::new(source, format!("invalid regex <{source}>: {e}"))
        })?;
        let element = RegexPatternElement {
            next_field: RefCell::new(None),
            original_next_field: RefCell::new(None),
            source: source.to_string(),
            pattern,
        };
        Ok(Rc::new(RefCell::new(element)))
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

impl PatternElement for RegexPatternElement {
    fn match_(&self, expr: &str, result: &mut MatchResult) -> bool {
        // TODO(Agent 4): port RegexPatternElement.match:
        //  let expr_index = result.expr_offset;
        //  let mut next_offset = parse_utils::next(expr, expr_index, result.parse_context);
        //  while let Some(offset) = next_offset {
        //      // Java: matcher.region(expr_index, offset).matches() — the
        //      // regex must match the ENTIRE slice expr[expr_index..offset].
        //      let slice = &expr[expr_index..offset];
        //      if let Some(m) = self.pattern.find(slice)... check full-slice match {
        //          let mut copy = result.clone();
        //          copy.expr_offset = offset;
        //          if self.match_next(expr, &mut copy) {
        //              copy.regex_results.insert(0, RegexMatchInfo { ... }); // addFirst!
        //              *result = copy;
        //              return true;
        //          }
        //      }
        //      next_offset = parse_utils::next(expr, offset, result.parse_context);
        //  }
        //  false
        todo!("Agent 4: port RegexPatternElement.match")
    }

    fn to_string_pretty(&self, _properties: &StringificationProperties) -> String {
        // Java: "<" + pattern + ">"
        format!("<{}>", self.source)
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
