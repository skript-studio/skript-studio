//! Hover information.
//!
//! Renders SkriptHub syntax documentation as markdown for the LSP hover
//! feature. The actual line-to-syntax matching is done by the pattern
//! engine thread (see `AppState::java_hover`); this module only handles
//! the markdown rendering and AST-resolved hover (variable types,
//! function signatures).

use crate::syntax::SyntaxEntry;

/// Markdown hover content for a position, or `None` if nothing matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverInfo {
    pub markdown: String,
    /// `(start_col, end_col)` of the highlighted range on the line, 0-based
    /// **byte** columns (the LSP layer converts them to UTF-16).
    pub range: (u32, u32),
}

/// Render an entry as a markdown hover string.
pub fn render(entry: &SyntaxEntry) -> String {
    let mut md = String::new();
    md.push_str(&format!("**{}**\n\n", entry.title));

    if !entry.description.trim().is_empty() {
        md.push_str(entry.description.trim());
        md.push_str("\n\n");
    }

    md.push_str(&format!(
        "`{}`\n\n",
        entry.syntax_pattern.replace("\r\n", "  \n")
    ));

    md.push_str(&format!(
        "- **Type:** {}\n",
        capitalize(entry.syntax_type.as_str())
    ));
    md.push_str(&format!("- **Addon:** {}\n", entry.addon.name));

    if let Some(rt) = &entry.return_type {
        if !rt.trim().is_empty() {
            md.push_str(&format!("- **Returns:** {rt}\n"));
        }
    }
    if let Some(ev) = &entry.event_values {
        if !ev.trim().is_empty() {
            md.push_str(&format!("- **Event values:** {ev}\n"));
        }
    }
    if !entry.required_plugins.is_empty() {
        let names: Vec<&str> = entry
            .required_plugins
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        md.push_str(&format!("- **Requires:** {}\n", names.join(", ")));
    }
    if entry.mark_as_removed {
        let since = entry.removed_since.as_deref().unwrap_or("unknown");
        md.push_str(&format!(
            "\n⚠️ **Removed** in {} version {}\n",
            entry.addon.name, since
        ));
    }
    md.trim_end().to_owned()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// AST-aware hover
// ---------------------------------------------------------------------------

use crate::ast::{node_at_offset, NodeRef, Program, Span, TopLevelItem};
use crate::completion::linecol_to_offset;
use crate::semantic::types::TypeInfo;
use crate::semantic::SemanticAnalysis;
use crate::validation::offset_to_linecol;

/// AST-resolved hover info: variable declaration, function definition, type info.
#[derive(Debug, Clone, PartialEq)]
pub struct AstHoverInfo {
    pub markdown: String,
    /// `(start_col, end_col)` on the hovered line, 0-based **byte** columns
    /// (the LSP layer converts them to UTF-16).
    pub range: (u32, u32),
}

/// Byte span of the word containing `cursor` (a byte offset into
/// `line`), or `None` when the cursor sits on non-word text (whitespace,
/// punctuation, quotes).
///
/// A "word" is a maximal run of `[A-Za-z0-9_]` — the character class of
/// Skript keywords, function names and variable names. Hover ranges use
/// this so hovering any character of `broadcast` highlights the whole
/// word instead of a single character.
pub fn word_span_at(line: &str, cursor: usize) -> Option<(usize, usize)> {
    if line.is_empty() {
        return None;
    }
    // Clamp the cursor into the line (LSP may pass line-end + 1) and onto a
    // character boundary (never slice a multi-byte character).
    let mut at = cursor.min(line.len());
    while at > 0 && !line.is_char_boundary(at) {
        at -= 1;
    }
    if at == line.len() {
        at = previous_char_boundary(line, at);
    }
    if !is_word_at(line, at) {
        return None;
    }
    let mut start = at;
    while start > 0 {
        let prev = previous_char_boundary(line, start);
        if !is_word_at(line, prev) {
            break;
        }
        start = prev;
    }
    let mut end = at + char_len_at(line, at);
    while end < line.len() && is_word_at(line, end) {
        end += char_len_at(line, end);
    }
    Some((start, end))
}

/// Byte offset of the character boundary immediately before `at` (`at > 0`).
fn previous_char_boundary(line: &str, at: usize) -> usize {
    let mut i = at.saturating_sub(1);
    while i > 0 && !line.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// UTF-8 length of the character starting at `at` (which must be a boundary).
fn char_len_at(line: &str, at: usize) -> usize {
    line[at..].chars().next().map(char::len_utf8).unwrap_or(0)
}

/// Whether the character at `at` is part of a hoverable Skript word.
///
/// A character immediately preceded by `§` is part of a colour/format code
/// (`§a`, `§b`, …), not a word, even though the code letter itself is ASCII
/// alphanumeric. Without this, `§dhello` would merge into one word.
fn is_word_at(line: &str, at: usize) -> bool {
    let Some(ch) = line[at..].chars().next() else {
        return false;
    };
    if !(ch.is_ascii_alphanumeric() || ch == '_') {
        return false;
    }
    !line[..at].ends_with('\u{00A7}')
}

/// Resolve hover at `(line, column)` using AST + semantic analysis.
/// `source` is the full document text; `line` is 0-based and `column` is a
/// 0-based **UTF-8 byte** column. LSP clients send UTF-16 columns, so the
/// server converts at the protocol boundary before calling this.
pub fn hover_at_ast(
    source: &str,
    line: u32,
    column: u32,
    program: &Program,
    analysis: &SemanticAnalysis,
) -> Option<AstHoverInfo> {
    let offset = linecol_to_offset(source, line, column);
    let node = node_at_offset(program, offset)?;

    let span = node_ref_span(&node)?;
    let (s_line, s_col) = offset_to_linecol(source, span.start);
    let (e_line, e_col) = offset_to_linecol(source, span.end);
    let hovered_line = source.lines().nth(line as usize).unwrap_or("");
    let range = if s_line == line && e_line == line {
        (s_col, e_col)
    } else {
        // The node spans multiple lines (or starts on another line), so a
        // span-based highlight would either glow a whole block or — in the
        // old code — collapse to a single character. Highlight the word
        // under the cursor instead; fall back to one character only when
        // the cursor is not on word text at all.
        word_span_at(hovered_line, column as usize)
            .map(|(s, e)| (s as u32, e as u32))
            .unwrap_or_else(|| {
                // Non-word text: highlight one whole character. `column`
                // is a byte column, so clamp to a boundary before slicing.
                let mut start = (column as usize).min(hovered_line.len());
                while start > 0 && !hovered_line.is_char_boundary(start) {
                    start -= 1;
                }
                let end = hovered_line[start..]
                    .chars()
                    .next()
                    .map(|c| start + c.len_utf8())
                    .unwrap_or(start);
                (start as u32, end as u32)
            })
    };

    match node {
        NodeRef::VariableExpr(expr) => {
            let var_name = extract_var_name(expr)?;
            let scope = analysis.scope_tree.scope_at_offset(offset);
            let symbol = analysis.scope_tree.lookup(scope, var_name)?;

            let mut md = format!("**{{{}}}** — {}\n\n", var_name, symbol.kind.describe());
            let (_, decl_line, decl_col) = span_to_linecol(source, symbol.declared_at);
            md.push_str(&format!(
                "Declared at line {}, column {}",
                decl_line + 1,
                decl_col + 1
            ));

            let span_start = expr_span_start(expr);
            if let TypeInfo::Known(ty) = analysis.type_inference.get_type(span_start) {
                md.push_str(&format!("\n\nType: {}", ty));
            }

            Some(AstHoverInfo {
                markdown: md,
                range,
            })
        }
        NodeRef::FunctionCall(expr) => {
            let func_name = extract_func_name(expr)?;
            let func = analysis.function_registry.get(func_name)?;

            let params: Vec<&str> = func
                .decl
                .parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect();
            let sig = params.join(", ");

            let mut md = format!("**{}**\n\n", func_name);
            md.push_str(&format!("`function {}({})`\n\n", func_name, sig));
            if let Some(ref rt) = func.decl.return_type {
                md.push_str(&format!("- **Returns:** {}\n", rt.name));
            }
            if !params.is_empty() {
                md.push_str("- **Parameters:**\n");
                for p in &func.decl.parameters {
                    let ty_str = p.type_.as_ref().map(|t| t.name.as_str()).unwrap_or("any");
                    md.push_str(&format!("  - `{}`: {}\n", p.name, ty_str));
                }
            }

            Some(AstHoverInfo {
                markdown: md,
                range,
            })
        }
        NodeRef::FunctionDecl(item) => {
            let fd = match item {
                TopLevelItem::Function(fd) => fd,
                _ => return None,
            };
            let params: Vec<&str> = fd.parameters.iter().map(|p| p.name.as_str()).collect();
            let sig = params.join(", ");
            let mut md = format!("**{}**\n\n", fd.name);
            md.push_str(&format!("`function {}({})`\n\n", fd.name, sig));
            if let Some(ref rt) = fd.return_type {
                md.push_str(&format!("- **Returns:** {}\n", rt.name));
            }
            Some(AstHoverInfo {
                markdown: md,
                range,
            })
        }
        NodeRef::BinaryExpr(expr) => {
            let span_start = expr_span_start(expr);
            match analysis.type_inference.get_type(span_start) {
                TypeInfo::Known(ty) => {
                    let md = format!("Expression type: `{}`", ty);
                    Some(AstHoverInfo {
                        markdown: md,
                        range,
                    })
                }
                _ => None,
            }
        }
        NodeRef::Literal(expr) => {
            let span_start = expr_span_start(expr);
            match analysis.type_inference.get_type(span_start) {
                TypeInfo::Known(ty) => {
                    let md = format!("Type: `{}`", ty);
                    Some(AstHoverInfo {
                        markdown: md,
                        range,
                    })
                }
                _ => None,
            }
        }
        NodeRef::PropertyAccess(expr) => {
            let span_start = expr_span_start(expr);
            match analysis.type_inference.get_type(span_start) {
                TypeInfo::Known(ty) => {
                    let md = format!("Type: `{}`", ty);
                    Some(AstHoverInfo {
                        markdown: md,
                        range,
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Extract the byte span from a NodeRef.
fn node_ref_span(node: &NodeRef) -> Option<Span> {
    match node {
        NodeRef::VariableExpr(e)
        | NodeRef::FunctionCall(e)
        | NodeRef::BinaryExpr(e)
        | NodeRef::UnaryExpr(e)
        | NodeRef::Literal(e)
        | NodeRef::EventValue(e)
        | NodeRef::PropertyAccess(e) => Some(e.span()),
        NodeRef::Assignment(s) | NodeRef::Section(s) => Some(s.span()),
        NodeRef::Command(i) | NodeRef::FunctionDecl(i) => Some(i.span()),
    }
}

/// Try to extract a variable name from an expression node.
fn extract_var_name(expr: &crate::ast::Expression) -> Option<&str> {
    match expr {
        crate::ast::Expression::Variable(v) => Some(v.name.as_str()),
        _ => None,
    }
}

/// Try to extract a function name from a call expression.
fn extract_func_name(expr: &crate::ast::Expression) -> Option<&str> {
    match expr {
        crate::ast::Expression::FunctionCall(call) => {
            if let crate::ast::Expression::Variable(v) = call.callee.as_ref() {
                Some(v.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Convert a Span to (start_offset, line, column) where line/col are 0-based.
fn span_to_linecol(source: &str, span: Span) -> (usize, u32, u32) {
    let (line, col) = offset_to_linecol(source, span.start);
    (span.start, line, col)
}

/// Get the span-start key used by TypeInference.
fn expr_span_start(expr: &crate::ast::Expression) -> usize {
    expr.span().start
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // AST-aware hover tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ast_hover_variable() {
        let source = "function test():\n\tset {_x} to 5\n\tbroadcast {_x}";
        let parse_result = crate::parser::parse_program(source);
        let analysis = crate::semantic::analyze(&parse_result.program);
        // Hover over {_x} in the broadcast line (line 2, col 12 = 'x')
        let result = hover_at_ast(source, 2, 12, &parse_result.program, &analysis);
        assert!(result.is_some(), "should resolve hover for {{_x}}");
        let info = result.unwrap();
        assert!(
            info.markdown.contains("_x"),
            "markdown should contain var name"
        );
        assert!(
            info.markdown.contains("Declared at"),
            "markdown should show declaration"
        );
    }

    #[test]
    fn test_ast_hover_function_call_on_self() {
        let source = "function test():\n\treturn test()";
        let parse_result = crate::parser::parse_program(source);
        let analysis = crate::semantic::analyze(&parse_result.program);
        // Hover over `test` in `test()` on line 2 (byte offset column 8 = 't' in 'test')
        let result = hover_at_ast(source, 1, 8, &parse_result.program, &analysis);
        assert!(result.is_some(), "should resolve hover for test() call");
        let info = result.unwrap();
        assert!(
            info.markdown.contains("test"),
            "markdown should contain function name"
        );
        assert!(
            info.markdown.contains("function"),
            "markdown should show 'function'"
        );
    }

    #[test]
    fn test_ast_hover_literal_returns_type() {
        let source = "on load:\n\tbroadcast \"hello\"";
        let parse_result = crate::parser::parse_program(source);
        let analysis = crate::semantic::analyze(&parse_result.program);
        // Hover over string content (line 2, col 13 = 'e' in 'hello')
        // \tbroadcast "hello"
        // tab(0) b(1) r(2) o(3) a(4) d(5) c(6) a(7) s(8) t(9) (10) "(11) h(12) e(13) l(14) l(15) o(16) "(17)
        let result = hover_at_ast(source, 1, 13, &parse_result.program, &analysis);
        assert!(result.is_some(), "should resolve hover for string literal");
        let info = result.unwrap();
        assert!(
            info.markdown.contains("text"),
            "literal type should be 'text'"
        );
    }

    #[test]
    fn test_ast_hover_unknown_returns_none() {
        let source = "on load:\n\tsend \"hello\"";
        let parse_result = crate::parser::parse_program(source);
        let analysis = crate::semantic::analyze(&parse_result.program);
        // Hover over `send` keyword — effect statements return None (falls through to syntax)
        let result = hover_at_ast(source, 1, 1, &parse_result.program, &analysis);
        assert!(
            result.is_none(),
            "hover over keyword should be None (falls through to syntax)"
        );
    }

    // -----------------------------------------------------------------------
    // Word-span extraction (hover highlight range)
    // -----------------------------------------------------------------------

    #[test]
    fn word_span_covers_whole_word_from_any_character() {
        let line = "broadcast \"hi\"";
        // Hovering 'b', 'o' or 't' must all yield the full word span.
        for cursor in [0usize, 3, 8] {
            assert_eq!(
                word_span_at(line, cursor),
                Some((0, 9)),
                "cursor {cursor} must highlight the whole word"
            );
        }
    }

    #[test]
    fn word_span_of_second_word_and_none_on_space() {
        let line = "send \"hello\"";
        // 'hello' spans bytes 6..11 inside the quotes.
        assert_eq!(word_span_at(line, 8), Some((6, 11)));
        // The space between the words is not word text.
        assert_eq!(word_span_at(line, 4), None);
        // Opening quote is not word text either.
        assert_eq!(word_span_at(line, 5), None);
    }

    #[test]
    fn word_span_clamps_out_of_range_cursor() {
        // LSP may send cursor == line.len() (or beyond) for line-end hovers.
        assert_eq!(word_span_at("broadcast", 100), Some((0, 9)));
        assert_eq!(word_span_at("", 0), None);
        assert_eq!(word_span_at("   ", 2), None);
    }

    #[test]
    fn word_span_does_not_merge_a_colour_code_into_the_next_word() {
        // \tbroadcast §a§b§c§dhello world
        // 'd' at byte 22 belongs to `§d`; "hello" starts at byte 23.
        let line = "\tbroadcast §a§b§c§dhello world";
        assert_eq!(word_span_at(line, 23), Some((23, 28)));
        assert_eq!(word_span_at(line, 22), None);
    }

    #[test]
    fn word_span_clamps_mid_character_cursors() {
        // Byte 1 is the continuation byte of `§`; `§h` is a format code, so
        // the first hoverable word starts at `e` (byte 3).
        let line = "§hello";
        assert_eq!(word_span_at(line, 1), None);
        assert_eq!(word_span_at(line, 3), Some((3, 7)));
    }

    #[test]
    fn ast_hover_range_is_word_not_single_character() {
        // A node whose span starts on an earlier line used to collapse the
        // highlight to `(column, column + 1)` — one glowing character.
        // Build such a case: hover the literal inside a multi-line
        // expression node that starts on the previous line.
        let source = "on load:\n\tset {_msg} to \"a\" &\n\t\t\"b\"";
        let parse_result = crate::parser::parse_program(source);
        let analysis = crate::semantic::analyze(&parse_result.program);
        // Line 2 is `\t\t\"b\"`; hovering inside `b` (col 3).
        let result = hover_at_ast(source, 2, 3, &parse_result.program, &analysis);
        if let Some(info) = result {
            // Whatever node resolves, the range must NOT be a single char
            // when the cursor is on a word character.
            assert!(
                info.range.1 > info.range.0 + 1 || info.range == (3, 4),
                "range must not shrink to a single character: {:?}",
                info.range
            );
        }
    }
}
