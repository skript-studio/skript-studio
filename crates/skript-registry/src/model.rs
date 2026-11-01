//! Serde model for the projected SkriptHub dump.
//! FROZEN CONTRACT — field names match the JSON exactly.

use serde::Deserialize;

/// Projected `addon` object of a SkriptHub entry.
#[derive(Debug, Clone, Deserialize)]
pub struct AddonInfo {
    pub name: String,
    #[serde(default)]
    pub link_to_addon: String,
    #[serde(default)]
    pub usage_score: f64,
}

/// Projected `required_plugins` item.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    #[serde(default)]
    pub link: Option<String>,
}

/// A SkriptHub syntax entry (one syntax element; its `syntax_pattern` may
/// contain multiple newline-separated patterns).
#[derive(Debug, Clone, Deserialize)]
pub struct SyntaxEntry {
    pub id: u64,
    #[serde(default)]
    pub json_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub syntax_pattern: String,
    pub syntax_type: String,
    pub addon: AddonInfo,
    #[serde(default)]
    pub required_plugins: Vec<PluginInfo>,
    #[serde(default)]
    pub return_type: Option<String>,
    #[serde(default)]
    pub type_usage: Option<String>,
    #[serde(default)]
    pub event_values: Option<String>,
    #[serde(default)]
    pub event_cancellable: bool,
    /// JSON-encoded string (`[{"name","isRequired","isSection"}]`), parsed
    /// lazily via [`SyntaxEntry::section_entries`].
    #[serde(default)]
    pub entries: Option<String>,
    #[serde(default)]
    pub keywords: Option<String>,
    #[serde(default)]
    pub compatible_addon_version: String,
    #[serde(default)]
    pub compatible_minecraft_version: String,
    #[serde(default)]
    pub mark_as_removed: bool,
    #[serde(default)]
    pub removed_since: Option<String>,
}

/// Parsed `syntax_type` string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SyntaxType {
    Expression,
    Effect,
    Event,
    Condition,
    Type,
    Section,
    Function,
    Structure,
    Other(String),
}

impl SyntaxType {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "expression" => SyntaxType::Expression,
            "effect" => SyntaxType::Effect,
            "event" => SyntaxType::Event,
            "condition" => SyntaxType::Condition,
            "type" => SyntaxType::Type,
            "section" => SyntaxType::Section,
            "function" => SyntaxType::Function,
            "structure" => SyntaxType::Structure,
            other => SyntaxType::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            SyntaxType::Expression => "expression",
            SyntaxType::Effect => "effect",
            SyntaxType::Event => "event",
            SyntaxType::Condition => "condition",
            SyntaxType::Type => "type",
            SyntaxType::Section => "section",
            SyntaxType::Function => "function",
            SyntaxType::Structure => "structure",
            SyntaxType::Other(other) => other,
        }
    }
}

/// A section entry definition (`entries` field of SkriptHub entries).
/// Keys arrive camelCased (`isRequired`, `isSection`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionEntryDef {
    pub name: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_section: bool,
}

impl SyntaxEntry {
    /// Parsed `syntax_type` value.
    pub fn syntax_type(&self) -> SyntaxType {
        SyntaxType::parse(&self.syntax_type)
    }

    /// The individual patterns of this entry: `syntax_pattern` split on
    /// `\r\n` / `\n`, each trimmed, empties dropped.
    pub fn split_patterns(&self) -> Vec<String> {
        let raw = self.syntax_pattern.replace("\r\n", "\n");
        raw.split('\n')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Lazily parsed section entries (`entries` JSON-in-JSON).
    pub fn section_entries(&self) -> Vec<SectionEntryDef> {
        match &self.entries {
            None => Vec::new(),
            Some(raw) => serde_json::from_str(raw).unwrap_or_default(),
        }
    }
}
