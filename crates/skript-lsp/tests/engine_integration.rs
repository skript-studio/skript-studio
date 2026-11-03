//! Integration tests for the diagnostic-merge wiring.
//!
//! These exercise the integration seam between the tower-lsp server and the
//! faithful pattern engine:
//!
//! * config plumbing — the `enabledAddons` / `installedPlugins`
//!   initialization options parse into `InitOptions` and are carried by
//!   `EffectiveConfig`;
//! * the `LineDiagnostic` → LSP `Diagnostic` mapping used by
//!   `publish_merged_diagnostics`;
//! * `AppState::ensure_engine` — building the embedded corpus and
//!   compiling every pattern on the dedicated engine thread;
//! * an end-to-end diagnostics check through
//!   `AppState::check_document_via_engine`.
//!
//! Run with `cargo test -p skript-lsp --test engine_integration`.

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

/// A fresh state has no engine; stats read `None` until it is built.
/// (Deliberately does NOT start the engine: with the pattern-compiler stubs
/// still `todo!()`, starting would panic-build on the engine thread, which
/// the server contains and reports as `Broken`.)
#[test]
fn engine_idle_on_fresh_state() {
    let state = AppState::new(EffectiveConfig::default());
    assert_eq!(state.engine_stats(), None);
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
    assert_eq!(
        sev(skript_checks::Severity::Error),
        Some(DiagnosticSeverity::ERROR)
    );
    assert_eq!(
        sev(skript_checks::Severity::Info),
        Some(DiagnosticSeverity::INFORMATION)
    );
}

/// `CheckConfig` is built from `EffectiveConfig`; versions stay `None`.
#[test]
fn check_config_built_from_effective_config() {
    let cfg = EffectiveConfig {
        enabled_addons: vec!["SkBee".to_owned()],
        installed_plugins: vec!["Vault".to_owned()],
        ..Default::default()
    };
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
/// (This was temporarily `#[ignore]`-gated while the `compile_pattern`
/// stub was still a `todo!()`; the compiler has since landed and the build
/// completes, so the gate was removed.)
#[test]
fn ensure_engine_builds_embedded_registry() {
    let state = AppState::new(EffectiveConfig::default());
    assert_eq!(state.engine_stats(), None, "engine starts unbuilt");

    let failures = state.ensure_engine();
    let stats = state.engine_stats().expect("stats after ensure");

    // Corpus facts (PORTING_CONTRACTS.md): 8712 entries, 13661 split
    // patterns. Assert the contract's lower bound plus the compile
    // invariant rather than exact counts where reasonable.
    assert!(
        stats.entries > 8000,
        "expected a large corpus, got {}",
        stats.entries
    );
    assert_eq!(
        stats.patterns_compiled + stats.compile_failures,
        13661,
        "every split pattern is either compiled or recorded as a failure"
    );
    assert_eq!(stats.compile_failures, failures);

    // Idempotent: a second ensure neither rebuilds nor disagrees.
    assert_eq!(state.ensure_engine(), failures);
    assert_eq!(state.engine_stats(), Some(stats));
}

// --- end-to-end diagnostics (gated on check_document) -----------------------

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
fn engine_flags_unparseable_line() {
    let state = AppState::new(EffectiveConfig::default());

    // Build the engine (blocks; the !Send registry lives on the engine
    // thread, so the check itself must go through the request channel).
    state.ensure_engine();
    assert!(state.engine_stats().is_some(), "engine must be ready");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    let script = "broadcast \"hi\"\nfrobnicate the widgets\n".to_owned();
    let diags = rt
        .block_on(state.check_document_via_engine(script, skript_checks::CheckConfig::default()))
        .expect("the engine must answer");

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

// --- hover through the engine thread -----------------------------------------

/// Regression test (2026-11): the engine thread used to run a two-channel
/// loop with `rx.blocking_recv()` on the check channel FIRST — a *blocking*
/// wait. While the check sender was alive (the server holds it for its
/// whole lifetime) the hover branch was unreachable, so every
/// `textDocument/hover` request queued forever and VSCode showed a
/// perpetual "Loading.." popup. Checks and hovers now share one FIFO
/// channel; this test fails by timing out if the starvation ever returns.
#[test]
fn hover_answers_while_check_requests_are_possible() {
    let state = AppState::new(EffectiveConfig::default());
    state.ensure_engine();
    assert!(state.engine_stats().is_some(), "engine must be ready");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    // Install the SkriptHub entry table exactly like a live server's
    // initialize() does, so entry IDs from the engine resolve.
    rt.block_on(state.refresh());
    assert!(state.ready(), "entry table must be installed");

    // 1. Serve one check request. The check sender stays alive inside
    //    `AppState` afterwards — exactly the live-server situation that
    //    used to starve the hover channel.
    let diags = rt.block_on(state.check_document_via_engine(
        "broadcast \"hi\"\n".to_owned(),
        skript_checks::CheckConfig::default(),
    ));
    assert!(diags.is_some(), "the check request must be answered");

    // 2. A hover request must now still be answered within the timeout.
    let hover = rt.block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            state.hover_via_engine("broadcast \"hi\"".to_owned(), Some(3)),
        )
        .await
    });
    match hover {
        // Timed out — the starvation bug is back.
        Err(_) => panic!(
            "hover_via_engine never answered while the check channel was \
             open — the engine thread is starving hover requests again"
        ),
        // Answered, but no match: acceptable only if the corpus really has
        // no pattern for this line, which would be a corpus regression.
        Ok(None) => panic!("'broadcast \"hi\"' must match at least one pattern"),
        Ok(Some((entry_id, span))) => {
            assert!(
                span.0 <= 3 && 3 <= span.1,
                "cursor 3 must fall inside the reported span {span:?}"
            );
            // The entry ID must resolve to a real SkriptHub entry.
            let exists = state
                .with_index(|table| table.get(entry_id).is_some())
                .unwrap_or(false);
            assert!(
                exists,
                "entry id {entry_id} must exist in the entry table"
            );

            // Indented lines (the normal case inside `on ...:` blocks)
            // must match too: the raw editor line has a leading tab, which
            // used to make every pattern fail and hover returned null.
            let indented = rt.block_on(state.hover_via_engine(
                "\tbroadcast \"hi\"".to_owned(),
                Some(4),
            ));
            assert!(
                indented.is_some(),
                "a tab-indented line must still match for hover"
            );
            // The reported span must be in ORIGINAL line coordinates
            // (offset 1, past the tab), not the trimmed line's.
            let (_, span) = indented.unwrap();
            assert_eq!(span, (1, 15), "span must map back past the tab: {span:?}");

            // Trailing inline comments must be stripped before matching.
            let commented = rt.block_on(state.hover_via_engine(
                "broadcast \"hi\" # greet the world".to_owned(),
                Some(3),
            ));
            assert!(
                commented.is_some(),
                "a line with a trailing comment must still match for hover"
            );
            let (_, span) = commented.unwrap();
            assert_eq!(span, (0, 14), "comment must not be part of the span: {span:?}");

            // Comment-only lines must not match anything.
            let comment_only = rt.block_on(
                state.hover_via_engine("# just a comment".to_owned(), Some(3)),
            );
            assert!(comment_only.is_none(), "comment-only line must not match");
        }
    }
}

/// Hover and check requests interleave correctly on the shared channel:
/// a hover sent while a check is still being served is answered right
/// after it, not dropped or reordered into silence.
#[test]
fn hover_and_check_interleave_on_shared_channel() {
    let state = AppState::new(EffectiveConfig::default());
    state.ensure_engine();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    rt.block_on(async {
        // Fire both concurrently; the check is the slower one.
        let check_fut = state.check_document_via_engine(
            "broadcast \"hi\"\nfrobnicate the widgets\n".to_owned(),
            skript_checks::CheckConfig::default(),
        );
        let hover_fut =
            state.hover_via_engine("broadcast \"hi\"".to_owned(), Some(0));
        let (check, hover) = tokio::join!(check_fut, hover_fut);
        assert!(check.is_some(), "check must be answered");
        assert!(hover.is_some(), "hover must be answered too");
    });
}
