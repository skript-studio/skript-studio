//! Fuzz test for the Skript parser — runs without cargo-fuzz/nightly.
//!
//! This is a lightweight fuzzer that generates random Skript-like input
//! from a constrained alphabet and feeds it to the parser. The goal is
//! NOT to find deep logic bugs (that's cargo-fuzz's job), but to verify
//! the parser NEVER PANICS on arbitrary input — including malformed,
//! truncated, and adversarial byte sequences.
//!
//! Run with: `cargo test -p skript-core --test fuzz_parser -- --nocapture`
//! For a longer run: `FUZZ_ITERATIONS=100000 cargo test -p skript-core --test fuzz_parser -- --nocapture --ignored`

use skript_core::parser::parse_program;

/// A simple deterministic PRNG (xorshift32) so test runs are reproducible.
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// Generate a random Skript-like string from a constrained alphabet.
/// Includes letters, digits, Skript syntax characters, whitespace, and
/// special characters that are likely to stress the lexer/parser.
fn random_input(state: &mut u32, max_len: usize) -> String {
    // Weighted alphabet: common chars appear more often.
    let alphabet: &[u8] = b"abcdefghijklmnopqrstuvwxyz     \n\t{}[]()<>:%\"#.,;=+-*/_0123456789";
    let len = (xorshift32(state) as usize) % max_len + 1;
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        let idx = (xorshift32(state) as usize) % alphabet.len();
        s.push(alphabet[idx] as char);
    }
    s
}

/// Generate adversarial inputs that are likely to trigger edge cases.
fn adversarial_inputs() -> Vec<String> {
    vec![
        // Empty and whitespace
        String::new(),
        " ".to_string(),
        "\n".to_string(),
        "\t".to_string(),
        "   \n\t  ".to_string(),
        // Unmatched brackets
        "{".to_string(),
        "}".to_string(),
        "[".to_string(),
        "]".to_string(),
        "(".to_string(),
        ")".to_string(),
        "<".to_string(),
        ">".to_string(),
        // Nested brackets
        "{{{{".to_string(),
        "}}}}".to_string(),
        "[[[[".to_string(),
        "))))".to_string(),
        // Unmatched quotes
        "\"".to_string(),
        "\"\"\"".to_string(),
        "\"unclosed".to_string(),
        // Variables
        "{_}".to_string(),
        "{::}".to_string(),
        "{a::b::c}".to_string(),
        "{}".to_string(),
        "{a b}".to_string(),
        // Colons everywhere
        ":::".to_string(),
        "::::".to_string(),
        // Deeply nested
        "((((((((((x))))))))))".to_string(),
        // Very long line
        "x".repeat(10000),
        // Many newlines
        "\n".repeat(1000),
        // Mixed indent
        "\t    \t  ".to_string(),
        // Unicode
        "Hello 世界 🌍".to_string(),
        "on join:\n\t发送 \"你好\"".to_string(),
        // Binary-ish garbage
        "\x00\x01\x02\x03\x04\x05".to_string(),
        // Skript keywords in weird places
        "if if if if if:".to_string(),
        "on on on on:".to_string(),
        "function function():".to_string(),
        "command /:".to_string(),
        // Numbers
        "1234567890".to_string(),
        "1.2.3.4".to_string(),
        "-1-2-3".to_string(),
        // Operators
        "+-*/%".to_string(),
        "== != <= >= < >".to_string(),
        // Comments
        "#".to_string(),
        "#####".to_string(),
        "# comment\n# another".to_string(),
        // MiniMessage
        "<>".to_string(),
        "</>".to_string(),
        "<red>hello</red>".to_string(),
        // String interpolation
        "%{}".to_string(),
        "\"%{}%\"".to_string(),
        // Percent signs
        "%%%".to_string(),
        "%text%".to_string(),
    ]
}

/// The core fuzz assertion: parse_program must NEVER panic.
fn assert_no_panic(input: &str) {
    let result = std::panic::catch_unwind(|| {
        let _ = parse_program(input);
    });
    assert!(
        result.is_ok(),
        "parser panicked on input: {:?}",
        if input.len() > 200 {
            &input[..200]
        } else {
            input
        }
    );
}

#[test]
fn fuzz_adversarial_inputs() {
    for input in adversarial_inputs() {
        assert_no_panic(&input);
    }
}

#[test]
fn fuzz_random_inputs() {
    let iterations: u32 = std::env::var("FUZZ_ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    let mut state = 0xDEADBEEF_u32; // fixed seed for reproducibility
    for i in 0..iterations {
        let input = random_input(&mut state, 500);
        if i % 1000 == 0 {
            eprintln!("fuzz iteration {i}/{iterations}");
        }
        assert_no_panic(&input);
    }
    eprintln!("fuzz: {iterations} random inputs parsed without panic");
}

/// A longer fuzz run, gated behind #[ignore] so it doesn't slow down
/// `cargo test`. Run with:
///   FUZZ_ITERATIONS=1000000 cargo test -p skript-core --test fuzz_parser -- --ignored --nocapture
#[test]
#[ignore = "long-running fuzz — run explicitly"]
fn fuzz_long_run() {
    let iterations: u32 = std::env::var("FUZZ_ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    let mut state = 0xCAFEBABE_u32;
    for i in 0..iterations {
        let input = random_input(&mut state, 1000);
        if i % 10000 == 0 {
            eprintln!("fuzz long-run iteration {i}/{iterations}");
        }
        assert_no_panic(&input);
    }
    eprintln!("fuzz long-run: {iterations} inputs parsed without panic");
}
