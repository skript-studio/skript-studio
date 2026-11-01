//! Parse helpers ported from `SkriptParser` and `Utils`.
//!
//! OWNER: Agent 3 (same agent as type_element.rs — these exist to serve
//! `TypePatternElement` and `RegexPatternElement`).
//!
//! Java sources ported here:
//! - `SkriptParser.next`                     (SkriptParser.java:1449)
//! - `SkriptParser.nextOccurrence`           (SkriptParser.java:1495)
//! - `SkriptParser.nextQuote` (private)      (SkriptParser.java:1367)
//! - `VariableString.nextVariableBracket`    (VariableString.java:283)
//! - `Utils.isPlural`                        (Utils.java:272)
//! - `Utils.couldBeSingular` (private)       (Utils.java:310)
//! - `Utils.toEnglishPlural`                 (Utils.java:344)
//! - the `Utils.plurals` static table        (Utils.java:50-99)
//!
//! # Indexing convention (UTF-16 → UTF-8)
//! Java indexes strings by UTF-16 code unit; Rust `&str` by UTF-8 byte.
//! For ASCII input (Skript scripts and patterns are overwhelmingly ASCII)
//! the two coincide. Wherever Java writes `index + 1` (advance one UTF-16
//! code unit) this port advances to the next UTF-8 **char boundary** so
//! that every offset we produce can be used directly for slicing. Java
//! could land between a surrogate pair with such an increment; we never
//! split a scalar. Each such site is marked inline.

use crate::parse_context::ParseContext;

// ==========================================================================
// SkriptParser.next / nextOccurrence (+ private helpers)
// ==========================================================================

/// Port of `SkriptParser.next(String, int, ParseContext)` (SkriptParser.java:1449).
///
/// Returns the next index (after `start`) at which an expression may end,
/// skipping quoted strings (with `""` escapes and `%expr%` interpolation),
/// variables (`{...}`, nested) and parentheses — unless `context` is
/// `Command` or `Parse`, in which case every single character is a boundary.
///
/// Java returns `-1` for an invalid string/variable/bracket or when
/// `startIndex >= expr.length()`; those map to `None`. The END of the input
/// IS a valid boundary (`Some(expr.len())`), matching Java's
/// `startIndex + 1` convention — callers use it to terminate their loops
/// only after considering the end-of-input candidate.
pub(crate) fn next(expr: &str, start: usize, context: ParseContext) -> Option<usize> {
    // Java throws StringIndexOutOfBoundsException for startIndex < 0 —
    // impossible with usize.
    let expr_len = expr.len();
    if start >= expr_len {
        return None;
    }

    if context == ParseContext::Command || context == ParseContext::Parse {
        return Some(plus_one_char(expr, start));
    }

    match char_at(expr, start) {
        // Java: index = nextQuote(expr, startIndex + 1);
        //      yield index < 0 ? -1 : index + 1;
        Some('"') => next_quote(expr, start + 1).map(|index| index + 1),
        // Java: index = VariableString.nextVariableBracket(expr, startIndex + 1);
        //      yield index < 0 ? -1 : index + 1;
        Some('{') => next_variable_bracket(expr, start + 1).map(|index| index + 1),
        // Java: for (index = startIndex + 1; index >= 0 && index < exprLength;
        //           index = next(expr, index, context))
        //          if (expr.charAt(index) == ')') yield index + 1;
        //      yield -1;
        // (the recursive `next` skips nested parens, quotes and variables)
        Some('(') => {
            let mut index = start + 1;
            while index < expr_len {
                if char_at(expr, index) == Some(')') {
                    return Some(index + 1); // ')' is ASCII: +1 is a boundary
                }
                index = next(expr, index, context)?;
            }
            None
        }
        // Java: default -> startIndex + 1;
        _ => Some(plus_one_char(expr, start)),
    }
}

/// Port of `SkriptParser.nextOccurrence(String, String, int, ParseContext, boolean)`
/// (SkriptParser.java:1495). Finds the next occurrence of `needle` in
/// `haystack` at or after `start`, skipping strings, variables and
/// parentheses (unless `context` is `Command`/`Parse`, where this is a plain
/// — and always case-SENSITIVE, regardless of `case_sensitive`! —
/// `indexOf`, faithfully to the Java early return).
///
/// Returns the index of the FIRST character of the needle. `None` when an
/// invalid string/variable/bracket is found or `start >= haystack.len()`.
pub(crate) fn next_occurrence(
    haystack: &str,
    needle: &str,
    start: usize,
    context: ParseContext,
    case_sensitive: bool,
) -> Option<usize> {
    // Java throws StringIndexOutOfBoundsException for startIndex < 0 —
    // impossible with usize.
    if context == ParseContext::Command || context == ParseContext::Parse {
        return index_of(haystack, needle, start);
    }

    let haystack_len = haystack.len();
    if start >= haystack_len {
        return None;
    }

    // DIVERGENCE: Java crashes with StringIndexOutOfBoundsException on an
    // empty needle (`needle.charAt(0)` at line 1507); the port instead
    // treats the empty needle as matching at the current position (the
    // behaviour of String.indexOf("")), so an empty literal successor
    // degrades to char-by-char offset iteration instead of a panic.
    if needle.is_empty() {
        return Some(start);
    }

    let first_char = needle.chars().next().unwrap();
    let starts_with_special_char = first_char == '"' || first_char == '{' || first_char == '(';

    let mut start_index = start;
    while start_index < haystack_len {
        let character = char_at(haystack, start_index)?;

        // Early check before special character handling
        if starts_with_special_char
            && region_matches(haystack, start_index, needle, !case_sensitive)
        {
            return Some(start_index);
        }

        match character {
            '"' => {
                start_index = next_quote(haystack, start_index + 1)?;
            }
            '{' => {
                start_index = next_variable_bracket(haystack, start_index + 1)?;
            }
            '(' => {
                // Use next() to skip to right after the closing parenthesis
                start_index = next(haystack, start_index, context)?;
            }
            _ => {}
        }

        if region_matches(haystack, start_index, needle, !case_sensitive) {
            return Some(start_index);
        }

        // Java: startIndex++ (one UTF-16 unit); advance one whole char so
        // the offset stays on a char boundary (identical for ASCII).
        start_index = plus_one_char(haystack, start_index);
    }

    None
}

/// Port of `SkriptParser.nextQuote(String, int)` (SkriptParser.java:1367,
/// private). Finds the closing double quote starting the scan after the
/// opening one; `""` is an escaped literal quote and `%` toggles
/// in-expression mode (Skript string interpolation).
fn next_quote(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let length = bytes.len();
    let mut in_expression = false;
    let mut i = start;
    while i < length {
        let character = bytes[i]; // '"' and '%' are ASCII: byte scan is safe
        if character == b'"' && !in_expression {
            if i == length - 1 || bytes[i + 1] != b'"' {
                return Some(i);
            }
            i += 1; // skip the second quote of a "" escape
        } else if character == b'%' {
            in_expression = !in_expression;
        }
        i += 1;
    }
    None
}

/// Port of `VariableString.nextVariableBracket(String, int)`
/// (VariableString.java:283). Index of the matching `}`, honouring nesting.
fn next_variable_bracket(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut variable_depth = 0usize;
    let mut index = start;
    while index < bytes.len() {
        let character = bytes[index]; // '{' and '}' are ASCII
        if character == b'}' {
            if variable_depth == 0 {
                return Some(index);
            }
            variable_depth -= 1;
        } else if character == b'{' {
            variable_depth += 1;
        }
        index += 1;
    }
    None
}

/// Java `String.indexOf(String, int)` semantics, used by `nextOccurrence`
/// in COMMAND/PARSE contexts: case-sensitive, boundary-aligned.
/// Like Java, `indexOf("", i)` is `min(i, len)` and `indexOf(nonEmpty, i)`
/// with `i >= len` is "not found".
fn index_of(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(haystack.len()));
    }
    if from >= haystack.len() {
        return None;
    }
    haystack
        .get(from..) // defensive: None when `from` is not a char boundary
        .and_then(|rest| rest.find(needle))
        .map(|i| i + from)
}

/// Java `String.regionMatches(ignoreCase, toffset, other, 0, other.length())`
/// adapted to UTF-8: char-by-char, bounds- and boundary-safe.
fn region_matches(haystack: &str, toffset: usize, needle: &str, ignore_case: bool) -> bool {
    let mut hay_chars = match haystack.get(toffset..) {
        Some(rest) => rest.chars(),
        None => return false, // out of bounds or not a char boundary
    };
    for needle_char in needle.chars() {
        match hay_chars.next() {
            Some(hay_char) => {
                if hay_char == needle_char {
                    continue;
                }
                if ignore_case && chars_equal_ignoring_case(hay_char, needle_char) {
                    continue;
                }
                return false;
            }
            None => return false,
        }
    }
    true
}

/// Java regionMatches(ignoreCase): two chars match when equal, when their
/// uppercased forms are equal, or when their lowercased forms are equal.
/// (Java compares single UTF-16 units via Character.toUpperCase/toLowerCase;
/// Rust's char::to_uppercase/to_lowercase are full Unicode mappings —
/// identical for the ASCII content Skript uses.)
fn chars_equal_ignoring_case(a: char, b: char) -> bool {
    a.to_lowercase().eq(b.to_lowercase()) || a.to_uppercase().eq(b.to_uppercase())
}

/// Java's `String.charAt(int)` adapted to UTF-8: the char starting at a
/// char-boundary byte index. `None` when out of bounds or (defensively)
/// when the index is not a char boundary.
fn char_at(s: &str, index: usize) -> Option<char> {
    s.get(index..).and_then(|rest| rest.chars().next())
}

/// Java's `index + 1` (advance one UTF-16 code unit) rendered boundary-safe:
/// returns the byte index of the char AFTER the one at `index` (or
/// `s.len()` when there is none). For ASCII input this is exactly `index+1`.
pub(crate) fn plus_one_char(s: &str, index: usize) -> usize {
    match char_at(s, index) {
        Some(c) => index + c.len_utf8(),
        None => s.len(),
    }
}

// ==========================================================================
// Utils.isPlural / Utils.toEnglishPlural
// ==========================================================================

/// Port of `Utils.isPlural(String)` (Utils.java:272) returning
/// `(updated_word, is_plural)` — Java's `PluralResult`.
///
/// If the word is plural, `updated` contains the singular variant;
/// otherwise `updated == word`.
pub fn is_plural(word: &str) -> (String, bool) {
    // Java: Preconditions.checkNotNull(word, "word cannot be null") —
    // &str is never null.

    if word.is_empty() {
        return (String::new(), false);
    }

    if could_be_singular(word) {
        return (word.to_string(), false);
    }

    for ending in PLURALS {
        if ending.is_complete_word {
            // Complete words shouldn't be used as partial pieces
            // DIVERGENCE: Java compares UTF-16 lengths, we compare byte
            // lengths — identical whenever the ending matched at all.
            if word.len() != ending.plural.len() {
                continue;
            }
        }

        if let Some(stripped) = word.strip_suffix(ending.plural) {
            return (format!("{}{}", stripped, ending.singular), true);
        }

        // Java: word.endsWith(ending.plural().toUpperCase(Locale.ENGLISH))
        // (to_uppercase of the all-ASCII endings is length-preserving)
        let plural_upper = ending.plural.to_uppercase();
        if word.ends_with(plural_upper.as_str()) {
            return (
                format!(
                    "{}{}",
                    &word[..word.len() - ending.plural.len()],
                    ending.singular.to_uppercase()
                ),
                true,
            );
        }
    }

    (word.to_string(), false)
}

/// Port of `Utils.couldBeSingular(String)` (Utils.java:310, private).
fn could_be_singular(word: &str) -> bool {
    for ending in PLURALS {
        // Java: ending.singular().isBlank()
        if ending.singular.trim().is_empty() {
            continue;
        }

        if ending.is_complete_word && ending.singular.len() != word.len() {
            continue; // Skip complete words
        }

        // Java: word.endsWith(ending.singular())
        //   || word.toLowerCase().endsWith(ending.singular())
        // DIVERGENCE: Java's toLowerCase() uses the default locale (could be
        // e.g. Turkish); we use locale-independent Unicode lowercasing.
        // Identical for ASCII.
        if word.ends_with(ending.singular) || word.to_lowercase().ends_with(ending.singular) {
            return true;
        }
    }
    false
}

/// Port of `Utils.toEnglishPlural(String)` (Utils.java:344).
pub fn to_english_plural(word: &str) -> String {
    // Java asserts word != null && word.length() != 0 — asserts are stripped
    // in the port; the function below is total.
    for ending in PLURALS {
        if ending.is_complete_word {
            // Complete words shouldn't be used as partial pieces
            if word.len() != ending.singular.len() {
                continue;
            }
        }
        if let Some(stripped) = word.strip_suffix(ending.singular) {
            return format!("{}{}", stripped, ending.plural);
        }
    }
    // Java: assert false; return word + "s";
    // Unreachable in practice — the trailing ("", "s") ending matches every
    // word — but kept as the Java fallback.
    format!("{word}s")
}

/// Port of Java `Utils.WordEnding` (Utils.java:805).
struct WordEnding {
    singular: &'static str,
    plural: &'static str,
    is_complete_word: bool,
}

/// Java `new WordEnding(singular, plural)` (isCompleteWord = false).
const fn we(singular: &'static str, plural: &'static str) -> WordEnding {
    WordEnding {
        singular,
        plural,
        is_complete_word: false,
    }
}

/// Java `new WordEnding(singular, plural, true)`.
const fn we_complete(singular: &'static str, plural: &'static str) -> WordEnding {
    WordEnding {
        singular,
        plural,
        is_complete_word: true,
    }
}

/// Port of the `Utils.plurals` static initializer (Utils.java:50-99), in
/// the exact order the Java LinkedList is built (order is significant:
/// first match wins).
///
/// DIVERGENCE: `Utils.addPluralOverride` (runtime registration used by
/// plugins) is not ported — there is no plugin runtime in static analysis.
static PLURALS: &[WordEnding] = &[
    we("axe", "axes"), // not complete since we have battleaxe, etc.
    we("x", "xes"),
    we("ay", "ays"),
    we("ey", "eys"),
    we("iy", "iys"),
    we("oy", "oys"),
    we("uy", "uys"),
    we("kie", "kies"),
    we_complete("zombie", "zombies"),
    we("y", "ies"),
    we_complete("wife", "wives"), // we have to do the -ife -> ives first
    we("life", "lives"),
    we_complete("knife", "knives"),
    we("ive", "ives"),
    we("lf", "lves"), // self shelf elf wolf half etc.
    we_complete("thief", "thieves"),
    we("ief", "iefs"), // chiefs, fiefs, briefs
    we("hoof", "hooves"),
    we("fe", "ves"), // most -f words' plurals can end in -fs as well as -ves
    we("h", "hes"),
    we("man", "men"),
    we("ui", "uis"),   // gui fix
    we("api", "apis"), // api fix
    we("us", "i"),
    we_complete("hoe", "hoes"),
    we_complete("toe", "toes"),
    we_complete("foe", "foes"),
    we_complete("woe", "woes"),
    we("o", "oes"),
    we_complete("alias", "aliases"),
    we_complete("gas", "gases"),
    we("child", "children"), // grandchild, etc.
    we_complete("sheep", "sheep"),
    // general ending
    we("", "s"),
];

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // next()
    // ------------------------------------------------------------------

    #[test]
    fn next_basics() {
        // every ordinary char is a boundary
        assert_eq!(next("hello world", 0, ParseContext::Default), Some(1));
        assert_eq!(next("hello world", 6, ParseContext::Default), Some(7));
        // end of input is a valid boundary
        assert_eq!(next("hello", 4, ParseContext::Default), Some(5));
        // start at/after end -> not found (Java -1)
        assert_eq!(next("hello", 5, ParseContext::Default), None);
        assert_eq!(next("hello", 99, ParseContext::Default), None);
        assert_eq!(next("", 0, ParseContext::Default), None);
    }

    #[test]
    fn next_command_and_parse_walk_char_by_char() {
        // COMMAND/PARSE never skip anything — not even quotes
        assert_eq!(next("\"a b\" c", 0, ParseContext::Command), Some(1));
        assert_eq!(next("\"a b\" c", 3, ParseContext::Parse), Some(4));
        assert_eq!(next("ab", 1, ParseContext::Command), Some(2));
    }

    #[test]
    fn next_skips_quoted_strings() {
        // closing quote of "hello world" is at 12 -> boundary after it is 13
        assert_eq!(
            next("\"hello world\" rest", 0, ParseContext::Default),
            Some(13)
        );
        // unterminated quote -> -1
        assert_eq!(next("\"hello", 0, ParseContext::Default), None);
        // "" escapes: closing quote at 10
        assert_eq!(
            next("\"a \"\"b\"\" c\" x", 0, ParseContext::Default),
            Some(11)
        );
        // "%" inside a string toggles expression mode: a quote while inside
        // an interpolated %expression% is NOT a closing quote
        assert_eq!(next("\"a %b% c\" d", 0, ParseContext::Default), Some(9));
        assert_eq!(next("\"%\"", 0, ParseContext::Default), None);
    }

    #[test]
    fn next_skips_variables() {
        assert_eq!(next("{var} x", 0, ParseContext::Default), Some(5));
        // nested variable brackets
        assert_eq!(next("{{a}{b}} x", 0, ParseContext::Default), Some(8));
        assert_eq!(next("{unclosed", 0, ParseContext::Default), None);
    }

    #[test]
    fn next_skips_parentheses() {
        assert_eq!(next("(a b) c", 0, ParseContext::Default), Some(5));
        // nested parens: the inner next() call skips the inner group
        assert_eq!(next("((a)) b", 0, ParseContext::Default), Some(5));
        // unbalanced
        assert_eq!(next("(a b", 0, ParseContext::Default), None);
        assert_eq!(next("(", 0, ParseContext::Default), None);
        // a ')' inside a quoted string is not a group closer: the group
        // ends at the ')' AFTER the closing quote (index 6) -> 7
        assert_eq!(next("(\"a)b\") c", 0, ParseContext::Default), Some(7));
        // quote opened inside the group and never closed
        assert_eq!(next("(\"a b) c", 0, ParseContext::Default), None);
    }

    // ------------------------------------------------------------------
    // next_occurrence()
    // ------------------------------------------------------------------

    #[test]
    fn next_occurrence_basics() {
        assert_eq!(
            next_occurrence(
                "hello world hello",
                "hello",
                0,
                ParseContext::Default,
                false
            ),
            Some(0)
        );
        assert_eq!(
            next_occurrence(
                "hello world hello",
                "hello",
                1,
                ParseContext::Default,
                false
            ),
            Some(12)
        );
        assert_eq!(
            next_occurrence("abc", "xyz", 0, ParseContext::Default, false),
            None
        );
        // start at/after end -> not found
        assert_eq!(
            next_occurrence("abc", "a", 3, ParseContext::Default, false),
            None
        );
    }

    #[test]
    fn next_occurrence_case_sensitivity() {
        assert_eq!(
            next_occurrence("Hello World", "hello", 0, ParseContext::Default, false),
            Some(0)
        );
        assert_eq!(
            next_occurrence("Hello World", "hello", 0, ParseContext::Default, true),
            None
        );
        assert_eq!(
            next_occurrence("Hello hello", "hello", 0, ParseContext::Default, true),
            Some(6)
        );
    }

    #[test]
    fn next_occurrence_skips_quoted_regions() {
        // the "apples" inside quotes must be skipped; the bare one found
        assert_eq!(
            next_occurrence(
                "say \"apples\" and apples",
                "apples",
                0,
                ParseContext::Default,
                false
            ),
            Some(17)
        );
        // case-insensitive across the quote skip
        assert_eq!(
            next_occurrence(
                "say \"Apples\" and apples",
                "apples",
                0,
                ParseContext::Default,
                false
            ),
            Some(17)
        );
        // unterminated quote -> -1
        assert_eq!(
            next_occurrence("\"apples", "apples", 0, ParseContext::Default, false),
            None
        );
    }

    #[test]
    fn next_occurrence_needle_starting_with_special_char() {
        // needle starts with '"': early check fires before quote skipping
        assert_eq!(
            next_occurrence("\"a\" \"b\"", "\"b\"", 0, ParseContext::Default, false),
            Some(4)
        );
        // needle starting with '('
        assert_eq!(
            next_occurrence("x (a) (b)", "(b)", 0, ParseContext::Default, false),
            Some(6)
        );
    }

    #[test]
    fn next_occurrence_skips_variables_and_parens() {
        // "b" inside {var} is skipped
        assert_eq!(
            next_occurrence("{b} b", "b", 0, ParseContext::Default, false),
            Some(4)
        );
        // "b" inside parens is skipped: next() jumps past "(b)"
        assert_eq!(
            next_occurrence("(b) b", "b", 0, ParseContext::Default, false),
            Some(4)
        );
    }

    #[test]
    fn next_occurrence_command_parse_is_plain_index_of() {
        // COMMAND/PARSE: plain indexOf — case-SENSITIVE even though
        // case_sensitive is false (faithful to the Java early return)
        assert_eq!(
            next_occurrence("Hello hello", "hello", 0, ParseContext::Command, false),
            Some(6)
        );
        assert_eq!(
            next_occurrence("Hello hello", "hello", 7, ParseContext::Parse, false),
            None
        );
        // and it does NOT skip quotes
        assert_eq!(
            next_occurrence("\"a b\" c", "a b", 0, ParseContext::Command, false),
            Some(1)
        );
    }

    #[test]
    fn next_occurrence_start_at_needle() {
        assert_eq!(
            next_occurrence("12 apples", "12", 0, ParseContext::Default, false),
            Some(0)
        );
    }

    // ------------------------------------------------------------------
    // is_plural / to_english_plural (cases traced through the Java logic)
    // ------------------------------------------------------------------

    #[test]
    fn is_plural_regular_words() {
        assert_eq!(is_plural("number"), ("number".into(), false));
        assert_eq!(is_plural("numbers"), ("number".into(), true));
        assert_eq!(is_plural("item"), ("item".into(), false));
        assert_eq!(is_plural("items"), ("item".into(), true));
        assert_eq!(is_plural("string"), ("string".into(), false));
        assert_eq!(is_plural("strings"), ("string".into(), true));
        assert_eq!(is_plural("player"), ("player".into(), false));
        assert_eq!(is_plural("players"), ("player".into(), true));
        assert_eq!(is_plural("location"), ("location".into(), false));
        assert_eq!(is_plural("locations"), ("location".into(), true));
    }

    #[test]
    fn is_plural_y_to_ies() {
        assert_eq!(is_plural("entity"), ("entity".into(), false));
        assert_eq!(is_plural("entities"), ("entity".into(), true));
        assert_eq!(is_plural("livingentities"), ("livingentity".into(), true));
    }

    #[test]
    fn is_plural_irregular_words() {
        assert_eq!(is_plural("zombie"), ("zombie".into(), false));
        assert_eq!(is_plural("zombies"), ("zombie".into(), true)); // complete word
        assert_eq!(is_plural("wolf"), ("wolf".into(), false));
        assert_eq!(is_plural("wolves"), ("wolf".into(), true)); // -lf -> -lves
        assert_eq!(is_plural("man"), ("man".into(), false));
        assert_eq!(is_plural("men"), ("man".into(), true));
        assert_eq!(is_plural("child"), ("child".into(), false));
        assert_eq!(is_plural("children"), ("child".into(), true));
        assert_eq!(is_plural("box"), ("box".into(), false));
        assert_eq!(is_plural("boxes"), ("box".into(), true));
        assert_eq!(is_plural("axe"), ("axe".into(), false));
        assert_eq!(is_plural("axes"), ("axe".into(), true));
        assert_eq!(is_plural("thief"), ("thief".into(), false));
        assert_eq!(is_plural("thieves"), ("thief".into(), true));
        assert_eq!(is_plural("hoof"), ("hoof".into(), false));
        assert_eq!(is_plural("hooves"), ("hoof".into(), true));
        assert_eq!(is_plural("knife"), ("knife".into(), false));
        assert_eq!(is_plural("knives"), ("knife".into(), true));
    }

    #[test]
    fn is_plural_complete_word_rules() {
        // "sheep"/"sheep" is a complete word: "sheep" stays singular and
        // "black sheep" is NOT matched against it (length differs)...
        assert_eq!(is_plural("sheep"), ("sheep".into(), false));
        // ...so a plural "sheeps" falls through to the generic -s rule
        assert_eq!(is_plural("sheeps"), ("sheep".into(), true));
        // "gases"/"aliases" only as complete words
        assert_eq!(is_plural("gases"), ("gas".into(), true));
        assert_eq!(is_plural("aliases"), ("alias".into(), true));
        // ...but "hoes" (complete) beats the partial -o -> -oes rule
        assert_eq!(is_plural("hoes"), ("hoe".into(), true));
        // partial rule for non-complete words: "shoes" -> "sho" (Java does
        // this too — faithful quirk of the -o -> -oes partial ending)
        assert_eq!(is_plural("shoes"), ("sho".into(), true));
    }

    #[test]
    fn is_plural_fix_endings() {
        assert_eq!(is_plural("gui"), ("gui".into(), false));
        assert_eq!(is_plural("guis"), ("gui".into(), true)); // gui fix
        assert_eq!(is_plural("api"), ("api".into(), false));
        assert_eq!(is_plural("apis"), ("api".into(), true)); // api fix
                                                             // "bus" ends with the ("us", "i") singular so it stays singular...
        assert_eq!(is_plural("bus"), ("bus".into(), false));
        // ...and "buses" does NOT end with "us" (it ends with "es"), so it
        // falls through to the generic ("", "s") ending -> "buse".
        // Faithful Java quirk.
        assert_eq!(is_plural("buses"), ("buse".into(), true));
    }

    #[test]
    fn is_plural_case_handling() {
        // uppercase plurals produce uppercase singulars (the toUpperCase
        // branch of the Java loop)
        assert_eq!(is_plural("APIS"), ("API".into(), true));
        assert_eq!(is_plural("ZOMBIES"), ("ZOMBIE".into(), true));
        // capitalised words keep their case
        assert_eq!(is_plural("Entities"), ("Entity".into(), true));
        // couldBeSingular lowercases the word before matching
        assert_eq!(is_plural("GUI"), ("GUI".into(), false));
    }

    #[test]
    fn is_plural_empty_word() {
        assert_eq!(is_plural(""), (String::new(), false));
    }

    #[test]
    fn to_english_plural_known_cases() {
        assert_eq!(to_english_plural("number"), "numbers");
        assert_eq!(to_english_plural("entity"), "entities");
        assert_eq!(to_english_plural("zombie"), "zombies");
        assert_eq!(to_english_plural("wolf"), "wolves");
        assert_eq!(to_english_plural("man"), "men");
        assert_eq!(to_english_plural("child"), "children");
        assert_eq!(to_english_plural("sheep"), "sheep");
        assert_eq!(to_english_plural("box"), "boxes");
        assert_eq!(to_english_plural("axe"), "axes");
        assert_eq!(to_english_plural("gui"), "guis");
        assert_eq!(to_english_plural("api"), "apis");
        assert_eq!(to_english_plural("life"), "lives");
        assert_eq!(to_english_plural("thief"), "thieves");
        assert_eq!(to_english_plural("hoof"), "hooves");
        assert_eq!(to_english_plural("alias"), "aliases");
        assert_eq!(to_english_plural("gas"), "gases");
        assert_eq!(to_english_plural("hoe"), "hoes");
        // the ("h", "hes") ending: every -h word pluralizes as -hes in
        // Skript's table, even "path" — faithful Java quirk (while
        // is_plural("paths") = ("path", true) via the generic -s rule)
        assert_eq!(to_english_plural("path"), "pathes");
        assert_eq!(is_plural("paths"), ("path".into(), true));
        // fallback through the generic ("", "s") ending
        assert_eq!(to_english_plural("item"), "items");
    }

    #[test]
    fn plural_round_trips() {
        // what from_string stores (singular) must to_string_pretty back to
        // the original plural spelling
        for word in [
            "numbers",
            "entities",
            "zombies",
            "wolves",
            "men",
            "children",
            "boxes",
            "guis",
            "apis",
            "hooves",
            "aliases",
            "players",
            "locations",
            "livingentities",
        ] {
            let (updated, plural) = is_plural(word);
            assert!(plural, "{word} should be plural");
            assert_eq!(to_english_plural(&updated), word);
        }
    }

    #[test]
    fn plurals_match_the_java_unit_test() {
        // Mirror of Skript's own UtilsPluralsTest
        // (src/test/java/org/skriptlang/skript/test/tests/localization/UtilsPluralsTest.java):
        // toEnglishPlural(singular) == plural AND
        // getEnglishPlural(plural).getFirst() == singular  (i.e. isPlural)
        let cases: &[(&str, &str)] = &[
            ("house", "houses"),
            ("cookie", "cookies"),
            ("creeper", "creepers"),
            ("cactus", "cacti"),
            ("rose", "roses"),
            ("dye", "dyes"),
            ("name", "names"),
            ("ingot", "ingots"),
            ("derp", "derps"),
            ("sheep", "sheep"),
            ("choir", "choirs"),
            ("man", "men"),
            ("child", "children"),
            ("hoe", "hoes"),
            ("toe", "toes"),
            ("hero", "heroes"),
            ("kidney", "kidneys"),
            ("anatomy", "anatomies"),
            ("axe", "axes"),
            ("knife", "knives"),
            ("elf", "elves"),
            ("shelf", "shelves"),
            ("self", "selves"),
            ("gui", "guis"),
        ];
        for (singular, plural) in cases {
            assert_eq!(
                &to_english_plural(singular),
                plural,
                "to_english_plural({singular})"
            );
            // The Java test only asserts getEnglishPlural(plural).getFirst()
            // == singular — NOT the plural flag. ("sheep" legitimately
            // reports plural=false via couldBeSingular's complete-word
            // rule, so the flag is not asserted here either.)
            let (updated, _was_plural) = is_plural(plural);
            assert_eq!(&updated, singular, "is_plural({plural})");
        }
    }
}
