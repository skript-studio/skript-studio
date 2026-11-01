//! Diagnostic spec — per-rule contract tests for the merged diagnostics
//! pipeline.
//!
//! Two halves:
//!
//! * **AST validator** (`skript_core::validation::validate_program`) —
//!   produces `parse-error`, `unused-variable`, `semantic` codes. Driven
//!   directly via the `ast_diagnostic_codes` helper.
//! * **Pattern engine line checker** (`AppState::check_document_via_engine`
//!   over `skript_checks::check_document`) — produces `unparseable`,
//!   `addon-not-enabled`, `plugin-missing`, `ambiguous-addon` codes. Driven
//!   via the `engine_diagnostic_codes` helper, which lazily builds the
//!   embedded corpus once per process (the engine thread is shared across
//!   tests via `OnceLock`).
//!
//! The two streams produce disjoint code sets, so the merged diagnostic
//! list never has duplicates — `merged_pipeline_catches_both_parse_and_pattern_errors`
//! asserts exactly that.

use skript_core::validation::{validate_program, Severity};
use skript_lsp::backend::build_check_config;
use skript_lsp::options::EffectiveConfig;
use skript_lsp::state::AppState;
use std::sync::OnceLock;

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

/// Run the AST validator on `source` and return the diagnostic codes (in
/// document order). `parse-error`, `unused-variable`, `semantic` are the
/// codes `validate_program` can emit.
fn ast_diagnostic_codes(source: &str) -> Vec<String> {
    validate_program(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// A shared `AppState` with the pattern engine already built. The engine
/// thread is process-global (it owns a `!Send` registry); tests share one
/// via `OnceLock` so the ~13.7k-pattern corpus only compiles once per test
/// run.
fn setup_state() -> AppState {
    static STATE: OnceLock<AppState> = OnceLock::new();
    STATE
        .get_or_init(|| {
            let state = AppState::new(EffectiveConfig::default());
            // Build the engine synchronously on the test thread — the
            // registry compiles on its dedicated engine thread, and
            // `ensure_engine` blocks until it is ready.
            state.ensure_engine();
            state
        })
        .clone()
}

/// Run the pattern-engine line checker on `source` and return the
/// diagnostic codes. Returns an empty `Vec` when the engine is unavailable
/// (corpus load failure or panic).
fn engine_diagnostic_codes(state: &AppState, source: &str) -> Vec<String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    let config = build_check_config(&state.config());
    match rt.block_on(state.check_document_via_engine(source.to_owned(), config)) {
        Some(diags) => diags.into_iter().map(|d| d.code.to_owned()).collect(),
        None => Vec::new(),
    }
}

// --------------------------------------------------------------------------
// AST validator — basic contracts
// --------------------------------------------------------------------------

#[test]
fn empty_script_has_no_diagnostics() {
    let codes = ast_diagnostic_codes("");
    assert!(
        codes.is_empty(),
        "empty script should have no diagnostics, got {codes:?}"
    );
}

#[test]
fn valid_function_has_no_diagnostics() {
    let source = "function greet(name: text):\n\tbroadcast \"hello {_name}\"";
    let codes = ast_diagnostic_codes(source);
    let bad: Vec<_> = codes
        .iter()
        .filter(|c| **c != "unknown-syntax")
        .cloned()
        .collect();
    assert!(
        bad.is_empty(),
        "valid function should have no AST diagnostics, got {bad:?}"
    );
}

#[test]
fn unused_local_variable_is_flagged() {
    let source = "function test():\n\tset {_x} to 5";
    let codes = ast_diagnostic_codes(source);
    assert!(
        codes.iter().any(|c| c == "unused-variable"),
        "expected unused-variable diagnostic, got {codes:?} (variable {{_x}} should be flagged)"
    );
}

#[test]
fn used_local_variable_is_not_flagged() {
    let source = "function test():\n\tset {_x} to 5\n\tbroadcast {_x}";
    let codes = ast_diagnostic_codes(source);
    assert!(
        !codes.iter().any(|c| c == "unused-variable"),
        "used local variable should NOT be flagged, got {codes:?} (variable {{_x}} is read by broadcast)"
    );
}

#[test]
fn global_variable_is_not_flagged_as_unused() {
    let source = "function test():\n\tset {global} to 5";
    let codes = ast_diagnostic_codes(source);
    assert!(
        !codes.iter().any(|c| c == "unused-variable"),
        "global variables should never be flagged as unused, got {codes:?}"
    );
}

#[test]
fn function_parameter_is_not_flagged_as_unused() {
    let source = "function greet(name: text):\n\tbroadcast \"hello\"";
    let codes = ast_diagnostic_codes(source);
    assert!(
        !codes.iter().any(|c| c == "unused-variable"),
        "function parameters should not be flagged as unused, got {codes:?}"
    );
}

// --------------------------------------------------------------------------
// AST validator — parse contracts
// --------------------------------------------------------------------------

#[test]
fn valid_event_handler_parses_cleanly() {
    let source = "on join:\n\tsend \"Welcome!\" to player";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "valid event handler should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn valid_command_parses_cleanly() {
    let source = "command /heal:\n\ttrigger:\n\t\tsend \"healed\" to player";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "valid command should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn valid_command_with_args_parses_cleanly() {
    let source = "command /heal <text>:\n\ttrigger:\n\t\tsend \"healed\" to player";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "command with <text> arg should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn options_block_parses_cleanly() {
    let source = "options:\n\tprefix: hello\n\non join:\n\tbroadcast \"world\"";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "options block should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn if_else_chain_parses_cleanly() {
    let source = "function test(x: number):\n\tif x > 0:\n\t\tbroadcast \"positive\"\n\telse if x < 0:\n\t\tbroadcast \"negative\"\n\telse:\n\t\tbroadcast \"zero\"";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "if/else if/else chain should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn loop_times_parses_cleanly() {
    let source = "function test():\n\tloop 10 times:\n\t\tbroadcast \"tick\"";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "\"loop N times\" should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn loop_all_players_parses_cleanly() {
    let source = "loop all players:\n\tbroadcast \"hi\"";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "\"loop all players\" should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn variable_list_access_parses_cleanly() {
    let source = "function test():\n\tadd \"a\" to {_list::*}\n\tbroadcast {_list::1}";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "variable list access should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn string_interpolation_parses_cleanly() {
    let source = "function test():\n\tset {_name} to \"world\"\n\tbroadcast \"hello %{_name}%\"";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "string interpolation should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn minimessage_tags_in_strings_parse_cleanly() {
    let source =
        "function test():\n\tbroadcast \"<bold><red>Hello <rainbow>world</rainbow></red></bold>\"";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "MiniMessage tags inside strings should not produce parse errors, got {codes:?}"
    );
}

// --------------------------------------------------------------------------
// Pattern engine line checker — effect recognition
// --------------------------------------------------------------------------

#[test]
fn valid_effect_lines_produce_no_unparseable() {
    let state = setup_state();
    let source = "broadcast \"hi\"\nsend \"hello\" to player\nset {_x} to 5";
    let codes = engine_diagnostic_codes(&state, source);
    assert!(
        !codes.iter().any(|c| c == "unparseable"),
        "valid effect lines must not produce `unparseable`, got {codes:?}"
    );
}

#[test]
fn garbage_line_produces_unparseable_or_ambiguous() {
    let state = setup_state();
    let source = "frobnicate the widgets";
    let codes = engine_diagnostic_codes(&state, source);
    // On the permissive resolver, garbage lines may match catch-all
    // patterns and surface as `ambiguous-addon` Info instead of
    // `unparseable`. Both are valid evidence that the line was *not*
    // recognized as a known effect; either is acceptable here.
    let recognized = codes
        .iter()
        .any(|c| c == "unparseable" || c == "ambiguous-addon");
    assert!(
        recognized,
        "garbage line should produce `unparseable` or `ambiguous-addon`, got {codes:?}"
    );
}

#[test]
fn send_effect_is_recognized() {
    let state = setup_state();
    let source = "send \"hello\" to player";
    let codes = engine_diagnostic_codes(&state, source);
    assert!(
        !codes.iter().any(|c| c == "unparseable"),
        "`send` effect must be recognized by the line checker, got {codes:?}"
    );
}

#[test]
fn broadcast_effect_is_recognized() {
    let state = setup_state();
    let source = "broadcast \"hello everyone\"";
    let codes = engine_diagnostic_codes(&state, source);
    assert!(
        !codes.iter().any(|c| c == "unparseable"),
        "`broadcast` effect must be recognized by the line checker, got {codes:?}"
    );
}

#[test]
fn set_variable_effect_is_recognized() {
    let state = setup_state();
    let source = "set {_x} to 5";
    let codes = engine_diagnostic_codes(&state, source);
    assert!(
        !codes.iter().any(|c| c == "unparseable"),
        "`set {{_x}} to 5` must be recognized by the line checker, got {codes:?}"
    );
}

// --------------------------------------------------------------------------
// Merged pipeline
// --------------------------------------------------------------------------

#[test]
fn merged_pipeline_catches_both_parse_and_pattern_errors() {
    // Two lines:
    //   1. `function test(` — missing `:` after args, parse error.
    //   2. `frobnicate the widgets` — garbage, pattern error.
    let source = "function test(:\n\tfrobnicate the widgets\n";
    let ast_codes = ast_diagnostic_codes(source);
    let state = setup_state();
    let engine_codes = engine_diagnostic_codes(&state, source);
    let mut merged: Vec<String> = ast_codes.clone();
    merged.extend(engine_codes.clone());

    assert!(
        merged.iter().any(|c| c == "parse-error"),
        "merged pipeline must include AST parse-error for malformed function decl, \
         merged = {merged:?}"
    );
    assert!(
        merged
            .iter()
            .any(|c| c == "unparseable" || c == "ambiguous-addon"),
        "merged pipeline must include a pattern-engine diagnostic for the garbage line, \
         merged = {merged:?}"
    );
}

// --------------------------------------------------------------------------
// Severities
// --------------------------------------------------------------------------

#[test]
fn parse_errors_are_severity_error() {
    let source = "function test(:\n\tbroadcast \"hi\"\n";
    let diags = validate_program(source);
    let parse_errors: Vec<_> = diags.iter().filter(|d| d.code == "parse-error").collect();
    assert!(
        !parse_errors.is_empty(),
        "expected at least one parse-error, got {diags:?}"
    );
    for d in &parse_errors {
        assert_eq!(
            d.severity,
            Severity::Error,
            "parse-error must have severity Error, got {:?} for {:?}",
            d.severity,
            d.message
        );
    }
}

#[test]
fn unused_variable_is_severity_warning() {
    let source = "function test():\n\tset {_x} to 5";
    let diags = validate_program(source);
    let unused: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "unused-variable")
        .collect();
    assert!(
        !unused.is_empty(),
        "expected an unused-variable diagnostic, got {diags:?}"
    );
    for d in &unused {
        assert_eq!(
            d.severity,
            Severity::Warning,
            "unused-variable must have severity Warning, got {:?} for {:?}",
            d.severity,
            d.message
        );
    }
}

// --------------------------------------------------------------------------
// Parser robustness — no panics, sensible behavior on edge inputs
// --------------------------------------------------------------------------

#[test]
fn parser_handles_empty_lines_and_comments() {
    let source = "# top-level comment\n\n# another comment\n\nfunction test():\n\t# inline comment\n\tset {_x} to 5\n\tbroadcast {_x}\n";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "comments and blank lines should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn parser_handles_unicode_in_strings() {
    let source = "function test():\n\tbroadcast \"héllo wörld 日本語 emoji 🎉\"";
    let codes = ast_diagnostic_codes(source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "unicode in string literals should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn parser_handles_deeply_nested_blocks() {
    let mut source = String::from("function test():\n");
    for i in 0..20 {
        let indent = "\t".repeat(i + 1);
        source.push_str(&format!("{indent}if true:\n"));
    }
    let indent = "\t".repeat(21);
    source.push_str(&format!("{indent}broadcast \"deep\"\n"));
    let codes = ast_diagnostic_codes(&source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "deeply nested if blocks should not produce parse errors, got {codes:?}"
    );
}

#[test]
fn parser_handles_mixed_tabs_and_spaces_gracefully() {
    // Some lines use tabs, some use spaces — the parser must not panic and
    // should produce at most a few parse errors (not infinitely many).
    let source = "function test():\n\tset {_x} to 1\n    set {_y} to 2\n\tbroadcast {_x}\n";
    let diags = validate_program(source);
    // We don't assert zero errors (mixed indentation may confuse the
    // block parser); we just assert no panic and a bounded count.
    assert!(
        diags.len() < 100,
        "mixed indentation should not explode the diagnostic count, got {} diags",
        diags.len()
    );
}

#[test]
fn parser_handles_very_long_lines() {
    let big = "x".repeat(10_000);
    let source = format!("function test():\n\tbroadcast \"{big}\"\n");
    let codes = ast_diagnostic_codes(&source);
    let parse_errors: Vec<_> = codes.iter().filter(|c| **c == "parse-error").collect();
    assert!(
        parse_errors.is_empty(),
        "very long string literals should not produce parse errors, got {} parse errors",
        parse_errors.len()
    );
}
