//! Integration tests for the diagnostic-merge wiring.
//!
//! These exercise the integration seam between the tower-lsp server and the
//! faithful Java-port pattern engine:
//!
//! * config plumbing — the `enabledAddons` / `installedPlugins`
//!   initialization options parse into `InitOptions` and are carried by
//!   `EffectiveConfig`;
//! * the `LineDiagnostic` → LSP `Diagnostic` mapping used by
//!   `publish_merged_diagnostics`;
//! * `AppState::ensure_java_engine` — building the embedded corpus and
//!   compiling every pattern on the dedicated engine thread;
//! * an end-to-end diagnostics check through `AppState::java_check`.
//!
//! The engine-dependent tests are `#[ignore]`-gated until the parallel
//! porting agents land their implementations (the signatures are frozen,
//! the bodies are not yet filled in):
//!
//! * Agent 5 — `skript_patterns::compile_pattern`
//! * Agent 9 — `skript_checks::check_document`
//!
//! Run them once they land with `cargo test -p skript-lsp --test
//! java_engine_integration -- --ignored`.

use skript_lsp::options::{EffectiveConfig, InitOptions};
use skript_lsp::state::AppState;
use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString, Position};

// --- config plumbing --------------------------------------------------------

/// The initialization options parse (camelCase) into the checker lists.
#[test]
fn init_options_expose_checker_lists() {
    let opts: InitOptions = serde_json::from_value(serde_json::json!({
        "enabledAddons": ["Skript", "SkBee"],
        "installedPlugins": ["Vault"],
        "maxCompletions": 250
    }))
    .expect("init options must parse");

    assert_eq!(opts.max_completions, Some(250));
    assert_eq!(
        opts.enabled_addons,
        Some(vec!["Skript".to_owned(), "SkBee".to_owned()])
    );
    assert_eq!(opts.installed_plugins, Some(vec!["Vault".to_owned()]));
}

/// `EffectiveConfig` defaults to an all-addons-enabled checker; the fields
/// are plain data that `apply_init_options` (backend) copies over.
#[test]
fn effective_config_carries_checker_fields() {
    let cfg = EffectiveConfig::default();
    assert!(cfg.enabled_addons.is_empty(), "empty = all addons enabled");
    assert!(cfg.installed_plugins.is_empty());
}

/// A fresh state has no java engine; stats read `None` until it is built.
/// (Deliberately does NOT start the engine: with the pattern-compiler stubs
/// still `todo!()`, starting would panic-build on the engine thread, which
/// the server contains and reports as `Broken`.)
#[test]
fn java_engine_idle_on_fresh_state() {
    let state = AppState::new(EffectiveConfig::default());
    assert_eq!(state.java_engine_stats(), None);
}

// --- diagnostics mapping ----------------------------------------------------

/// `LineDiagnostic` → LSP `Diagnostic` mapping used by the merged
/// diagnostics path.
#[test]
fn line_diagnostic_mapping_matches_contract() {
    let d = skript_checks::LineDiagnostic {
        line: 3,
        col_start: 4,
        col_end: 12,
        severity: skript_checks::Severity::Warning,
        code: "addon-not-enabled",
        message: "this line requires the SkBee addon".to_owned(),
    };
    let lsp = skript_lsp::backend::line_diagnostic_to_lsp(d);

    assert_eq!(lsp.range.start, Position::new(3, 4));
    assert_eq!(lsp.range.end, Position::new(3, 12));
    assert_eq!(lsp.severity, Some(DiagnosticSeverity::WARNING));
    match lsp.code.expect("code must be set") {
        NumberOrString::String(s) => assert_eq!(s, "addon-not-enabled"),
        other => panic!("expected a string code, got {other:?}"),
    }
    assert_eq!(lsp.message, "this line requires the SkBee addon");

    // Severity triad: Error→1, Warning→2, Info→3.
    let sev = |s| {
        skript_lsp::backend::line_diagnostic_to_lsp(skript_checks::LineDiagnostic {
            line: 0,
            col_start: 0,
            col_end: 1,
            severity: s,
            code: "x",
            message: String::new(),
        })
        .severity
    };
    assert_eq!(sev(skript_checks::Severity::Error), Some(DiagnosticSeverity::ERROR));
    assert_eq!(sev(skript_checks::Severity::Info), Some(DiagnosticSeverity::INFORMATION));
}

/// `CheckConfig` is built from `EffectiveConfig`; versions stay `None`.
#[test]
fn check_config_built_from_effective_config() {
    let mut cfg = EffectiveConfig::default();
    cfg.enabled_addons = vec!["SkBee".to_owned()];
    cfg.installed_plugins = vec!["Vault".to_owned()];
    let cc = skript_lsp::backend::build_check_config(&cfg);

    assert!(cc.enabled_addons.contains("SkBee"));
    assert!(!cc.enabled_addons.contains("Skript"));
    assert!(cc.installed_plugins.contains("Vault"));
    assert!(cc.skript_version.is_none());
    assert!(cc.minecraft_version.is_none());

    // Empty config = all addons enabled (empty set, per skript-checks).
    let empty = skript_lsp::backend::build_check_config(&EffectiveConfig::default());
    assert!(empty.enabled_addons.is_empty());
    assert!(empty.installed_plugins.is_empty());
}

// --- engine build -----------------------------------------------------------

/// The engine builds the embedded corpus on its dedicated thread and
/// compiles every split pattern exactly once.
///
/// (This was temporarily `#[ignore]`-gated while Agent 5's `compile_pattern`
/// was still a `todo!()` stub; the compiler has since landed and the build
/// completes, so the gate was removed.)
#[test]
fn ensure_java_engine_builds_embedded_registry() {
    let state = AppState::new(EffectiveConfig::default());
    assert_eq!(state.java_engine_stats(), None, "engine starts unbuilt");

    let failures = state.ensure_java_engine();
    let stats = state.java_engine_stats().expect("stats after ensure");

    // Corpus facts (PORTING_CONTRACTS.md): 8712 entries, 13661 split
    // patterns. Assert the contract's lower bound plus the compile
    // invariant rather than exact counts where reasonable.
    assert!(stats.entries > 8000, "expected a large corpus, got {}", stats.entries);
    assert_eq!(
        stats.patterns_compiled + stats.compile_failures,
        13661,
        "every split pattern is either compiled or recorded as a failure"
    );
    assert_eq!(stats.compile_failures, failures);

    // Idempotent: a second ensure neither rebuilds nor disagrees.
    assert_eq!(state.ensure_java_engine(), failures);
    assert_eq!(state.java_engine_stats(), Some(stats));
}

// --- end-to-end diagnostics (gated on Agent 9: check_document) --------------

/// A script with one valid effect line and one garbage line yields exactly
/// one `unparseable` Error diagnostic — on the garbage line (0-based line
/// 1) — which maps onto an LSP ERROR.
///
/// STATUS (observed 2026-11 with the first landed `check_document`): the
/// garbage line currently matches a few very permissive multi-addon
/// patterns (skript-reflect / Hippo / SkriptWebAPI), so the checker emits
/// `ambiguous-addon` Info diagnostics instead of `unparseable`. The
/// assertion below encodes the *intended* contract; un-ignore once the
/// checker's rules (or the resolver strategy) make garbage lines fail to
/// match. See the `unparseable` rule in skript-checks' crate docs.
#[test]
fn java_port_flags_unparseable_line() {
    let state = AppState::new(EffectiveConfig::default());

    // Build the engine (blocks; the !Send registry lives on the engine
    // thread, so the check itself must go through the request channel).
    state.ensure_java_engine();
    assert!(state.java_engine_stats().is_some(), "engine must be ready");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    let script = "broadcast \"hi\"\nfrobnicate the widgets\n".to_owned();
    let diags = rt
        .block_on(state.java_check(script, skript_checks::CheckConfig::default()))
        .expect("the java engine must answer");

    // Exactly one Error in the whole document …
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == skript_checks::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "expected exactly one Error, got {diags:?}");
    // … it is the `unparseable` rule …
    assert_eq!(errors[0].code, "unparseable");
    // … and it sits on the garbage line (0-based line index 1).
    assert_eq!(errors[0].line, 1, "the garbage line is line index 1");

    // The LSP mapping preserves severity and line.
    let lsp = skript_lsp::backend::line_diagnostic_to_lsp(errors[0].clone());
    assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(lsp.range.start.line, 1);
    assert_eq!(lsp.range.end.line, 1);
    assert!(lsp.range.end.character > lsp.range.start.character);
}
