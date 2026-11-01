//! Completion generation.
//!
//! Given a line prefix the user is typing, produce ranked completion items.
//! Each item carries enough data for the LSP layer to render it (label, insert
//! text as a snippet, detail, documentation). Ranking favors exact prefix
//! matches and addons with higher `usage_score`.

use crate::matching::EntryTable;
use crate::semantic::{FunctionRegistry, SemanticAnalysis};
use crate::semantic::scope::SymbolKind;
use crate::syntax::{SyntaxEntry, SyntaxType};
use std::collections::HashSet;

/// One completion suggestion, in LSP-agnostic form.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionItem {
    /// Human-readable label, shown in the picker. Usually the pattern itself.
    pub label: String,
    /// Inserted text, formatted as an LSP snippet (`${1:type}` tab-stops).
    pub insert_text: String,
    /// `filter_text` for LSP: the words used to match against user input.
    pub filter_text: String,
    /// Short detail line, e.g. `"Effect — Skript"`.
    pub detail: String,
    /// Markdown documentation shown in the details pane.
    pub documentation: String,
    /// The source entry id, for telemetry / hover reuse.
    pub entry_id: u32,
    /// Ranking bucket; lower sorts first.
    pub sort_bucket: u8,
    /// Tie-breaker score from the addon usage metric.
    pub usage_score: f64,
}

/// Context in which the user is asking for completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionContext {
    /// Top of the file (or after `options:`): structural constructs only.
    TopLevel,
    /// Inside an event/function/command body: effects, conditions, expressions.
    Body,
}

/// Produce completions for `prefix` (the text on the current line before the
/// cursor) in `ctx`. Results are sorted and capped at `max`.
pub fn completions(
    prefix: &str,
    ctx: CompletionContext,
    table: &EntryTable,
    max: usize,
) -> Vec<CompletionItem> {
    let prefix_lower = prefix.trim().to_ascii_lowercase();

    let mut items: Vec<CompletionItem> = Vec::new();

    // Always offer structural completions at the top level.
    if ctx == CompletionContext::TopLevel {
        items.extend(structural_completions(&prefix_lower));
    }

    // From the syntax index, take every (entry, variant) whose first literal
    // word contains the prefix as a substring.
    for entry in table.iter() {
        if !is_relevant_for_context(entry.syntax_type, ctx) {
            continue;
        }
        for variant in entry.pattern_variants() {
            let Some(first_literal) = first_literal_word(&variant) else {
                continue;
            };
            let (bucket, matches) = rank_prefix(&first_literal, &prefix_lower);
            if !matches {
                continue;
            }
            items.push(build_item(entry, &variant, &first_literal, bucket));
        }
    }

    // Deduplicate by label+insert_text so multi-variant entries don't repeat.
    let mut seen: HashSet<String> = HashSet::new();
    items.retain(|it| seen.insert(format!("{}|{}", it.label, it.insert_text)));

    // Sort: bucket asc, then usage_score desc, then label asc.
    items.sort_by(|a, b| {
        a.sort_bucket
            .cmp(&b.sort_bucket)
            .then_with(|| b.usage_score.partial_cmp(&a.usage_score).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.label.cmp(&b.label))
    });

    items.truncate(max);
    items
}

/// Completion items for structural keywords available in body context.
pub fn complete_keywords(prefix: &str) -> Vec<CompletionItem> {
    let keywords: &[(&str, &str, &str)] = &[
        ("if", "if ${1:condition}:", "Conditional section"),
        ("else if", "else if ${1:condition}:", "Else-if branch"),
        ("else", "else:", "Else branch"),
        ("while", "while ${1:condition}:", "While loop"),
        ("do while", "do while ${1:condition}:", "Do-while loop"),
        ("loop", "loop ${1:items}:", "Loop through items"),
        ("for each", "for each ${1:variable} in ${2:items}:", "For-each loop"),
        ("set", "set ${1:variable} to ${2:value}", "Set variable"),
        ("delete", "delete ${1:variable}", "Delete variable"),
        ("add", "add ${1:value} to ${2:variable}", "Add to variable"),
        ("remove", "remove ${1:value} from ${2:variable}", "Remove from variable"),
    ];

    let prefix_lower = prefix.trim().to_lowercase();
    let mut items: Vec<CompletionItem> = keywords.iter()
        .filter(|(label, _, _)| {
            prefix_lower.is_empty() || label.starts_with(&prefix_lower)
        })
        .map(|(label, insert, detail)| CompletionItem {
            label: label.to_string(),
            insert_text: insert.to_string(),
            filter_text: label.to_string(),
            detail: detail.to_string(),
            documentation: String::new(),
            entry_id: 0,
            sort_bucket: if *label == prefix_lower { 0 } else { 1 },
            usage_score: 0.0,
        })
        .collect();

    items.sort_by(|a, b| a.sort_bucket.cmp(&b.sort_bucket).then(a.label.cmp(&b.label)));
    items
}

/// Completion items for in-scope variables (local, parameters, globals).
pub fn complete_variables(
    prefix: &str,
    analysis: &SemanticAnalysis,
    source: &str,
    line: u32,
) -> Vec<CompletionItem> {
    let byte_offset = linecol_to_offset(source, line, 0);
    let scope_id = analysis.scope_tree.scope_at_offset(byte_offset);

    let prefix_lower = prefix.trim().to_lowercase();
    let mut items = Vec::new();
    let mut seen_names = HashSet::new();

    let mut current = Some(scope_id);
    while let Some(idx) = current {
        let scope = &analysis.scope_tree.scopes[idx];
        for sym in scope.symbols.values() {
            match sym.kind {
                SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::LoopVariable => {
                    let var_label = format!("{{{}}}", sym.name);
                    let matches = prefix_lower.is_empty()
                        || var_label.to_lowercase().starts_with(&prefix_lower)
                        || sym.name.to_lowercase().starts_with(&prefix_lower);
                    if matches && seen_names.insert(sym.name.clone()) {
                        items.push(CompletionItem {
                            label: var_label,
                            insert_text: format!("{{{}}}", sym.name),
                            filter_text: sym.name.clone(),
                            detail: format!("{} \u{2014} Skript", sym.kind.describe()),
                            documentation: String::new(),
                            entry_id: 0,
                            sort_bucket: 1,
                            usage_score: 0.0,
                        });
                    }
                }
                _ => {}
            }
        }
        current = scope.parent;
    }

    items
}

/// Completion items for function calls in body context.
pub fn complete_functions(
    prefix: &str,
    registry: &FunctionRegistry,
) -> Vec<CompletionItem> {
    let prefix_lower = prefix.trim().to_lowercase();
    let mut items = Vec::new();

    for func in registry.functions() {
        let name = &func.name;
        if !prefix_lower.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }

        let params: Vec<&str> = func.decl.parameters.iter().map(|p| p.name.as_str()).collect();
        let sig = params.join(", ");
        let insert = if params.is_empty() {
            format!("{}()", name)
        } else {
            let snippet_params: Vec<String> = params.iter().enumerate()
                .map(|(i, p)| format!("${{{}:{}}}", i + 1, p))
                .collect();
            format!("{}({})", name, snippet_params.join(", "))
        };

        items.push(CompletionItem {
            label: format!("{}()", name),
            insert_text: insert,
            filter_text: name.clone(),
            detail: format!("function({}) \u{2014} Skript", sig),
            documentation: String::new(),
            entry_id: 0,
            sort_bucket: if name.eq_ignore_ascii_case(&prefix_lower) { 0 } else { 1 },
            usage_score: 0.0,
        });
    }

    items.sort_by(|a, b| a.sort_bucket.cmp(&b.sort_bucket).then(a.label.cmp(&b.label)));
    items
}

/// Convert (line, column) to byte offset. Both are 0-based.
pub fn linecol_to_offset(source: &str, line: u32, column: u32) -> usize {
    let mut current_line = 0u32;
    let mut offset = 0usize;
    for (i, c) in source.char_indices() {
        if current_line == line {
            offset = i + column as usize;
            break;
        }
        if c == '\n' {
            current_line += 1;
        }
    }
    if current_line < line {
        source.len()
    } else {
        offset.min(source.len())
    }
}

/// Only effects/conditions/expressions are useful in bodies; everything but
/// events/types/sections is useful at the top level too (since a body can
/// appear anywhere).
fn is_relevant_for_context(ty: SyntaxType, ctx: CompletionContext) -> bool {
    match ctx {
        CompletionContext::TopLevel => !matches!(ty, SyntaxType::Event | SyntaxType::Other),
        CompletionContext::Body => matches!(
            ty,
            SyntaxType::Effect | SyntaxType::Condition | SyntaxType::Expression
        ),
    }
}

/// Extract the first literal run of a pattern: e.g. `send %text%` -> `send`.
/// Returns `None` if the pattern starts with a placeholder/optional/alt.
fn first_literal_word(variant: &str) -> Option<String> {
    let trimmed = variant.trim_start();
    // Read up to the first pattern metachar.
    let mut out = String::new();
    let mut chars = trimmed.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == '%' || c == '(' || c == '[' {
            break;
        }
        out.push(c);
        chars.next();
    }
    let word = out.trim().to_owned();
    if word.is_empty() {
        None
    } else {
        Some(word)
    }
}

/// Bucket a candidate against the prefix.
///   0 = exact first-word match (best)
///   1 = first word starts with prefix
///   2 = first word contains prefix
fn rank_prefix(first_literal: &str, prefix_lower: &str) -> (u8, bool) {
    let fl = first_literal.to_ascii_lowercase();
    if prefix_lower.is_empty() {
        return (2, true);
    }
    if fl == prefix_lower {
        return (0, true);
    }
    if fl.starts_with(prefix_lower) {
        return (1, true);
    }
    if fl.contains(prefix_lower) {
        return (2, true);
    }
    (3, false)
}

fn build_item(
    entry: &SyntaxEntry,
    variant: &str,
    first_literal: &str,
    bucket: u8,
) -> CompletionItem {
    let label = variant.trim().to_owned();
    let insert_text = pattern_to_snippet(variant);
    let detail = format!("{} — {}", capitalize(entry.syntax_type.as_str()), entry.addon.name);
    let documentation = build_docs(entry);
    CompletionItem {
        label,
        insert_text,
        filter_text: first_literal.to_ascii_lowercase(),
        detail,
        documentation,
        entry_id: entry.id,
        sort_bucket: bucket,
        usage_score: entry.addon.usage_score,
    }
}

/// Convert a Skript pattern into an LSP snippet: `%type%` -> `${n:type}`.
fn pattern_to_snippet(variant: &str) -> String {
    let mut out = String::new();
    let mut chars = variant.chars().peekable();
    let mut tab = 1;
    while let Some(c) = chars.next() {
        if c == '%' {
            // Read until the closing %.
            let mut ty = String::new();
            while let Some(&n) = chars.peek() {
                if n == '%' {
                    chars.next();
                    break;
                }
                ty.push(n);
                chars.next();
            }
            let label = ty.trim().trim_start_matches('-');
            // LSP snippet syntax: `${n:placeholder}`. We build it manually to
            // avoid `format!` brace-escaping confusion.
            if label.is_empty() || label == "*" {
                out.push_str("${");
                out.push_str(&tab.to_string());
                out.push_str(":value}");
            } else {
                out.push_str("${");
                out.push_str(&tab.to_string());
                out.push(':');
                out.push_str(label);
                out.push('}');
            }
            tab += 1;
        } else if c == '\\' {
            // Escaped char: emit verbatim.
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

fn build_docs(entry: &SyntaxEntry) -> String {
    let mut md = String::new();
    if !entry.description.trim().is_empty() {
        md.push_str(entry.description.trim());
    }
    if let Some(rt) = &entry.return_type {
        if !rt.trim().is_empty() {
            md.push_str(&format!("\n\n**Returns:** {rt}"));
        }
    }
    md.trim().to_owned()
}

/// Hand-written structural completions for the top level.
fn structural_completions(prefix_lower: &str) -> Vec<CompletionItem> {
    let candidates: [(&str, &str, &str); 4] = [
        ("on", "on ${1:event}:", "Start an event handler"),
        ("command", "command /${1:name}:", "Define a custom command"),
        ("function", "function ${1:name}(${2:args}):", "Define a function"),
        ("options", "options:", "Declare an options block"),
    ];
    candidates
        .iter()
        .filter_map(|(kw, snippet, desc)| {
            if prefix_lower.is_empty() || kw.starts_with(&prefix_lower) {
                Some(CompletionItem {
                    label: (*kw).to_owned(),
                    insert_text: (*snippet).to_owned(),
                    filter_text: (*kw).to_owned(),
                    detail: "Structure — Skript".to_owned(),
                    documentation: (*desc).to_owned(),
                    entry_id: 0,
                    sort_bucket: 0,
                    usage_score: 0.0,
                })
            } else {
                None
            }
        })
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Addon;

    fn entry(id: u32, pattern: &str, ty: SyntaxType, usage: f64) -> SyntaxEntry {
        SyntaxEntry {
            id,
            title: format!("entry-{id}"),
            description: "desc".to_owned(),
            syntax_pattern: pattern.to_owned(),
            syntax_type: ty,
            addon: Addon {
                name: "Skript".to_owned(),
                link_to_addon: String::new(),
                usage_score: usage,
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

    #[test]
    fn returns_send_completion_for_se_prefix() {
        let table = EntryTable::from_entries(vec![
            entry(1, "send %text% to %player%", SyntaxType::Effect, 100.0),
            entry(2, "set %object% to %object%", SyntaxType::Effect, 50.0),
        ]);
        let items = completions("se", CompletionContext::Body, &table, 50);
        assert!(items.iter().any(|i| i.label.starts_with("send")));
        assert!(items.iter().any(|i| i.label.starts_with("set")));
        assert!(!items.iter().any(|i| i.filter_text == "give"));
    }

    #[test]
    fn ranks_exact_prefix_first() {
        let table = EntryTable::from_entries(vec![
            entry(1, "send %text%", SyntaxType::Effect, 1.0),
            entry(2, "send message %text%", SyntaxType::Effect, 1000.0),
        ]);
        let items = completions("send", CompletionContext::Body, &table, 50);
        // Exact first-word match should win despite lower usage.
        assert_eq!(items[0].label, "send %text%");
    }

    #[test]
    fn snippet_has_tab_stops() {
        let table = EntryTable::from_entries(vec![entry(
            1,
            "send %text% to %player%",
            SyntaxType::Effect,
            1.0,
        )]);
        let items = completions("send", CompletionContext::Body, &table, 10);
        let send = items.iter().find(|i| i.label == "send %text% to %player%").unwrap();
        assert!(send.insert_text.contains("${1:text}"));
        assert!(send.insert_text.contains("${2:player}"));
    }

    #[test]
    fn toplevel_includes_structural_completions() {
        let table = EntryTable::from_entries(vec![entry(
            1,
            "send %text%",
            SyntaxType::Effect,
            1.0,
        )]);
        let items = completions("o", CompletionContext::TopLevel, &table, 50);
        assert!(items.iter().any(|i| i.label == "on"));
        assert!(items.iter().any(|i| i.label == "options"));
    }

    #[test]
    fn body_excludes_events() {
        let table = EntryTable::from_entries(vec![
            entry(1, "on join", SyntaxType::Event, 1.0),
            entry(2, "send %text%", SyntaxType::Effect, 1.0),
        ]);
        let items = completions("", CompletionContext::Body, &table, 50);
        assert!(!items.iter().any(|i| i.label.contains("join")));
        assert!(items.iter().any(|i| i.label.starts_with("send")));
    }

    #[test]
    fn respects_max_limit() {
        let table = EntryTable::from_entries(vec![
            entry(1, "send %text%", SyntaxType::Effect, 1.0),
            entry(2, "send x %text%", SyntaxType::Effect, 1.0),
            entry(3, "send y %text%", SyntaxType::Effect, 1.0),
        ]);
        let items = completions("send", CompletionContext::Body, &table, 2);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_complete_keywords_prefix() {
        let items = complete_keywords("if");
        assert!(items.iter().any(|i| i.label == "if"));
        assert!(!items.iter().any(|i| i.label == "set"));
    }

    #[test]
    fn test_complete_keywords_empty_prefix() {
        let items = complete_keywords("");
        assert!(items.len() >= 11);
        assert!(items.iter().any(|i| i.label == "if"));
        assert!(items.iter().any(|i| i.label == "set"));
    }

    #[test]
    fn test_complete_keywords_partial_prefix() {
        let items = complete_keywords("el");
        assert!(items.iter().any(|i| i.label == "else"));
        assert!(items.iter().any(|i| i.label == "else if"));
        assert!(!items.iter().any(|i| i.label == "if"));
    }

    #[test]
    fn test_complete_keywords_exact_first() {
        let items = complete_keywords("if");
        assert_eq!(items[0].sort_bucket, 0);
        assert_eq!(items[0].label, "if");
    }

    #[test]
    fn test_complete_functions_no_match() {
        let registry = FunctionRegistry::new();
        let items = complete_functions("zzz", &registry);
        assert!(items.is_empty());
    }

    #[test]
    fn test_complete_functions_with_params() {
        let mut registry = FunctionRegistry::new();
        registry.register(&crate::ast::FunctionDecl {
            name: "greet".into(),
            parameters: vec![crate::ast::Parameter {
                name: "name".into(),
                type_: None,
                default: None,
                span: crate::ast::Span::new(0, 4),
            }],
            return_type: None,
            body: vec![],
            span: crate::ast::Span::new(0, 0),
        }).ok();
        let items = complete_functions("", &registry);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "greet()");
        assert!(items[0].insert_text.contains("${1:name}"));
    }

    #[test]
    fn test_complete_variables_empty_prefix() {
        let source = "function test():\n\tset {_x} to 5\n\tset {_y} to 10\n\t# cursor";
        let parse_result = crate::parser::parse_program(source);
        let analysis = crate::semantic::analyze(&parse_result.program);
        let items = complete_variables("", &analysis, source, 3);
        assert!(items.iter().any(|i| i.label == "{_x}"));
        assert!(items.iter().any(|i| i.label == "{_y}"));
    }

    #[test]
    fn test_complete_variables_parameter_included() {
        let source = "function greet(name: text):\n\t# cursor";
        let parse_result = crate::parser::parse_program(source);
        let analysis = crate::semantic::analyze(&parse_result.program);
        let items = complete_variables("", &analysis, source, 1);
        assert!(items.iter().any(|i| i.label == "{name}"));
        assert!(items.iter().any(|i| i.detail.contains("Parameter")));
    }

    #[test]
    fn test_complete_variables_filter_by_prefix() {
        let source = "function test():\n\tset {_abc} to 5\n\tset {_xyz} to 10\n\t# cursor";
        let parse_result = crate::parser::parse_program(source);
        let analysis = crate::semantic::analyze(&parse_result.program);
        let items = complete_variables("{_a", &analysis, source, 3);
        assert!(items.iter().any(|i| i.label == "{_abc}"));
        assert!(!items.iter().any(|i| i.label == "{_xyz}"));
    }

    #[test]
    fn test_linecol_to_offset() {
        let source = "line0\nline1\nline2";
        assert_eq!(linecol_to_offset(source, 0, 0), 0);
        assert_eq!(linecol_to_offset(source, 1, 0), 6);
        assert_eq!(linecol_to_offset(source, 2, 0), 12);
        assert_eq!(linecol_to_offset(source, 5, 0), source.len());
    }
}
