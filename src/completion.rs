//! Completion generation.
//!
//! Given a line prefix the user is typing, produce ranked completion items.
//! Each item carries enough data for the LSP layer to render it (label, insert
//! text as a snippet, detail, documentation). Ranking favors exact prefix
//! matches and addons with higher `usage_score`.

use crate::matching::EntryTable;
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
}
