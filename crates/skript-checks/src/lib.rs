//! Static diagnostic rules for Skript scripts.
//!
//! OWNER: Agent 9. Line-based static analysis (v1). For each script line:
//!  1. Skip blank lines and `#` comments (including trailing inline
//!     comments, which are stripped by a tiny quote-aware scanner).
//!  2. Classify: `on ...:` (event) / other `...:` (section head — section,
//!     structure or condition) / otherwise (effect or condition).
//!  3. Try matching against the compiled patterns of the corresponding
//!     syntax types (a `TypeResolver` — usually `PermissiveResolver` — is
//!     installed by the caller; `with_type_resolver` falls back to
//!     `PermissiveResolver` when none was installed).
//!  4. Emit diagnostics per the rules below.
//!
//! Rules (code -> severity, in priority order — at most ONE per line):
//!  - `unparseable`          Error   — no pattern of any matching syntax type fits
//!  - `addon-not-enabled`    Warning — only fits patterns from addons not in
//!    `config.enabled_addons` (when non-empty)
//!  - `plugin-missing`       Warning — matching entry requires plugins not in
//!    `config.installed_plugins`
//!  - `ambiguous-addon`      Info    — matches entries from >= 2 different addons
//!
//! # Heuristics (documented divergences / approximations)
//!
//! * **Indentation does not matter** in v1: every line is checked
//!   independently, with no block context. Lines inside strings or
//!   multi-line structures are still checked line-by-line.
//! * **Classification.** `on ...:` (case-insensitive `on` + space, trailing
//!   `:`) is an event. Any other `:`-terminated line is treated as a
//!   *section head* with candidates `[Section, Structure, Condition]`:
//!   most `:`-terminated lines are either sections (`loop %objects%:`),
//!   structures (`command /home:`) or conditions carrying a section body
//!   (`if player is op:`). Known limitation: `every 5 seconds:` (a real
//!   Skript event without the `on` prefix) is classified as a head.
//!   Everything else is a *statement* with candidates `[Effect, Condition]`.
//! * **Inline comments.** A `#` outside double quotes starts a comment.
//!   The scanner toggles quote state on every `"`; this is exactly right
//!   for Skript string literals, where a literal quote inside a string is
//!   written doubled (`"say ""hi"""`). Backslash escapes are not treated
//!   specially.
//! * **Case-insensitive comparisons** of addon names (both against
//!   `enabled_addons` and when counting "different addons") and of plugin
//!   names against `installed_plugins`.
//! * **Closest-syntax hints.** For `unparseable` lines the 3 closest entry
//!   titles are those whose FIRST pattern shares the longest common
//!   word-prefix with the line (words = lowercased, split on
//!   non-alphanumerics). Ties are broken by the number of pattern words
//!   appearing anywhere in the line, then by registry order. Entries with
//!   zero shared prefix words are not suggested.
//! * **Corpus reality note:** with the permissive resolver a handful of
//!   catch-all patterns in the full corpus (skript-reflect's
//!   `[(await)] %javaobject%`, SkJade's `%booleans%`, Hippo's
//!   `%modifiers% %pair%`, skript-mirror's `[on] [all] %javatype%`) match
//!   every line, so on the full 8712-entry corpus `unparseable` cannot
//!   fire and `ambiguous-addon` fires often. The rules are still correct
//!   per spec; a typed resolver (future) will tighten this.
//!
//! Column spans: cover the trimmed line content (0-based UTF-8 character
//! counts — not byte offsets — into the original line).

use std::collections::HashSet;

use skript_patterns::{ParseContext, ParseFlags};
use skript_registry::{Registry, SyntaxType};

/// Max number of "did you mean" titles on `unparseable` diagnostics.
const HINT_LIMIT: usize = 3;
/// Max number of addons / titles listed inside rule messages.
const LIST_LIMIT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct LineDiagnostic {
    /// 0-based line index.
    pub line: usize,
    /// 0-based character columns.
    pub col_start: usize,
    pub col_end: usize,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct CheckConfig {
    /// Empty = all addons treated as enabled.
    pub enabled_addons: HashSet<String>,
    pub installed_plugins: HashSet<String>,
    pub skript_version: Option<String>,
    pub minecraft_version: Option<String>,
}

/// Check a whole document. `registry` must have had `compile_all()` called.
/// The caller is responsible for installing a `TypeResolver` (usually
/// `PermissiveResolver`) on the calling thread before calling this; when
/// none is installed, `skript_patterns::with_type_resolver` falls back to
/// `PermissiveResolver`.
pub fn check_document(
    text: &str,
    registry: &Registry,
    config: &CheckConfig,
) -> Vec<LineDiagnostic> {
    // Computed once for the fallback `unparseable` message
    // ("no matching syntax in 174 addons" for the embedded corpus).
    let addon_count = distinct_addon_count(registry);
    let mut out = Vec::new();
    for (line_no, raw) in text.lines().enumerate() {
        let Some(analyzed) = analyze_line(raw) else {
            continue;
        };
        let matches = collect_matches(&analyzed.stripped, analyzed.kind.candidates(), registry);
        if let Some(d) = evaluate_matches(
            line_no,
            analyzed.col_start,
            analyzed.col_end,
            &analyzed.stripped,
            analyzed.kind,
            &matches,
            registry,
            config,
            addon_count,
        ) {
            out.push(d);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Line classification
// ---------------------------------------------------------------------------

/// How a (non-blank, non-comment) line is classified, and which syntax
/// types are tried against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    /// `on ...:` — Skript event header.
    Event,
    /// Any other `...:`-terminated line — section / structure / condition
    /// with a section body (see the module docs for the rationale).
    Head,
    /// Everything else — effect or condition.
    Statement,
}

const EVENT_CANDIDATES: &[SyntaxType] = &[SyntaxType::Event];
const HEAD_CANDIDATES: &[SyntaxType] = &[
    SyntaxType::Section,
    SyntaxType::Structure,
    SyntaxType::Condition,
];
const STATEMENT_CANDIDATES: &[SyntaxType] = &[SyntaxType::Effect, SyntaxType::Condition];

impl LineKind {
    fn candidates(self) -> &'static [SyntaxType] {
        match self {
            LineKind::Event => EVENT_CANDIDATES,
            LineKind::Head => HEAD_CANDIDATES,
            LineKind::Statement => STATEMENT_CANDIDATES,
        }
    }

    /// Kind name used in `unparseable` messages. Heads are called
    /// "section" and statements "effect" because those are the most common
    /// intents of such lines (heuristic, see module docs).
    fn name(self) -> &'static str {
        match self {
            LineKind::Event => "event",
            LineKind::Head => "section",
            LineKind::Statement => "effect",
        }
    }
}

/// One non-skipped line, ready for matching.
struct AnalyzedLine {
    /// 0-based char column of the first non-whitespace character.
    col_start: usize,
    /// Char column just past the last content character (after stripping
    /// any trailing inline comment and trailing whitespace; a trailing
    /// `:` is included in the span).
    col_end: usize,
    /// Comment-stripped, `:`-stripped, trimmed text used for matching.
    stripped: String,
    kind: LineKind,
}

/// Analyze one raw script line. Returns `None` for lines to skip:
/// blank/whitespace-only lines and lines whose first non-space character
/// is `#`.
fn analyze_line(raw: &str) -> Option<AnalyzedLine> {
    let (first_char_index, first_char) = raw.char_indices().find(|(_, c)| !c.is_whitespace())?;
    if first_char == '#' {
        return None;
    }
    let code = strip_inline_comment(raw);
    let trimmed = code.trim();
    if trimmed.is_empty() {
        // Defensive: unreachable because the first non-whitespace char is
        // neither '#' nor whitespace and comment stripping only cuts a
        // suffix after it.
        return None;
    }
    // Character (not byte) columns into the ORIGINAL line.
    let col_start = raw[..first_char_index].chars().count();
    let col_end = col_start + trimmed.chars().count();
    let (stripped, kind) = if starts_with_on(trimmed) && trimmed.ends_with(':') {
        (strip_trailing_colon(trimmed), LineKind::Event)
    } else if trimmed.ends_with(':') {
        (strip_trailing_colon(trimmed), LineKind::Head)
    } else {
        (trimmed, LineKind::Statement)
    };
    Some(AnalyzedLine {
        col_start,
        col_end,
        stripped: stripped.to_string(),
        kind,
    })
}

/// Cut the line at the first `#` that is not inside double quotes.
///
/// Quote handling is a simple toggle on every `"` — which is exactly the
/// rule Skript itself uses (a literal quote inside a string is doubled,
/// `""`, so the toggle stays balanced). Backslash escapes are NOT treated
/// specially.
fn strip_inline_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Case-insensitive `on ` prefix (the ASCII keyword; Skript's own keywords
/// are ASCII).
fn starts_with_on(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('o' | 'O'))
        && matches!(chars.next(), Some('n' | 'N'))
        && matches!(chars.next(), Some(' '))
}

/// `"...foo :"` -> `"...foo"` (input must end with `:` and be trimmed).
fn strip_trailing_colon(trimmed: &str) -> &str {
    trimmed[..trimmed.len() - 1].trim_end()
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

/// One entry (and which of its patterns) matched a line.
#[derive(Debug, Clone, Copy)]
struct MatchedEntry {
    entry_index: usize,
    /// Carried for future per-pattern diagnostics (e.g. "matched via
    /// pattern #2"); the v1 rules only need the entry.
    #[allow(dead_code)]
    pattern_index: usize,
}

/// Try every compiled pattern of entries whose syntax type is in
/// `candidates` against `stripped`. One matching pattern per entry is
/// enough for the rules, so only the first hit per entry is recorded.
///
/// This is the only part of the crate that touches the pattern engine;
/// with an un-`compile_all()`-ed registry it simply returns no matches.
fn collect_matches(
    stripped: &str,
    candidates: &[SyntaxType],
    registry: &Registry,
) -> Vec<MatchedEntry> {
    let mut out = Vec::new();
    for ty in candidates {
        for &entry_index in registry.indices_for_type(ty) {
            for compiled in registry.compiled_for_entry(entry_index) {
                // Catch-all patterns (no keyword prefilter — no required
                // literal text, e.g. `<.+>`, `%booleans%`, `[on] %javatype%`)
                // match nearly any input and carry no discriminative value
                // for line-level diagnostics. Skipping them is what makes
                // the `unparseable` rule reachable on the full corpus; the
                // same insight exists in skript-core as `is_bare_type`.
                if compiled.pattern.is_catch_all() {
                    continue;
                }
                if compiled
                    .pattern
                    .match_(stripped, ParseFlags::ALL, ParseContext::Default)
                    .is_some()
                {
                    out.push(MatchedEntry {
                        entry_index,
                        pattern_index: compiled.pattern_index,
                    });
                    break;
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

/// Apply the four diagnostic rules to one line's match list.
/// Priority: unparseable (stop) > addon-not-enabled > plugin-missing >
/// ambiguous-addon; at most one diagnostic is produced.
#[allow(clippy::too_many_arguments)]
fn evaluate_matches(
    line: usize,
    col_start: usize,
    col_end: usize,
    stripped: &str,
    kind: LineKind,
    matches: &[MatchedEntry],
    registry: &Registry,
    config: &CheckConfig,
    addon_count: usize,
) -> Option<LineDiagnostic> {
    // ---- Rule 1: `unparseable` (Error) — no syntax type fits at all. ----
    if matches.is_empty() {
        let hints = closest_hints(stripped, kind.candidates(), registry, HINT_LIMIT);
        let message = if hints.is_empty() {
            format!(
                "unparseable {} (no matching syntax in {} addons): {}",
                kind.name(),
                addon_count,
                quote(stripped)
            )
        } else {
            let quoted: Vec<String> = hints.iter().map(|t| quote(t)).collect();
            format!(
                "no syntax matches {} — did you mean: {}?",
                quote(stripped),
                quoted.join(", ")
            )
        };
        return Some(LineDiagnostic {
            line,
            col_start,
            col_end,
            severity: Severity::Error,
            code: "unparseable",
            message,
        });
    }

    // ---- Rule 2: `addon-not-enabled` (Warning) — every match comes from
    // an addon outside the user's enabled set (only checked when the set
    // is non-empty; an empty set means "all addons enabled"). ----
    if !config.enabled_addons.is_empty()
        && matches
            .iter()
            .all(|m| !addon_enabled(&registry.entries()[m.entry_index].addon.name, config))
    {
        let addons = matched_addon_names(matches, registry);
        return Some(LineDiagnostic {
            line,
            col_start,
            col_end,
            severity: Severity::Warning,
            code: "addon-not-enabled",
            message: format!(
                "only matches addons that are not enabled: {}",
                join_limited(&addons, LIST_LIMIT)
            ),
        });
    }

    // ---- Rule 3: `plugin-missing` (Warning) — some matched entry
    // requires plugins that are not in `installed_plugins`. ----
    let mut missing: Vec<(String, Vec<String>)> = Vec::new();
    for entry_index in dedup_entry_indices(matches) {
        let entry = &registry.entries()[entry_index];
        let mut missing_plugins: Vec<String> = entry
            .required_plugins
            .iter()
            .filter(|p| !plugin_installed(&p.name, config))
            .map(|p| p.name.clone())
            .collect();
        missing_plugins.dedup();
        if !missing_plugins.is_empty() {
            missing.push((entry.title.clone(), missing_plugins));
        }
    }
    if !missing.is_empty() {
        let parts: Vec<String> = missing
            .iter()
            .take(LIST_LIMIT)
            .map(|(title, plugins)| {
                format!(
                    "syntax {} requires plugin(s): {}",
                    quote(title),
                    plugins.join(", ")
                )
            })
            .collect();
        let mut message = parts.join("; ");
        if missing.len() > LIST_LIMIT {
            message.push_str(&format!(" (and {} more)", missing.len() - LIST_LIMIT));
        }
        return Some(LineDiagnostic {
            line,
            col_start,
            col_end,
            severity: Severity::Warning,
            code: "plugin-missing",
            message,
        });
    }

    // ---- Rule 4: `version-mismatch` (Warning) — a matched entry's
    // compatible_addon_version is higher than the configured skript_version.
    // Only checked when skript_version is set (e.g. "2.6"). The version
    // fields in SkriptHub are free-form strings like "2.0" or
    // "1.0 (basic), 2.0 (extended)" — we extract the first numeric version
    // and compare. ----
    if let Some(ref target) = config.skript_version {
        if let Some(target_ver) = parse_version(target) {
            let mut incompatible: Vec<(String, String)> = Vec::new();
            for entry_index in dedup_entry_indices(matches) {
                let entry = &registry.entries()[entry_index];
                let compat = &entry.compatible_addon_version;
                if compat.is_empty() {
                    continue;
                }
                if let Some(entry_ver) = parse_version(compat) {
                    if entry_ver > target_ver {
                        incompatible.push((entry.title.clone(), compat.clone()));
                    }
                }
            }
            if !incompatible.is_empty() {
                let parts: Vec<String> = incompatible
                    .iter()
                    .take(LIST_LIMIT)
                    .map(|(title, ver)| {
                        format!(
                            "syntax {} requires addon version {} (> {})",
                            quote(title),
                            ver,
                            target
                        )
                    })
                    .collect();
                let mut message = parts.join("; ");
                if incompatible.len() > LIST_LIMIT {
                    message.push_str(&format!(" (and {} more)", incompatible.len() - LIST_LIMIT));
                }
                return Some(LineDiagnostic {
                    line,
                    col_start,
                    col_end,
                    severity: Severity::Warning,
                    code: "version-mismatch",
                    message,
                });
            }
        }
    }

    // ---- Rule 5: `ambiguous-addon` (Info) — matches span >= 2 different
    // addons (case-insensitive names). ----
    let addons = matched_addon_names(matches, registry);
    if addons.len() >= 2 {
        return Some(LineDiagnostic {
            line,
            col_start,
            col_end,
            severity: Severity::Info,
            code: "ambiguous-addon",
            message: format!(
                "matches entries from multiple addons: {}",
                join_limited(&addons, LIST_LIMIT)
            ),
        });
    }

    None
}

/// The 3 "closest" entry titles for an unparseable line: entries (of the
/// candidate types) whose FIRST pattern shares the longest common
/// word-prefix with the line. Words are lowercased and split on
/// non-alphanumerics. Ties: more pattern words present anywhere in the
/// line wins, then lower registry index. Zero-prefix entries are never
/// suggested; titles are deduplicated.
fn closest_hints(
    stripped: &str,
    candidates: &[SyntaxType],
    registry: &Registry,
    limit: usize,
) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let line_words = lower_words(stripped);
    if line_words.is_empty() {
        return Vec::new();
    }
    let line_set: HashSet<&str> = line_words.iter().map(String::as_str).collect();
    // (prefix_len, overlap, entry_index, title)
    let mut scored: Vec<(usize, usize, usize, String)> = Vec::new();
    for ty in candidates {
        for &entry_index in registry.indices_for_type(ty) {
            let entry = &registry.entries()[entry_index];
            let Some(first_pattern) = entry.split_patterns().into_iter().next() else {
                continue;
            };
            let pattern_words = lower_words(&first_pattern);
            let prefix = line_words
                .iter()
                .zip(pattern_words.iter())
                .take_while(|(a, b)| a == b)
                .count();
            if prefix == 0 {
                continue;
            }
            let overlap = pattern_words
                .iter()
                .filter(|w| line_set.contains(w.as_str()))
                .count();
            scored.push((prefix, overlap, entry_index, entry.title.clone()));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    let mut out: Vec<String> = Vec::new();
    for (_, _, _, title) in scored {
        if out.len() >= limit {
            break;
        }
        if !out.contains(&title) {
            out.push(title);
        }
    }
    out
}

/// Lowercased words, split on non-alphanumerics (`"loop %objects%"` ->
/// `["loop", "objects"]`).
fn lower_words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

/// Distinct (case-insensitive) addon names in the registry — 174 for the
/// embedded corpus. Used in the fallback `unparseable` message.
fn distinct_addon_count(registry: &Registry) -> usize {
    let mut seen: HashSet<String> = HashSet::new();
    for entry in registry.entries() {
        seen.insert(entry.addon.name.to_lowercase());
    }
    seen.len()
}

/// Matched addons in first-seen order, deduplicated case-insensitively
/// (displaying the first-seen spelling).
fn matched_addon_names(matches: &[MatchedEntry], registry: &Registry) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for m in matches {
        let name = &registry.entries()[m.entry_index].addon.name;
        if seen.insert(name.to_lowercase()) {
            out.push(name.clone());
        }
    }
    out
}

/// Matched entry indices, first-seen order, deduplicated.
fn dedup_entry_indices(matches: &[MatchedEntry]) -> Vec<usize> {
    let mut seen: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for m in matches {
        if seen.insert(m.entry_index) {
            out.push(m.entry_index);
        }
    }
    out
}

/// Addon enabled? Case-insensitive membership in `enabled_addons`.
fn addon_enabled(addon: &str, config: &CheckConfig) -> bool {
    config
        .enabled_addons
        .iter()
        .any(|a| a.eq_ignore_ascii_case(addon))
}

/// Plugin installed? Case-insensitive membership in `installed_plugins`.
fn plugin_installed(plugin: &str, config: &CheckConfig) -> bool {
    config
        .installed_plugins
        .iter()
        .any(|p| p.eq_ignore_ascii_case(plugin))
}

/// Parse the first version number from a free-form SkriptHub version string.
///
/// SkriptHub's `compatible_addon_version` is a free-form string like
/// `"2.0"`, `"1.0 (basic), 2.0 (extended)"`, or
/// `"1.0 (break), unknown (mine), 2.6 (BlockData support)"`. We extract
/// the first `major.minor.patch` number we can find. Returns `None` if
/// no version number is found.
///
/// Comparison: `(major, minor, patch)` tuples — `2.6 > 2.5 > 2.0 > 1.0`.
fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    // Find the first run of digits, then try to parse major.minor.patch.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && !bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    // Parse major
    let mut major = 0u32;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        major = major * 10 + (bytes[i] - b'0') as u32;
        i += 1;
    }
    let mut minor = 0u32;
    let mut patch = 0u32;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            minor = minor * 10 + (bytes[i] - b'0') as u32;
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                patch = patch * 10 + (bytes[i] - b'0') as u32;
                i += 1;
            }
        }
    }
    Some((major, minor, patch))
}

/// `"A, B, C (and 2 more)"` — first `limit` items plus an overflow count.
fn join_limited(items: &[String], limit: usize) -> String {
    if items.len() <= limit {
        items.join(", ")
    } else {
        format!(
            "{} (and {} more)",
            items[..limit].join(", "),
            items.len() - limit
        )
    }
}

/// Wrap in double quotes (no escaping — Skript lines are shown verbatim).
fn quote(s: &str) -> String {
    format!("\"{s}\"")
}

/// Private per-line pipeline (line 0, columns spanning the stripped text).
/// Kept as a separate helper so tests can drive the full rule chain
/// without going through `check_document`; with an un-compiled registry
/// (no `compile_all`) it deterministically takes the `unparseable` path.
#[allow(dead_code)] // test surface (see `unparseable_messages`)
fn check_line_inner(
    stripped: &str,
    kind: LineKind,
    registry: &Registry,
    config: &CheckConfig,
) -> Option<LineDiagnostic> {
    let addon_count = distinct_addon_count(registry);
    let matches = collect_matches(stripped, kind.candidates(), registry);
    evaluate_matches(
        0,
        0,
        stripped.chars().count(),
        stripped,
        kind,
        &matches,
        registry,
        config,
        addon_count,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::rc::Rc;

    use skript_patterns::{set_type_resolver, PermissiveResolver};

    /// Tiny inline registry (un-compiled — `compile_all` needs the pattern
    /// compiler from Agent 5, so un-gated tests never call it).
    ///
    /// Entry indices:
    /// 0 Detonate (effect, Skript)          `detonate %entities%`
    /// 1 Play Sound (effect, SkBee)         `play sound %strings% [to %players%]`
    /// 2 Kill (effect, Skript)              `kill %livingentities%`
    /// 3 Kill Entities (effect, SkBee)      `kill %entities%`
    /// 4 On Join (event, Skript)            `[on] [player] (login|logging in|join[ing])`
    /// 5 Is Op (condition, Skript)          `[the] player is op`
    /// 6 Despawn Citizen (effect, skript-citizens, requires Citizens)
    /// 7 Loop (section, Skript)             `loop %objects%`
    /// 8 Spawn NPC (effect, skript-citizens, requires Citizens + Vault)
    fn mini_registry() -> Registry {
        const JSON: &str = r#"[
            {"id": 1, "json_id": "EffDetonate", "title": "Detonate",
             "syntax_type": "effect", "syntax_pattern": "detonate %entities%",
             "addon": {"name": "Skript"}},
            {"id": 2, "json_id": "EffPlaySound", "title": "Play Sound",
             "syntax_type": "effect", "syntax_pattern": "play sound %strings% [to %players%]",
             "addon": {"name": "SkBee"}},
            {"id": 3, "json_id": "EffKill", "title": "Kill",
             "syntax_type": "effect", "syntax_pattern": "kill %livingentities%",
             "addon": {"name": "Skript"}},
            {"id": 4, "json_id": "EffKillEntities", "title": "Kill Entities",
             "syntax_type": "effect", "syntax_pattern": "kill %entities%",
             "addon": {"name": "SkBee"}},
            {"id": 5, "json_id": "EvtJoin", "title": "On Join",
             "syntax_type": "event", "syntax_pattern": "[on] [player] (login|logging in|join[ing])",
             "addon": {"name": "Skript"}},
            {"id": 6, "json_id": "CondIsOp", "title": "Is Op",
             "syntax_type": "condition", "syntax_pattern": "[the] player is op",
             "addon": {"name": "Skript"}},
            {"id": 7, "json_id": "EffDespawnCitizen", "title": "Despawn Citizen",
             "syntax_type": "effect", "syntax_pattern": "despawn citizen %entities%",
             "addon": {"name": "skript-citizens"},
             "required_plugins": [{"name": "Citizens"}]},
            {"id": 8, "json_id": "SecLoop", "title": "Loop",
             "syntax_type": "section", "syntax_pattern": "loop %objects%",
             "addon": {"name": "Skript"}},
            {"id": 9, "json_id": "EffSpawnNpc", "title": "Spawn NPC",
             "syntax_type": "effect", "syntax_pattern": "spawn npc %string%",
             "addon": {"name": "skript-citizens"},
             "required_plugins": [{"name": "Citizens"}, {"name": "Vault"}]}
        ]"#;
        Registry::from_json_str(JSON).expect("mini registry must parse")
    }

    fn addons(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// Drive `evaluate_matches` directly with a synthetic match list
    /// (entry indices into the mini registry), bypassing the compiler.
    fn eval(
        registry: &Registry,
        config: &CheckConfig,
        kind: LineKind,
        stripped: &str,
        matched_entries: &[usize],
    ) -> Option<LineDiagnostic> {
        let matches: Vec<MatchedEntry> = matched_entries
            .iter()
            .map(|&entry_index| MatchedEntry {
                entry_index,
                pattern_index: 0,
            })
            .collect();
        evaluate_matches(7, 2, 30, stripped, kind, &matches, registry, config, 3)
    }

    // ------------------------------------------------------------------
    // Un-gated: classification, comment stripping, columns
    // ------------------------------------------------------------------

    #[test]
    fn line_classification_and_comment_stripping() {
        // Skipped lines.
        assert!(analyze_line("").is_none());
        assert!(analyze_line("   ").is_none());
        assert!(analyze_line("\t\t").is_none());
        assert!(analyze_line("# comment").is_none());
        assert!(analyze_line("    # indented comment").is_none());
        assert!(analyze_line("\t# tab-indented comment").is_none());

        // Events: "on " prefix (case-insensitive) + trailing ':'.
        let a = analyze_line("on join:").unwrap();
        assert_eq!(a.kind, LineKind::Event);
        assert_eq!(a.stripped, "on join");
        assert_eq!((a.col_start, a.col_end), (0, 8));

        let a = analyze_line("ON JOIN:").unwrap();
        assert_eq!(a.kind, LineKind::Event);
        assert_eq!(a.stripped, "ON JOIN");

        // "once" is NOT an event (third char is not a space).
        let a = analyze_line("once only:").unwrap();
        assert_eq!(a.kind, LineKind::Head);
        assert_eq!(a.stripped, "once only");

        // Other ':'-terminated lines are section heads.
        let a = analyze_line("if player is op:").unwrap();
        assert_eq!(a.kind, LineKind::Head);
        assert_eq!(a.stripped, "if player is op");

        let a = analyze_line("loop all players:").unwrap();
        assert_eq!(a.kind, LineKind::Head);
        assert_eq!(a.stripped, "loop all players");

        // ':' separated from the head by a space.
        let a = analyze_line("on join :").unwrap();
        assert_eq!(a.kind, LineKind::Event);
        assert_eq!(a.stripped, "on join");

        // Plain statements.
        let a = analyze_line("broadcast \"hello\"").unwrap();
        assert_eq!(a.kind, LineKind::Statement);
        assert_eq!(a.stripped, "broadcast \"hello\"");
        assert_eq!(a.col_end, "broadcast \"hello\"".chars().count());

        let a = analyze_line("set {_x} to 5").unwrap();
        assert_eq!(a.kind, LineKind::Statement);
        assert_eq!(a.stripped, "set {_x} to 5");

        // Trailing inline comments are stripped before matching; the span
        // stops before the comment.
        let a = analyze_line("broadcast \"hello\" # greeting").unwrap();
        assert_eq!(a.stripped, "broadcast \"hello\"");
        assert_eq!((a.col_start, a.col_end), (0, 17));

        // '#' inside quotes does NOT start a comment.
        let a = analyze_line("broadcast \"has # inside\" # real comment").unwrap();
        assert_eq!(a.stripped, "broadcast \"has # inside\"");
        assert_eq!(a.col_end, "broadcast \"has # inside\"".chars().count());

        let a = analyze_line("send \"x\"# tight").unwrap();
        assert_eq!(a.stripped, "send \"x\"");
        assert_eq!(a.col_end, 8);

        let a = analyze_line("on join: # fired on join").unwrap();
        assert_eq!(a.kind, LineKind::Event);
        assert_eq!(a.stripped, "on join");
        assert_eq!((a.col_start, a.col_end), (0, 8));

        // Columns: indentation is counted in characters, not bytes.
        let a = analyze_line("    broadcast \"hi\"").unwrap();
        assert_eq!((a.col_start, a.col_end), (4, 18));

        // Multibyte characters: col_start skips the tab, col_end counts
        // characters (the 'π' is 2 bytes but 1 char).
        let a = analyze_line("\tπ broadcast \"x\"").unwrap();
        assert_eq!(a.col_start, 1);
        assert_eq!(a.col_end, 1 + "π broadcast \"x\"".chars().count());
        assert!(a.col_end < "\tπ broadcast \"x\"".len() + 1); // byte len is bigger
    }

    #[test]
    fn strip_inline_comment_is_quote_aware() {
        assert_eq!(strip_inline_comment("no comment"), "no comment");
        assert_eq!(strip_inline_comment("# lead"), "");
        assert_eq!(
            strip_inline_comment("broadcast \"a # b\" # c"),
            "broadcast \"a # b\" "
        );
        // Unterminated string: the '#' stays inside the quote.
        assert_eq!(
            strip_inline_comment("\"unterminated # still string"),
            "\"unterminated # still string"
        );
        // Doubled quotes ("" = literal quote in Skript) keep the scanner
        // balanced.
        assert_eq!(
            strip_inline_comment("say \"\"hi\"\" # done"),
            "say \"\"hi\"\" "
        );
    }

    // ------------------------------------------------------------------
    // Un-gated: closest-hint scoring (pure string logic)
    // ------------------------------------------------------------------

    #[test]
    fn closest_hints_prefers_longest_word_prefix() {
        let reg = mini_registry();
        let eff = [SyntaxType::Effect];

        assert_eq!(
            closest_hints("detonate all creepers", &eff, &reg, 3),
            ["Detonate"]
        );
        assert_eq!(
            closest_hints("play sounds to everyone", &eff, &reg, 3),
            ["Play Sound"]
        );
        // "despawn citizen" shares TWO leading words with Despawn Citizen.
        assert_eq!(
            closest_hints("despawn citizen", &eff, &reg, 3),
            ["Despawn Citizen"]
        );

        // Two Kill entries tie on prefix length; stable registry order.
        assert_eq!(
            closest_hints("kill everything", &eff, &reg, 3),
            ["Kill", "Kill Entities"]
        );

        // The prefix must start at the FIRST word.
        assert!(closest_hints("please detonate stuff", &eff, &reg, 3).is_empty());
        // No shared first word at all.
        assert!(closest_hints("xyzzy", &eff, &reg, 3).is_empty());

        // Events: "on join" shares "on" with the join event's first pattern.
        let evt = [SyntaxType::Event];
        assert_eq!(closest_hints("on join", &evt, &reg, 3), ["On Join"]);

        // Heads: the section candidate set includes Loop.
        let head = [
            SyntaxType::Section,
            SyntaxType::Structure,
            SyntaxType::Condition,
        ];
        assert_eq!(closest_hints("loop all players", &head, &reg, 3), ["Loop"]);
    }

    #[test]
    fn join_limited_appends_and_more() {
        let items: Vec<String> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(join_limited(&items, 3), "a, b, c (and 2 more)");
        assert_eq!(join_limited(&items, 5), "a, b, c, d, e");
        let two: Vec<String> = ["x", "y"].iter().map(|s| s.to_string()).collect();
        assert_eq!(join_limited(&two, 3), "x, y");
    }

    // ------------------------------------------------------------------
    // Un-gated: the rule chain via check_line_inner / eval. The mini
    // registry is NOT compiled, so check_line_inner deterministically
    // takes the unparseable path; the other rules are driven through
    // `eval` with synthetic match lists.
    // ------------------------------------------------------------------

    #[test]
    fn unparseable_messages() {
        let reg = mini_registry();
        let cfg = CheckConfig::default();

        // With a hint (uncompiled registry -> no matches -> unparseable).
        let d = check_line_inner("detonate all creepers", LineKind::Statement, &reg, &cfg).unwrap();
        assert_eq!(d.code, "unparseable");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(
            d.message,
            "no syntax matches \"detonate all creepers\" — did you mean: \"Detonate\"?"
        );
        // Two-word prefix hint.
        let d = check_line_inner("despawn citizen", LineKind::Statement, &reg, &cfg).unwrap();
        assert_eq!(
            d.message,
            "no syntax matches \"despawn citizen\" — did you mean: \"Despawn Citizen\"?"
        );

        // Fallback message when nothing shares a leading word.
        let d = check_line_inner("xyzzy", LineKind::Statement, &reg, &cfg).unwrap();
        assert_eq!(
            d.message,
            "unparseable effect (no matching syntax in 3 addons): \"xyzzy\""
        );
        // Kind names: event / section / effect.
        let d = check_line_inner("xyzzy", LineKind::Event, &reg, &cfg).unwrap();
        assert_eq!(
            d.message,
            "unparseable event (no matching syntax in 3 addons): \"xyzzy\""
        );
        let d = check_line_inner("xyzzy", LineKind::Head, &reg, &cfg).unwrap();
        assert_eq!(
            d.message,
            "unparseable section (no matching syntax in 3 addons): \"xyzzy\""
        );
        // The placeholder span covers the whole stripped text.
        assert_eq!((d.col_start, d.col_end), (0, 5));
    }

    #[test]
    fn rule_addon_not_enabled() {
        let reg = mini_registry();

        // SkBee-only match with only Skript enabled.
        let cfg = CheckConfig {
            enabled_addons: addons(&["Skript"]),
            ..Default::default()
        };
        let d = eval(
            &reg,
            &cfg,
            LineKind::Statement,
            "play sound \"note.pling\"",
            &[1],
        )
        .unwrap();
        assert_eq!(d.code, "addon-not-enabled");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.message, "only matches addons that are not enabled: SkBee");

        // Empty enabled set = everything enabled -> no diagnostic.
        let cfg = CheckConfig::default();
        assert!(eval(
            &reg,
            &cfg,
            LineKind::Statement,
            "play sound \"note.pling\"",
            &[1]
        )
        .is_none());

        // Enabled-addon matching is case-insensitive.
        let cfg = CheckConfig {
            enabled_addons: addons(&["SKBEE"]),
            ..Default::default()
        };
        assert!(eval(
            &reg,
            &cfg,
            LineKind::Statement,
            "play sound \"note.pling\"",
            &[1]
        )
        .is_none());

        // A single enabled match is enough to stop the rule; with a second
        // addon present it degrades to ambiguous-addon instead.
        let cfg = CheckConfig {
            enabled_addons: addons(&["Skript"]),
            ..Default::default()
        };
        let d = eval(&reg, &cfg, LineKind::Statement, "kill all players", &[2, 3]).unwrap();
        assert_eq!(d.code, "ambiguous-addon");
    }

    #[test]
    fn rule_plugin_missing() {
        let reg = mini_registry();

        // No plugins installed at all.
        let cfg = CheckConfig::default();
        let d = eval(
            &reg,
            &cfg,
            LineKind::Statement,
            "despawn citizen player",
            &[6],
        )
        .unwrap();
        assert_eq!(d.code, "plugin-missing");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(
            d.message,
            "syntax \"Despawn Citizen\" requires plugin(s): Citizens"
        );

        // Installed check is case-insensitive.
        let cfg = CheckConfig {
            installed_plugins: addons(&["citizens"]),
            ..Default::default()
        };
        assert!(eval(
            &reg,
            &cfg,
            LineKind::Statement,
            "despawn citizen player",
            &[6]
        )
        .is_none());

        // Two entries with missing plugins are joined with "; ", plugin
        // names with ", ".
        let cfg = CheckConfig::default();
        let d = eval(&reg, &cfg, LineKind::Statement, "x", &[6, 8]).unwrap();
        assert_eq!(
            d.message,
            "syntax \"Despawn Citizen\" requires plugin(s): Citizens; \
             syntax \"Spawn NPC\" requires plugin(s): Citizens, Vault"
        );
        // Vault is still missing even with Citizens installed.
        let cfg = CheckConfig {
            installed_plugins: addons(&["Citizens"]),
            ..Default::default()
        };
        let d = eval(&reg, &cfg, LineKind::Statement, "x", &[8]).unwrap();
        assert_eq!(d.message, "syntax \"Spawn NPC\" requires plugin(s): Vault");

        // Priority: plugin-missing beats ambiguous-addon (nothing installed).
        let cfg = CheckConfig::default();
        let d = eval(&reg, &cfg, LineKind::Statement, "x", &[6, 1]).unwrap();
        assert_eq!(d.code, "plugin-missing");
    }

    #[test]
    fn rule_ambiguous_and_priority_order() {
        let reg = mini_registry();
        let cfg = CheckConfig::default();

        // addon-not-enabled beats plugin-missing: the Citizens entry is
        // the ONLY match and its addon is not enabled.
        let cfg2 = CheckConfig {
            enabled_addons: addons(&["Skript"]),
            ..Default::default()
        };
        let d = eval(
            &reg,
            &cfg2,
            LineKind::Statement,
            "despawn citizen player",
            &[6],
        )
        .unwrap();
        assert_eq!(d.code, "addon-not-enabled");

        // Ambiguous: two different addons, everything enabled.
        let d = eval(&reg, &cfg, LineKind::Statement, "kill all players", &[2, 3]).unwrap();
        assert_eq!(d.code, "ambiguous-addon");
        assert_eq!(d.severity, Severity::Info);
        assert_eq!(
            d.message,
            "matches entries from multiple addons: Skript, SkBee"
        );

        // Single addon, no plugins, all enabled -> no diagnostic.
        assert!(eval(&reg, &cfg, LineKind::Statement, "detonate creepers", &[0]).is_none());
        // Two entries from the SAME addon -> not ambiguous.
        assert!(eval(&reg, &cfg, LineKind::Statement, "x", &[0, 2]).is_none());
        // unparseable beats everything (empty match list).
        assert!(eval(&reg, &cfg, LineKind::Statement, "x", &[]).is_some());
    }

    // ------------------------------------------------------------------
    // Un-gated: full pipeline on an UN-compiled registry. compile_all()
    // needs the pattern compiler (Agent 5), so without it nothing matches
    // and every content line is unparseable — which still exercises the
    // skip logic, classification, column computation and both message
    // shapes end to end.
    // ------------------------------------------------------------------

    #[test]
    fn check_document_uncompiled_registry_reports_unparseable() {
        let reg = mini_registry();
        let cfg = CheckConfig::default();
        let text = "# header\non join:\n\n\tbroadcast \"hello\"\nset {_x} to 5 # trailing\n";
        let ds = check_document(text, &reg, &cfg);
        assert_eq!(ds.len(), 3, "{ds:?}");

        // line 1: "on join:" -> event, cols 0..8, hint "On Join".
        assert_eq!(ds[0].line, 1);
        assert_eq!((ds[0].col_start, ds[0].col_end), (0, 8));
        assert_eq!(ds[0].code, "unparseable");
        assert_eq!(ds[0].severity, Severity::Error);
        assert_eq!(
            ds[0].message,
            "no syntax matches \"on join\" — did you mean: \"On Join\"?"
        );

        // line 3: tab-indented statement.
        assert_eq!(ds[1].line, 3);
        assert_eq!(ds[1].col_start, 1);
        assert_eq!(ds[1].col_end, 1 + "broadcast \"hello\"".chars().count());
        assert_eq!(ds[1].code, "unparseable");

        // line 4: comment stripped, fallback message (no hints).
        assert_eq!(ds[2].line, 4);
        assert_eq!(ds[2].code, "unparseable");
        assert_eq!(
            ds[2].message,
            "unparseable effect (no matching syntax in 3 addons): \"set {_x} to 5\""
        );
    }

    // ------------------------------------------------------------------
    // Gated on the pattern compiler (Agent 5) + matcher (Agent 6).
    // Run with: cargo test -p skript-checks --lib -- --ignored
    // ------------------------------------------------------------------

    /// Deterministic full-pipeline test on the mini registry: covers ALL
    /// FOUR rules plus clean lines, skips, and column computation.
    #[test]
    fn full_pipeline_mini_registry() {
        set_type_resolver(Rc::new(PermissiveResolver));
        let mut reg = mini_registry();
        let failures = reg.compile_all();
        assert!(failures.is_empty(), "compile failures: {failures:?}");

        let cfg = CheckConfig {
            enabled_addons: addons(&["Skript", "skript-citizens"]),
            ..Default::default()
        };

        let text = concat!(
            "# mini full-pipeline (gated on the pattern compiler)\n",
            "\n",
            "on join:\n",
            "    loop all players:\n",
            "        detonate all creepers\n",
            "frobnicate the widget\n",
            "play sound \"note.pling\"\n",
            "despawn citizen player\n",
            "kill all players\n",
        );
        let ds = check_document(text, &reg, &cfg);

        // Lines 0/1 skipped, 2 (event), 3 (section head) and 4 (effect)
        // match enabled Skript syntax with no plugin needs -> clean.
        // Lines 5..=8 hit the four rules in order.
        assert_eq!(ds.len(), 4, "{ds:?}");
        assert_eq!((ds[0].line, ds[0].code), (5, "unparseable"));
        assert_eq!(
            ds[0].message,
            "unparseable effect (no matching syntax in 3 addons): \"frobnicate the widget\""
        );
        assert_eq!((ds[0].col_start, ds[0].col_end), (0, 21));

        assert_eq!((ds[1].line, ds[1].code), (6, "addon-not-enabled"));
        assert_eq!(ds[1].severity, Severity::Warning);
        assert_eq!(
            ds[1].message,
            "only matches addons that are not enabled: SkBee"
        );

        assert_eq!((ds[2].line, ds[2].code), (7, "plugin-missing"));
        assert_eq!(ds[2].severity, Severity::Warning);
        assert_eq!(
            ds[2].message,
            "syntax \"Despawn Citizen\" requires plugin(s): Citizens"
        );

        assert_eq!((ds[3].line, ds[3].code), (8, "ambiguous-addon"));
        assert_eq!(ds[3].severity, Severity::Info);
        assert_eq!(
            ds[3].message,
            "matches entries from multiple addons: Skript, SkBee"
        );
    }

    /// Full pipeline against the real embedded corpus (8712 entries).
    ///
    /// NOTE: `unparseable` CANNOT fire on the full corpus with the
    /// permissive resolver — a handful of catch-all patterns
    /// (skript-reflect's `[(await)] %javaobject%`, SkJade's `%booleans%`,
    /// Hippo's `%modifiers% %pair%`, skript-mirror's
    /// `[on] [all] %javatype%`) match every line. That rule is covered
    /// deterministically by `full_pipeline_mini_registry` above.
    #[test]
    fn full_pipeline_embedded_corpus() {
        set_type_resolver(Rc::new(PermissiveResolver));
        let mut reg = Registry::embedded().expect("embedded corpus must load");
        let failures = reg.compile_all();
        // 17 genuinely-malformed patterns live in the SkriptHub dump (see
        // registry's `compile_all_embedded_expects_known_failures`); the
        // backslash-restore fallback repairs 15 more. Both counts are
        // pinned here so a corpus update that changes them is noticed.
        assert_eq!(
            failures.len(),
            17,
            "expected 17 known-malformed corpus patterns, got {}: {failures:?}",
            failures.len()
        );
        assert_eq!(reg.repaired().len(), 15);

        let cfg = CheckConfig {
            enabled_addons: addons(&["Skript", "skUtilities"]),
            ..Default::default()
        };

        let text = concat!(
            "# full-pipeline smoke test\n",
            "\n",
            "on join:\n",
            "broadcast \"hello\"\n",
            "destroy event-train\n",
            "restart server\n",
            "if player is op:\n",
        );
        let ds = check_document(text, &reg, &cfg);

        // At most one diagnostic per line, in line order.
        for pair in ds.windows(2) {
            assert!(pair[0].line < pair[1].line, "duplicate line: {ds:?}");
        }
        // Lines 0/1 skipped. Since the catch-all filter landed, `on join:`
        // matches ONLY Skript's event (skript-mirror's/reflect's catch-all
        // event patterns no longer produce spurious ambiguity), so line 2
        // is now clean and lines 3..=6 carry the four rules.
        assert_eq!(ds.len(), 4, "{ds:?}");
        let line_code: Vec<(usize, &str)> = ds.iter().map(|d| (d.line, d.code)).collect();
        assert_eq!(
            line_code,
            vec![
                (3, "ambiguous-addon"),   // broadcast: Skript + skript-reflect + Hippo + ...
                (4, "addon-not-enabled"), // destroy event-train: only traincarts/non-catch-alls match
                (5, "plugin-missing"),    // restart server: skUtilities requires Spigot
                (6, "ambiguous-addon"), // if player is op: Skript Is Op + Conquer (catch-alls filtered)
            ]
        );
        assert!(
            ds[1].message.contains("skript-traincarts"),
            "{}",
            ds[1].message
        );
        assert!(
            ds[2].message.contains("Restart Server"),
            "{}",
            ds[2].message
        );
    }

    // --- version parsing -----------------------------------------------

    #[test]
    fn parse_version_simple() {
        assert_eq!(parse_version("2.0"), Some((2, 0, 0)));
        assert_eq!(parse_version("2.6"), Some((2, 6, 0)));
        assert_eq!(parse_version("1.20.4"), Some((1, 20, 4)));
    }

    #[test]
    fn parse_version_free_form() {
        // SkriptHub format: "1.0 (basic), 2.0 (extended)" → first version
        assert_eq!(
            parse_version("1.0 (basic), 2.0 (extended)"),
            Some((1, 0, 0))
        );
        assert_eq!(
            parse_version("1.0 (break), unknown (mine), 2.6 (BlockData support)"),
            Some((1, 0, 0))
        );
    }

    #[test]
    fn parse_version_no_digits() {
        assert_eq!(parse_version("unknown"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn parse_version_comparison() {
        let v2_0 = parse_version("2.0").unwrap();
        let v2_6 = parse_version("2.6").unwrap();
        let v1_0 = parse_version("1.0").unwrap();
        assert!(v2_6 > v2_0);
        assert!(v2_0 > v1_0);
    }
}
