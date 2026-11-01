//! Benchmarks for the pattern engine: corpus compilation + single-pattern
//! compile + pattern matching.
//!
//! Run with: `cargo bench -p skript-registry`
//! Results land in `target/criterion/`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use skript_patterns::compile_pattern;
use skript_registry::Registry;

fn bench_compile_corpus(c: &mut Criterion) {
    c.bench_function("compile_all (8712 entries, 13661 patterns)", |b| {
        b.iter(|| {
            let mut reg = Registry::embedded().expect("embedded corpus must load");
            let failures = black_box(reg.compile_all());
            // Sanity: the corpus has 17 known-malformed patterns.
            assert!(
                failures.len() <= 50,
                "unexpected failure count: {}",
                failures.len()
            );
        });
    });
}

fn bench_compile_single_pattern(c: &mut Criterion) {
    // A representative real-world pattern from the SkriptHub corpus.
    let patterns = [
        "send %text% to %player%",
        "give %itemtype% to %player%",
        "set {%variable%} to %object%",
        "add %object% to %object%",
        "if %object% is %object%",
    ];

    c.bench_function("compile_pattern (single)", |b| {
        b.iter(|| {
            for p in &patterns {
                let _ = black_box(compile_pattern(p));
            }
        });
    });
}

fn bench_match_pattern(c: &mut Criterion) {
    use skript_patterns::{
        set_type_resolver, MatchResult, ParseContext, ParseFlags, PermissiveResolver, SkriptPattern,
    };
    use std::rc::Rc;

    // Compile a simple pattern once, then benchmark matching against it.
    let pattern: SkriptPattern =
        compile_pattern("send %text% to %player%").expect("pattern must compile");

    c.bench_function("match (send \"hello\" to Steve)", |b| {
        b.iter(|| {
            set_type_resolver(Rc::new(PermissiveResolver));
            let mut result = MatchResult::new(0);
            pattern.match_(
                black_box("send \"hello\" to Steve"),
                ParseFlags::ALL,
                ParseContext::Default,
            );
            black_box(&mut result);
        });
    });
}

criterion_group!(
    benches,
    bench_compile_corpus,
    bench_compile_single_pattern,
    bench_match_pattern
);
criterion_main!(benches);
