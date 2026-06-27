//! Skript lexer for semantic tokens.
//!
//! Produces a flat list of typed spans (keyword, type, variable, string,
//! comment, operator, ...) suitable for LSP `textDocument/semanticTokens`.
//!
//! The design is deliberately line-based and regex-driven: Skript has no
//! context-free grammar we can reuse here, and VS Code's semantic-token
//! protocol is tolerant of overlapping/imperfect tokenization. The goal is
//! "good enough highlighting" on top of the TextMate grammar, not a precise
//! parse.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Semantic token type. Names match LSP `SemanticTokenType` conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticType {
    Keyword,
    Type,
    Variable,
    String,
    Comment,
    Operator,
    Event,
    Function,
    Number,
}

impl SemanticType {
    /// LSP wire name (lowercase, matches `SemanticTokenType`).
    pub fn as_str(self) -> &'static str {
        match self {
            SemanticType::Keyword => "keyword",
            SemanticType::Type => "type",
            SemanticType::Variable => "variable",
            SemanticType::String => "string",
            SemanticType::Comment => "comment",
            SemanticType::Operator => "operator",
            SemanticType::Event => "event",
            SemanticType::Function => "function",
            SemanticType::Number => "number",
        }
    }
}

/// A typed span in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub line: u32,
    pub col: u32,
    pub length: u32,
    pub ty: SemanticType,
}

/// Tokenize a whole document into semantic tokens. Line/column are 0-based.
pub fn tokenize(text: &str) -> Vec<SemanticToken> {
    let mut out = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        tokenize_line(line, i as u32, &mut out);
    }
    out
}

fn tokenize_line(line: &str, line_idx: u32, out: &mut Vec<SemanticToken>) {
    // Comments consume the whole rest of the line.
    if let Some(pos) = line.find('#') {
        // Only treat `#` as a comment if it's not inside a string. We do a
        // cheap check: count unescaped quotes before it.
        if !is_inside_string(&line[..pos]) {
            out.push(SemanticToken {
                line: line_idx,
                col: pos as u32,
                length: (line.len() - pos) as u32,
                ty: SemanticType::Comment,
            });
            return;
        }
    }

    let bytes = line.as_bytes();
    let mut i = 0;
    let mut prev_word: Option<&str> = None;

    while i < bytes.len() {
        let rest = &line[i..];

        // Whitespace.
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Double-quoted string.
        if rest.starts_with('"') {
            let len = match_string(rest, '"');
            push(out, line_idx, i, len, SemanticType::String);
            i += len;
            prev_word = None;
            continue;
        }
        // Single-quoted string (`''like this''`).
        if rest.starts_with("''") {
            let len = match_two_char_string(rest);
            push(out, line_idx, i, len, SemanticType::String);
            i += len;
            prev_word = None;
            continue;
        }

        // Variable `{...}` (nested allowed).
        if rest.starts_with('{') {
            let len = match_braced(rest);
            if len > 0 {
                push(out, line_idx, i, len, SemanticType::Variable);
                i += len;
                prev_word = None;
                continue;
            }
        }

        // Number.
        if let Some(len) = match_number(rest) {
            push(out, line_idx, i, len, SemanticType::Number);
            i += len;
            prev_word = None;
            continue;
        }

        // Operators.
        if let Some(len) = match_operator(rest) {
            push(out, line_idx, i, len, SemanticType::Operator);
            i += len;
            prev_word = None;
            continue;
        }

        // Word.
        if let Some(word) = match_word(rest) {
            let len = word.len();
            let ty = classify_word(word, prev_word);
            push(out, line_idx, i, len, ty);
            i += len;
            prev_word = Some(word);
            continue;
        }

        // Anything else: advance by one char to avoid an infinite loop.
        let len = next_char_len(rest);
        i += len;
        prev_word = None;
    }
}

fn push(out: &mut Vec<SemanticToken>, line: u32, col: usize, len: usize, ty: SemanticType) {
    if len == 0 {
        return;
    }
    out.push(SemanticToken {
        line,
        col: col as u32,
        length: len as u32,
        ty,
    });
}

fn match_string(rest: &str, quote: char) -> usize {
    let mut chars = rest.char_indices();
    let _ = chars.next(); // consume opening quote
    for (i, c) in chars {
        if c == quote {
            return i + c.len_utf8();
        }
    }
    rest.len()
}

fn match_two_char_string(rest: &str) -> usize {
    // `''text''`: find the closing `''`.
    if let Some(end) = rest[2..].find("''") {
        end + 4
    } else {
        rest.len()
    }
}

fn match_braced(rest: &str) -> usize {
    let mut depth = 0i32;
    let bytes = rest.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    0
}

fn match_number(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if !bytes.first().map(|b| b.is_ascii_digit() || *b == b'-' || *b == b'+').unwrap_or(false) {
        return None;
    }
    let mut i = 0;
    if bytes[i] == b'-' || bytes[i] == b'+' {
        i += 1;
    }
    let mut saw_digit = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        saw_digit = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if saw_digit {
        Some(i)
    } else {
        None
    }
}

fn match_operator(rest: &str) -> Option<usize> {
    // Multi-char operators first.
    for op in [">=", "<=", "!=", "=="] {
        if rest.starts_with(op) {
            return Some(op.len());
        }
    }
    if matches!(rest.chars().next(), Some('=' | '>' | '<')) {
        Some(1)
    } else {
        None
    }
}

fn match_word(rest: &str) -> Option<&str> {
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'\'' {
            i += 1;
        } else {
            break;
        }
    }
    if i == 0 {
        None
    } else {
        Some(&rest[..i])
    }
}

fn next_char_len(rest: &str) -> usize {
    rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1)
}

fn classify_word(word: &str, prev_word: Option<&str>) -> SemanticType {
    let lower = word.to_ascii_lowercase();

    // Event name following `on`.
    if prev_word.map(|p| p.eq_ignore_ascii_case("on")).unwrap_or(false) {
        return SemanticType::Event;
    }
    // Function name following `function`.
    if prev_word.map(|p| p.eq_ignore_ascii_case("function")).unwrap_or(false) {
        return SemanticType::Function;
    }

    if is_keyword(&lower) {
        SemanticType::Keyword
    } else if is_type(&lower) {
        SemanticType::Type
    } else if is_operator_word(&lower) {
        SemanticType::Operator
    } else {
        SemanticType::Keyword
    }
}

fn is_keyword(lower: &str) -> bool {
    KEYWORDS.get_or_init(|| {
        [
            "on", "if", "else", "loop", "while", "for", "set", "delete", "clear",
            "add", "remove", "give", "function", "command", "trigger", "options",
            "execute", "return", "stop", "exit", "continue", "cancel", "wait",
            "broadcast", "send", "make", "spawn", "teleport", "heal", "damage",
            "play", "run", "with", "at", "to", "from", "into", "of", "in",
            "named", "where", "by", "as", "and", "or", "not", "is", "isn't",
            "true", "false", "yes", "no", "now", "every", "after", "before",
        ]
        .iter()
        .copied()
        .collect::<HashSet<_>>()
    })
    .contains(lower)
}

fn is_type(lower: &str) -> bool {
    TYPES.get_or_init(|| {
        [
            "player", "players", "number", "numbers", "integer", "text", "string",
            "strings", "location", "locations", "entity", "entities", "item",
            "items", "itemstack", "itemtype", "boolean", "world", "worlds",
            "gamemode", "material", "materials", "block", "blocks", "object",
            "objects", "slot", "inventory", "enchantment", "potion", "vector",
            "direction", "date", "timespan", "timeperiod", "damagecause",
            "entitytype", "entitydata", "livingentity", "offlineplayer",
            "commandsender", "color", "chatcolor",
        ]
        .iter()
        .copied()
        .collect::<HashSet<_>>()
    })
    .contains(lower)
}

fn is_operator_word(lower: &str) -> bool {
    matches!(
        lower,
        "and" | "or" | "not" | "is" | "isn't" | "contains" | "between"
    )
}

fn is_inside_string(before: &str) -> bool {
    let mut in_string = false;
    let mut prev = ' ';
    for c in before.chars() {
        if c == '"' && prev != '\\' {
            in_string = !in_string;
        }
        prev = c;
    }
    in_string
}

// Static sets, built once and cached.
static KEYWORDS: OnceLock<HashSet<&'static str>> = OnceLock::new();
static TYPES: OnceLock<HashSet<&'static str>> = OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_comment_line() {
        let toks = tokenize("# hello");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].ty, SemanticType::Comment);
        assert_eq!(toks[0].length, 7);
    }

    #[test]
    fn tokenizes_string_and_variable() {
        let toks = tokenize(r#"send "hi" to {_p}"#);
        let types: Vec<_> = toks.iter().map(|t| t.ty).collect();
        assert!(types.contains(&SemanticType::String));
        assert!(types.contains(&SemanticType::Variable));
    }

    #[test]
    fn tokenizes_event_name_after_on() {
        let toks = tokenize("on join:");
        let join = toks.iter().find(|t| t.col == 3).unwrap();
        assert_eq!(join.ty, SemanticType::Event);
    }

    #[test]
    fn tokenizes_function_name_after_function() {
        let toks = tokenize("function foo():");
        let foo = toks.iter().find(|t| t.col == 9).unwrap();
        assert_eq!(foo.ty, SemanticType::Function);
    }

    #[test]
    fn tokenizes_number() {
        let toks = tokenize("set x to 42");
        let num = toks.iter().find(|t| t.ty == SemanticType::Number).unwrap();
        assert_eq!(num.length, 2);
    }

    #[test]
    fn ignores_hash_inside_string() {
        let toks = tokenize(r#"send "a # b""#);
        assert!(toks.iter().all(|t| t.ty != SemanticType::Comment));
    }
}
