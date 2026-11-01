//! Port of Java `ch.njol.skript.patterns.Keyword`.
//!
//! OWNER: Agent 6 (same agent as pattern.rs).

use crate::elements::{
    ChoicePatternElement, ElementRef, GroupPatternElement, LiteralPatternElement,
};

/// A required component of a pattern, used as a cheap prefilter before the
/// full match (see `SkriptPattern.match`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyword {
    /// Java `SimpleKeyword` — requires a specific string to be present.
    Simple {
        keyword: String,
        starting: bool,
        ending: bool,
    },
    /// Java `ChoiceKeyword` — at least one alternative's keyword set must
    /// be fully present.
    Choice { choices: Vec<Vec<Keyword>> },
}

impl Keyword {
    /// Java `Keyword.isPresent(String)`.
    pub fn is_present(&self, expr: &str) -> bool {
        match self {
            Keyword::Simple {
                keyword,
                starting,
                ending,
            } => {
                if *starting {
                    expr.starts_with(keyword.as_str())
                } else if *ending {
                    expr.ends_with(keyword.as_str())
                } else {
                    expr.contains(keyword.as_str())
                }
            }
            Keyword::Choice { choices } => choices
                .iter()
                .any(|keywords| keywords.iter().all(|k| k.is_present(expr))),
        }
    }

    /// Port of `Keyword.buildKeywords(PatternElement)` — walks the
    /// `original_next` chain. Choice recursion only at depth <= 1; groups
    /// recurse with depth + 1; ParseTag elements do not affect `starting`.
    pub fn build_keywords(first: &ElementRef) -> Vec<Keyword> {
        todo!("Agent 6: port buildKeywords(first, starting=true, depth=0)")
    }

    /// Port of `Keyword.computeMinLength(PatternElement)` — walks the
    /// `original_next` chain summing mandatory characters (literals count
    /// non-space chars; choices take the minimum alternative; optional,
    /// type, regex and parse-tag elements count 0).
    pub fn compute_min_length(first: &ElementRef) -> usize {
        todo!("Agent 6: port computeMinLength")
    }
}
