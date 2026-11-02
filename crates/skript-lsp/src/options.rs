//! CLI options and LSP initialization options.
//!
//! The server reads configuration from three sources, in order of precedence
//! (highest first):
//!   1. Command-line flags (set by the VSCode extension or any launcher)
//!   2. `initializationOptions` sent by the client in the LSP `initialize` request
//!   3. Built-in defaults
//!
//! All stdout discipline lives elsewhere; this module only describes values.

use clap::Parser;
use serde::Deserialize;

/// Command-line flags accepted by the `skript-lsp` binary.
///
/// Spawned with **zero args**, the server binds an ephemeral port on the
/// loopback interface and prints `LISTENING <port>` to stdout.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "skript-lsp",
    about = "Language Server Protocol implementation for the Skript scripting language",
    version
)]
pub struct CliOptions {
    /// Use stdio transport (standard LSP). When set, --port is ignored.
    #[arg(long)]
    pub r#stdio: bool,

    /// TCP port to bind on 127.0.0.1. `0` (the default) asks the OS for an
    /// ephemeral port; the chosen port is printed to stdout.
    #[arg(long, default_value_t = 0)]
    pub port: u16,

    /// Tracing log level written to **stderr**. Never affects stdout.
    #[arg(long, default_value = "info")]
    pub log_level: LogLevelArg,
}

/// Newtype around `tracing::Level` so clap can parse it.
#[derive(Clone, Copy, Debug)]
pub struct LogLevelArg(pub tracing::Level);

impl clap::ValueEnum for LogLevelArg {
    fn value_variants<'a>() -> &'a [Self] {
        const VARIANTS: &[LogLevelArg] = &[
            LogLevelArg(tracing::Level::TRACE),
            LogLevelArg(tracing::Level::DEBUG),
            LogLevelArg(tracing::Level::INFO),
            LogLevelArg(tracing::Level::WARN),
            LogLevelArg(tracing::Level::ERROR),
        ];
        VARIANTS
    }

    fn to_possible_value<'a>(&self) -> Option<clap::builder::PossibleValue> {
        let name = match self.0 {
            tracing::Level::TRACE => "trace",
            tracing::Level::DEBUG => "debug",
            tracing::Level::INFO => "info",
            tracing::Level::WARN => "warn",
            tracing::Level::ERROR => "error",
        };
        Some(clap::builder::PossibleValue::new(name))
    }
}

impl Default for LogLevelArg {
    fn default() -> Self {
        LogLevelArg(tracing::Level::INFO)
    }
}

/// How the LSP surfaces the AST parser's errors.
///
/// The recursive-descent parser is deliberately partial (it powers AST
/// features like folding/symbols/completion); the corpus line-checker is
/// the authority for "is this valid Skript". Genuinely broken lines are
/// reported `unparseable` by the checker, so parser errors are demoted to
/// hints by default to avoid a wall of false red squiggles on modern
/// constructs the AST parser does not model yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParserDiagnostics {
    Error,
    #[default]
    Hint,
    Off,
}

impl ParserDiagnostics {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "error" => Some(ParserDiagnostics::Error),
            "hint" => Some(ParserDiagnostics::Hint),
            "off" | "none" | "false" => Some(ParserDiagnostics::Off),
            _ => None,
        }
    }
}

/// Schema for `.skriptrc.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SkriptRcToml {
    /// Addon names the project uses, in addition to core Skript.
    pub addons: Vec<String>,
    /// How to surface AST parser errors: "error", "hint" (default) or
    /// "off".
    pub parser_diagnostics: Option<String>,
}

/// `initializationOptions` sent by the client. Every field is optional.
///
/// The diagnostics engine is not configurable — both the AST validator
/// and the pattern-engine line checker always run and their results are
/// merged for maximum Skript parity.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InitOptions {
    pub max_completions: Option<usize>,
    /// Addons the pattern-engine checker treats as enabled (empty/absent = all
    /// addons enabled).
    pub enabled_addons: Option<Vec<String>>,
    /// Plugins the pattern-engine checker treats as installed.
    pub installed_plugins: Option<Vec<String>>,
    /// Skript version the target server runs (e.g. "2.6"). When set,
    /// the checker warns about syntax entries that require a higher
    /// addon version (the `version-mismatch` diagnostic).
    pub skript_version: Option<String>,
    /// Minecraft version the target server runs (e.g. "1.20.4").
    /// Currently informational; reserved for future version-gated syntax.
    pub minecraft_version: Option<String>,
    /// How to surface AST parser errors: "error", "hint" (default) or
    /// "off".
    pub parser_diagnostics: Option<String>,
}

/// Effective runtime configuration after merging CLI + init + defaults.
/// Owned by [`crate::state::AppState`].
#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    /// Cap on the number of completion items returned per request.
    pub max_completions: usize,
    /// Addon names to load syntax for, in addition to always-loaded "Skript".
    /// Empty = only load core Skript entries.
    pub extra_addons: Vec<String>,
    /// Addons the pattern-engine checker treats as enabled. Empty = all addons
    /// are treated as enabled (no `addon-not-enabled` warnings).
    pub enabled_addons: Vec<String>,
    /// Plugins the pattern-engine checker treats as installed.
    pub installed_plugins: Vec<String>,
    /// Skript version the target server runs (e.g. "2.6"). When set,
    /// enables the `version-mismatch` diagnostic.
    pub skript_version: Option<String>,
    /// Minecraft version the target server runs (e.g. "1.20.4").
    pub minecraft_version: Option<String>,
    /// Severity policy for AST parser errors (default: hint).
    pub parser_diagnostics: ParserDiagnostics,
}

impl Default for EffectiveConfig {
    fn default() -> Self {
        EffectiveConfig {
            max_completions: 100,
            extra_addons: Vec::new(),
            enabled_addons: Vec::new(),
            installed_plugins: Vec::new(),
            skript_version: None,
            minecraft_version: None,
            parser_diagnostics: ParserDiagnostics::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skript_rc_empty_addons() {
        let rc: SkriptRcToml = toml::from_str("").unwrap();
        assert!(rc.addons.is_empty());
    }

    #[test]
    fn skript_rc_missing_addons_key() {
        let rc: SkriptRcToml = toml::from_str("other = true").unwrap();
        assert!(rc.addons.is_empty());
    }

    #[test]
    fn skript_rc_with_addons() {
        let rc: SkriptRcToml = toml::from_str(r#"addons = ["SkBee", "SkQuery"]"#).unwrap();
        assert_eq!(rc.addons, vec!["SkBee", "SkQuery"]);
    }

    #[test]
    fn skript_rc_duplicate_addons() {
        let rc: SkriptRcToml = toml::from_str(r#"addons = ["SkBee", "SkBee"]"#).unwrap();
        let mut addons = rc.addons;
        addons.dedup();
        assert_eq!(addons, vec!["SkBee"]);
    }

    #[test]
    fn effective_config_default_extra_addons_empty() {
        let cfg = EffectiveConfig::default();
        assert!(cfg.extra_addons.is_empty());
        assert_eq!(cfg.max_completions, 100);
    }

    // --- checker list options -------------------------------------------

    #[test]
    fn effective_config_defaults() {
        let cfg = EffectiveConfig::default();
        assert!(cfg.enabled_addons.is_empty(), "empty = all addons enabled");
        assert!(cfg.installed_plugins.is_empty());
        assert_eq!(cfg.max_completions, 100);
    }

    #[test]
    fn init_options_parse_checker_lists() {
        let opts: InitOptions = serde_json::from_value(serde_json::json!({
            "enabledAddons": ["Skript", "SkBee"],
            "installedPlugins": ["Vault"],
            "maxCompletions": 250
        }))
        .unwrap();
        assert_eq!(opts.max_completions, Some(250));
        assert_eq!(
            opts.enabled_addons,
            Some(vec!["Skript".to_owned(), "SkBee".to_owned()])
        );
        assert_eq!(opts.installed_plugins, Some(vec!["Vault".to_owned()]));
    }

    #[test]
    fn init_options_fields_are_optional() {
        let opts: InitOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(opts.enabled_addons, None);
        assert_eq!(opts.installed_plugins, None);
        assert_eq!(opts.max_completions, None);
    }

    #[test]
    fn parser_diagnostics_parse() {
        assert_eq!(
            ParserDiagnostics::parse("error"),
            Some(ParserDiagnostics::Error)
        );
        assert_eq!(
            ParserDiagnostics::parse("HINT"),
            Some(ParserDiagnostics::Hint)
        );
        assert_eq!(
            ParserDiagnostics::parse("off"),
            Some(ParserDiagnostics::Off)
        );
        assert_eq!(ParserDiagnostics::default(), ParserDiagnostics::Hint);
        assert_eq!(ParserDiagnostics::parse("nonsense"), None);
    }

    #[test]
    fn skript_rc_parser_diagnostics() {
        let rc: SkriptRcToml = toml::from_str(r#"parser_diagnostics = "error""#).unwrap();
        assert_eq!(rc.parser_diagnostics.as_deref(), Some("error"));
    }
}
