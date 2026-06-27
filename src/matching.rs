//! Pattern matching engine.
//!
//! Given a [`SyntaxIndex`] of compiled patterns and a line of user code,
//! find every syntax element that the line could be an instance of, and report
//! any type mismatches among the captured arguments.

use crate::pattern::{compile_pattern, parse_pattern, CompiledPattern};
use crate::syntax::{SyntaxEntry, SyntaxType};
use crate::types::{check_type, TypeCheck};
use std::collections::HashMap;
use std::sync::Arc;

/// A precompiled, ready-to-query index of every syntax element.
///
/// Built once from the SkriptHub dump and stored behind an `Arc` so request
/// handlers can share it cheaply.
#[derive(Debug)]
pub struct SyntaxIndex {
    /// All compiled variants across all entries, in source order.
    entries: Vec<IndexedEntry>,
}

#[derive(Debug)]
struct IndexedEntry {
    /// Source entry id; used to look up the full record for hover/completion.
    pub entry_id: u32,
    pub syntax_type: SyntaxType,
    /// Every variant of the entry's pattern, compiled independently.
    pub variants: Vec<CompiledVariant>,
}

#[derive(Debug)]
struct CompiledVariant {
    pub compiled: CompiledPattern,
}

/// A single match result: which entry matched, plus per-argument type checks.
#[derive(Debug, Clone)]
pub struct Match {
    /// Id of the [`SyntaxEntry`] that matched.
    pub entry_id: u32,
    pub syntax_type: SyntaxType,
    /// `Ok` or a description of the mismatch, one per `%type%` in source order.
    pub type_checks: Vec<TypeCheck>,
    /// Byte range of the overall match within the line.
    pub span: (usize, usize),
}

impl SyntaxIndex {
    /// Compile every variant of every entry into a queryable index.
    ///
    /// Variants that fail to compile (e.g. malformed regex from a bad pattern)
    /// are skipped with a tracing warning rather than aborting the whole build.
    pub fn build(entries: &[SyntaxEntry]) -> Self {
        let mut indexed: Vec<IndexedEntry> = Vec::with_capacity(entries.len());

        for entry in entries {
            let mut variants = Vec::new();
            for raw in entry.pattern_variants() {
                let parsed = parse_pattern(&raw);
                match compile_pattern(&parsed) {
                    Ok(compiled) => variants.push(CompiledVariant { compiled }),
                    Err(e) => {
                        tracing::warn!(
                            entry_id = entry.id,
                            pattern = %raw,
                            error = %e,
                            "failed to compile pattern variant; skipping"
                        );
                    }
                }
            }
            if !variants.is_empty() {
                indexed.push(IndexedEntry {
                    entry_id: entry.id,
                    syntax_type: entry.syntax_type,
                    variants,
                });
            }
        }

        tracing::info!(entry_count = indexed.len(), "built syntax index");
        SyntaxIndex { entries: indexed }
    }

    /// Find every entry whose pattern matches `line` as a prefix. Type-checking
    /// is done per match so callers can surface mismatch warnings independently.
    /// Used for completion and hover — call [`SyntaxIndex::matches_full`] for
    /// validation diagnostics.
    pub fn matches(&self, line: &str) -> Vec<Match> {
        let trimmed = line.trim_start();
        let leading_ws = line.len() - trimmed.len();
        let mut out = Vec::new();
        for entry in &self.entries {
            for variant in &entry.variants {
                // `captures` lets us read the per-type capture groups; `find`
                // would only give the overall span.
                let Some(caps) = variant.compiled.regex.captures(trimmed) else {
                    continue;
                };
                let overall = match caps.get(0) {
                    Some(m) => m,
                    None => continue,
                };
                // A regex match isn't anchored; only accept matches at the
                // start of the (trimmed) line.
                if overall.start() != 0 {
                    continue;
                }
                let type_checks: Vec<TypeCheck> = variant
                    .compiled
                    .type_groups
                    .iter()
                    .map(|(ty, idx)| {
                        let captured = caps
                            .get(*idx)
                            .map(|c| c.as_str())
                            .unwrap_or("");
                        check_type(ty, captured)
                    })
                    .collect();
                let end = leading_ws + overall.end();
                out.push(Match {
                    entry_id: entry.entry_id,
                    syntax_type: entry.syntax_type,
                    type_checks,
                    span: (leading_ws, end),
                });
            }
        }
        out
    }

    /// Like [`SyntaxIndex::matches`] but uses an end-anchored regex so only
    /// patterns that consume the entire (trimmed) line are returned. Used for
    /// validation diagnostics to avoid false positives from patterns like
    /// `%boolean%` matching arbitrary lines.
    pub fn matches_full(&self, line: &str) -> Vec<Match> {
        let trimmed = line.trim_start();
        let leading_ws = line.len() - trimmed.len();
        let mut out = Vec::new();
        for entry in &self.entries {
            for variant in &entry.variants {
                let Some(caps) = variant.compiled.full_regex.captures(trimmed) else {
                    continue;
                };
                let overall = match caps.get(0) {
                    Some(m) => m,
                    None => continue,
                };
                if overall.start() != 0 {
                    continue;
                }
                let type_checks: Vec<TypeCheck> = variant
                    .compiled
                    .type_groups
                    .iter()
                    .map(|(ty, idx)| {
                        let captured = caps
                            .get(*idx)
                            .map(|c| c.as_str())
                            .unwrap_or("");
                        check_type(ty, captured)
                    })
                    .collect();
                let end = leading_ws + overall.end();
                out.push(Match {
                    entry_id: entry.entry_id,
                    syntax_type: entry.syntax_type,
                    type_checks,
                    span: (leading_ws, end),
                });
            }
        }
        out
    }
}

/// Cheap handle to the full entry table, indexed by `id`.
///
/// The [`SyntaxIndex`] only keeps `id`s; look up the rest (title, description,
/// addon, ...) here when rendering hover/completion UI.
#[derive(Debug, Clone)]
pub struct EntryTable {
    by_id: HashMap<u32, Arc<SyntaxEntry>>,
}

impl EntryTable {
    pub fn from_entries(entries: Vec<SyntaxEntry>) -> Self {
        let by_id = entries
            .into_iter()
            .map(|e| (e.id, Arc::new(e)))
            .collect();
        EntryTable { by_id }
    }

    pub fn get(&self, id: u32) -> Option<&SyntaxEntry> {
        self.by_id.get(&id).map(|arc| arc.as_ref())
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Iterate over all entries. Order is unspecified.
    pub fn iter(&self) -> impl Iterator<Item = &SyntaxEntry> {
        self.by_id.values().map(|arc| arc.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{Addon, SyntaxEntry};
    use std::sync::Arc;

    fn entry(id: u32, pattern: &str, syntax_type: SyntaxType) -> SyntaxEntry {
        SyntaxEntry {
            id,
            title: format!("entry-{id}"),
            description: String::new(),
            syntax_pattern: pattern.to_owned(),
            syntax_type,
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

    #[test]
    fn matches_simple_send_pattern() {
        let entries = vec![entry(1, "send %text% to %player%", SyntaxType::Effect)];
        let index = SyntaxIndex::build(&entries);
        let matches = index.matches(r#"send "hello" to Steve"#);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].entry_id, 1);
        assert!(matches[0].type_checks.iter().all(|c| matches!(c, TypeCheck::Ok)));
    }

    #[test]
    fn does_not_match_unrelated_line() {
        let entries = vec![entry(1, "send %text% to %player%", SyntaxType::Effect)];
        let index = SyntaxIndex::build(&entries);
        assert!(index.matches("give sword to player").is_empty());
    }

    #[test]
    fn reports_type_mismatch() {
        let entries = vec![entry(2, "set health to %number%", SyntaxType::Effect)];
        let index = SyntaxIndex::build(&entries);
        let matches = index.matches("set health to Steve");
        assert_eq!(matches.len(), 1);
        assert!(matches!(&matches[0].type_checks[0], TypeCheck::Mismatch(_)));
    }

    #[test]
    fn matches_multiple_variants() {
        let entries = vec![entry(
            3,
            "add %player% to group\r\nremove %player% from group",
            SyntaxType::Effect,
        )];
        let index = SyntaxIndex::build(&entries);
        assert_eq!(index.matches("add Steve to group").len(), 1);
        assert_eq!(index.matches("remove Steve from group").len(), 1);
    }

    #[test]
    fn entry_table_looks_up_by_id() {
        let entries = vec![entry(42, "x", SyntaxType::Effect)];
        let table = EntryTable::from_entries(entries);
        assert_eq!(table.get(42).unwrap().title, "entry-42");
        assert!(table.get(7).is_none());
        assert_eq!(table.len(), 1);
        // The Arc round-trips.
        let _: &Arc<SyntaxEntry> = table.by_id.get(&42).unwrap();
    }
}
