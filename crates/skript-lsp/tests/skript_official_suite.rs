//! Official Skript test corpus — fuzz/baseline suite.
//!
//! Embeds the entire `Skript/src/test/skript/{tests,junit}` tree (573
//! `.sk` files) at compile time via `include_dir!` and runs four checks:
//!
//! 1. `parser_never_panics_on_official_corpus` — every file parses without
//!    panicking (`catch_unwind` around `parse_program`).
//! 2. `official_corpus_ast_baseline` — every file parses + validates without
//!    panicking; per-file diagnostic counts are captured for inspection.
//! 3. `syntax_files_parse_at_high_rate` — at least 90% of files under
//!    `syntaxes/` parse cleanly (no `parse-error` diagnostics). These are
//!    the canonical "this should parse" corpus; a regression here means a
//!    real parser bug.
//! 4. `regression_files_parse_without_crash` — every file under
//!    `regressions/` parses without crashing. Regression tests cover edge
//!    cases (long numbers, weird strings, unicode), so they *will* produce
//!    diagnostics — we only assert no crash here.
//!
//! The corpus is the source of truth for "what real Skript looks like"; a
//! parser that mishandles it will mishandle user code. These tests catch
//! panics (the worst regression) and provide a parsing-quality baseline.

use include_dir::{include_dir, Dir};
use skript_core::parse_program;
use skript_core::validation::validate_program;
use std::panic::{catch_unwind, AssertUnwindSafe};

const CORPUS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/skript-official");

/// Per-file analysis result.
#[derive(Debug, Clone)]
struct FileResult {
    /// Path relative to the corpus root, e.g. `syntaxes/effects/EffPush.sk`.
    path: String,
    /// Did parsing or validating panic?
    panicked: bool,
    /// Number of AST diagnostics produced (parse-error + unused-variable +
    /// semantic). `None` if the analysis panicked.
    diagnostic_count: Option<usize>,
    /// Number of `parse-error` diagnostics specifically.
    parse_error_count: Option<usize>,
    /// Coarse category for per-bucket assertions.
    category: &'static str,
}

/// Categorize a file by its content.
///
/// * `test-framework` — files whose first non-blank, non-comment line starts
///   with `test "..."` (the Skript JUnit framework).
/// * `standard-skript` — files that contain typical Skript top-level
///   constructs (`on ` event header, `command /…`, `function …(`,
///   `options:`, or `aliases:`).
/// * `other` — everything else.
fn category(content: &[u8]) -> &'static str {
    let text = match std::str::from_utf8(content) {
        Ok(s) => s,
        Err(_) => return "other",
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("test ") {
            return "test-framework";
        }
        break;
    }
    // Look anywhere in the file for typical Skript top-level constructs.
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("on ")
            || trimmed.starts_with("command /")
            || trimmed.starts_with("command ")
            || trimmed.starts_with("function ")
            || trimmed.starts_with("options:")
            || trimmed.starts_with("aliases:")
        {
            return "standard-skript";
        }
    }
    "other"
}

/// Parse + validate one file inside `catch_unwind`.
fn analyze(content: &[u8]) -> FileResult {
    let cat = category(content);
    let text = match std::str::from_utf8(content) {
        Ok(s) => s,
        Err(_) => {
            return FileResult {
                path: String::new(),
                panicked: false,
                diagnostic_count: Some(0),
                parse_error_count: Some(0),
                category: cat,
            };
        }
    };

    // `parse_program` and `validate_program` are pure Rust but the parser
    // is a hand-written recursive descent; if a bug ever sends it into an
    // infinite recursion or hits an `unwrap` on a `None`, we want to know
    // which file did it, not abort the whole test run.
    let result = catch_unwind(AssertUnwindSafe(|| {
        let pr = parse_program(text);
        let diags = validate_program(text);
        let parse_errors = pr.errors.len();
        let total = diags
            .iter()
            .filter(|d| {
                d.code == "parse-error" || d.code == "unused-variable" || d.code == "semantic"
            })
            .count();
        // validate_program also re-parses; reconcile the parse-error count
        // from the parser's own report (the parser is the source of truth).
        let parse_error_count = diags
            .iter()
            .filter(|d| d.code == "parse-error")
            .count()
            .max(parse_errors);
        (total, parse_error_count)
    }));

    match result {
        Ok((total, parse_errors)) => FileResult {
            path: String::new(),
            panicked: false,
            diagnostic_count: Some(total),
            parse_error_count: Some(parse_errors),
            category: cat,
        },
        Err(_) => FileResult {
            path: String::new(),
            panicked: true,
            diagnostic_count: None,
            parse_error_count: None,
            category: cat,
        },
    }
}

/// Walk a directory recursively, collecting `(path, contents)` pairs.
///
/// `Dir::path()` returns the path relative to the `include_dir!` root, so
/// we don't need to thread a prefix through the recursion — we just read
/// the stored path off each entry.
fn collect_from_dir(dir: &Dir<'_>, out: &mut Vec<(String, &'static [u8])>) {
    for file in dir.files() {
        let path = file.path().to_string_lossy().into_owned();
        // SAFETY: `include_dir!` embeds the bytes for the static lifetime
        // of the binary; the transmute is the documented way to surface
        // that fact to the type system.
        let contents: &'static [u8] =
            unsafe { std::mem::transmute::<&[u8], &'static [u8]>(file.contents()) };
        out.push((path, contents));
    }
    for subdir in dir.dirs() {
        collect_from_dir(subdir, out);
    }
}

/// All `(path, contents)` pairs in the embedded corpus.
fn collect_scripts() -> Vec<(String, &'static [u8])> {
    let mut out = Vec::new();
    collect_from_dir(&CORPUS, &mut out);
    // Filter to .sk files; the corpus also contains README.md and a few
    // environment .json files we don't want to parse.
    out.into_iter()
        .filter(|(p, _)| p.ends_with(".sk"))
        .collect()
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[test]
fn parser_never_panics_on_official_corpus() {
    let scripts = collect_scripts();
    assert!(
        !scripts.is_empty(),
        "corpus must be embedded — found 0 files"
    );
    println!("corpus: {} files", scripts.len());

    let mut panics = Vec::new();
    for (path, contents) in &scripts {
        let mut result = analyze(contents);
        result.path = path.clone();
        if result.panicked {
            panics.push(path.clone());
        }
    }

    assert!(
        panics.is_empty(),
        "parser panicked on {} file(s): {:#?}",
        panics.len(),
        panics
    );
}

#[test]
fn official_corpus_ast_baseline() {
    let scripts = collect_scripts();
    let mut panics = 0;
    let mut total_diags: usize = 0;
    let mut total_parse_errors: usize = 0;
    let mut total_files = 0;

    // Per-category breakdown, for human inspection when this test fails.
    let mut by_category: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();

    for (path, contents) in &scripts {
        let mut result = analyze(contents);
        result.path = path.clone();
        total_files += 1;
        if result.panicked {
            panics += 1;
            continue;
        }
        *by_category.entry(result.category).or_insert(0) += 1;
        if let Some(n) = result.diagnostic_count {
            total_diags += n;
        }
        if let Some(n) = result.parse_error_count {
            total_parse_errors += n;
        }
    }

    println!(
        "AST baseline: {} files, {} panicked, {} total diagnostics ({} parse-errors)",
        total_files, panics, total_diags, total_parse_errors
    );
    println!("by category: {by_category:?}");

    assert_eq!(
        panics, 0,
        "validation panicked on {panics} file(s) — see stderr above"
    );
}

#[test]
fn syntax_files_parse_at_high_rate() {
    let scripts = collect_scripts();
    let syntax_files: Vec<_> = scripts
        .iter()
        .filter(|(p, _)| p.starts_with("syntaxes/") || p.contains("/syntaxes/"))
        .collect();
    assert!(
        !syntax_files.is_empty(),
        "no syntaxes/ files found in corpus"
    );

    let mut clean = 0usize;
    let mut total = 0usize;
    let mut failures = Vec::new();
    for (path, contents) in &syntax_files {
        total += 1;
        let mut result = analyze(contents);
        result.path = (*path).clone();
        if result.panicked {
            failures.push(((*path).clone(), "panic".to_owned()));
            continue;
        }
        let parse_errors = result.parse_error_count.unwrap_or(0);
        if parse_errors == 0 {
            clean += 1;
        } else {
            failures.push(((*path).clone(), format!("{parse_errors} parse-errors")));
        }
    }

    let rate = clean as f64 / total as f64;
    println!(
        "syntaxes/ parse rate: {clean}/{total} = {:.1}%",
        rate * 100.0
    );
    if !failures.is_empty() {
        println!("first 10 failing files:");
        for (p, why) in failures.iter().take(10) {
            println!("  {p}: {why}");
        }
    }
    assert!(
        rate >= 0.90,
        "expected >=90% of syntaxes/ to parse cleanly, got {:.1}% ({clean}/{total}); \
         {} files had parse errors",
        rate * 100.0,
        failures.len()
    );
}

#[test]
fn regression_files_parse_without_crash() {
    let scripts = collect_scripts();
    let regression_files: Vec<_> = scripts
        .iter()
        .filter(|(p, _)| p.starts_with("regressions/") || p.contains("/regressions/"))
        .collect();
    assert!(
        !regression_files.is_empty(),
        "no regressions/ files found in corpus"
    );

    let mut panics = Vec::new();
    for (path, contents) in &regression_files {
        let mut result = analyze(contents);
        result.path = (*path).clone();
        if result.panicked {
            panics.push((*path).clone());
        }
    }

    assert!(
        panics.is_empty(),
        "parser crashed on {} regression file(s): {:#?}",
        panics.len(),
        panics
    );
}
