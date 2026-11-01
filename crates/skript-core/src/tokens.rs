//! Skript semantic tokenizer.
//!
//! Produces a flat list of typed spans (keyword, type, variable, string,
//! comment, operator, ...) suitable for LSP `textDocument/semanticTokens`.
//!
//! Uses the AST lexer (3-mode: Normal / InString / InStringInterpolation)
//! instead of the old line-by-line regex approach, so string interpolation,
//! nested braces, and MiniMessage tags are handled correctly.

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
    use crate::lexer::{Lexer, Token};

    // ---- build line-start byte-offset table ---------------------------------
    let line_starts: Vec<usize> = {
        let mut starts = vec![0usize];
        for (i, c) in text.char_indices() {
            if c == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    };

    // ---- 1. comment pre-pass ------------------------------------------------
    // The lexer skips `#` entirely, so we detect comments ourselves.
    let mut comment_positions: Vec<(usize, usize)> = Vec::new();
    for (line_idx, line) in text.split('\n').enumerate() {
        if line_idx >= line_starts.len() {
            break;
        }
        let trimmed = line.trim_start();
        if let Some(comment_start) = trimmed.find('#') {
            if !is_inside_string(&trimmed[..comment_start]) {
                let trim_offset = line.len() - trimmed.len();
                let col = trim_offset + comment_start;
                let offset = line_starts[line_idx] + col;
                let len = line.len() - col;
                comment_positions.push((offset, len));
            }
        }
    }

    // ---- 2. lexer pass ------------------------------------------------------
    let mut lexer = Lexer::new(text);
    // Per-line vector of (byte_offset, length, type)
    let mut tokens_by_line: Vec<Vec<(usize, usize, SemanticType)>> =
        vec![Vec::new(); line_starts.len()];
    let mut prev_word: Option<String> = None;

    loop {
        let spanned = lexer.next_token();
        let start = spanned.span.start;
        let len = spanned.span.end - start;

        match spanned.token {
            Token::Eof => break,

            // Structural tokens – not interesting for highlighting.
            Token::Indent | Token::Dedent | Token::Newline => continue,

            // Delimiters.
            Token::LParen
            | Token::RParen
            | Token::LBrace
            | Token::RBrace
            | Token::LBracket
            | Token::RBracket => continue,

            // String content (simple + interpolation fragments + MiniMessage).
            Token::String(_)
            | Token::StrText(_)
            | Token::MiniMessageOpen(_)
            | Token::MiniMessageClose(_) => {
                let li = line_index_for_offset(&line_starts, start);
                tokens_by_line[li].push((start, len, SemanticType::String));
            }
            Token::StrInterpolationStart | Token::StrInterpolationEnd => {
                // Just markers – skip.
            }

            // Variables.
            Token::Variable(_) | Token::EventValue(_) => {
                let li = line_index_for_offset(&line_starts, start);
                tokens_by_line[li].push((start, len, SemanticType::Variable));
            }

            // Numbers.
            Token::Number(_) => {
                let li = line_index_for_offset(&line_starts, start);
                tokens_by_line[li].push((start, len, SemanticType::Number));
            }

            // Booleans → highlighted as keywords (matches old behaviour).
            Token::Boolean(_) => {
                let li = line_index_for_offset(&line_starts, start);
                tokens_by_line[li].push((start, len, SemanticType::Keyword));
            }

            // Keywords.
            Token::Keyword(ref word) => {
                let lower = word.to_ascii_lowercase();
                let ty = if is_operator_word(&lower) {
                    SemanticType::Operator
                } else if prev_word.as_deref() == Some("on") {
                    SemanticType::Event
                } else if prev_word.as_deref() == Some("function") {
                    SemanticType::Function
                } else {
                    SemanticType::Keyword
                };
                let li = line_index_for_offset(&line_starts, start);
                tokens_by_line[li].push((start, len, ty));
                prev_word = Some(word.to_string());
            }

            // Identifiers (need context-sensitive classification).
            Token::Identifier(ref word) => {
                let lower = word.to_ascii_lowercase();
                let ty = if prev_word.as_deref() == Some("on") {
                    SemanticType::Event
                } else if prev_word.as_deref() == Some("function") {
                    SemanticType::Function
                } else if is_type(&lower) {
                    SemanticType::Type
                } else if is_operator_word(&lower) {
                    SemanticType::Operator
                } else {
                    SemanticType::Keyword
                };
                let li = line_index_for_offset(&line_starts, start);
                tokens_by_line[li].push((start, len, ty));
                prev_word = Some(word.clone());
            }

            // Operators.
            Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::Caret
            | Token::Tilde
            | Token::EqualEqual
            | Token::NotEqual
            | Token::LessEqual
            | Token::GreaterEqual
            | Token::Less
            | Token::Greater
            | Token::Equal
            | Token::ColonEqual
            | Token::PlusEqual
            | Token::MinusEqual
            | Token::StarEqual
            | Token::SlashEqual
            | Token::Dot
            | Token::Arrow
            | Token::Pipe => {
                let li = line_index_for_offset(&line_starts, start);
                tokens_by_line[li].push((start, len, SemanticType::Operator));
            }

            // Everything else (Colon, Comma, Semicolon, …) – skip.
            _ => {}
        }
    }

    // ---- 3. merge comment positions into line data --------------------------
    for &(offset, len) in &comment_positions {
        let li = line_index_for_offset(&line_starts, offset);
        tokens_by_line[li].push((offset, len, SemanticType::Comment));
    }

    // ---- 4. produce output --------------------------------------------------
    let mut result = Vec::new();
    for (line_idx, line_tokens) in tokens_by_line.iter().enumerate() {
        if line_tokens.is_empty() {
            continue;
        }
        let mut sorted = line_tokens.clone();
        sorted.sort_by_key(|&(offset, _, _)| offset);
        let merged = merge_adjacent_same_type(&sorted);
        for &(offset, length, ty) in &merged {
            let col = offset - line_starts[line_idx];
            result.push(SemanticToken {
                line: line_idx as u32,
                col: col as u32,
                length: length as u32,
                ty,
            });
        }
    }

    result
}

// ---- helpers ----------------------------------------------------------------

fn line_index_for_offset(line_starts: &[usize], offset: usize) -> usize {
    line_starts
        .binary_search_by(|&start| start.cmp(&offset))
        .unwrap_or_else(|i| i.saturating_sub(1))
        .min(line_starts.len() - 1)
}

fn merge_adjacent_same_type(
    tokens: &[(usize, usize, SemanticType)],
) -> Vec<(usize, usize, SemanticType)> {
    if tokens.is_empty() {
        return vec![];
    }
    let mut merged = Vec::new();
    let mut cur = tokens[0];
    for &next in &tokens[1..] {
        if next.0 == cur.0 + cur.1 && next.2 == cur.2 {
            cur.1 += next.1;
        } else {
            merged.push(cur);
            cur = next;
        }
    }
    merged.push(cur);
    merged
}

fn is_type(lower: &str) -> bool {
    TYPES
        .get_or_init(|| {
            [
                "player",
                "players",
                "number",
                "numbers",
                "integer",
                "text",
                "string",
                "strings",
                "location",
                "locations",
                "entity",
                "entities",
                "item",
                "items",
                "itemstack",
                "itemtype",
                "boolean",
                "world",
                "worlds",
                "gamemode",
                "material",
                "materials",
                "block",
                "blocks",
                "object",
                "objects",
                "slot",
                "inventory",
                "enchantment",
                "potion",
                "vector",
                "direction",
                "date",
                "timespan",
                "timeperiod",
                "damagecause",
                "entitytype",
                "entitydata",
                "livingentity",
                "offlineplayer",
                "commandsender",
                "color",
                "chatcolor",
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

/// Check if a `#` at the current position in a line is (probably) inside
/// a double-quoted string. Cheap heuristic: odd number of unescaped
/// double-quotes before the position means we are inside a string.
fn is_inside_string(before: &str) -> bool {
    let mut in_str = false;
    let mut prev = ' ';
    for c in before.chars() {
        if c == '"' && prev != '\\' {
            in_str = !in_str;
        }
        prev = c;
    }
    in_str
}

// Static sets, built once and cached.
static TYPES: OnceLock<HashSet<&'static str>> = OnceLock::new();

// ---- tests ------------------------------------------------------------------

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

    #[test]
    fn tokenizes_string_interpolation() {
        let toks = tokenize(r#"send "hello %{name}% world""#);
        let strings: Vec<_> = toks
            .iter()
            .filter(|t| t.ty == SemanticType::String)
            .collect();
        // StrText fragments + the optional String token for the leading
        // portion all come through as `SemanticType::String`.
        assert!(!strings.is_empty(), "expected at least one string token");
    }

    #[test]
    fn tokenizes_minimessage_tag() {
        let toks = tokenize(r#"send "<red>text</red>""#);
        let strings: Vec<_> = toks
            .iter()
            .filter(|t| t.ty == SemanticType::String)
            .collect();
        assert!(!strings.is_empty(), "expected at least one string token");
    }

    #[test]
    fn tokenizes_variable_nested_braces() {
        let toks = tokenize("set {_list::%{x}%} to 1");
        let vars: Vec<_> = toks
            .iter()
            .filter(|t| t.ty == SemanticType::Variable)
            .collect();
        assert_eq!(vars.len(), 1);
    }

    #[test]
    fn tokenizes_type_after_keyword() {
        let toks = tokenize("set {_x} to player");
        let types: Vec<_> = toks.iter().filter(|t| t.ty == SemanticType::Type).collect();
        assert!(!types.is_empty(), "expected at least one type token");
    }

    #[test]
    fn tokenizes_operator_word() {
        let toks = tokenize("x contains y");
        let ops: Vec<_> = toks
            .iter()
            .filter(|t| t.ty == SemanticType::Operator)
            .collect();
        assert!(!ops.is_empty(), "expected at least one operator token");
    }
}
