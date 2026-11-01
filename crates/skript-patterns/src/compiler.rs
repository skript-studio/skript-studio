//! Port of Java `ch.njol.skript.patterns.PatternCompiler`.
//!
//! OWNER: Agent 5. THE critical module: the parser of the pattern DSL.
//!
//! Java source: /home/z/my-project/Skript/src/main/java/ch/njol/skript/patterns/PatternCompiler.java
//! `nextBracket` lives in SkriptParser.java:1285 — ported below as a
//! private helper operating on the same char buffer the compiler uses.
//!
//! The pattern structure is a linked list of `PatternElement`s, where
//! `next` points to the next element to be matched (which can be on an
//! outer level), and where `original_next` points to the next element on
//! the same level.
//!
//! DIVERGENCE (UTF-16 → chars): Java indexes pattern strings by UTF-16
//! code units (`String.charAt` / `substring`). This port decodes the
//! pattern to a `Vec<char>` per recursion level and indexes by `char`;
//! for the (BMP-heavy, ~all-ASCII) SkriptHub corpus the two are identical,
//! and `char` indexing can never slice through a multi-byte sequence.

use std::cell::RefCell;
use std::rc::Rc;

use crate::elements::{
    ChoicePatternElement, ElementRef, GroupPatternElement, LiteralPatternElement,
    OptionalPatternElement, ParseTagPatternElement, RegexPatternElement, TypePatternElement,
};
use crate::error::MalformedPatternError;
use crate::pattern::SkriptPattern;

/// Port of `PatternCompiler.compile(String)` — the public entry point.
///
/// Java wraps unexpected `RuntimeException`s into
/// "caught exception while compiling pattern"; the Rust port has no
/// unexpected runtime exceptions (every failure path returns
/// [`MalformedPatternError`] directly), so that catch has no equivalent.
/// DIVERGENCE: a trailing `\` is reported by [`compile_element`] as a
/// `MalformedPatternError` instead of Java's `StringIndexOutOfBounds`
/// being caught here and wrapped.
pub fn compile_pattern(pattern: &str) -> Result<SkriptPattern, MalformedPatternError> {
    let (first, expression_amount) = compile_chain(pattern)?;
    Ok(SkriptPattern::new(first, expression_amount))
}

/// Testable core of [`compile_pattern`]: compiles `pattern` into the first
/// link of the element chain and returns it together with the number of
/// `%type%` slots encountered (Java: the final value of the shared
/// `AtomicInteger expressionOffset`).
///
/// `SkriptPattern::new` (Agent 6) builds keywords/min-length and may still
/// `todo!()`, so structure tests use this function directly.
fn compile_chain(pattern: &str) -> Result<(ElementRef, usize), MalformedPatternError> {
    let mut expression_offset = 0usize; // Java: new AtomicInteger(0)
    let chars: Vec<char> = pattern.chars().collect();
    let first = compile_element(&chars, &mut expression_offset)?;
    Ok((first, expression_offset))
}

/// Port of `PatternCompiler.compile(String, AtomicInteger)` — the recursive
/// element-chain builder.
///
/// The `expression_offset` keeps track of which index the next
/// [`TypePatternElement`] should be initiated with (Java: the shared
/// `AtomicInteger` threaded through every recursion level).
fn compile_element(
    pattern: &[char],
    expression_offset: &mut usize,
) -> Result<ElementRef, MalformedPatternError> {
    let mut literal_builder = String::new();
    let mut first: Option<ElementRef> = None;

    let mut i = 0usize;
    while i < pattern.len() {
        let c = pattern[i];
        match c {
            '[' => {
                if !literal_builder.is_empty() {
                    first = Some(append_element(
                        first,
                        LiteralPatternElement::new_rc(literal_builder.clone()),
                    ));
                    literal_builder.clear();
                }

                let end = next_bracket(pattern, ']', '[', i + 1, true)?;
                let child = compile_element(&pattern[i + 1..end], expression_offset)?;

                first = Some(append_element(first, OptionalPatternElement::new_rc(child)));

                i = end; // the loop step below moves past the closing bracket
            }
            '(' => {
                if !literal_builder.is_empty() {
                    first = Some(append_element(
                        first,
                        LiteralPatternElement::new_rc(literal_builder.clone()),
                    ));
                    literal_builder.clear();
                }

                let end = next_bracket(pattern, ')', '(', i + 1, true)?;
                let child = compile_element(&pattern[i + 1..end], expression_offset)?;

                first = Some(append_element(first, GroupPatternElement::new_rc(child)));

                i = end;
            }
            '|' => {
                if !literal_builder.is_empty() {
                    first = Some(append_element(
                        first,
                        LiteralPatternElement::new_rc(literal_builder.clone()),
                    ));
                    literal_builder.clear();
                }

                let prev_first = first.clone();

                // Java: reuse the chain head when it is already a Choice,
                // else create one seeded with the previous head (or an
                // empty element when there was none).
                let head_is_choice = match first.as_ref() {
                    Some(f) => f.borrow().as_any().is::<ChoicePatternElement>(),
                    None => false,
                };
                let choice: ElementRef = if head_is_choice {
                    first.clone().expect("checked head_is_choice above")
                } else {
                    let choice = ChoicePatternElement::new_rc();
                    {
                        let borrowed = choice.borrow();
                        let cc = borrowed
                            .as_any()
                            .downcast_ref::<ChoicePatternElement>()
                            .expect("just created a ChoicePatternElement");
                        cc.add(prev_first.unwrap_or_else(get_empty));
                    }
                    first = Some(choice.clone());
                    choice
                };
                {
                    let borrowed = choice.borrow();
                    let cc = borrowed
                        .as_any()
                        .downcast_ref::<ChoicePatternElement>()
                        .expect("head_is_choice checked above");
                    // NOTE: the Java adds an EMPTY alternative here (getEmpty()).
                    cc.add(get_empty());
                }
            }
            '¦' | ':' => {
                // The accumulated literal becomes the tag source (or, for
                // '¦', the integer parse mark).
                // Java: String tag = literalBuilder.toString(); (then reset)
                let tag = std::mem::take(&mut literal_builder);

                let element: ElementRef = if c == '¦' {
                    // Old parse marks cannot be tags.
                    let mark = tag.parse::<i32>().map_err(|_| {
                        MalformedPatternError::new(
                            chars_to_string(pattern),
                            format!("invalid parse mark at {i}"),
                        )
                    })?;
                    // DIVERGENCE: Java stores parse marks as a signed int
                    // and XORs them into MatchResult.mark; the frozen Rust
                    // API takes usize. `Integer.parseInt` semantics are
                    // mirrored exactly by `i32::from_str`; the cast only
                    // wraps for negative marks, which do not occur in the
                    // corpus (real marks are small non-negative integers).
                    ParseTagPatternElement::new_rc_mark(mark as usize)
                } else {
                    ParseTagPatternElement::new_rc_tag(tag)
                };

                first = Some(append_element(first, element));
            }
            '%' => {
                if !literal_builder.is_empty() {
                    first = Some(append_element(
                        first,
                        LiteralPatternElement::new_rc(literal_builder.clone()),
                    ));
                    literal_builder.clear();
                }

                // Java: pattern.indexOf('%', i + 1) — a PLAIN search, not
                // escape-aware (an escaped \% inside the slot is still found).
                let end = find_char(pattern, '%', i + 1).ok_or_else(|| {
                    MalformedPatternError::new(
                        chars_to_string(pattern),
                        format!("single percentage sign at {i}"),
                    )
                })?;
                let expr_offset = *expression_offset; // Java: getAndIncrement()
                *expression_offset += 1;
                let inner = chars_to_string(&pattern[i + 1..end]);
                // Java: TypePatternElement.fromString(...) — returns Self, so
                // wrap it in the shared handle here.
                let element: ElementRef = Rc::new(RefCell::new(TypePatternElement::from_string(
                    &inner,
                    expr_offset,
                )));

                first = Some(append_element(first, element));

                i = end;
            }
            '<' => {
                if !literal_builder.is_empty() {
                    first = Some(append_element(
                        first,
                        LiteralPatternElement::new_rc(literal_builder.clone()),
                    ));
                    literal_builder.clear();
                }

                let end = find_char(pattern, '>', i + 1).ok_or_else(|| {
                    MalformedPatternError::new(
                        chars_to_string(pattern),
                        format!("missing closing regex bracket '>' at {i}"),
                    )
                })?;

                // Java compiles java.util.regex.Pattern here and wraps a
                // PatternSyntaxException into a MalformedPatternException;
                // RegexPatternElement::new_rc reports the compile error as a
                // MalformedPatternError directly (DIVERGENCE: the message
                // wording is built inside new_rc, without the "at {i}" part).
                let inner = chars_to_string(&pattern[i + 1..end]);
                let element = RegexPatternElement::new_rc(&inner)?;

                first = Some(append_element(first, element));

                i = end;
            }
            '\\' => {
                // Java: i++; literalBuilder.append(pattern.charAt(i));
                // DIVERGENCE: when the backslash is the LAST character Java
                // throws StringIndexOutOfBoundsException, which the outer
                // compile(String) catches and wraps as "caught exception
                // while compiling pattern". The Rust port reports the
                // trailing backslash directly as a MalformedPatternError.
                i += 1;
                if i >= pattern.len() {
                    return Err(MalformedPatternError::new(
                        chars_to_string(pattern),
                        format!("trailing backslash at {i}"),
                    ));
                }
                literal_builder.push(pattern[i]);
            }
            _ => {
                literal_builder.push(c);
            }
        }
        i += 1;
    }

    if !literal_builder.is_empty() {
        first = Some(append_element(
            first,
            LiteralPatternElement::new_rc(literal_builder),
        ));
    }

    // Java: if (first == null) return getEmpty();
    Ok(first.unwrap_or_else(get_empty))
}

/// Port of `PatternCompiler.appendElement(PatternElement, PatternElement)`.
///
/// Adds `next` to the end of the list given by `first`; returns the new
/// first element of the list.
///
/// CRITICAL DETAILS from the Java source:
/// - if `first` is null, or is an EMPTY Literal with `next == null`,
///   return `next` (the empty head is replaced);
/// - if `first` is a Choice: `choice.setLast(appendElement(choice.getLast(), next))`
///   and return the choice (appending "hello" to "a|b" yields "a|b hello"
///   with alternatives "a" and "b hello");
/// - otherwise walk `next` to the last element, then `last.setNext(next)`
///   (VIRTUAL — Choice/Group/Optional/ParseTag overrides propagate here!)
///   and `last.originalNext = next` (plain field write, like Java's direct
///   field access which bypasses any setter).
fn append_element(first: Option<ElementRef>, next: ElementRef) -> ElementRef {
    // if "first" is null or not meaningful (empty), simply return "next"
    let first = match first {
        None => return next,
        Some(first) => first,
    };
    {
        let borrowed = first.borrow();
        if let Some(literal) = borrowed.as_any().downcast_ref::<LiteralPatternElement>() {
            if literal.is_empty() && borrowed.next().is_none() {
                return next;
            }
        }
    }

    // if appending to a choice, we need to append to the last choice
    // ex: appending "hello" to "a|b", result should be "a|b hello" where
    // the choices are "a" and "b hello"
    // Some(Some(last)) = first is a Choice with alternatives;
    // Some(None)      = first is an EMPTY Choice (Java's getLast() would
    //                   throw NoSuchElementException — the compiler never
    //                   produces this, treat like the null case);
    // None            = first is not a Choice.
    let choice_last: Option<Option<ElementRef>> = {
        let borrowed = first.borrow();
        borrowed
            .as_any()
            .downcast_ref::<ChoicePatternElement>()
            .map(|choice| choice.get_last())
    };
    if let Some(last) = choice_last {
        // `first` is a Choice. Recurse FIRST (no borrow held across the
        // recursive call), then patch the last alternative.
        let new_last = append_element(last, next.clone());
        {
            let borrowed = first.borrow();
            let choice = borrowed
                .as_any()
                .downcast_ref::<ChoicePatternElement>()
                .expect("downcast succeeded above");
            choice.set_last(new_last);
        }
        return first;
    }

    // get last element
    let mut cursor = first.clone();
    loop {
        // RefCell discipline: end the borrow before any mutating call.
        let deeper = {
            let borrowed = cursor.borrow();
            borrowed.next()
        };
        match deeper {
            Some(deeper) => cursor = deeper,
            None => break,
        }
    }

    // append to this element
    {
        let borrowed = cursor.borrow();
        // VIRTUAL call — the Choice/Group/Optional/ParseTag overrides
        // propagate `next` into their child chains here.
        borrowed.set_next(Some(next.clone()));
        // Plain write, like Java's direct `last.originalNext = next` field
        // access (bypasses any override).
        borrowed.set_original_next_raw(Some(next));
    }

    first
}

/// Port of `SkriptParser.nextBracket` (SkriptParser.java:1285).
///
/// Finds the closing bracket of the group at `start` (i.e. `start` has to
/// be *in* a group). A backslash escapes the following character,
/// including brackets.
///
/// NOTE: the Java javadoc ("otherwise it returns pattern.length()") does
/// not match the implementation — the code throws "Missing closing
/// bracket" when `is_group` and returns `-1` when not. The implementation
/// is ported, not the doc.
///
/// DIVERGENCE: Java returns `-1` when the bracket is not found and
/// `is_group == false`; that case is folded into the `Err` here. The
/// compiler only ever calls this with `is_group == true` (both bracket
/// kinds), so the `-1` path is unreachable in this port.
fn next_bracket(
    pattern: &[char],
    closing_bracket: char,
    opening_bracket: char,
    start: usize,
    is_group: bool,
) -> Result<usize, MalformedPatternError> {
    let mut index = 0usize;
    let mut i = start;
    while i < pattern.len() {
        let c = pattern[i];
        if c == '\\' {
            i += 1; // skip the escaped character (Java: i++ inside the body)
        } else if c == closing_bracket {
            if index == 0 {
                if !is_group {
                    return Err(MalformedPatternError::new(
                        chars_to_string(pattern),
                        format!("Unexpected closing bracket '{closing_bracket}'"),
                    ));
                }
                return Ok(i);
            }
            index -= 1;
        } else if c == opening_bracket {
            index += 1;
        }
        i += 1;
    }
    if is_group {
        return Err(MalformedPatternError::new(
            chars_to_string(pattern),
            format!("Missing closing bracket '{closing_bracket}'"),
        ));
    }
    // Java: return -1 (see DIVERGENCE above).
    Err(MalformedPatternError::new(
        chars_to_string(pattern),
        format!("Missing closing bracket '{closing_bracket}'"),
    ))
}

/// Java `String.indexOf(int ch, int fromIndex)` on the char buffer — a
/// plain (NOT escape-aware) forward search.
fn find_char(pattern: &[char], needle: char, from: usize) -> Option<usize> {
    let mut i = from;
    while i < pattern.len() {
        if pattern[i] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Port of `PatternCompiler.getEmpty()` — an empty literal.
fn get_empty() -> ElementRef {
    LiteralPatternElement::new_rc("")
}

/// Java passes `String` substrings down the recursion (and into error
/// constructors, which therefore carry the CURRENT level's substring);
/// rebuild the equivalent `String` from the char slice.
fn chars_to_string(chars: &[char]) -> String {
    chars.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::{
        ChoicePatternElement, GroupPatternElement, LiteralPatternElement, OptionalPatternElement,
        ParseTagPatternElement, RegexPatternElement, TypePatternElement,
    };
    use crate::match_result::{ParseFlags, TypeSpec};

    // ---- downcast / access helpers -------------------------------------
    // RefCell discipline: every outer borrow ends at a statement boundary;
    // Rc's are cloned out before any nested use.

    fn kind(el: &ElementRef) -> &'static str {
        let borrowed = el.borrow();
        let any = borrowed.as_any();
        if any.is::<LiteralPatternElement>() {
            "literal"
        } else if any.is::<OptionalPatternElement>() {
            "optional"
        } else if any.is::<GroupPatternElement>() {
            "group"
        } else if any.is::<ChoicePatternElement>() {
            "choice"
        } else if any.is::<TypePatternElement>() {
            "type"
        } else if any.is::<RegexPatternElement>() {
            "regex"
        } else if any.is::<ParseTagPatternElement>() {
            "parse_tag"
        } else {
            "unknown"
        }
    }

    fn lit(el: &ElementRef) -> String {
        let borrowed = el.borrow();
        borrowed
            .as_any()
            .downcast_ref::<LiteralPatternElement>()
            .expect("literal element")
            .literal()
            .to_string()
    }

    fn is_empty_literal(el: &ElementRef) -> bool {
        let borrowed = el.borrow();
        borrowed
            .as_any()
            .downcast_ref::<LiteralPatternElement>()
            .map(|l| l.is_empty())
            .unwrap_or(false)
    }

    /// Child of an Optional/Group element.
    fn child_of(el: &ElementRef) -> ElementRef {
        let borrowed = el.borrow();
        if let Some(optional) = borrowed.as_any().downcast_ref::<OptionalPatternElement>() {
            return optional.pattern_element();
        }
        if let Some(group) = borrowed.as_any().downcast_ref::<GroupPatternElement>() {
            return group.pattern_element();
        }
        panic!("not an optional/group element");
    }

    fn alts_of(el: &ElementRef) -> Vec<ElementRef> {
        let borrowed = el.borrow();
        borrowed
            .as_any()
            .downcast_ref::<ChoicePatternElement>()
            .expect("choice element")
            .alternatives()
    }

    fn next_of(el: &ElementRef) -> Option<ElementRef> {
        el.borrow().next()
    }

    fn onext_of(el: &ElementRef) -> Option<ElementRef> {
        el.borrow().original_next()
    }

    /// (tag, mark) of a ParseTag element.
    fn tag_info(el: &ElementRef) -> (Option<String>, usize) {
        let borrowed = el.borrow();
        let tag = borrowed
            .as_any()
            .downcast_ref::<ParseTagPatternElement>()
            .expect("parse tag element");
        let out = (tag.tag.borrow().clone(), tag.mark);
        out
    }

    /// (expression_index, classes, is_nullable, flag_mask, time).
    fn type_info(el: &ElementRef) -> (usize, Vec<TypeSpec>, bool, u32, i32) {
        let borrowed = el.borrow();
        let t = borrowed
            .as_any()
            .downcast_ref::<TypePatternElement>()
            .expect("type element");
        (
            t.expression_index,
            t.classes.clone(),
            t.is_nullable,
            t.flag_mask,
            t.time,
        )
    }

    fn regex_source(el: &ElementRef) -> String {
        let borrowed = el.borrow();
        borrowed
            .as_any()
            .downcast_ref::<RegexPatternElement>()
            .expect("regex element")
            .source()
            .to_string()
    }

    fn walk_kinds(first: &ElementRef) -> Vec<&'static str> {
        let mut out = Vec::new();
        let mut cursor = first.clone();
        loop {
            out.push(kind(&cursor));
            let deeper = {
                let borrowed = cursor.borrow();
                borrowed.next()
            };
            match deeper {
                Some(deeper) => cursor = deeper,
                None => break,
            }
        }
        out
    }

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    // ---- next_bracket (pure, no element dependencies) -------------------

    #[test]
    fn next_bracket_simple_and_nested() {
        let p = chars("[]");
        assert_eq!(next_bracket(&p, ']', '[', 1, true).unwrap(), 1);

        let p = chars("[[a]]");
        assert_eq!(next_bracket(&p, ']', '[', 1, true).unwrap(), 4);

        let p = chars("(a(b)c)");
        assert_eq!(next_bracket(&p, ')', '(', 1, true).unwrap(), 6);

        let p = chars("[a b]");
        assert_eq!(next_bracket(&p, ']', '[', 1, true).unwrap(), 4);
    }

    #[test]
    fn next_bracket_escapes_with_backslash() {
        // \] does not close the group...
        let p = chars("[a\\]b]");
        assert_eq!(next_bracket(&p, ']', '[', 1, true).unwrap(), 5);
        // ...and an escaped opening bracket does not nest.
        let p = chars("[a\\[b]");
        assert_eq!(next_bracket(&p, ']', '[', 1, true).unwrap(), 5);
        // a trailing backslash is simply skipped (Java: no exception here)
        let p = chars("[ab\\");
        assert!(next_bracket(&p, ']', '[', 1, true).is_err());
    }

    #[test]
    fn next_bracket_missing_closing() {
        let p = chars("[abc");
        let err = next_bracket(&p, ']', '[', 1, true).unwrap_err();
        assert!(err.message.contains("Missing closing bracket"), "{err}");
        let p = chars("(abc");
        let err = next_bracket(&p, ')', '(', 1, true).unwrap_err();
        assert!(err.message.contains("Missing closing bracket"), "{err}");
    }

    // ---- compiler error paths (no element dependencies) ------------------

    #[test]
    fn single_percentage_sign_is_error() {
        let err = compile_chain("100%").unwrap_err();
        assert!(err.message.contains("single percentage sign"), "{err}");
        // Java throws from the recursive compile() call, so the error carries
        // the CURRENT level's substring as its pattern:
        let err = compile_chain("[100%]").unwrap_err();
        assert!(err.message.contains("single percentage sign"), "{err}");
        assert_eq!(err.pattern, "100%");
    }

    #[test]
    fn unmatched_brackets_are_errors() {
        let err = compile_chain("[abc").unwrap_err();
        assert!(err.message.contains("Missing closing bracket"), "{err}");
        assert_eq!(err.pattern, "[abc");

        let err = compile_chain("(abc").unwrap_err();
        assert!(err.message.contains("Missing closing bracket"), "{err}");

        // unbalanced nesting: "(a[b)" — the '[' opens another ']' level
        let err = compile_chain("(a[b)").unwrap_err();
        assert!(err.message.contains("Missing closing bracket"), "{err}");
    }

    #[test]
    fn invalid_parse_mark_is_error() {
        let err = compile_chain("x¦").unwrap_err();
        assert!(err.message.contains("invalid parse mark"), "{err}");
    }

    #[test]
    fn trailing_backslash_is_error() {
        // DIVERGENCE: Java throws StringIndexOutOfBounds (caught by the
        // outer compile as "caught exception while compiling pattern").
        assert!(compile_chain("ab\\").is_err());
        // ...but inside a bracket it is a missing-bracket error (Java's
        // nextBracket skips the escaped char and runs out of input).
        let err = compile_chain("[ab\\").unwrap_err();
        assert!(err.message.contains("Missing closing bracket"), "{err}");
    }

    // ---- literal-only structure (no cross-agent dependencies) ------------

    #[test]
    fn literal_chain() {
        let (first, count) = compile_chain("hello world").unwrap();
        assert_eq!(count, 0);
        assert_eq!(kind(&first), "literal");
        assert_eq!(lit(&first), "hello world");
        assert!(next_of(&first).is_none());
        assert!(onext_of(&first).is_none());
    }

    #[test]
    fn empty_pattern_is_empty_literal() {
        let (first, count) = compile_chain("").unwrap();
        assert_eq!(count, 0);
        assert_eq!(kind(&first), "literal");
        assert!(is_empty_literal(&first));
    }

    #[test]
    fn escapes_end_up_in_the_literal() {
        let (first, _) = compile_chain("\\:").unwrap();
        assert_eq!(kind(&first), "literal");
        assert_eq!(lit(&first), ":");
        assert!(next_of(&first).is_none());

        let (first, _) = compile_chain("\\|").unwrap();
        assert_eq!(kind(&first), "literal");
        assert_eq!(lit(&first), "|");
        assert!(next_of(&first).is_none());

        // escaped % is a literal, not a type slot
        let (first, count) = compile_chain("100\\%").unwrap();
        assert_eq!(count, 0);
        assert_eq!(lit(&first), "100%");
        assert!(next_of(&first).is_none());
    }

    #[test]
    fn regex_element_structure() {
        let (first, count) = compile_chain("<[0-9]+>").unwrap();
        assert_eq!(count, 0);
        assert_eq!(kind(&first), "regex");
        assert_eq!(regex_source(&first), "[0-9]+");
        assert!(next_of(&first).is_none());
        assert!(onext_of(&first).is_none());
    }

    #[test]
    fn invalid_regex_is_error() {
        // Java wraps the PatternSyntaxException; new_rc reports it directly.
        assert!(compile_chain("<[>+>").is_err());
    }

    #[test]
    fn missing_regex_closing_bracket_is_error() {
        let err = compile_chain("<[0-9]+").unwrap_err();
        assert!(
            err.message.contains("missing closing regex bracket"),
            "{err}"
        );
    }

    // ---- THE structure checks (task list) --------------------------------

    #[test]
    fn compile_structure_checks() {
        // "hello world" → single literal chain
        let (first, count) = compile_chain("hello world").unwrap();
        assert_eq!(count, 0);
        assert_eq!(kind(&first), "literal");
        assert_eq!(lit(&first), "hello world");
        assert!(next_of(&first).is_none());
        assert!(onext_of(&first).is_none());

        // "[the] name" → Optional(literal "the") + literal " name", linked
        let (first, _) = compile_chain("[the] name").unwrap();
        assert_eq!(kind(&first), "optional");
        let child = child_of(&first);
        assert_eq!(lit(&child), "the");
        let name = next_of(&child).expect("child chain reaches 'name'");
        assert_eq!(lit(&name), " name");
        let opt_next = next_of(&first).expect("optional linked");
        assert!(Rc::ptr_eq(&opt_next, &name));
        assert!(Rc::ptr_eq(&onext_of(&first).unwrap(), &name));

        // "(a|b) c" → Choice with 2 alternatives; " c" is reachable from
        // BOTH alternatives (appendElement's choice/setNext propagation)
        let (first, _) = compile_chain("(a|b) c").unwrap();
        assert_eq!(kind(&first), "group");
        let choice = child_of(&first);
        assert_eq!(kind(&choice), "choice");
        let alts = alts_of(&choice);
        assert_eq!(alts.len(), 2);
        assert_eq!(lit(&alts[0]), "a");
        assert_eq!(lit(&alts[1]), "b");
        let tail_a = next_of(&alts[0]).expect("'a' linked");
        let tail_b = next_of(&alts[1]).expect("'b' linked");
        assert_eq!(lit(&tail_a), " c");
        assert!(Rc::ptr_eq(&tail_a, &tail_b));
        assert!(Rc::ptr_eq(&next_of(&first).unwrap(), &tail_a));
        assert!(Rc::ptr_eq(&onext_of(&first).unwrap(), &tail_a));
        assert!(next_of(&tail_a).is_none());

        // "a|b|c" → ONE Choice with 3 alternatives
        let (first, _) = compile_chain("a|b|c").unwrap();
        assert_eq!(kind(&first), "choice");
        let alts = alts_of(&first);
        assert_eq!(alts.len(), 3);
        assert_eq!(lit(&alts[0]), "a");
        assert_eq!(lit(&alts[1]), "b");
        assert_eq!(lit(&alts[2]), "c");
        for alt in &alts {
            assert!(next_of(alt).is_none());
        }

        // "%number% apples" → Type(expression_index 0, [number]) + literal
        let (first, count) = compile_chain("%number% apples").unwrap();
        assert_eq!(count, 1);
        assert_eq!(kind(&first), "type");
        let (expr_index, classes, is_nullable, flag_mask, time) = type_info(&first);
        assert_eq!(expr_index, 0);
        assert_eq!(
            classes,
            vec![TypeSpec {
                code_name: "number".to_string(),
                is_plural: false
            }]
        );
        assert!(!is_nullable);
        assert_eq!(flag_mask & ParseFlags::ALL.0, ParseFlags::ALL.0);
        assert_eq!(time, 0);
        let tail = next_of(&first).expect("type linked");
        assert_eq!(lit(&tail), " apples");
        assert!(Rc::ptr_eq(&onext_of(&first).unwrap(), &tail));
        assert!(next_of(&tail).is_none());

        // "detonate %entities%" → literal + type
        let (first, count) = compile_chain("detonate %entities%").unwrap();
        assert_eq!(count, 1);
        assert_eq!(kind(&first), "literal");
        assert_eq!(lit(&first), "detonate ");
        let ty = next_of(&first).expect("literal linked");
        assert_eq!(kind(&ty), "type");
        let (expr_index, classes, _, _, _) = type_info(&ty);
        assert_eq!(expr_index, 0);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].code_name, "entity");
        assert!(classes[0].is_plural);

        // "<[0-9]+>" → RegexPatternElement present
        let (first, _) = compile_chain("<[0-9]+>").unwrap();
        assert_eq!(kind(&first), "regex");
        assert_eq!(regex_source(&first), "[0-9]+");

        // errors
        assert!(compile_chain("100%").is_err());
        assert!(compile_chain("[abc").is_err());
        assert!(compile_chain("(abc").is_err());
    }

    // ---- type flags (depends on Agent 3's from_string) -------------------

    #[test]
    fn type_element_flags() {
        // "-" → nullable
        let (el, count) = compile_chain("%-entities%").unwrap();
        assert_eq!(count, 1);
        let (expr_index, classes, is_nullable, flag_mask, time) = type_info(&el);
        assert_eq!(expr_index, 0);
        assert!(is_nullable);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].code_name, "entity");
        assert!(classes[0].is_plural);
        assert_eq!(flag_mask & ParseFlags::ALL.0, ParseFlags::ALL.0);
        assert_eq!(time, 0);

        // "*" → clears PARSE_EXPRESSIONS
        let (el, _) = compile_chain("%*objects%").unwrap();
        let (_, classes, is_nullable, flag_mask, _) = type_info(&el);
        assert!(!is_nullable);
        assert_eq!(classes[0].code_name, "object");
        assert!(classes[0].is_plural);
        assert_eq!(
            flag_mask & ParseFlags::EXPRESSIONS.0,
            0,
            "'*' must clear PARSE_EXPRESSIONS"
        );
        assert_ne!(
            flag_mask & ParseFlags::LITERALS.0,
            0,
            "PARSE_LITERALS must survive '*'"
        );

        // "~" → clears PARSE_LITERALS
        let (el, _) = compile_chain("%~text%").unwrap();
        let (_, classes, is_nullable, flag_mask, _) = type_info(&el);
        assert!(!is_nullable);
        assert_eq!(classes[0].code_name, "text");
        assert!(!classes[0].is_plural);
        assert_eq!(
            flag_mask & ParseFlags::LITERALS.0,
            0,
            "'~' must clear PARSE_LITERALS"
        );
        assert_ne!(
            flag_mask & ParseFlags::EXPRESSIONS.0,
            0,
            "PARSE_EXPRESSIONS must survive '~'"
        );

        // "@-1" → time state
        let (el, _) = compile_chain("%number@-1%").unwrap();
        let (expr_index, classes, _, flag_mask, time) = type_info(&el);
        assert_eq!(expr_index, 0);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].code_name, "number");
        assert!(!classes[0].is_plural);
        assert_eq!(flag_mask & ParseFlags::ALL.0, ParseFlags::ALL.0);
        assert_eq!(time, -1);
    }

    // ---- choice edge cases (depend on Agent 2's Choice impl) -------------

    #[test]
    fn trailing_pipe_keeps_empty_alternative() {
        // the Java `|` handler adds getEmpty() as the new last alternative
        let (first, _) = compile_chain("a|").unwrap();
        assert_eq!(kind(&first), "choice");
        let alts = alts_of(&first);
        assert_eq!(alts.len(), 2);
        assert_eq!(lit(&alts[0]), "a");
        assert!(is_empty_literal(&alts[1]));
    }

    #[test]
    fn leading_pipe_seeds_with_empty_alternative() {
        let (first, _) = compile_chain("|a").unwrap();
        assert_eq!(kind(&first), "choice");
        let alts = alts_of(&first);
        assert_eq!(alts.len(), 2);
        assert!(is_empty_literal(&alts[0]));
        assert_eq!(lit(&alts[1]), "a");
    }

    #[test]
    fn parse_marks_in_choice_alternatives() {
        // Real Skript pattern shape (ExprLightLevel):
        // "(1¦sky|2¦sun|3¦block)" → each alternative starts with a
        // ParseTag(mark) followed by the literal.
        let (first, _) = compile_chain("(1¦sky|2¦sun|3¦block)").unwrap();
        assert_eq!(kind(&first), "group");
        let choice = child_of(&first);
        let alts = alts_of(&choice);
        assert_eq!(alts.len(), 3);
        let expected = [("sky", 1usize), ("sun", 2), ("block", 3)];
        for (alt, (text, mark)) in alts.iter().zip(expected) {
            assert_eq!(kind(alt), "parse_tag", "alternative is a ParseTag");
            let (tag, got_mark) = tag_info(alt);
            assert_eq!(tag, None, "marks have no tag");
            assert_eq!(got_mark, mark);
            let l = next_of(alt).expect("mark linked to literal");
            assert_eq!(lit(&l), text);
        }
    }

    #[test]
    fn parse_mark_alone() {
        let (first, _) = compile_chain("1¦").unwrap();
        assert_eq!(kind(&first), "parse_tag");
        let (tag, mark) = tag_info(&first);
        assert_eq!(tag, None);
        assert_eq!(mark, 1);
        assert!(next_of(&first).is_none());
    }

    // ---- parse tags (depend on Agent 2's ParseTag::set_next) -------------

    #[test]
    fn tag_derivation_for_group_choice() {
        // ":(a|b)" — after compile + linking, ParseTag::setNext rewrites the
        // choice's alternatives into ParseTag-wrapped literals.
        let (first, _) = compile_chain(":(a|b)").unwrap();
        assert_eq!(kind(&first), "parse_tag");
        let (outer_tag, _) = tag_info(&first);
        assert_eq!(outer_tag.as_deref(), Some(""));

        let group = next_of(&first).expect("parse tag linked to group");
        assert_eq!(kind(&group), "group");
        let choice = child_of(&group);
        assert_eq!(kind(&choice), "choice");
        let alts = alts_of(&choice);
        assert_eq!(alts.len(), 2);
        for (alt, expected) in alts.iter().zip(["a", "b"]) {
            assert_eq!(kind(alt), "parse_tag");
            let (tag, _) = tag_info(alt);
            assert_eq!(tag.as_deref(), Some(expected));
            let l = next_of(alt).expect("tag linked to its literal");
            assert_eq!(lit(&l), expected);
            assert!(Rc::ptr_eq(&onext_of(alt).unwrap(), &l));
        }
    }

    #[test]
    fn tag_derivation_for_optional_literal() {
        // Real Skript pattern shape (ExprItemWithTooltip): "with[:out]"
        // matches "with" (optional skipped) or "without" (the ParseTag is
        // zero-width and records the tag "out", then the literal matches).
        let (first, _) = compile_chain("with[:out]").unwrap();
        assert_eq!(kind(&first), "literal");
        assert_eq!(lit(&first), "with");

        let opt = next_of(&first).expect("literal linked to optional");
        assert_eq!(kind(&opt), "optional");
        let inner_tag = child_of(&opt);
        assert_eq!(kind(&inner_tag), "parse_tag");
        let (tag, _) = tag_info(&inner_tag);
        // derived by ParseTag::setNext from the following literal
        assert_eq!(tag.as_deref(), Some("out"));
        let l = next_of(&inner_tag).expect("inner tag linked to literal");
        assert_eq!(lit(&l), "out");
        assert!(next_of(&l).is_none());
    }

    #[test]
    fn tag_before_colon_is_literal_text() {
        // the ':' handler reads the ACCUMULATED literal as the tag
        let (first, _) = compile_chain("with:").unwrap();
        assert_eq!(kind(&first), "parse_tag");
        let (tag, _) = tag_info(&first);
        assert_eq!(tag.as_deref(), Some("with"));
        assert!(next_of(&first).is_none());
    }

    // ---- real-world SkriptHub pattern (integration-ish) ------------------

    #[test]
    fn real_world_bone_meal_pattern() {
        let (first, count) = compile_chain("apply [%-number%] bone[ ]meal[s] [to %blocks%]")
            .expect("real SkriptHub pattern must compile");
        assert_eq!(count, 2);

        // top-level next-chain:
        // literal("apply ") → optional → literal(" bone") → optional →
        // literal("meal") → optional → literal(" ") → optional
        assert_eq!(
            walk_kinds(&first),
            vec![
                "literal", "optional", "literal", "optional", "literal", "optional", "literal",
                "optional",
            ]
        );
        assert_eq!(lit(&first), "apply ");

        // first optional wraps the nullable %-number% slot
        let opt1 = next_of(&first).unwrap();
        let ty = child_of(&opt1);
        assert_eq!(kind(&ty), "type");
        let (expr_index, _, is_nullable, _, _) = type_info(&ty);
        assert_eq!(expr_index, 0);
        assert!(is_nullable);
        // Optional::setNext propagated: the type's next is " bone"
        let bone = next_of(&ty).expect("type linked to ' bone'");
        assert_eq!(lit(&bone), " bone");

        // the last optional's child chain reaches the %blocks% slot (index 1)
        let opt4 = {
            let mut cursor = first.clone();
            loop {
                let deeper = next_of(&cursor).expect("chain continues");
                cursor = deeper;
                if kind(&cursor) == "optional" && next_of(&cursor).is_none() {
                    break;
                }
            }
            cursor
        };
        let to_lit = child_of(&opt4);
        assert_eq!(lit(&to_lit), "to ");
        let blocks = next_of(&to_lit).expect("'to ' linked to %blocks%");
        assert_eq!(kind(&blocks), "type");
        let (expr_index, classes, _, _, _) = type_info(&blocks);
        assert_eq!(expr_index, 1);
        assert_eq!(classes[0].code_name, "block");
        assert!(classes[0].is_plural);
    }

    #[test]
    fn escaped_bracket_inside_optional() {
        // nextBracket skips the escaped ']', so the group closes at the
        // final ']' and the inner literal is "a]b"
        let (first, _) = compile_chain("[a\\]b]").unwrap();
        assert_eq!(kind(&first), "optional");
        assert_eq!(lit(&child_of(&first)), "a]b");
    }
}
