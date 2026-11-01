//! Diagnostics generation.
//!
//! Turn a parsed Skript document into a list of [`Diagnostic`]s. The LSP
//! layer maps these onto `tower-lsp` types; this module stays free of LSP
//! types so it is unit-testable on its own.
//!
//! Diagnostic rules:
//!   * Parse errors → Error
//!   * Semantic errors (unused variables, type mismatches) → Warning

use crate::parser::{parse_program, ParseResult};
use crate::semantic::{analyze, SemanticAnalysis};

/// Severity of a diagnostic. Mirrors LSP's `DiagnosticSeverity` ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// A source-language-agnostic diagnostic. The LSP layer converts the
/// 0-indexed `line` and `start`/`end` columns into `tower-lsp` ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub severity: Severity,
    /// Short machine-readable source code, e.g. `"skript"`.
    pub source: String,
    /// Stable identifier for the rule that fired, e.g. `"unknown-syntax"`.
    pub code: String,
    pub message: String,
}

/// Convert a byte `offset` in `source` to 0-based (line, col).
pub fn offset_to_linecol(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut last_newline = 0usize;
    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            last_newline = i + 1;
        }
    }
    let col = offset.saturating_sub(last_newline) as u32;
    (line, col)
}

/// Produce diagnostics from already-parsed data (no re-parsing).
pub fn validate_analysis(
    parse_result: &ParseResult,
    analysis: &SemanticAnalysis,
    source: &str,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for err in &parse_result.errors {
        let (line, start_col) = offset_to_linecol(source, err.span.start);
        let (_, end_col) = offset_to_linecol(source, err.span.end);
        diags.push(Diagnostic {
            line,
            start_col,
            end_col: end_col.max(start_col + 1),
            severity: Severity::Error,
            source: "skript".to_owned(),
            code: "parse-error".to_owned(),
            message: err.message.clone(),
        });
    }

    for err in &analysis.errors {
        diags.push(Diagnostic {
            line: 0,
            start_col: 0,
            end_col: 0,
            severity: Severity::Warning,
            source: "skript".to_owned(),
            code: "semantic".to_owned(),
            message: err.clone(),
        });
    }

    for decl in analysis.variable_tracker.unused_locals() {
        let (line, start_col) = offset_to_linecol(source, decl.declared_at.start);
        let (_, end_col) = offset_to_linecol(source, decl.declared_at.end);
        diags.push(Diagnostic {
            line,
            start_col,
            end_col: end_col.max(start_col + 1),
            severity: Severity::Warning,
            source: "skript".to_owned(),
            code: "unused-variable".to_owned(),
            message: format!("Unused local variable: {}", decl.name),
        });
    }

    diags
}

/// Validate a document using the AST parser + semantic analysis.
/// Supplements the line-based validation with AST-aware diagnostics.
pub fn validate_program(source: &str) -> Vec<Diagnostic> {
    let parse_result = parse_program(source);
    let analysis = analyze(&parse_result.program);
    validate_analysis(&parse_result, &analysis, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // validate_program tests
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_program_empty() {
        let diags = validate_program("");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn test_validate_program_unused_variable() {
        let source = "function test():\n\tset {_x} to 5";
        let diags = validate_program(source);
        assert!(
            diags.iter().any(|d| d.code == "unused-variable"),
            "expected unused-variable diagnostic, got {diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("_x")),
            "expected message to mention _x, got {diags:?}"
        );
    }

    #[test]
    fn test_validate_program_valid_code() {
        let source = "function test():\n\tset {_x} to 5\n\tbroadcast {_x}";
        let diags = validate_program(source);
        let bad: Vec<_> = diags
            .iter()
            .filter(|d| d.code != "unknown-syntax")
            .collect();
        assert!(
            bad.is_empty(),
            "expected no AST diagnostics for valid code, got {bad:?}"
        );
    }

    #[test]
    fn test_offset_to_linecol() {
        let source = "hello\nworld\nfoo";
        //  h  e  l  l  o  \n  w  o  r  l  d  \n  f  o  o
        //  0  1  2  3  4  5   6  7  8  9  10 11  12 13 14
        assert_eq!(offset_to_linecol(source, 0), (0, 0));
        assert_eq!(offset_to_linecol(source, 3), (0, 3));
        assert_eq!(offset_to_linecol(source, 6), (1, 0));
        assert_eq!(offset_to_linecol(source, 12), (2, 0));
        assert_eq!(offset_to_linecol(source, 13), (2, 1));
        // past-the-end gives column = offset - last_newline
        assert_eq!(offset_to_linecol(source, 100), (2, 88));
    }

    #[test]
    fn test_offset_to_linecol_crlf() {
        // Windows-style line endings — LF is still the only line separator
        // recognized, so carriage returns are just part of the column count.
        let source = "a\r\nb";
        //  a  \r  \n  b
        //  0  1   2   3
        assert_eq!(offset_to_linecol(source, 0), (0, 0));
        assert_eq!(offset_to_linecol(source, 1), (0, 1));
        assert_eq!(offset_to_linecol(source, 3), (1, 0));
    }
}
