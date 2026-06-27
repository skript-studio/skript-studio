//! SkriptHub data model.
//!
//! Mirrors the JSON returned by `https://skripthub.net/api/v1/addonsyntaxlist/`.
//! Every field that the LSP cares about is deserialized here; fields we don't
//! need yet are still captured so a future feature doesn't require schema
//! changes.
//!
//! See `docs/superpowers/specs/2026-06-19-skript-lsp-design.md` for the field
//! reference. The notable quirks handled here:
//!   * `syntax_pattern` may contain multiple variants separated by `\r\n`.
//!   * `entries` (for sections) is a *stringified* JSON array, not a real one.
//!   * Many fields are `null` *or* empty depending on `syntax_type`.

use serde::{Deserialize, Serialize};

/// The kind of a syntax element. Matches the string values produced by
/// SkriptHub (`"event"`, `"effect"`, `"condition"`, `"expression"`,
/// `"section"`, plus a couple of rarer ones we coerce into known variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyntaxType {
    Event,
    Effect,
    Condition,
    Expression,
    Section,
    /// Catch-all for unknown / future values. Keeps deserialization total.
    Other,
}

impl SyntaxType {
    /// Lowercase label used in UI strings ("effect", "expression", ...).
    pub fn as_str(self) -> &'static str {
        match self {
            SyntaxType::Event => "event",
            SyntaxType::Effect => "effect",
            SyntaxType::Condition => "condition",
            SyntaxType::Expression => "expression",
            SyntaxType::Section => "section",
            SyntaxType::Other => "other",
        }
    }
}

/// Metadata about the addon that provides a syntax element.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Addon {
    pub name: String,
    /// SkriptHub calls this `link_to_addon`.
    #[serde(default)]
    pub link_to_addon: String,
    #[serde(default)]
    pub usage_score: f64,
}

/// A non-Skript Minecraft plugin required for a syntax element to work
/// (e.g. `Citizens` for NPC effects).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub link: String,
}

/// A sub-entry declared by a `section` syntax element. `entries` in the raw
/// JSON is a *stringified* JSON array; the API client parses it into this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_section: bool,
}

/// One syntax documentation entry, as returned by the SkriptHub API.
///
/// Fields use `#[serde(default)]` liberally because SkriptHub returns `null`
/// for inapplicable fields rather than omitting them, and occasionally returns
/// `""` instead of `null`. Defaulting keeps deserialization total.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxEntry {
    pub id: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Raw pattern(s). May contain multiple variants separated by `\r\n`.
    #[serde(default)]
    pub syntax_pattern: String,
    pub syntax_type: SyntaxType,
    #[serde(default)]
    pub addon: Addon,
    #[serde(default)]
    pub return_type: Option<String>,
    #[serde(default)]
    pub required_plugins: Vec<Plugin>,
    #[serde(default)]
    pub event_values: Option<String>,
    #[serde(default)]
    pub type_usage: Option<String>,
    /// Parsed from the stringified JSON `entries` field by the client.
    /// `None` for non-section entries or when the field is missing/invalid.
    #[serde(default)]
    pub entries: Option<Vec<SectionEntry>>,
    #[serde(default)]
    pub compatible_addon_version: String,
    #[serde(default)]
    pub compatible_minecraft_version: String,
    #[serde(default)]
    pub json_id: Option<String>,
    #[serde(default)]
    pub event_cancellable: bool,
    #[serde(default)]
    pub mark_as_removed: bool,
    #[serde(default)]
    pub removed_since: Option<String>,
}

impl SyntaxEntry {
    /// Split the raw `syntax_pattern` into its individual variants.
    ///
    /// SkriptHub joins multiple valid forms with `\r\n`; we also tolerate a
    /// bare `\n`. Empty fragments are dropped and surrounding whitespace is
    /// trimmed so downstream pattern compilation never sees a blank pattern.
    pub fn pattern_variants(&self) -> Vec<String> {
        self.syntax_pattern
            .split("\r\n")
            .flat_map(|line| line.split('\n'))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_minimal_expression() {
        let json = r#"{
            "id": 848,
            "title": "Version",
            "description": "The version of Bukkit, Minecraft or Skript respectively.",
            "syntax_pattern": "([craft]bukkit|minecraft|skript)( |-)version",
            "syntax_type": "expression",
            "addon": {"name": "Skript", "link_to_addon": "https://github.com/SkriptLang/Skript", "usage_score": 1238.7},
            "return_type": "Text",
            "required_plugins": [],
            "event_values": null,
            "type_usage": "",
            "entries": null,
            "compatible_addon_version": "2.0",
            "compatible_minecraft_version": "",
            "json_id": "ExprVersion",
            "event_cancellable": false,
            "mark_as_removed": false,
            "removed_since": null
        }"#;
        let entry: SyntaxEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.id, 848);
        assert_eq!(entry.syntax_type, SyntaxType::Expression);
        assert_eq!(entry.addon.name, "Skript");
        assert!((entry.addon.usage_score - 1238.7).abs() < 1e-6);
        assert_eq!(entry.return_type.as_deref(), Some("Text"));
        assert_eq!(entry.pattern_variants(), vec!["([craft]bukkit|minecraft|skript)( |-)version"]);
    }

    #[test]
    fn deserializes_effect_with_null_fields() {
        let json = r#"{
            "id": 25,
            "title": "Add/Remove Players from Group Scores",
            "description": "Add or removed a players group based score.",
            "syntax_pattern": "add %player% to group score [with id] %string%\r\n(delete|remove) %player% from group [id based] score %string%",
            "syntax_type": "effect",
            "addon": {"name": "skRayFall", "link_to_addon": "https://dev.bukkit.org/projects/skrayfall", "usage_score": 26.5},
            "return_type": null,
            "required_plugins": [],
            "event_values": null,
            "type_usage": null,
            "entries": null,
            "compatible_addon_version": "",
            "compatible_minecraft_version": "",
            "json_id": null,
            "event_cancellable": false,
            "mark_as_removed": false,
            "removed_since": null
        }"#;
        let entry: SyntaxEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.syntax_type, SyntaxType::Effect);
        assert_eq!(entry.return_type, None);
        assert_eq!(
            entry.pattern_variants(),
            vec![
                "add %player% to group score [with id] %string%",
                "(delete|remove) %player% from group [id based] score %string%",
            ]
        );
    }

    #[test]
    fn deserializes_unknown_syntax_type_to_other() {
        let json = r#"{"id": 1, "syntax_type": "supereffect", "syntax_pattern": "x"}"#;
        let entry: SyntaxEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.syntax_type, SyntaxType::Other);
    }
}
