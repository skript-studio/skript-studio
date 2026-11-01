# Porting Contracts (FROZEN API)

This document is the single source of truth for cross-module APIs in the
`skript-patterns` / `skript-registry` / `skript-checks` crates. **Multiple
agents are porting different files in parallel.** Everything listed under
"FROZEN" here must keep its exact signature; if something frozen blocks you,
implement the closest approximation in YOUR files and report the friction in
your final message.

## Project

Faithful port of Skript's (v2.17.1) pattern-matching engine from Java to
Rust, for a static-analysis LSP (diagnostics while typing; no runtime).
Java sources: `/home/z/my-project/Skript/src/main/java/ch/njol/skript/`.

## Relationship to the skript-studio base

This workspace is the monorepo at https://github.com/skript-studio/skript-lsp
(branch `merge/monorepo`), which contains:

- `crates/skript-core` (in-tree, was a submodule) — lexer, parser, AST,
  semantics, hover, completion, and a REGEX-APPROXIMATION of the pattern
  DSL (`pattern.rs` compiles patterns to `regex::Regex`; e.g. it treats
  `[craft]` as a per-char class instead of Java's optional literal, and
  ignores `<regex>`, `:tag`, `¦mark`, `@time`, `*`/`~` flags).
- `crates/skript-lsp` — a complete tower-lsp server (12 handler types,
  stdio + WebSocket) built on skript-core.
- `extensions/vscode-skript` (in-tree, was a submodule) — the mainstream
  VSCode integration.
- `apps/skript-studio-ui/` was retired (standalone Tauri app); history
  preserved in git log.

Our three crates are the FAITHFUL Java port that slots alongside:

- `crates/skript-patterns` — 1:1 port of `ch.njol.skript.patterns`.
- `crates/skript-registry` — SkriptHub corpus loader + `compile_all` gate.
- `crates/skript-checks` — diagnostics on top of the faithful engine.

The original "DO NOT modify skript-core (submodule)" constraint no
longer applies — skript-core is in-tree and may be modified directly.
The `extensions/` directory is still owned by the VSCode extension
upstream. Integration of the faithful engine happens in `crates/skript-lsp`
behind config flags (`engine = "java-port"`).

## Workspace layout

```
skript-lsp/                    (monorepo, work branch: merge/monorepo)
├── crates/
│   ├── skript-core/           # in-tree (was submodule)
│   ├── skript-lsp/            # LSP server — integrates the faithful engine
│   ├── skript-patterns/       # OURS: faithful engine port
│   ├── skript-registry/       # OURS: corpus loader + validation gate
│   └── skript-checks/         # OURS: diagnostic rules
├── extensions/
│   └── vscode-skript/         # in-tree (was submodule)
├── PORTING_CONTRACTS.md
└── env.sh
```

## Toolchain

`cargo` is NOT on PATH. Source the env file first:

```
source /home/z/my-project/skript-studio/skript-lsp/env.sh
cd /home/z/my-project/skript-studio/skript-lsp && cargo check -p skript-patterns
```

Other agents run cargo concurrently; if you see "Blocking waiting for file
lock", just wait.

## Rust conventions (apply everywhere)

1. **Java `MatchResult match(String, MatchResult)` (nullable) becomes**
   ```rust
   fn match_(&self, expr: &str, result: &mut MatchResult) -> bool
   ```
   `true` = success (Java non-null). **On success you MUST write the winning
   state back into `*result`** (Java returned a new object; we write back).
   **On failure the contents of `result` are unspecified** — callers must
   never rely on it after a failure. Wherever Java passes a *copy* into a
   sub-match (`matchResult.copy()`), clone locally and only write back on
   success.

2. **Java `int` returning `-1` becomes `Option<usize>`** in new helper code
   (`next`, `nextOccurrence`, `nextBracket`).

3. **Element linking.** Java's `PatternElement` base class has
   `next` / `originalNext` fields (linked-list with back-patching). In Rust
   every element struct carries:
   ```rust
   pub(crate) next_field: RefCell<Option<ElementRef>>,
   pub(crate) original_next_field: RefCell<Option<ElementRef>>,
   ```
   and implements the six plumbing methods exactly as in the stubs
   (`next`, `original_next`, `set_next_raw`, `set_original_next_raw`,
   `as_any`, plus the element's own `match_` / `to_string_pretty`).
   Trait-provided methods (`set_next` default, `set_last_next`, `match_next`,
   `to_full_string`) live in `elements/mod.rs` — do not reimplement them.

4. **RefCell discipline.** The outer `RefCell<dyn PatternElement>` borrow
   (`el.borrow()`) must never be held while calling a method that borrows the
   SAME element again. Pattern: end borrows at statement boundaries; clone
   the `Rc` before nested calls. Inner field RefCells (`next_field` etc.) are
   separate from the outer RefCell, so `el.borrow().set_next(...)` is safe.

5. **Java `Character.toLowerCase(c)`** → `c.to_lowercase().next()` semantics:
   compare lowercased single chars (Unicode-aware; patterns are ~ASCII).

6. **Divergences** from the Java source must be marked with a
   `// DIVERGENCE:` comment explaining why (e.g. TypeResolver seam replacing
   the full expression parser; parse-log handlers not ported).

## FROZEN: skript-patterns public API

```rust
// lib.rs re-exports
pub use compiler::compile_pattern;
pub use error::MalformedPatternError;
pub use match_result::{MatchResult, ParseFlags, RegexMatchInfo, SlotMatch, TypeSpec};
pub use parse_context::ParseContext;
pub use pattern::{SkriptPattern, StringificationProperties};
pub use resolver::{PermissiveResolver, TypeResolver, set_type_resolver, with_type_resolver};

// compiler.rs
pub fn compile_pattern(pattern: &str) -> Result<SkriptPattern, MalformedPatternError>;

// pattern.rs
impl SkriptPattern {
    pub fn new(first: ElementRef, expression_amount: usize) -> Self;
    pub fn match_(&self, expr: &str, flags: ParseFlags, context: ParseContext) -> Option<MatchResult>;
    pub fn count_types(&self) -> usize;
}
pub struct StringificationProperties { /* frozen fields below; may ADD more */ }
impl StringificationProperties { pub const DEFAULT: Self; }

// elements/mod.rs
pub type ElementRef = Rc<RefCell<dyn PatternElement>>;
pub trait PatternElement: Debug { /* frozen: see file */ }

// element constructors + accessors (frozen)
impl LiteralPatternElement { pub fn new_rc(literal: impl Into<String>) -> ElementRef; pub fn literal(&self) -> &str; pub fn is_empty(&self) -> bool; }
impl OptionalPatternElement { pub fn new_rc(child: ElementRef) -> ElementRef; pub fn pattern_element(&self) -> ElementRef; }
impl GroupPatternElement   { pub fn new_rc(child: ElementRef) -> ElementRef; pub fn pattern_element(&self) -> ElementRef; }
impl ChoicePatternElement  { pub fn new_rc() -> ElementRef; pub fn add(&self, el: ElementRef); pub fn get_last(&self) -> Option<ElementRef>; pub fn set_last(&self, el: ElementRef); pub fn alternatives(&self) -> Vec<ElementRef>; }
impl TypePatternElement    { pub fn from_string(s: &str, expression_index: usize) -> Self; }
impl RegexPatternElement   { pub fn new_rc(source: &str) -> Result<ElementRef, MalformedPatternError>; pub fn source(&self) -> &str; }
impl ParseTagPatternElement{ pub fn new_rc_tag(tag: impl Into<String>) -> ElementRef; pub fn new_rc_mark(mark: usize) -> ElementRef; }
```

## FROZEN: skript-registry public API

```rust
// model.rs (complete serde structs — the field names match the JSON)
pub struct SyntaxEntry { /* all fields pub */ }
pub struct AddonInfo { pub name: String, pub link_to_addon: String, pub usage_score: f64 }
pub struct PluginInfo { pub name: String, pub link: Option<String> }
pub enum SyntaxType { Expression, Effect, Event, Condition, Type, Section, Function, Structure, Other(String) }
impl SyntaxEntry { pub fn syntax_type(&self) -> SyntaxType; pub fn split_patterns(&self) -> Vec<String>; }

// registry.rs
pub struct Registry { /* ... */ }
impl Registry {
    pub fn from_json_str(s: &str) -> Result<Registry, serde_json::Error>;
    pub fn from_path(path: &Path) -> Result<Registry, RegistryError>;
    pub fn embedded() -> Result<Registry, RegistryError>;          // include_bytes!(testdata)
    pub fn entries(&self) -> &[SyntaxEntry];
    pub fn by_json_id(&self, id: &str) -> &[usize];                // entry indices
    pub fn by_addon(&self, addon: &str) -> &[usize];
    pub fn indices_for_type(&self, ty: &SyntaxType) -> &[usize];
    pub fn compile_all(&mut self) -> Vec<CompileFailure>;          // compiles every split pattern; stores CompiledSyntax
    pub fn compiled_for_entry(&self, entry_index: usize) -> &[CompiledSyntax];
}
pub struct CompiledSyntax { pub entry_index: usize, pub pattern_index: usize, pub raw: String, pub pattern: SkriptPattern }
pub struct CompileFailure { pub entry_index: usize, pub pattern_index: usize, pub raw: String, pub error: MalformedPatternError }

// lattice.rs
pub struct TypeLattice { /* ... */ }
impl TypeLattice {
    pub fn build(registry: &Registry) -> TypeLattice;
    pub fn lookup(&self, name: &str) -> Option<usize>;             // TypeId = usize
    pub fn is_subtype(&self, sub: usize, sup: usize) -> bool;
    pub fn known_types(&self) -> usize;
}
```

## FROZEN: skript-checks public API

```rust
pub enum Severity { Error, Warning, Info }
pub struct LineDiagnostic { pub line: usize, pub col_start: usize, pub col_end: usize, pub severity: Severity, pub code: &'static str, pub message: String }
pub struct CheckConfig { pub enabled_addons: HashSet<String>, pub installed_plugins: HashSet<String>, pub skript_version: Option<String>, pub minecraft_version: Option<String> }
pub fn check_document(text: &str, registry: &Registry, config: &CheckConfig) -> Vec<LineDiagnostic>;
```

## Corpus facts (for tests)

SkriptHub dump of 2026-11: **8712 entries**, **174 addons**, **13661 split
patterns** (multi-pattern entries are `\r\n`-separated), **660** entries with
`syntax_type == "type"`. Testdata file:
`crates/skript-registry/testdata/skripthub.json` (6.8 MB).

## Element semantics cheatsheet (from the Java source)

- `LiteralPatternElement.match` — char-by-char, case-insensitive; spaces in
  the pattern are lenient (leading/trailing/doubled input spaces tolerated).
- `OptionalPatternElement` — constructor unwraps `[(a|b)]` to `[a|b]` when
  the child is a Group whose `next == null`. `setNext` also propagates
  `setLastNext(next)` into the child chain. Match: try child on a copy, else
  `match_next`.
- `GroupPatternElement` — delegates to child; `setNext` propagates
  `setLastNext(next)` into the child chain.
- `ChoicePatternElement` — alternatives in a `RefCell<Vec<ElementRef>>`;
  `setNext` propagates `setLastNext(next)` to EVERY alternative; match tries
  each on a fresh copy, first success wins; `toString` joins alternatives'
  `to_full_string` with `|`.
- `TypePatternElement` — `%type%` slots. `from_string` parses leading
  `-`/`*`/`~` flags, trailing `@time`, `/`-separated type names, plural
  detection. `match` iterates candidate end-offsets (`initOffset` /
  `advanceOffset` driven by the next literal element), validates the tail
  with `match_next` on a copy, then delegates the substring to the
  **TypeResolver seam** (`resolver::with_type_resolver`), writes
  `expressions[expression_index]`. Java's parse caches / unparsed-literal
  backup / log handlers are NOT ported (static LSP does not need them).
- `RegexPatternElement` — iterates candidate end-offsets via
  `parse_utils::next`, does an anchored region match on
  `&expr[expr_index..next_offset]`, on success inserts the match info at the
  FRONT of `regex_results`.
- `ParseTagPatternElement` — has `tag: RefCell<Option<String>>` and
  `mark: usize`. On match: push tag (if non-empty), `mark ^= self.mark`,
  then `match_next`. Its `setNext` override derives tags for `:(a|b)` /
  `:[a|b]` shapes (port it exactly — it rewrites Choice alternatives in
  place). `toString`: `""` when `exclude_parse_tags`, else `tag + ":"` or
  `mark + ":"` when tag is None, `""` when tag is empty.
- `PatternCompiler` — `[`→Optional, `(`→Group, `|`→Choice (merging when the
  chain head is already a Choice; appends empty alternative), `¦` (U+00A6)
  →integer parse mark, `:` →string tag, `%...%` →TypePatternElement (counts
  `expression_index`), `<...>` →Regex (compile errors →
  MalformedPatternError), `\` escapes next char, else literal. Uses
  `SkriptParser.nextBracket` (SkriptParser.java:1285). `appendElement`
  replaces empty head literals, recurses into the last Choice alternative,
  otherwise walks `next` and calls `last.setNext(next)` +
  `last.originalNext = next` (NOTE: the `originalNext` write bypasses the
  `setNext` override — keep that).
- `SkriptPattern.match_` — lowercase the input, prefilter with keywords
  (`isPresent`), `min_length` check, `trim()`, size `expressions` to
  `expression_amount`, run `first.borrow().match_(...)`.
- `Keyword` — `SimpleKeyword` (starts/ends/contains) and `ChoiceKeyword`
  (set of sets, one alternative's keywords must ALL be present); built by
  walking the `originalNext` chain, recursion depth ≤ 1 for choices, groups
  recurse with depth+1; ParseTag elements do not affect `starting`.
