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
    /// `(start_col, end_col)` of the highlighted range on the line, 0-based.
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
    /// `(start_col, end_col)` on the hovered line, 0-based.
    pub range: (u32, u32),
}

/// Resolve hover at `(line, column)` using AST + semantic analysis.
/// `source` is the full document text, `line` and `column` are 0-based.
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
    let range = if s_line == line && e_line == line {
        (s_col, e_col)
    } else if s_line == line {
        (s_col, s_col + 1)
    } else {
        (column, column + 1)
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
}
