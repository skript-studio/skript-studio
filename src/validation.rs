//! Diagnostics generation.
//!
//! Turn a parsed Skript document + a [`SyntaxIndex`] into a list of
//! [`Diagnostic`]s. The LSP layer maps these onto `tower-lsp` types; this
//! module stays free of LSP types so it is unit-testable on its own.
//!
//! Diagnostic rules (see the design doc):
//!   * Skip blank lines, comments, section headers, variable assignments.
//!   * Warn on lines that match no known pattern.
//!   * Warn when a matched entry is marked removed.
//!   * Warn when a matched entry requires an external plugin.
//!   * Warn on type mismatches among captured arguments.

use crate::matching::{EntryTable, Match, SyntaxIndex};
use crate::types::TypeCheck;
use crate::SyntaxEntry;

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

/// Produce diagnostics for an entire document.
///
/// `lines` are the document's lines without trailing newlines. Line/column
/// indices are 0-based.
pub fn validate_document(
    lines: &[&str],
    index: &SyntaxIndex,
    table: &EntryTable,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if should_skip(line) {
            continue;
        }
        validate_line(*line, i as u32, index, table, &mut diags);
    }
    diags
}

fn validate_line(
    line: &str,
    line_idx: u32,
    index: &SyntaxIndex,
    table: &EntryTable,
    out: &mut Vec<Diagnostic>,
) {
    let matches = index.matches_full(line);

    if matches.is_empty() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }
        out.push(Diagnostic {
            line: line_idx,
            start_col: 0,
            end_col: line.len().max(1) as u32,
            severity: Severity::Warning,
            source: "skript".to_owned(),
            code: "unknown-syntax".to_owned(),
            message: format!(
                "Line does not match any known Skript syntax: {trimmed:?}"
            ),
        });
        return;
    }

    for m in &matches {
        emit_for_match(m, table, line_idx, out);
    }
}

fn emit_for_match(
    m: &Match,
    table: &EntryTable,
    line_idx: u32,
    out: &mut Vec<Diagnostic>,
) {
    let Some(entry) = table.get(m.entry_id) else {
        return;
    };

    // Removed syntax always wins.
    if entry.mark_as_removed {
        let version = entry.removed_since.as_deref().unwrap_or("unknown");
        out.push(Diagnostic {
            line: line_idx,
            start_col: m.span.0 as u32,
            end_col: m.span.1 as u32,
            severity: Severity::Warning,
            source: "skript".to_owned(),
            code: "removed-syntax".to_owned(),
            message: format!(
                "{} was removed in {} version {}",
                entry.title, entry.addon.name, version
            ),
        });
    }

    // Required plugin reminder.
    if !entry.required_plugins.is_empty() {
        let names: Vec<&str> = entry.required_plugins.iter().map(|p| p.name.as_str()).collect();
        out.push(Diagnostic {
            line: line_idx,
            start_col: m.span.0 as u32,
            end_col: m.span.1 as u32,
            severity: Severity::Information,
            source: "skript".to_owned(),
            code: "requires-plugin".to_owned(),
            message: format!(
                "{} requires plugin(s): {}",
                entry.title,
                names.join(", ")
            ),
        });
    }

    // Type mismatches within the matched arguments.
    for (i, check) in m.type_checks.iter().enumerate() {
        if let TypeCheck::Mismatch(reason) = check {
            out.push(Diagnostic {
                line: line_idx,
                start_col: m.span.0 as u32,
                end_col: m.span.1 as u32,
                severity: Severity::Warning,
                source: "skript".to_owned(),
                code: format!("type-mismatch-{i}"),
                message: format!("Argument {}: {}", i + 1, reason),
            });
        }
    }
}

/// Decide whether a line should not be checked at all.
///
/// Skipped lines: blank, comment-only, section headers (`on ...:`,
/// `command /x:`, `function ...:`, `trigger:`, `options:`, `if/else/loop/while
/// ...:`), and bare variable assignments (`set`/`delete`/`add`/`remove` on a
/// `{...}` variable). The goal is to avoid noisy "unknown syntax" warnings on
/// structural lines we don't fully parse yet.
fn should_skip(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return true;
    }
    if is_section_header(trimmed) {
        return true;
    }
    if is_structural_keyword_line(trimmed) {
        return true;
    }
    false
}

/// Lines that end with `:` and introduce an indented block.
fn is_section_header(trimmed: &str) -> bool {
    if !trimmed.ends_with(':') {
        return false;
    }
    let head = trimmed.trim_end_matches(':').trim();
    // `on <event>:`, `command /x:`, `function name(...):`, `trigger:`,
    // `options:`, plus `if/else/loop/while/for ...:`.
    let lower = head.to_ascii_lowercase();
    lower.starts_with("on ")
        || lower.starts_with("command ")
        || lower.starts_with("function ")
        || lower == "trigger"
        || lower == "options"
        || lower.starts_with("if ")
        || lower.starts_with("else if ")
        || lower.starts_with("else")
        || lower.starts_with("loop ")
        || lower.starts_with("while ")
        || lower.starts_with("for ")
}

/// Lines that begin with a structural keyword we don't want to validate
/// against the syntax index (e.g. `else:`, `stop`, `exit`, `cancel event`,
/// `return`, indented section markers handled above).
fn is_structural_keyword_line(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "else" | "stop" | "exit" | "cancel event" | "continue" | "return"
    )
}

/// Convenience helper used by hover/completion tests: returns the first entry
/// that matches the line, if any.
pub fn first_match<'a>(
    line: &str,
    index: &SyntaxIndex,
    table: &'a EntryTable,
) -> Option<&'a SyntaxEntry> {
    index
        .matches(line)
        .into_iter()
        .find_map(|m| table.get(m.entry_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{Addon, SyntaxType};
    use crate::SyntaxEntry;

    fn entry(id: u32, pattern: &str, ty: SyntaxType) -> SyntaxEntry {
        SyntaxEntry {
            id,
            title: format!("entry-{id}"),
            description: String::new(),
            syntax_pattern: pattern.to_owned(),
            syntax_type: ty,
            addon: Addon {
                name: "Skript".to_owned(),
                link_to_addon: String::new(),
                usage_score: 1.0,
            },
            return_type: None,
            required_plugins: Vec::new(),
            event_values: None,
            type_usage: None,
            entries: None,
            compatible_addon_version: String::new(),
            compatible_minecraft_version: String::new(),
            json_id: None,
            event_cancellable: false,
            mark_as_removed: false,
            removed_since: None,
        }
    }

    fn build(entries: Vec<SyntaxEntry>) -> (SyntaxIndex, EntryTable) {
        let index = SyntaxIndex::build(&entries);
        let table = EntryTable::from_entries(entries);
        (index, table)
    }

    #[test]
    fn unknown_line_produces_warning() {
        let (index, table) = build(vec![entry(1, "send %text% to %player%", SyntaxType::Effect)]);
        let diags = validate_document(&["give sword"], &index, &table);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "unknown-syntax");
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    #[test]
    fn skips_comments_and_section_headers() {
        let (index, table) = build(vec![entry(1, "send %text%", SyntaxType::Effect)]);
        let lines = vec!["# a comment", "on join:", "    trigger:", ""];
        let diags = validate_document(&lines, &index, &table);
        assert!(diags.is_empty(), "got diagnostics: {diags:?}");
    }

    #[test]
    fn matched_line_emits_no_unknown_syntax() {
        let (index, table) = build(vec![entry(1, "send %text% to %player%", SyntaxType::Effect)]);
        let diags = validate_document(&[r#"send "hi" to Steve"#], &index, &table);
        assert!(
            diags.iter().all(|d| d.code != "unknown-syntax"),
            "got unknown-syntax: {diags:?}"
        );
    }

    #[test]
    fn reports_removed_syntax() {
        let mut e = entry(7, "old effect %text%", SyntaxType::Effect);
        e.mark_as_removed = true;
        e.removed_since = Some("2.9".to_owned());
        let (index, table) = build(vec![e]);
        let diags = validate_document(&["old effect hello"], &index, &table);
        assert!(diags.iter().any(|d| d.code == "removed-syntax"));
    }

    #[test]
    fn reports_required_plugin() {
        let mut e = entry(8, "make npc %number% dance", SyntaxType::Effect);
        e.required_plugins = vec![crate::syntax::Plugin {
            name: "Citizens".to_owned(),
            link: String::new(),
        }];
        let (index, table) = build(vec![e]);
        let diags = validate_document(&["make npc 5 dance"], &index, &table);
        assert!(diags.iter().any(|d| d.code == "requires-plugin"));
    }

    #[test]
    fn reports_type_mismatch() {
        let (index, table) = build(vec![entry(9, "set health to %number%", SyntaxType::Effect)]);
        let diags = validate_document(&["set health to Steve"], &index, &table);
        assert!(diags.iter().any(|d| d.code.starts_with("type-mismatch")));
    }
}
