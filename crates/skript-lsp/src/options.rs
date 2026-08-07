//! CLI options and LSP initialization options.
//!
//! The server reads configuration from three sources, in order of precedence
//! (highest first):
//!   1. Command-line flags (set by the Tauri sidecar launcher)
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

/// Schema for `.skriptrc.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SkriptRcToml {
    /// Addon names the project uses, in addition to core Skript.
    pub addons: Vec<String>,
}

/// `initializationOptions` sent by the client. Every field is optional.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InitOptions {
    pub max_completions: Option<usize>,
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
}

impl Default for EffectiveConfig {
    fn default() -> Self {
        EffectiveConfig {
            max_completions: 100,
            extra_addons: Vec::new(),
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
}
