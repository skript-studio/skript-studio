//! Corpus regression test: every known-valid Skript script under
//! `test-fixtures/` must pass the line-based checker with ZERO Error
//! diagnostics.
//!
//! Sources (all valid — they run on real Skript servers):
//!  - `SKRIPT_CODE/` — the official SkriptLang examples, mirrored from
//!    https://github.com/SkriptLang/Skript/tree/master/src/main/resources/scripts/-examples
//!  - `onix_minigames.sk` — a real-world production minigames script
//!    (SkBee + FaweSK server) that previously produced 461 false
//!    positives through this exact pipeline.
//!
//! Run: cargo test -p skript-checks --test fixtures_corpus -- --nocapture

use skript_checks::{check_document, CheckConfig, Severity};
use skript_patterns::{set_type_resolver, PermissiveResolver};
use skript_registry::Registry;
use std::path::PathBuf;
use std::rc::Rc;

fn fixture_files() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures");
    let mut out: Vec<PathBuf> = vec![root.join("onix_minigames.sk")];
    let mut skript_code: Vec<PathBuf> = walk(&root.join("SKRIPT_CODE"));
    skript_code.sort();
    out.extend(skript_code);
    assert!(
        !out.is_empty() && out.iter().all(|p| p.exists()),
        "fixtures missing: {out:?}"
    );
    out
}

fn walk(dir: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else if p.extension().is_some_and(|x| x == "sk") {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn known_valid_scripts_have_no_error_diagnostics() {
    set_type_resolver(Rc::new(PermissiveResolver));
    let mut reg = Registry::embedded().expect("embedded corpus");
    reg.compile_all();
    let cfg = CheckConfig::default();

    let mut total_errors = 0;
    for file in fixture_files() {
        let text = std::fs::read_to_string(&file).expect("read fixture");
        let errors: Vec<_> = check_document(&text, &reg, &cfg)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            total_errors += errors.len();
            eprintln!("{}: {} errors", file.display(), errors.len());
            for d in errors.iter().take(8) {
                eprintln!("  line {}: {} [{}]", d.line + 1, d.message, d.code);
            }
            if errors.len() > 8 {
                eprintln!("  ... and {} more", errors.len() - 8);
            }
        }
    }
    assert_eq!(
        total_errors, 0,
        "known-valid scripts produced {total_errors} false-positive Errors — \
         see stderr for the offending lines"
    );
}
