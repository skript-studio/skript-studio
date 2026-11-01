//! Entry table — cheap handle to the full syntax entry set, indexed by `id`.
//!
//! Used by completion (iterating entries for prefix matching) and hover
//! (resolving an entry ID returned by the pattern engine thread to a
//! `SyntaxEntry` for markdown rendering).
//!
//! This is `Send`-safe (uses `Arc`, not `Rc`) so it can live in the
//! async-shared `AppState`. The `!Send` pattern-matching `Registry` lives
//! on the dedicated engine thread; `EntryTable` only stores entry data,
//! not compiled patterns.

use crate::syntax::SyntaxEntry;
use std::collections::HashMap;
use std::sync::Arc;

/// Cheap handle to the full entry table, indexed by `id`.
#[derive(Debug, Clone)]
pub struct EntryTable {
    by_id: HashMap<u32, Arc<SyntaxEntry>>,
}

impl EntryTable {
    pub fn from_entries(entries: Vec<SyntaxEntry>) -> Self {
        let by_id = entries.into_iter().map(|e| (e.id, Arc::new(e))).collect();
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
    use crate::syntax::{Addon, SyntaxType};

    fn entry(id: u32, pattern: &str) -> SyntaxEntry {
        SyntaxEntry {
            id,
            title: format!("entry-{id}"),
            description: String::new(),
            syntax_pattern: pattern.to_owned(),
            syntax_type: SyntaxType::Effect,
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
    fn entry_table_looks_up_by_id() {
        let entries = vec![entry(42, "x")];
        let table = EntryTable::from_entries(entries);
        assert_eq!(table.get(42).unwrap().title, "entry-42");
        assert!(table.get(7).is_none());
        assert_eq!(table.len(), 1);
        let _: &Arc<SyntaxEntry> = table.by_id.get(&42).unwrap();
    }
}
