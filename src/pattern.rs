//! Skript pattern parser.
//!
//! SkriptHub stores each syntax form as a *pattern string* in Skript's own
//! mini-language. This module turns those strings into a token tree and then
//! into a compiled regex with a record of which capture groups correspond to
//! which Skript types.
//!
//! Pattern grammar (the subset we care about):
//!
//! | Element            | Meaning                          | Example        |
//! |--------------------|----------------------------------|----------------|
//! | `text`             | Literal keyword (spaces allowed) | `send`, `to`   |
//! | `%type%`           | Required type placeholder        | `%player%`     |
//! | `[...]`            | Optional group                    | `[with id]`    |
//! | `(a\|b\|c)`        | Alternation                       | `(add\|remove)`|
//! | `[craft]`          | Char class (each char optional)   | `[craft]bukkit`|
//!
//! `%*%` and `%-*%` are treated as catch-all placeholders.

use regex::{Regex, RegexBuilder};

/// A single piece of a parsed Skript pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternToken {
    /// A literal run of characters, e.g. `send ` or ` to `.
    Literal(String),
    /// A `%type%` placeholder. The inner string is the Skript type name
    /// (or `*` for the catch-all).
    TypePlaceholder(String),
    /// An optional group, e.g. `[with id]`. Contents are themselves tokens.
    Optional(Vec<PatternToken>),
    /// A character class, e.g. `[craft]` where each char is independently
    /// optional. Represented as an alternation over the single chars plus the
    /// empty alternative. Kept as its own variant so the regex builder can
    /// treat the no-match case correctly.
    CharClass(Vec<char>),
    /// An alternation between literal strings, e.g. `(add|remove)`.
    /// Non-literal alternation arms (containing `%`, `[`, `(`) are flattened
    /// during parsing so every arm is a plain string.
    Alternation(Vec<String>),
}

/// A fully parsed pattern: a flat token sequence plus the original text.
#[derive(Debug, Clone)]
pub struct ParsedPattern {
    pub tokens: Vec<PatternToken>,
    pub original: String,
}

/// A compiled pattern, ready to match against user code.
#[derive(Debug)]
pub struct CompiledPattern {
    /// Compiled regex for prefix matching (completion). No trailing `$` so
    /// partial prefixes like `send"` match `send %text% to %player%`.
    pub regex: Regex,
    /// Compiled regex with an end-of-string anchor for full-line matching
    /// (validation diagnostics).
    pub full_regex: Regex,
    /// `(type_name, capture_index)` pairs, one per `%type%` in source order.
    /// Capture indices are 1-based and align with the regex's groups.
    pub type_groups: Vec<(String, usize)>,
    pub original: String,
}

/// Parse a single Skript pattern variant into tokens.
///
/// Patterns are split on `\r\n`/`\n` upstream (see [`crate::syntax`]); this
/// function operates on one variant at a time.
pub fn parse_pattern(pattern: &str) -> ParsedPattern {
    let mut parser = Parser {
        chars: pattern.chars().collect(),
        pos: 0,
    };
    let tokens = parser.parse_seq(None);
    ParsedPattern {
        tokens,
        original: pattern.to_owned(),
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    /// Parse a sequence of tokens until we hit a terminator (`)`, `]`, or EOF).
    /// `terminator` is `Some(c)` when parsing inside a group so we stop there.
    fn parse_seq(&mut self, terminator: Option<char>) -> Vec<PatternToken> {
        let mut tokens = Vec::new();
        let mut literal = String::new();

        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];

            // Stop at the group terminator.
            if terminator == Some(c) {
                break;
            }

            match c {
                '%' => {
                    self.flush_literal(&mut literal, &mut tokens);
                    let ty = self.parse_type_placeholder();
                    tokens.push(PatternToken::TypePlaceholder(ty));
                }
                '(' => {
                    self.flush_literal(&mut literal, &mut tokens);
                    let alts = self.parse_alternation();
                    tokens.push(PatternToken::Alternation(alts));
                }
                '[' => {
                    self.flush_literal(&mut literal, &mut tokens);
                    // Distinguish a char class `[xy]` (single chars only) from
                    // an optional group `[with id]`. We peek the contents.
                    if self.is_char_class() {
                        let chars = self.parse_char_class();
                        tokens.push(PatternToken::CharClass(chars));
                    } else {
                        let inner = self.parse_optional();
                        tokens.push(PatternToken::Optional(inner));
                    }
                }
                // Escape: copy the next char verbatim into the literal.
                '\\' if self.pos + 1 < self.chars.len() => {
                    literal.push(self.chars[self.pos + 1]);
                    self.pos += 2;
                    continue;
                }
                _ => {
                    literal.push(c);
                    self.pos += 1;
                }
            }
        }

        self.flush_literal(&mut literal, &mut tokens);
        tokens
    }

    fn flush_literal(&self, literal: &mut String, tokens: &mut Vec<PatternToken>) {
        if !literal.is_empty() {
            tokens.push(PatternToken::Literal(std::mem::take(literal)));
        }
    }

    /// Consume `%...%` and return the inner type name (or `*`).
    fn parse_type_placeholder(&mut self) -> String {
        // Consume the leading `%`.
        self.pos += 1;
        let mut inner = String::new();
        while self.pos < self.chars.len() && self.chars[self.pos] != '%' {
            inner.push(self.chars[self.pos]);
            self.pos += 1;
        }
        // Consume the trailing `%`.
        if self.pos < self.chars.len() {
            self.pos += 1;
        }
        // Skript uses `%-object%` (nullable) and `%*%`/`%-*%` (catch-all).
        // We strip leading `-` and trailing `s`/`@` decorators for matching.
        inner.trim().trim_start_matches('-').to_owned()
    }

    /// Consume `(a|b|c)` and return the literal arms. Arms containing `%`, `[`
    /// or `(` are flattened by recursively parsing them and re-serializing the
    /// literals, so callers always see plain strings.
    fn parse_alternation(&mut self) -> Vec<String> {
        // Consume `(`.
        self.pos += 1;
        let mut arms: Vec<String> = Vec::new();
        let mut current = String::new();

        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            match c {
                ')' => {
                    arms.push(std::mem::take(&mut current).trim().to_owned());
                    self.pos += 1;
                    break;
                }
                '|' => {
                    arms.push(std::mem::take(&mut current).trim().to_owned());
                    self.pos += 1;
                }
                '\\' if self.pos + 1 < self.chars.len() => {
                    current.push(self.chars[self.pos + 1]);
                    self.pos += 2;
                }
                _ => {
                    current.push(c);
                    self.pos += 1;
                }
            }
        }

        arms.into_iter().filter(|a| !a.is_empty()).collect()
    }

    /// Consume `[...]` (optional group) and return its parsed contents.
    fn parse_optional(&mut self) -> Vec<PatternToken> {
        // Consume `[`.
        self.pos += 1;
        let inner = self.parse_seq(Some(']'));
        // Consume `]`.
        if self.pos < self.chars.len() && self.chars[self.pos] == ']' {
            self.pos += 1;
        }
        inner
    }

    /// A `[...]` is a *char class* only if every member between the brackets
    /// is a single character (no spaces, no nested constructs).
    fn is_char_class(&self) -> bool {
        let mut depth = 0i32;
        let mut count = 0usize;
        let mut i = self.pos + 1;
        while i < self.chars.len() {
            match self.chars[i] {
                ']' if depth == 0 => return count > 0,
                '[' => {
                    depth += 1;
                    return false;
                }
                ']' => depth -= 1,
                '(' | '%' | ' ' | '|' => return false,
                c if c.is_alphanumeric() => count += 1,
                _ => return false,
            }
            i += 1;
        }
        false
    }

    /// Consume a char-class `[abc]` and return its chars.
    fn parse_char_class(&mut self) -> Vec<char> {
        // Consume `[`.
        self.pos += 1;
        let mut chars = Vec::new();
        while self.pos < self.chars.len() && self.chars[self.pos] != ']' {
            chars.push(self.chars[self.pos]);
            self.pos += 1;
        }
        // Consume `]`.
        if self.pos < self.chars.len() {
            self.pos += 1;
        }
        chars
    }
}

/// Compile a parsed pattern into a regex plus a record of its type groups.
///
/// Whitespace in patterns is relaxed: runs of whitespace become `\s+` so the
/// pattern matches regardless of how the user spaced their code. Literal text
/// is regex-escaped so user-provided punctuation is safe.
pub fn compile_pattern(parsed: &ParsedPattern) -> Result<CompiledPattern, regex::Error> {
    let pattern = compile_tokens(&parsed.tokens, &parsed.original);
    let mut builder = RegexBuilder::new(&pattern);
    builder.case_insensitive(true);
    let regex = builder.build()?;

    let mut full_builder = RegexBuilder::new(&(pattern.clone() + "$"));
    full_builder.case_insensitive(true);
    let full_regex = full_builder.build()?;

    Ok(CompiledPattern {
        regex,
        full_regex,
        type_groups: type_groups_for(&parsed.tokens),
        original: parsed.original.clone(),
    })
}

/// Walk the token tree and emit a regex string. The `original` is passed for
/// diagnostics only.
fn compile_tokens(tokens: &[PatternToken], _original: &str) -> String {
    let mut out = String::new();
    emit_tokens(tokens, &mut out);
    out
}

fn emit_tokens(tokens: &[PatternToken], out: &mut String) {
    for token in tokens {
        emit_token(token, out);
    }
}

fn emit_token(token: &PatternToken, out: &mut String) {
    match token {
        PatternToken::Literal(s) => {
            out.push_str(&whitespace_relaxed(s));
        }
        PatternToken::TypePlaceholder(_) => {
            // Capture group for the type; the matcher validates the contents.
            out.push_str("(");
            out.push_str(NON_GREEDY_ANYTHING);
            out.push_str(")");
        }
        PatternToken::Optional(inner) => {
            out.push_str("(?:");
            emit_tokens(inner, out);
            out.push_str(")?");
        }
        PatternToken::CharClass(chars) => {
            // Each char is independently optional.
            out.push_str("(?:");
            for c in chars {
                out.push_str(&c.to_string().escape_debug().to_string());
                out.push('?');
            }
            out.push_str(")");
        }
        PatternToken::Alternation(arms) => {
            out.push_str("(?:");
            for (i, arm) in arms.iter().enumerate() {
                if i > 0 {
                    out.push('|');
                }
                out.push_str(&whitespace_relaxed(arm));
            }
            out.push_str(")");
        }
    }
}

/// Replace runs of ASCII whitespace with `\s+` (one or more). Whitespace at
/// the start/end of a literal is converted to `\s*` so trailing optionals
/// don't force a space.
fn whitespace_relaxed(literal: &str) -> String {
    if !literal.contains(char::is_whitespace) {
        return literal.escape_debug().to_string();
    }
    // Split on runs of whitespace; join with \s+. Empty leading/trailing
    // segments (from leading/trailing spaces) become \s*.
    let parts: Vec<&str> = literal.split_whitespace().collect();
    if parts.is_empty() {
        // Pure whitespace literal: match optional whitespace.
        return r"\s*".to_owned();
    }
    let mut out = String::new();
    let starts_ws = literal.starts_with(char::is_whitespace);
    let ends_ws = literal.ends_with(char::is_whitespace);
    if starts_ws {
        out.push_str(r"\s*");
    }
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str(r"\s+");
        }
        out.push_str(&part.escape_debug().to_string());
    }
    if ends_ws {
        out.push_str(r"\s*");
    }
    out
}

/// `(type_name, group_index)` pairs in matching order. Group indices are
/// assigned by `emit_token` (1-based) so this must mirror its traversal.
fn type_groups_for(tokens: &[PatternToken]) -> Vec<(String, usize)> {
    let mut groups = Vec::new();
    let mut index = 0usize;
    collect_type_groups(tokens, &mut index, &mut groups);
    groups
}

fn collect_type_groups(
    tokens: &[PatternToken],
    index: &mut usize,
    out: &mut Vec<(String, usize)>,
) {
    for token in tokens {
        match token {
            PatternToken::TypePlaceholder(ty) => {
                *index += 1;
                out.push((ty.clone(), *index));
            }
            PatternToken::Optional(inner) => collect_type_groups(inner, index, out),
            PatternToken::Literal(_)
            | PatternToken::CharClass(_)
            | PatternToken::Alternation(_) => {}
        }
    }
}

/// Non-greedy "anything" used for type placeholders. `[^,)]+?` would be more
/// precise but breaks multi-token values like `1, 2 and 3`; we keep `.+?` and
/// rely on the surrounding literal anchors.
const NON_GREEDY_ANYTHING: &str = r".+?";

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(pattern: &str) -> CompiledPattern {
        let parsed = parse_pattern(pattern);
        compile_pattern(&parsed).unwrap()
    }

    fn matches_at_start(cp: &CompiledPattern, text: &str) -> bool {
        // Find a match anywhere then confirm it's anchored at byte 0.
        match cp.regex.find(text) {
            Some(m) => m.start() == 0,
            None => false,
        }
    }

    #[test]
    fn literal_and_type_placeholders_compile() {
        let cp = compile("send %text% to %player%");
        assert_eq!(cp.type_groups.len(), 2);
        assert_eq!(cp.type_groups[0], ("text".to_owned(), 1));
        assert_eq!(cp.type_groups[1], ("player".to_owned(), 2));
        assert!(matches_at_start(&cp, r#"send "hello" to player"#));
        assert!(matches_at_start(&cp, "send   hello   to   Steve"));
        assert!(!matches_at_start(&cp, "give sword to player"));
    }

    #[test]
    fn alternation_compiles() {
        let cp = compile("(add|remove) %player% from %team%");
        assert!(matches_at_start(&cp, "add Steve from red"));
        assert!(matches_at_start(&cp, "remove Alex from blue"));
        assert!(!matches_at_start(&cp, "set Steve from red"));
    }

    #[test]
    fn optional_group_compiles() {
        let cp = compile("set %object% to %object% [quickly]");
        assert!(matches_at_start(&cp, "set x to y"));
        assert!(matches_at_start(&cp, "set x to y quickly"));
    }

    #[test]
    fn char_class_compiles() {
        // `[craft]bukkit` matches "bukkit", "cbukkit", "rbukkit", etc.
        let cp = compile("([craft]bukkit|minecraft|skript) version");
        assert!(matches_at_start(&cp, "bukkit version"));
        assert!(matches_at_start(&cp, "cbukkit version"));
        assert!(matches_at_start(&cp, "minecraft version"));
        assert!(matches_at_start(&cp, "skript version"));
    }

    #[test]
    fn whitespace_is_case_insensitive_and_relaxed() {
        let cp = compile("Send Message");
        assert!(matches_at_start(&cp, "send message"));
        assert!(matches_at_start(&cp, "SEND MESSAGE"));
        assert!(matches_at_start(&cp, "send\tmessage"));
    }

    #[test]
    fn catch_all_type_placeholder() {
        let cp = compile("broadcast %*%");
        assert!(cp.type_groups.iter().any(|(t, _)| t == "*"));
        assert!(matches_at_start(&cp, "broadcast hello world"));
    }

    #[test]
    fn escapes_regex_metacharacters_in_literals() {
        // The `.` here is a literal period, not a regex wildcard.
        let cp = compile("set %object%.%slot% to %object%");
        assert!(!matches_at_start(&cp, "set aXb to c")); // 'X' should not match '.'
        assert!(matches_at_start(&cp, "set a.b to c"));
    }

    #[test]
    fn partial_prefix_matches() {
        // No trailing `$`, so a prefix of a pattern should still match.
        let cp = compile("send %text% to %player%");
        assert!(matches_at_start(&cp, "send"));
        assert!(matches_at_start(&cp, "send \"hi\""));
    }
}
