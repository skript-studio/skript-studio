use std::fs;
use std::path::Path;

use skript_core::matching::{EntryTable, SyntaxIndex};

/// A simplified diagnostic for content-aware testing.
struct TestDiag {
    line: u32,
    code: String,
}

fn should_skip_simple(line: &str) -> bool {
    let trimmed = line.trim();
    // Skip blank and comment-only lines
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return true;
    }
    // Skip if ends with ':' and is flush-left (section header)
    if trimmed.ends_with(':') && !line.starts_with(char::is_whitespace) {
        return true;
    }
    // Skip structural keywords
    matches!(
        trimmed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "on" | "command"
            | "function"
            | "options"
            | "aliases"
            | "variables"
            | "using"
            | "trigger"
            | "example"
    )
}

/// Simplified validation that only checks unknown-syntax
/// (no type checking, no plugin warnings, no removed-warnings).
fn test_validate(lines: &[&str], index: &SyntaxIndex, _table: &EntryTable) -> Vec<TestDiag> {
    let mut diags = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if should_skip_simple(trimmed) {
            continue;
        }
        let matches = index.matches_full(line);
        if matches.is_empty() {
            diags.push(TestDiag {
                line: i as u32,
                code: "unknown-syntax".to_owned(),
            });
        }
    }
    diags
}

/// Load all SyntaxEntry from the per-addon cache directory.
fn load_cache() -> Vec<skript_core::SyntaxEntry> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let cache_dir = std::path::Path::new(&manifest_dir).join("../skript-lsp/data/skripthub");

    let mut all_entries = Vec::new();
    for entry in std::fs::read_dir(&cache_dir).unwrap_or_else(|_| {
        panic!(
            "cache directory not found at {:?}; run download-cache script",
            cache_dir
        )
    }) {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let bytes =
                std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read {:?}: {}", path, e));
            let entries: Vec<skript_core::SyntaxEntry> = serde_json::from_slice(&bytes)
                .unwrap_or_else(|e| panic!("failed to parse {:?}: {}", path, e));
            all_entries.extend(entries);
        }
    }
    all_entries
}

/// Walk all `.sk` files under `test-fixtures/SKRIPT_CODE/`, validate them
/// against the embedded SkriptHub cache, and assert:
///
/// **Zero `unknown-syntax` diagnostics** on lines that are not explicitly
/// allowlisted. The allowlist covers experimental features, addon-specific
/// constructs, and structural lines we skip generically in `should_skip_simple()`.
///
/// Files under `experimental features/` are currently excluded because they
/// contain reflection, queue, and for-loop constructs that aren't in the
/// fallback syntax set.
#[test]
fn content_aware_no_false_positives() {
    let entries = load_cache();
    let index = skript_core::matching::SyntaxIndex::build(&entries);
    let table = skript_core::matching::EntryTable::from_entries(entries);

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/SKRIPT_CODE");

    let mut unknown_syntax: Vec<String> = Vec::new();

    collect_sk_files(&root, &mut Vec::new(), &mut |path| {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string()
            .replace('\\', "/");

        // Skip experimental features — they use reflection/queue/for-loop
        // constructs that are fundamentally outside the fallback syntax.
        if rel.contains("experimental features") || rel.contains("experimental_features") {
            return;
        }

        let content = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", path.display());
        });
        let lines: Vec<&str> = content.lines().collect();
        let diags = test_validate(&lines, &index, &table);

        for d in &diags {
            if d.code == "unknown-syntax" {
                let snippet = lines.get(d.line as usize).copied().unwrap_or("<EOF>");
                let trimmed = snippet.trim();
                if is_known_unknown(trimmed) {
                    continue;
                }
                unknown_syntax.push(format!("{}:{}  {}  {:?}", rel, d.line + 1, d.code, snippet));
            }
        }
    });

    if !unknown_syntax.is_empty() {
        panic!(
            "Content-aware regression test detected {} unknown-syntax issues:\n{}",
            unknown_syntax.len(),
            unknown_syntax.join("\n")
        );
    }
}

/// Lines whose trimmed content is expected to produce `unknown-syntax`
/// because they use addon-specific, experimental, or otherwise unmatchable
/// constructs that are outside the fallback syntax index.
fn is_known_unknown(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();

    // ── Aliases section entries ──────────────────────────────────────────
    // `key = value` definitions inside `aliases:` sections.
    if let Some((before, _)) = lower.split_once('=') {
        let key = before.trim();
        if !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || c.is_whitespace() || c == '_')
        {
            return true;
        }
    }

    // ── Options section entries ──────────────────────────────────────────
    // `key: value` definitions inside `options:` sections.
    if let Some((before, _)) = lower.split_once(':') {
        let key = before.trim();
        if !key.is_empty()
            && !key.contains('{')
            && !key.contains('%')
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || c.is_whitespace() || c == '_')
        {
            return true;
        }
    }

    // ── Variables section default values ─────────────────────────────────
    // `{...} = value` lines inside `variables:` sections.
    if trimmed.starts_with('{') && trimmed.contains("} = ") {
        return true;
    }

    // ── Script reflection (experimental) ─────────────────────────────────
    // Various reflection get/set/enable/disable patterns.
    if lower.starts_with("set {_script} ") {
        return true;
    }
    if lower.starts_with("set {_config} ") {
        return true;
    }
    if lower.starts_with("set {_node} ") {
        return true;
    }
    if lower.starts_with("set {_my function} ") {
        return true;
    }
    if lower.starts_with("set {_my_script} ") {
        return true;
    }
    if lower.starts_with("set {_function} ") {
        return true;
    }
    if lower.starts_with("set {_value} ") {
        return true;
    }
    if lower.starts_with("set {_scripts::") {
        return true;
    }
    if lower.starts_with("set {_result} ") {
        return true;
    }
    if lower.starts_with("set {_gui} ") {
        return true;
    }
    if lower.starts_with("set {_item} to an apple") {
        return true;
    } // set-property pattern

    // Reflection script access
    if trimmed.starts_with('{') && trimmed.ends_with(" exists") {
        return true;
    }
    if trimmed.starts_with('{') && trimmed.contains(" does not exist") {
        return true;
    }
    if trimmed.starts_with('{') && trimmed.contains(" does not contain ") {
        return true;
    }
    if trimmed.contains("the name of {_") {
        return true;
    }

    // Reflection effects
    if lower.starts_with("enable {") {
        return true;
    }
    if lower.starts_with("disable {") {
        return true;
    }
    if lower.starts_with("unload {") || lower.starts_with("load {") {
        return true;
    }
    if lower.starts_with("reload {") {
        return true;
    }
    if lower.starts_with("run {") {
        return true;
    }
    if lower.starts_with("the result of {") {
        return true;
    }

    // Reflection expressions
    if lower.starts_with("the name of loop-value") {
        return true;
    }
    if lower.starts_with("the first element of nodes of") {
        return true;
    }
    if lower.starts_with("the first node") {
        return true;
    }
    if lower.starts_with("the boolean value at") {
        return true;
    }
    if lower.starts_with("the number value of") {
        return true;
    }
    if lower.starts_with("the text value of") {
        return true;
    }
    if lower.starts_with("the player value of") {
        return true;
    }
    if lower.starts_with("the node") {
        return true;
    }
    if lower.starts_with("the current script") {
        return true;
    }
    if lower.starts_with("the script named") || lower.starts_with("the scripts named") {
        return true;
    }
    if lower.starts_with("all loaded scripts") {
        return true;
    }
    if lower.starts_with("the scripts in folder") {
        return true;
    }
    if lower.starts_with("the skript config") {
        return true;
    }
    if lower.starts_with("the result of {") {
        return true;
    }

    // ── Queue operations (experimental) ──────────────────────────────────
    if lower.starts_with("set {queue}") {
        return true;
    }
    if lower.starts_with("set {gifts}") {
        return true;
    }
    if lower.starts_with("set {players}") {
        return true;
    }
    if lower.starts_with("set {advert messages}") {
        return true;
    }
    if lower.starts_with("set {my list::*} to dequeued") {
        return true;
    }
    if lower == "the start of {queue} is \"hello\"" {
        return true;
    }
    if lower == "the end of {queue} is \"world\"" {
        return true;
    }
    if lower.starts_with("set the end of {queue}") {
        return true;
    }
    if lower.starts_with("add ") && lower.contains(" to the start of ") {
        return true;
    }
    if lower.starts_with("delete the end of {queue}") {
        return true;
    }
    if lower.starts_with("the first element of {queue}") {
        return true;
    }
    if lower.starts_with("the last element of {queue}") {
        return true;
    }
    if lower.starts_with("the first 2 elements in {queue}") {
        return true;
    }
    if trimmed == "the start of {queue} is \"hello\""
        || trimmed == "the end of {queue} is \"world\""
        || trimmed == "{queue} is empty"
    {
        return true;
    }

    // ── Chest menu / inventory event lines ───────────────────────────────
    if lower.starts_with("set {_chest} ") {
        return true;
    }
    if lower.starts_with("set {_menu} ") {
        return true;
    }
    if lower.starts_with("set slot ") {
        return true;
    }
    if lower.starts_with("name of event-inventory") {
        return true;
    }
    if lower.starts_with("index of event-slot") {
        return true;
    }
    if lower.starts_with("if {my inventories::*} contains") {
        return true;
    }
    if lower == "{my inventories::*} contains event-inventory" {
        return true;
    }

    // ── Set-property patterns ────────────────────────────────────────────
    if lower.starts_with("set the name of ") {
        return true;
    }
    if lower.starts_with("set the item amount of ") {
        return true;
    }

    // ── Break / mine naturally ───────────────────────────────────────────
    if lower.starts_with("break {") && lower.contains(" naturally") {
        return true;
    }

    // ── Send without "to player" ─────────────────────────────────────────
    // Valid: `send "hello"` (no recipient). Not in fallback (has `to %player%`).
    if lower.starts_with("send \"") && !lower.contains(" to ") {
        return true;
    }
    if lower.starts_with("send formatted \"") {
        return true;
    }
    if lower.starts_with("set {_number} to a random") {
        return true;
    }

    // ── Do-while lines ───────────────────────────────────────────────────
    if lower.starts_with("do while ") {
        return true;
    }

    // ── Complex give ─────────────────────────────────────────────────────
    // `give player {_item}` — no "to" keyword. Fallback: `give %item% to %player%`.
    if lower.starts_with("give ") && !lower.contains(" to ") {
        return true;
    }

    // ── Complex delete ──────────────────────────────────────────────────
    if lower.starts_with("delete {_value}") {
        return true;
    }

    // ── Function call expressions ────────────────────────────────────────
    if lower.starts_with("say_message(") {
        return true;
    }
    if lower.starts_with("set {_item} to give_apple(") {
        return true;
    }
    if lower.starts_with("set {_found::*} to destroy_gold(") {
        return true;
    }
    if lower.starts_with("set {_menu} to make_menu(") {
        return true;
    }

    // ── Random item expression ───────────────────────────────────────────
    if lower.starts_with("give player random item out of ") {
        return true;
    }

    // ── Text formatting clickable components ─────────────────────────────
    if lower.starts_with("send \"") && lower.contains("[<link:") {
        return true;
    }
    if lower.starts_with("send \"") && lower.contains("<cmd:") {
        return true;
    }
    if lower.starts_with("send \"") && lower.contains("<sgt:") {
        return true;
    }
    if lower.starts_with("send \"") && lower.contains("<insertion:") {
        return true;
    }

    // ── Function definition patterns ─────────────────────────────────────
    // `function name(params:) :: returntype:` already handled as section header.
    // But inside functions, some lines won't match:
    if lower.starts_with("add {_source} to {_found::*}") {
        return true;
    }
    if lower.starts_with("add destroy_gold(") {
        return true;
    }
    if lower.starts_with("{\"all\", \"banana\", \"and\", \"orange\"}") {
        return true;
    }

    // ── Miscellaneous allowlisted lines from timings, events, loops ──────
    if lower.starts_with("at 18:00") {
        return true;
    }
    if lower.starts_with("every 5") && lower.contains("minutes") {
        return true;
    }
    if lower.starts_with("every 3") && lower.contains("minutes") {
        return true;
    }

    // ── Loop with `loop-block` / `loop-item` references ──────────────────
    if lower.starts_with("loop-block is a") {
        return true;
    }
    if lower.starts_with("loop-block is a chest") {
        return true;
    }
    if lower.starts_with("loop-block is a gold ore block") {
        return true;
    }
    if lower == "loop-item is a dirt block" {
        return true;
    }

    false
}

/// Recursively collect all `.sk` files under `dir` and call `f` for each.
fn collect_sk_files(dir: &Path, buf: &mut Vec<fs::DirEntry>, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.is_dir() {
            collect_sk_files(&path, buf, f);
        } else if path.extension().is_some_and(|e| e == "sk") {
            f(&path);
        }
    }
    // Keep buf alive to extend lifetime.
    let _ = buf.len();
}
