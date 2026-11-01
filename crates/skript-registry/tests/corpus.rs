//! Corpus validation integration tests — the make-or-break gate of the
//! whole port: does the faithful pattern engine really handle the full
//! SkriptHub dump?
//!
//! OWNER: Agent 8.
//!
//! - [`corpus_shape`]: pure registry/JSON checks, always runs.
//! - [`all_patterns_compile`] and [`spot_check_matching`]: exercise
//!   `skript_patterns::compile_pattern` / `SkriptPattern::match_`, which
//!   are still being ported (Agents 5/6). They are `#[ignore]`-gated on
//!   "compiler+registry (integration)" and will be un-ignored at
//!   integration time (`cargo test -p skript-registry -- --ignored`).

use std::rc::Rc;

use skript_patterns::{
    compile_pattern, set_type_resolver, ParseContext, ParseFlags, PermissiveResolver,
};
use skript_registry::{Registry, SyntaxType};

/// The embedded corpus must load and match the frozen corpus facts
/// (PORTING_CONTRACTS.md / `scripts/prepare_corpus.py`). Deliberately
/// duplicates the `registry.rs` lib test — it validates the same facts
/// from the integration-test side (tests/ dir) too.
#[test]
fn corpus_shape() {
    let reg = Registry::embedded().expect("embedded corpus must load");

    // 8712 entries, 174 distinct addons (lowercased names), 660 type
    // entries.
    assert_eq!(reg.entries().len(), 8712);
    let addon_count = reg
        .entries()
        .iter()
        .map(|e| e.addon.name.to_lowercase())
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(addon_count, 174);
    assert_eq!(reg.indices_for_type(&SyntaxType::Type).len(), 660);

    // 13661 split patterns across all entries (\r\n multi-patterns).
    let split_total: usize = reg.entries().iter().map(|e| e.split_patterns().len()).sum();
    assert_eq!(split_total, 13661);
}

/// Every split pattern in the corpus must compile through the faithful
/// engine. Target: ≥99.6% of the 13661 patterns (<= 50 failures) and more
/// than 13000 successful compiles; each failure is printed to stderr with
/// its entry and error for triage.
#[test]
fn all_patterns_compile() {
    let mut reg = Registry::embedded().expect("embedded corpus must load");
    let failures = reg.compile_all();

    for failure in &failures {
        let entry = &reg.entries()[failure.entry_index];
        eprintln!(
            "COMPILE FAILURE entry #{} ({}, addon {}) pattern #{}: {:?} -> {}",
            failure.entry_index,
            entry.title,
            entry.addon.name,
            failure.pattern_index,
            failure.raw,
            failure.error
        );
    }

    assert!(
        failures.len() <= 50,
        "expected >=99.6% of 13661 patterns to compile, got {} failures",
        failures.len()
    );

    let compiled_total: usize = (0..reg.entries().len())
        .map(|i| reg.compiled_for_entry(i).len())
        .sum();
    assert!(
        compiled_total > 13000,
        "only {compiled_total} patterns compiled (expected > 13000)"
    );
    // Conservation: every split pattern is either compiled or a failure.
    assert_eq!(compiled_total + reg.failures().len(), 13661);
}

/// Curated spot checks of pattern matching against REAL corpus patterns
/// (each pattern string below exists verbatim in the embedded dump).
/// Runs with the permissive type resolver so `%type%` slots accept any
/// non-empty text; the assertions focus on the literal/optional/choice
/// machinery of the engine.
#[test]
fn spot_check_matching() {
    set_type_resolver(Rc::new(PermissiveResolver));

    // (pattern, input, should_match)
    let cases: &[(&str, &str, bool)] = &[
        // --- positives (9) ---
        // Skript "Detonate": simple effect, one type slot at the end.
        ("detonate %entities%", "detonate last creeper", true),
        // Skript "Apply Bone Meal": optional [%number%] slot skipped,
        // "bone[ ]meal" space group, trailing optional "[to %blocks%]".
        (
            "apply [%number%] bone[ ]meal[s] [to %blocks%]",
            "apply bone meal to targeted block",
            true,
        ),
        // Skript "Bukkit Version": leading choice group + separator choice.
        ("([craft]bukkit|minecraft|skript)( |-)version", "minecraft version", true),
        // Skript "Kill": type slot at the end.
        ("kill %entities%", "kill all monsters", true),
        // Skript "Make Fly": optional-free literal tail.
        ("make %players% fly", "make all players fly", true),
        // Skript "Teleport": leading optional, (to|direction) choice, two
        // type slots, nested optional group.
        (
            "[force] teleport %entities% (to|%direction%) %location% [[while] retaining %teleportflags%]",
            "teleport player to spawn",
            true,
        ),
        // Skript "Instantly Explode": optional [creeper[s]] skipped.
        ("instantly explode [creeper[s]] %livingentities%", "instantly explode all creepers", true),
        // Skript "Make Say": choice group with an inner optional.
        (
            "make %players% (say|send [the] message[s]) %strings%",
            "make player say \"hello world\"",
            true,
        ),
        // SkBee-style condition that STARTS with a type slot.
        ("%players% (is|are) (in vanish mode|vanished)", "player is in vanish mode", true),
        // --- negatives (5) ---
        // Wrong leading literal.
        ("detonate %entities%", "explode the server", false),
        // Missing required separator between the choice and "version".
        ("([craft]bukkit|minecraft|skript)( |-)version", "bukkitversion", false),
        // Alternative not in the choice group.
        ("([craft]bukkit|minecraft|skript)( |-)version", "spigot version", false),
        // Empty %entities% slot: the permissive resolver rejects empty.
        ("kill %entities%", "kill", false),
        // Trailing literal mismatch.
        ("make %players% fly", "make players swim", false),
    ];

    assert!(
        cases.len() >= 10,
        "curated case list must stay at 10+ triples"
    );
    assert!(
        cases.iter().filter(|(_, _, m)| !m).count() >= 3,
        "curated case list must keep at least 3 negative cases"
    );

    for (pattern, input, should_match) in cases {
        let compiled = compile_pattern(pattern)
            .unwrap_or_else(|e| panic!("curated pattern {pattern:?} must compile: {e}"));
        let matched = compiled.match_(input, ParseFlags::ALL, ParseContext::Default);
        assert_eq!(
            matched.is_some(),
            *should_match,
            "pattern {pattern:?} vs input {input:?}: expected match={should_match}, got {matched:?}"
        );
    }
}
