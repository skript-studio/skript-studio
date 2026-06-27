//! SkriptHub API client.
//!
//! Fetches the addon syntax list and normalizes it into [`SyntaxEntry`].
//! The only non-trivial transformation is expanding the stringified-JSON
//! `entries` field on section elements into a real `Vec<SectionEntry>`.

use crate::syntax::{SectionEntry, SyntaxEntry};
use serde::Deserialize;
use thiserror::Error;
use url::Url;

/// Default endpoint for the SkriptHub addon syntax dump.
pub const DEFAULT_SKRIPTHUB_URL: &str =
    "https://skripthub.net/api/v1/addonsyntaxlist/";

/// Errors that can occur while fetching or parsing the syntax list.
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("response body was not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Raw shape of an entry as returned by SkriptHub, before normalization.
///
/// `entries` arrives as a *string* containing JSON, so we read it as a
/// `String` here and parse it explicitly in [`normalize`].
#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(flatten)]
    entry: SyntaxEntry,
    /// Stringified JSON array (sections) or null/empty otherwise.
    #[serde(default)]
    entries: Option<String>,
}

/// Fetch the full syntax list from `url` and normalize each entry.
///
/// The request is made with a browser-like `User-Agent`; some SkriptHub
/// deployments reject requests without one. A generous timeout is used because
/// the dump is several MB.
pub async fn fetch_syntax_list(url: &str) -> Result<Vec<SyntaxEntry>, FetchError> {
    let parsed = Url::parse(url).map_err(|e| FetchError::InvalidUrl(e.to_string()))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(FetchError::InvalidUrl(format!(
            "expected http(s) URL, got scheme {}",
            parsed.scheme()
        )));
    }

    let client = reqwest::Client::builder()
        .user_agent("skript-lsp/0.1 (+https://github.com/User/skript-lsp-rust)")
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let raw: Vec<RawEntry> = client.get(url).send().await?.error_for_status()?.json().await?;
    Ok(raw.into_iter().map(normalize).collect())
}

/// Expand the stringified `entries` JSON on section entries; pass everything
/// else through unchanged.
fn normalize(raw: RawEntry) -> SyntaxEntry {
    let mut entry = raw.entry;
    entry.entries = raw.entries.as_deref().and_then(parse_section_entries);
    entry
}

/// Parse a stringified JSON array of section sub-entries. Returns `None` on
/// any parse failure (we treat malformed `entries` as "no entries" rather than
/// dropping the whole syntax element).
fn parse_section_entries(raw: &str) -> Option<Vec<SectionEntry>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<Vec<SectionEntry>>(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::SyntaxType;

    #[test]
    fn normalize_parses_stringified_entries_for_sections() {
        let raw_json = r#"entries=[{"name":"id","isRequired":true,"isSection":false},{"name":"sub","isRequired":false,"isSection":true}]"#;
        // Build a raw JSON document that mirrors what SkriptHub sends.
        let document = format!(
            r#"[{{
                "id": 1,
                "title": "Custom Section",
                "description": "",
                "syntax_pattern": "custom section",
                "syntax_type": "section",
                "addon": {{"name": "Skript", "link_to_addon": "", "usage_score": 0.0}},
                {raw_json}
            }}]"#
        );
        let raw_entries: Vec<RawEntry> = serde_json::from_str(&document).unwrap();
        let entry = normalize(raw_entries.into_iter().next().unwrap());
        assert_eq!(entry.syntax_type, SyntaxType::Section);
        let entries = entry.entries.expect("section entries should parse");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "id");
        assert!(entries[0].is_required);
        assert!(!entries[0].is_section);
        assert_eq!(entries[1].name, "sub");
        assert!(entries[1].is_section);
    }

    #[test]
    fn normalize_drops_malformed_entries() {
        let document = r#"[{
            "id": 2,
            "title": "X",
            "syntax_pattern": "x",
            "syntax_type": "section",
            "addon": {"name": "Skript", "link_to_addon": "", "usage_score": 0.0},
            "entries": "not-json"
        }]"#;
        let raw_entries: Vec<RawEntry> = serde_json::from_str(document).unwrap();
        let entry = normalize(raw_entries.into_iter().next().unwrap());
        assert!(entry.entries.is_none());
    }

    #[test]
    fn invalid_url_scheme_rejected() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(fetch_syntax_list("ftp://example.com"))
            .unwrap_err();
        assert!(matches!(err, FetchError::InvalidUrl(_)));
    }
}
