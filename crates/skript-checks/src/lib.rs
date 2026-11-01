//! Static diagnostic rules for Skript scripts.
//!
//! OWNER: Agent 9. Fill in `check_document`.
//!
//! Design: line-based static analysis (v1). For each script line:
//!  1. Skip blank lines and `#` comments.
//!  2. Classify: `on ...:` (event) / `...:` (section or structure) /
//!     otherwise (effect / condition).
//!  3. Try matching against the compiled patterns of the corresponding
//!     syntax types (PermissiveResolver is installed by the caller).
//!  4. Emit diagnostics per the rules below.
//!
//! Rules (code -> severity):
//!  - `unparseable`          Error   — no pattern of any matching syntax type fits
//!  - `addon-not-enabled`    Warning — only fits patterns from addons not in
//!                                     `config.enabled_addons` (when non-empty)
//!  - `plugin-missing`       Warning — matching entry requires plugins not in
//!                                     `config.installed_plugins`
//!  - `ambiguous-addon`      Info    — matches entries from >= 2 different addons
//!
//! Column spans: cover the trimmed line content (UTF-8 char counts, 0-based).

use std::collections::HashSet;

use skript_registry::Registry;

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
/// `PermissiveResolver`) on the calling thread before calling this.
pub fn check_document(text: &str, registry: &Registry, config: &CheckConfig) -> Vec<LineDiagnostic> {
    todo!("Agent 9: implement the rules")
}
