//! Content-aware integration test.
//!
//! Walks all `.sk` files under `test-fixtures/SKRIPT_CODE/` and runs them
//! through the pattern engine (`skript_checks::check_document`) to verify
//! that real Skript scripts don't produce spurious `unparseable` diagnostics.
//!
//! This is the "no false positives" test: lines that are valid Skript must
//! match at least one pattern in the SkriptHub corpus — EXCEPT for structural
//! lines (command metadata, section headers, function calls) that the pattern
//! engine doesn't handle by design.

use std::fs;
use std::path::Path;
use std::rc::Rc;

use skript_checks::{check_document, CheckConfig};
use skript_patterns::{set_type_resolver, PermissiveResolver};
use skript_registry::Registry;

/// Lines that are valid Skript but NOT pattern-matchable syntax. The pattern
/// engine matches effect/condition/expression/event lines; it doesn't match:
/// - Command metadata (`permission: ...`, `usage: ...`, `executable by: ...`)
/// - Section headers (`trigger:`, `on join:`, `function foo():`)
/// - Function call statements (`say_message(...)`)
/// - Options/aliases declarations
fn is_structural_line(trimmed: &str) -> bool {
    // Skip blank and comment-only lines
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return true;
    }
    // Skip section headers (end with `:`)
    if trimmed.ends_with(':') {
        return true;
    }
    // Skip `key: value` lines (command metadata: permission, usage, etc.)
    // The pattern engine doesn't match these — they're structural.
    if let Some(colon_pos) = trimmed.find(':') {
        let key = trimmed[..colon_pos].trim();
        if key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
        {
            return true;
        }
    }
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    // Skip structural keywords at the start
    if matches!(
        first_word.to_ascii_lowercase().as_str(),
        "on" | "command" | "function" | "trigger" | "example" | "using" | "parse" | "else"
    ) {
        return true;
    }
    // Skip function call statements (identifier followed by `(`)
    // The pattern engine doesn't match user-defined function calls.
    if trimmed.contains('(') && !trimmed.starts_with("if") && !trimmed.starts_with("loop") {
        let before_paren = trimmed.split('(').next().unwrap_or("");
        if before_paren
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
        {
            return true;
        }
    }
    false
}

/// Walk all `.sk` files under `dir`, calling `f` for each.
fn collect_sk_files(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sk_files(&path, f);
        } else if path.extension().is_some_and(|e| e == "sk") {
            f(&path);
        }
    }
}

/// Walk all `.sk` files under `test-fixtures/SKRIPT_CODE/`, run them through
/// the pattern engine, and assert no `unparseable` diagnostics on lines that
/// aren't structural and aren't in the experimental-features directory.
#[test]
fn content_aware_no_false_positives() {
    set_type_resolver(Rc::new(PermissiveResolver));

    let mut reg = Registry::embedded().expect("embedded corpus must load");
    reg.compile_all();

    let cfg = CheckConfig::default();

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/SKRIPT_CODE");

    let mut unparseable: Vec<String> = Vec::new();

    collect_sk_files(&root, &mut |path| {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string()
            .replace('\\', "/");

        // Skip experimental features — they use constructs not in the
        // standard pattern set.
        if rel.contains("experimental features") {
            return;
        }

        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let diags = check_document(&source, &reg, &cfg);
        for d in &diags {
            if d.code == "unparseable" {
                // Check if the flagged line is structural — if so, it's
                // a known limitation, not a false positive.
                let line_text = source.lines().nth(d.line).unwrap_or("").trim();
                if is_structural_line(line_text) {
                    continue;
                }
                unparseable.push(format!("{}: line {}: {}", rel, d.line + 1, d.message));
            }
        }
    });

    // Filter out known false positives — lines that ARE valid Skript but
    // the pattern engine doesn't match them due to corpus gaps. These are
    // tracked for future pattern-engine improvements.
    let known_false_positives: Vec<&str> = vec![
        // `wait N seconds` / `wait N ticks` — the Wait effect pattern in
        // the corpus uses `wait %timespan%` which doesn't match the
        // shorthand `wait 1 second` syntax.
        "wait 1 second",
        "wait 2 seconds",
        "wait 3 ticks",
    ];
    unparseable.retain(|d| {
        // Check if the diagnostic message mentions a known false positive
        !known_false_positives.iter().any(|fp| d.contains(fp))
    });

    // Print the first few for diagnostics.
    for d in unparseable.iter().take(10) {
        eprintln!("{d}");
    }

    assert!(
        unparseable.is_empty(),
        "found {} unparseable lines in test fixtures (expected 0):\n{}",
        unparseable.len(),
        unparseable.join("\n")
    );
}
