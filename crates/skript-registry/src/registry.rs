//! The compiled pattern registry.
//!
//! OWNER: Agent 7. Fill in the `todo!()` bodies.

use std::collections::HashMap;
use std::path::Path;

use skript_patterns::{MalformedPatternError, SkriptPattern};

use crate::model::{SyntaxEntry, SyntaxType};

/// Error loading a registry.
#[derive(Debug)]
pub enum RegistryError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Io(e) => write!(f, "registry io error: {e}"),
            RegistryError::Json(e) => write!(f, "registry json error: {e}"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// One successfully compiled pattern of one registry entry.
#[derive(Debug)]
pub struct CompiledSyntax {
    pub entry_index: usize,
    /// Which split pattern of the entry this is (0-based).
    pub pattern_index: usize,
    /// The raw pattern string.
    pub raw: String,
    pub pattern: SkriptPattern,
}

/// A pattern that failed to compile.
#[derive(Debug, Clone)]
pub struct CompileFailure {
    pub entry_index: usize,
    pub pattern_index: usize,
    pub raw: String,
    pub error: MalformedPatternError,
}

/// The loaded SkriptHub corpus plus compiled patterns and indices.
#[derive(Debug, Default)]
pub struct Registry {
    entries: Vec<SyntaxEntry>,
    /// `compiled[i]` = compiled patterns of `entries[i]`.
    compiled: Vec<Vec<CompiledSyntax>>,
    failures: Vec<CompileFailure>,
    /// Patterns fixed by the backslash-restore fallback (see `compile_all`).
    repaired: Vec<(usize, usize)>,
    by_json_id: HashMap<String, Vec<usize>>,
    by_addon: HashMap<String, Vec<usize>>,
    by_type: HashMap<SyntaxType, Vec<usize>>,
}

impl Registry {
    /// Parse from a JSON string (an array of SkriptHub entries).
    pub fn from_json_str(s: &str) -> Result<Registry, serde_json::Error> {
        let entries: Vec<SyntaxEntry> = serde_json::from_str(s)?;

        let mut by_json_id: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_addon: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_type: HashMap<SyntaxType, Vec<usize>> = HashMap::new();

        for (index, entry) in entries.iter().enumerate() {
            // Only entries with a json_id are indexed; 462 json_ids are
            // duplicated across addons, hence `Vec<usize>`.
            if let Some(json_id) = &entry.json_id {
                by_json_id.entry(json_id.clone()).or_default().push(index);
            }
            // Addon key is the lowercased addon name; lookup lowercases the
            // query (see [`Registry::by_addon`]).
            by_addon
                .entry(entry.addon.name.to_lowercase())
                .or_default()
                .push(index);
            by_type.entry(entry.syntax_type()).or_default().push(index);
        }

        // NOTE: `vec![Vec::new(); entries.len()]` would require
        // `CompiledSyntax: Clone` (and thus `SkriptPattern: Clone`, which it
        // is not) — build the empty per-entry vecs via an iterator instead.
        let compiled: Vec<Vec<CompiledSyntax>> = (0..entries.len()).map(|_| Vec::new()).collect();

        Ok(Registry {
            entries,
            compiled,
            failures: Vec::new(),
            repaired: Vec::new(),
            by_json_id,
            by_addon,
            by_type,
        })
    }

    /// Load + parse from a file path.
    pub fn from_path(path: &Path) -> Result<Registry, RegistryError> {
        let s = std::fs::read_to_string(path).map_err(RegistryError::Io)?;
        Self::from_json_str(&s).map_err(RegistryError::Json)
    }

    /// Embedded corpus (`include_bytes!("../testdata/skripthub.json")`).
    pub fn embedded() -> Result<Registry, RegistryError> {
        const CORPUS: &[u8] = include_bytes!("../testdata/skripthub.json");
        let s = std::str::from_utf8(CORPUS).map_err(|e| {
            RegistryError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("embedded corpus is not valid utf-8: {e}"),
            ))
        })?;
        Self::from_json_str(s).map_err(RegistryError::Json)
    }

    /// All entries, in dump order.
    pub fn entries(&self) -> &[SyntaxEntry] {
        &self.entries
    }

    /// Entry indices registered under a `json_id` (may be several across
    /// addons — 462 duplicate json_ids exist in the corpus).
    pub fn by_json_id(&self, id: &str) -> &[usize] {
        self.by_json_id.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Entry indices belonging to an addon (case-insensitive match).
    pub fn by_addon(&self, addon: &str) -> &[usize] {
        self.by_addon
            .get(&addon.to_lowercase())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Entry indices for a syntax type.
    pub fn indices_for_type(&self, ty: &SyntaxType) -> &[usize] {
        self.by_type.get(ty).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Compile every split pattern through `skript_patterns::compile_pattern`.
    /// Returns the failures; successes are stored for
    /// [`Registry::compiled_for_entry`]. Safe to call once at startup.
    ///
    /// # SkriptHub data repair (fallback only)
    ///
    /// SkriptHub's scraper strips backslashes from some patterns: Skript's
    /// own `ExprLocationAt` registers `[\\(]` / `[\\)]` (escaped literal
    /// parens inside optionals) but the dump stores `[(]` / `[)]` — shapes
    /// the Java `PatternCompiler` provably rejects as well. When (and only
    /// when) a pattern fails to compile AND contains one of those sequences,
    /// it is retried once with the escape restored (`[(]` → `[\\(]`,
    /// `[)]` → `[\\)]`). Well-formed patterns are never touched, and the
    /// engine itself stays byte-faithful to Java.
    pub fn compile_all(&mut self) -> Vec<CompileFailure> {
        self.failures.clear();
        self.repaired.clear();
        for entry_index in 0..self.entries.len() {
            self.compiled[entry_index].clear();
            let patterns = self.entries[entry_index].split_patterns();
            for (pattern_index, raw) in patterns.into_iter().enumerate() {
                let attempt = skript_patterns::compile_pattern(&raw)
                    .map(|pattern| (raw.clone(), pattern, false))
                    .map_err(|error| (raw.clone(), error))
                    .or_else(|(raw, error)| {
                        if raw.contains("[(]") || raw.contains("[)]") {
                            let repaired = raw.replace("[(]", "[\\(]").replace("[)]", "[\\)]");
                            skript_patterns::compile_pattern(&repaired)
                                .map(|pattern| (repaired, pattern, true))
                                .map_err(|e| (raw.clone(), e))
                        } else {
                            Err((raw.clone(), error))
                        }
                    });
                match attempt {
                    Ok((raw, pattern, was_repaired)) => {
                        if was_repaired {
                            self.repaired.push((entry_index, pattern_index));
                        }
                        self.compiled[entry_index].push(CompiledSyntax {
                            entry_index,
                            pattern_index,
                            raw,
                            pattern,
                        });
                    }
                    Err((raw, error)) => {
                        self.failures.push(CompileFailure {
                            entry_index,
                            pattern_index,
                            raw,
                            error,
                        });
                    }
                }
            }
        }
        // Keep a copy in `self.failures` for post-hoc inspection via
        // [`Registry::failures`] while still returning the same data.
        self.failures.clone()
    }

    /// Patterns repaired by the SkriptHub backslash-restore fallback during
    /// the last `compile_all` (entry_index, pattern_index).
    pub fn repaired(&self) -> &[(usize, usize)] {
        &self.repaired
    }

    /// Compiled patterns of one entry (empty until `compile_all`).
    pub fn compiled_for_entry(&self, entry_index: usize) -> &[CompiledSyntax] {
        self.compiled
            .get(entry_index)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// All compile failures from the last `compile_all`.
    pub fn failures(&self) -> &[CompileFailure] {
        &self.failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full embedded corpus must load and match the corpus facts
    /// (PORTING_CONTRACTS.md / `scripts/prepare_corpus.py`).
    #[test]
    fn load_embedded() {
        let reg = Registry::embedded().expect("embedded corpus must load");

        // 8712 entries, 174 distinct addon names (keys are lowercased).
        assert_eq!(reg.entries().len(), 8712);
        assert_eq!(reg.by_addon.len(), 174);

        // json_id index: "ExprVersion" is registered by 3 addons in this
        // dump (Version, Version - from String, Http Version).
        assert_eq!(reg.by_json_id("ExprVersion").len(), 3);
        // Every returned index really carries that json_id.
        for &i in reg.by_json_id("ExprVersion") {
            assert_eq!(reg.entries()[i].json_id.as_deref(), Some("ExprVersion"));
        }

        // 13661 split patterns across all entries (\r\n multi-patterns).
        let split_total: usize = reg.entries().iter().map(|e| e.split_patterns().len()).sum();
        assert_eq!(split_total, 13661);

        // Per-type counts (corpus stats from prepare_corpus.py).
        let expected = [
            (SyntaxType::Expression, 4132),
            (SyntaxType::Effect, 1796),
            (SyntaxType::Event, 988),
            (SyntaxType::Condition, 842),
            (SyntaxType::Type, 660),
            (SyntaxType::Section, 196),
            (SyntaxType::Function, 78),
            (SyntaxType::Structure, 20),
        ];
        let mut indexed_total = 0;
        for (ty, count) in &expected {
            assert_eq!(reg.indices_for_type(ty).len(), *count, "type {ty:?}");
            indexed_total += reg.indices_for_type(ty).len();
        }
        // No `Other` types in this dump: every entry is indexed exactly once.
        assert_eq!(indexed_total, 8712);
        assert!(reg
            .indices_for_type(&SyntaxType::Other("nope".into()))
            .is_empty());

        // Nothing is compiled before compile_all.
        assert!(reg.failures().is_empty());
        assert!(reg.compiled_for_entry(0).is_empty());
        assert!(reg.compiled_for_entry(999_999).is_empty()); // out of range
    }

    /// Addon lookup is case-insensitive against the lowercased keys.
    #[test]
    fn embedded_addon_lookup_is_case_insensitive() {
        let reg = Registry::embedded().expect("embedded corpus must load");
        let lower = reg.by_addon("skript");
        let upper = reg.by_addon("SKRIPT");
        assert!(!lower.is_empty(), "skript addon must exist");
        assert_eq!(lower, upper);
        for &i in lower {
            assert_eq!(reg.entries()[i].addon.name.to_lowercase(), "skript");
        }
    }

    /// Small inline corpus: multi-pattern `\r\n` splitting, missing optional
    /// fields (`json_id` null, `required_plugins` absent, partial `addon`),
    /// and JSON-in-JSON `entries` (section_entries) parsing.
    #[test]
    fn from_json_str_small_corpus() {
        let json = r#"[
            {
                "id": 1,
                "json_id": null,
                "title": "Line Break",
                "syntax_type": "expression",
                "syntax_pattern": "[a] line break\r\n[an] (invis(ible|ibility)|new) line",
                "addon": {
                    "name": "Skript",
                    "link_to_addon": "https://skriptlang.org",
                    "usage_score": 123.4
                }
            },
            {
                "id": 2,
                "json_id": "SecLoop",
                "title": "Loop",
                "syntax_type": "section",
                "syntax_pattern": "loop %objects%",
                "addon": { "name": "Skript" },
                "entries": "[{\"name\": \"conditions\", \"isRequired\": false, \"isSection\": true}]"
            }
        ]"#;

        let reg = Registry::from_json_str(json).expect("inline corpus must parse");
        assert_eq!(reg.entries().len(), 2);

        // Entry 0: \r\n multi-pattern splits into two trimmed patterns;
        // `json_id: null` -> NOT indexed.
        let entry0 = &reg.entries()[0];
        assert_eq!(
            entry0.split_patterns(),
            ["[a] line break", "[an] (invis(ible|ibility)|new) line"]
        );
        assert!(reg.by_json_id("LineBreak").is_empty());
        assert!(reg.by_json_id("").is_empty());

        // Entry 1: single pattern, indexed json_id, section entries parsed
        // out of the JSON-in-JSON `entries` field.
        let entry1 = &reg.entries()[1];
        assert_eq!(entry1.split_patterns(), ["loop %objects%"]);
        assert_eq!(reg.by_json_id("SecLoop"), &[1]);

        let defs = entry1.section_entries();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "conditions");
        // Section-entry JSON-in-JSON: camelCase keys (`isRequired`,
        // `isSection`) populate `is_required` / `is_section` via the
        // `#[serde(rename_all = "camelCase")]` on `SectionEntryDef`.
        assert!(!defs[0].is_required, "json says isRequired: false");
        assert!(defs[0].is_section, "json says isSection: true");

        // Missing optional fields default cleanly (no required_plugins key,
        // no addon link_to_addon / usage_score keys).
        assert!(entry1.required_plugins.is_empty());
        assert_eq!(entry1.addon.link_to_addon, "");
        assert_eq!(entry1.addon.usage_score, 0.0);
        // The present addon fields of entry 0 deserialize.
        assert_eq!(entry0.addon.link_to_addon, "https://skriptlang.org");
        assert_eq!(entry0.addon.usage_score, 123.4);

        // Addon index: lowercased key, case-insensitive lookup, dump order.
        assert_eq!(reg.by_addon("skript"), &[0, 1]);
        assert_eq!(reg.by_addon("SKRIPT"), &[0, 1]);
        assert!(reg.by_addon("skrayfall").is_empty());

        // Type index.
        assert_eq!(reg.indices_for_type(&SyntaxType::Expression), &[0]);
        assert_eq!(reg.indices_for_type(&SyntaxType::Section), &[1]);
        assert!(reg.indices_for_type(&SyntaxType::Effect).is_empty());

        // Nothing compiled yet.
        assert!(reg.compiled_for_entry(0).is_empty());
        assert!(reg.failures().is_empty());

        // Malformed JSON -> serde error.
        assert!(Registry::from_json_str("{ not json").is_err());
        // Wrong shape (object instead of array) -> serde error.
        assert!(Registry::from_json_str("{\"id\": 1}").is_err());
    }

    /// `from_path` round-trips through a temp file; missing files map to
    /// `RegistryError::Io`.
    #[test]
    fn from_path_roundtrip() {
        let json = r#"[{"id": 7, "title": "Reload", "syntax_type": "effect",
            "syntax_pattern": "reload [the] (config|scripts)",
            "addon": {"name": "Skript"}}]"#;
        let path =
            std::env::temp_dir().join(format!("skript-registry-test-{}.json", std::process::id()));
        std::fs::write(&path, json).expect("write temp file");
        let reg = Registry::from_path(&path).expect("from_path must succeed");
        assert_eq!(reg.entries().len(), 1);
        assert_eq!(reg.entries()[0].title, "Reload");
        assert_eq!(reg.by_addon("Skript"), &[0]);
        let _ = std::fs::remove_file(&path);

        // Deleted path -> Io error.
        let err = Registry::from_path(&path).expect_err("deleted path must fail");
        assert!(matches!(err, RegistryError::Io(_)), "got {err:?}");
    }

    /// Full-corpus compile through the faithful Java-port engine.
    ///
    /// Expected: 17 failures out of 13661 (99.88% compile rate). All 17 are
    /// genuinely malformed patterns in SkriptHub's database (hand-typed
    /// typos like `ax[imum]` for `max[imum]`, odd `%` counts, stray
    /// brackets, `¦` misuse) — the Java `PatternCompiler` rejects every one
    /// of them identically. Separately, 15 patterns are auto-repaired by
    /// the backslash-restore fallback (`[(]` → `[\\(]`, a provable
    /// SkriptHub scraper artifact; see `compile_all`'s docs).
    #[test]
    fn compile_all_embedded_expects_known_failures() {
        let mut reg = Registry::embedded().expect("embedded corpus must load");
        let failures = reg.compile_all();
        for f in &failures {
            let e = &reg.entries()[f.entry_index];
            eprintln!(
                "FAILURE [{}] {}/{}: {:?} -> {}",
                e.id, e.addon.name, e.title, f.raw, f.error.message
            );
        }
        assert_eq!(
            failures.len(),
            17,
            "expected 17 genuinely-malformed corpus patterns, got {} (first: {:?})",
            failures.len(),
            failures.first()
        );
        // The backslash-restore fallback repaired the `[(]`/`[)]` family
        // (Skript's own Location At + the Skirt coordinate patterns).
        assert_eq!(reg.repaired().len(), 15, "scraper-escape repairs");
        // Every split pattern is either compiled or recorded as a failure.
        let compiled_total: usize = (0..reg.entries().len())
            .map(|i| reg.compiled_for_entry(i).len())
            .sum();
        assert_eq!(compiled_total + reg.failures().len(), 13661);
        // The returned vec and self.failures agree.
        assert_eq!(reg.failures().len(), failures.len());
    }
}
