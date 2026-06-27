//! Hover information.
//!
//! Given a line and a column, find the syntax element the user is hovering
//! over and render its documentation as markdown.

use crate::matching::{EntryTable, SyntaxIndex};
use crate::syntax::SyntaxEntry;

/// Markdown hover content for a position, or `None` if nothing matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverInfo {
    pub markdown: String,
    /// `(start_col, end_col)` of the highlighted range on the line, 0-based.
    pub range: (u32, u32),
}

/// Build hover info for `column` on `line`. `column` is a 0-based byte offset
/// into the line.
pub fn hover_at(
    line: &str,
    column: u32,
    index: &SyntaxIndex,
    table: &EntryTable,
) -> Option<HoverInfo> {
    let matches = index.matches(line);
    // Prefer the match whose span contains the column; fall back to the first.
    let chosen = matches
        .iter()
        .find(|m| column >= m.span.0 as u32 && column <= m.span.1 as u32)
        .or_else(|| matches.first())?;
    let entry = table.get(chosen.entry_id)?;
    Some(HoverInfo {
        markdown: render(entry),
        range: (chosen.span.0 as u32, chosen.span.1 as u32),
    })
}

/// Render an entry as a markdown hover string.
fn render(entry: &SyntaxEntry) -> String {
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
        let names: Vec<&str> = entry.required_plugins.iter().map(|p| p.name.as_str()).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{Addon, SyntaxType};

    fn entry(id: u32, pattern: &str, ty: SyntaxType) -> SyntaxEntry {
        SyntaxEntry {
            id,
            title: format!("title-{id}"),
            description: "Does a thing.".to_owned(),
            syntax_pattern: pattern.to_owned(),
            syntax_type: ty,
            addon: Addon {
                name: "Skript".to_owned(),
                link_to_addon: String::new(),
                usage_score: 1.0,
            },
            return_type: Some("Text".to_owned()),
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
    fn hover_returns_markdown_for_matching_line() {
        let entries = vec![entry(1, "send %text% to %player%", SyntaxType::Effect)];
        let index = SyntaxIndex::build(&entries);
        let table = EntryTable::from_entries(entries);
        let hover = hover_at(r#"send "hi" to Steve"#, 3, &index, &table).unwrap();
        assert!(hover.markdown.contains("**title-1**"));
        assert!(hover.markdown.contains("Does a thing."));
        assert!(hover.markdown.contains("**Addon:** Skript"));
    }

    #[test]
    fn hover_returns_none_for_unmatched_line() {
        let entries = vec![entry(1, "send %text%", SyntaxType::Effect)];
        let index = SyntaxIndex::build(&entries);
        let table = EntryTable::from_entries(entries);
        assert!(hover_at("totally unrelated", 0, &index, &table).is_none());
    }

    #[test]
    fn hover_includes_removed_warning() {
        let mut e = entry(2, "old effect %text%", SyntaxType::Effect);
        e.mark_as_removed = true;
        e.removed_since = Some("2.9".to_owned());
        let index = SyntaxIndex::build(&[e.clone()]);
        let table = EntryTable::from_entries(vec![e]);
        let hover = hover_at("old effect hi", 1, &index, &table).unwrap();
        assert!(hover.markdown.contains("⚠️ **Removed**"));
    }
}
