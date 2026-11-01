//! The type lattice derived from the corpus.
//!
//! OWNER: Agent 8.
//!
//! Built from (a) the 660 `syntax_type == "type"` entries (their patterns
//! are plain type names like `folder|dir|box` or `[the] text[s]`), and
//! (b) the `return_type` strings of expression entries (3559 non-null in
//! this dump; the porting notes said 7590 — the observed corpus count is
//! 3559, see the report). Provides tolerant lookup (case-insensitive,
//! singular/plural) and a small hardcoded hierarchy for subtype checks.
//!
//! Normalized key: lowercase, `%...%` unwrapped, all whitespace removed,
//! then conservatively singularized (`entities` → `entity`). Every raw
//! spelling harvested along the way is kept in an alias map, and every
//! alternative of a type entry (`folder|dir|box`) points at the same
//! [`TypeId`], so `lookup("dir") == lookup("folder") == lookup("boxes")`
//! style queries all resolve.
//!
//! DIVERGENCE from Skript: the real type system resolves code names to
//! `ClassInfo` objects registered at runtime, with a full converter graph.
//! This static lattice only knows the *names* that exist in the corpus
//! plus a tiny, deliberately conservative subtype hierarchy (see
//! [`WELL_KNOWN_SUBTYPE_EDGES`]).

use std::collections::HashSet;

use crate::model::SyntaxType;
use crate::registry::Registry;

/// Opaque type id (index into the lattice's type table).
pub type TypeId = usize;

/// Cap on the number of literal variants expanded from one type pattern
/// (patterns like `mongo[db|sk][ |-]document[s]` multiply fast; the cap
/// keeps a pathological corpus entry from exploding).
const MAX_VARIANTS: usize = 256;

/// Hardcoded subtype edges among well-known Skript types, as
/// `(subtype key, supertype key)` pairs of *normalized* lattice keys.
///
/// Deliberately MINIMAL — only edges that are certainly true in Skript's
/// type system (mirroring the Bukkit class hierarchy Skript builds on):
///
/// - `player < living entity` (Bukkit: `Player extends LivingEntity`)
/// - `living entity < entity`
/// - `item type < item` (an item type is usable wherever an item is)
/// - `integer < number`
///
/// Edges whose endpoints do not both exist in the corpus are skipped.
const WELL_KNOWN_SUBTYPE_EDGES: &[(&str, &str)] = &[
    ("player", "livingentity"),
    ("livingentity", "entity"),
    ("itemtype", "item"),
    ("integer", "number"),
];

/// Irregular English plurals that appear in Skript type names.
fn irregular_plural(word: &str) -> Option<&'static str> {
    match word {
        "entities" => Some("entity"),
        "indices" => Some("index"),
        "children" => Some("child"),
        _ => None,
    }
}

/// Conservative English singularization heuristic.
///
/// - irregulars first (`entities` → `entity`, …)
/// - `…ies` → `…y` (`difficulties` → `difficulty`)
/// - `…sses/xes/zes/ches/shes` → strip `es` (`classes` → `class`,
///   `boxes` → `box`, `matches` → `match`)
/// - `…ss` / `…us` stay (`glass`, `status`, `cactus`)
/// - otherwise strip a final `s` (`objects` → `object`)
///
/// Wrong-but-deterministic results (e.g. `series` → `sery`, `axes` →
/// `ax`) are harmless: normalization is applied identically when
/// registering and when querying, so every raw form still resolves to the
/// id it was registered under.
fn singularize(word: &str) -> String {
    if let Some(s) = irregular_plural(word) {
        return s.to_string();
    }
    if word.len() > 3 && word.ends_with("ies") {
        // Safe slice: "ies" is ASCII, so len-3 is a char boundary.
        return format!("{}y", &word[..word.len() - 3]);
    }
    for suffix in ["sses", "xes", "zes", "ches", "shes"] {
        if word.ends_with(suffix) {
            return word[..word.len() - 2].to_string();
        }
    }
    if word.ends_with("ss") || word.ends_with("us") {
        return word.to_string();
    }
    if word.len() > 1 && word.ends_with('s') {
        return word[..word.len() - 1].to_string();
    }
    word.to_string()
}

/// Normalize a type name to its lattice key: trim, lowercase, unwrap a
/// `%...%` wrapper, drop all whitespace, singularize.
fn normalize(name: &str) -> String {
    let mut s = name.trim().to_lowercase();
    if s.len() > 2 && s.starts_with('%') && s.ends_with('%') {
        // Safe slice: '%' is one byte on both ends.
        s = s[1..s.len() - 1].to_string();
    }
    let mut key = String::with_capacity(s.len());
    for c in s.chars() {
        if !c.is_whitespace() {
            key.push(c);
        }
    }
    singularize(&key)
}

/// Lowercase a raw name and drop all whitespace (the alias-map key form).
fn squeeze_lower(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// Strip `<...>` regex regions. Some corpus entries use the look-alike
/// arrowheads `˂` (U+02C2) / `˃` (U+02C3) instead of ASCII brackets.
fn strip_regex_regions(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut depth = 0usize;
    for c in pattern.chars() {
        match c {
            '<' | '\u{02C2}' => depth += 1,
            '>' | '\u{02C3}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Expand a type-declaration pattern into its literal name variants by
/// interpreting the pattern DSL:
///
/// - `[...]` optional groups — absent first, then each inner alternative
///   (`[the] text[s]` → `text`, `the text`, `texts`, `the texts`);
/// - `(...)` choice groups — each alternative, order preserved
///   (`entit(y|ies)` → `entity`, `entities`);
/// - `|` top-level alternatives (`folder|dir|box` → three names);
/// - `%...%` type slots — the inner `/`- or `|`-separated code names as
///   literals, plus the slot-absent variant;
/// - `<...>` / `˂...˃` regex regions are stripped beforehand.
///
/// Expansion is capped at [`MAX_VARIANTS`] variants.
fn expand_pattern(pattern: &str) -> Vec<String> {
    let chars: Vec<char> = strip_regex_regions(pattern).chars().collect();
    let mut pos = 0usize;
    parse_alternatives(&chars, &mut pos)
        .into_iter()
        .map(|v| v.trim().to_string())
        .collect()
}

/// Parse one bracket level into the list of alternative strings. Stops
/// (without consuming) at `]`, `)` or end of input.
///
/// Alternatives are `|`-separated *sequences*; within a sequence, optional
/// and choice groups multiply the running variant set. `buf` accumulates
/// literal characters and is "sealed" (folded into every variant of the
/// current sequence) before a group or a `|` — this is what makes
/// `entit(y|ies)` expand to `entity`/`entities` instead of concatenating
/// across the choice.
fn parse_alternatives(s: &[char], pos: &mut usize) -> Vec<String> {
    // Sequences of alternatives at this level; each sequence is the set of
    // literal variants built so far for that `|`-branch.
    let mut sequences: Vec<Vec<String>> = vec![vec![String::new()]];
    let mut buf = String::new();

    while *pos < s.len() {
        let c = s[*pos];
        match c {
            ']' | ')' => break,
            '[' => {
                seal(&mut sequences, &mut buf);
                *pos += 1;
                let inner = parse_alternatives(s, pos);
                if *pos < s.len() && s[*pos] == ']' {
                    *pos += 1;
                }
                // Optional: the group may be absent (first) or present as
                // one of its inner alternatives.
                multiply(sequences.last_mut().unwrap(), &inner, true);
            }
            '(' => {
                seal(&mut sequences, &mut buf);
                *pos += 1;
                let inner = parse_alternatives(s, pos);
                if *pos < s.len() && s[*pos] == ')' {
                    *pos += 1;
                }
                multiply(sequences.last_mut().unwrap(), &inner, false);
            }
            '|' => {
                seal(&mut sequences, &mut buf);
                sequences.push(vec![String::new()]);
                *pos += 1;
            }
            '%' => {
                seal(&mut sequences, &mut buf);
                // %...% slot: harvest the inner type-code alternatives
                // (e.g. `%primitivetype/javatype%`), also allow absent.
                let mut j = *pos + 1;
                while j < s.len() && s[j] != '%' {
                    j += 1;
                }
                if j < s.len() && j > *pos + 1 {
                    let body: String = s[*pos + 1..j].iter().collect();
                    *pos = j + 1;
                    let parts: Vec<String> = body
                        .split(['/', '|'])
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .map(str::to_string)
                        .collect();
                    multiply(sequences.last_mut().unwrap(), &parts, true);
                } else {
                    // Stray '%' — treat as a literal-ish no-op.
                    *pos += 1;
                }
            }
            _ => {
                buf.push(c);
                *pos += 1;
            }
        }
    }
    seal(&mut sequences, &mut buf);

    // Flatten in (sequence, variant) order, capped.
    let mut out = Vec::new();
    'outer: for seq in sequences {
        for v in seq {
            out.push(v);
            if out.len() >= MAX_VARIANTS {
                break 'outer;
            }
        }
    }
    out
}

/// Fold `buf` into every variant of the current (last) sequence.
fn seal(sequences: &mut [Vec<String>], buf: &mut String) {
    if buf.is_empty() {
        return;
    }
    let b = std::mem::take(buf);
    for v in sequences.last_mut().expect("always a current sequence") {
        v.push_str(&b);
    }
}

/// Multiply every variant of `seq` by `group`'s alternatives. When
/// `optional`, the absent variant comes first.
fn multiply(seq: &mut Vec<String>, group: &[String], optional: bool) {
    let mut next = Vec::with_capacity(seq.len() * (group.len() + 1));
    for v in seq.iter() {
        if optional {
            next.push(v.clone());
        }
        for alt in group {
            next.push(format!("{v}{alt}"));
        }
    }
    next.truncate(MAX_VARIANTS);
    *seq = next;
}

/// Turn raw expansion variants into candidate type names: split on `,`,
/// drop fragments containing `.` or `:` (java enum ids like
/// `despawnreasons.chunk_unload`, parse tags), strip leading articles
/// (`the`/`a`/`an`), drop empties and fragments without any alphanumeric
/// character.
fn clean_candidates(variants: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for variant in variants {
        for frag in variant.split(',') {
            let frag = frag.trim();
            if frag.contains('.') || frag.contains(':') {
                continue;
            }
            let mut words: Vec<&str> = frag.split_whitespace().collect();
            while matches!(words.first(), Some(&"the") | Some(&"a") | Some(&"an")) {
                words.remove(0);
            }
            if words.is_empty() {
                continue;
            }
            let joined = words.join(" ");
            if !joined.chars().any(|c| c.is_ascii_alphanumeric()) {
                continue;
            }
            out.push(joined);
        }
    }
    out
}

/// Split a free-form `return_type` string into candidate type names.
///
/// These strings are not patterns, but they carry noise: `Item / Material`,
/// `String & Item Stack`, `%conversationmessage%`, `[Navigation] Type`,
/// `String ("PRIVATE", "BOT", …)`. Handling: unwrap `%...%`, strip regex
/// regions, cut at the first `(`, split on `/` `&` `,`, strip `[`/`]` and
/// leading articles, drop junk fragments.
fn split_return_type(rt: &str) -> Vec<String> {
    let mut s = rt.trim().to_string();
    if s.len() > 2 && s.starts_with('%') && s.ends_with('%') {
        s = s[1..s.len() - 1].to_string();
    }
    let s = strip_regex_regions(&s);
    let s = match s.find('(') {
        Some(i) => &s[..i],
        None => s.as_str(),
    };
    // Drop optional-group brackets (`[Navigation] Type`).
    let s = s.replace(['[', ']'], "");
    // Split multi-type lists on `/`, `&` and `,`.
    clean_candidates(s.split(['/', '&', ',']).map(str::to_string))
}

#[derive(Debug, Default)]
pub struct TypeLattice {
    /// Normalized (lowercase, singular) name -> TypeId.
    names: std::collections::HashMap<String, TypeId>,
    /// Canonical display names, indexed by TypeId.
    display: Vec<String>,
    /// Alias edges: alias name -> canonical name.
    aliases: std::collections::HashMap<String, String>,
    /// Hierarchy edges: subtype id -> parents.
    parents: Vec<Vec<TypeId>>,
    /// Whether `display[i]` was contributed by the official `Skript`
    /// addon (used to upgrade merged display names).
    skript_official: Vec<bool>,
}

impl TypeLattice {
    /// Build the lattice from a loaded (not necessarily compiled) registry.
    pub fn build(registry: &Registry) -> TypeLattice {
        let mut lattice = TypeLattice::default();

        // (a) Harvest type names from the 660 `type` entries. Every split
        // pattern of one entry declares the SAME type (e.g. Skript's
        // "Item Type": `item[ ]type[s]` + `material[s]`), so all cleaned
        // candidates of an entry share one TypeId; the first candidate is
        // the canonical name, and if a candidate's key already exists (a
        // previous entry, possibly from another addon) the entry merges
        // into that type instead of creating a new one.
        for &entry_index in registry.indices_for_type(&SyntaxType::Type) {
            let entry = &registry.entries()[entry_index];

            let mut candidates: Vec<String> = Vec::new();
            for pattern in entry.split_patterns() {
                candidates.extend(clean_candidates(expand_pattern(&pattern)));
            }
            if candidates.is_empty() {
                continue;
            }

            let keys: Vec<String> = candidates.iter().map(|c| normalize(c)).collect();
            let key_set: HashSet<&String> = keys.iter().collect();
            let is_official = entry.addon.name.eq_ignore_ascii_case("skript");

            // Display name: the entry's title when the title is itself one
            // of the entry's names ("Entity", "Item Type"), else the first
            // candidate.
            let display = {
                let title = entry.title.trim();
                if !title.is_empty() && key_set.contains(&normalize(title)) {
                    title.to_string()
                } else {
                    candidates[0].clone()
                }
            };

            let tid = if let Some(&tid) = keys.iter().find_map(|k| lattice.names.get(k)) {
                tid
            } else {
                let tid = lattice.display.len();
                lattice.names.insert(keys[0].clone(), tid);
                lattice.display.push(display.clone());
                lattice.skript_official.push(is_official);
                tid
            };
            // A merge from the official Skript addon wins the display name.
            if is_official && !lattice.skript_official[tid] {
                lattice.display[tid] = display;
                lattice.skript_official[tid] = true;
            }
            for (candidate, key) in candidates.iter().zip(keys.iter()) {
                lattice.names.entry(key.clone()).or_insert(tid);
                lattice.aliases.insert(squeeze_lower(candidate), key.clone());
            }
        }

        // (b) Harvest `return_type` strings of all entries. Each name is an
        // alias pointing at the type of the same normalized name if that
        // exists, else it becomes its own type.
        for entry in registry.entries() {
            let Some(rt) = entry.return_type.as_deref() else {
                continue;
            };
            for name in split_return_type(rt) {
                let key = normalize(&name);
                if !lattice.names.contains_key(&key) {
                    let tid = lattice.display.len();
                    lattice.names.insert(key.clone(), tid);
                    lattice.display.push(name.clone());
                    lattice.skript_official.push(false);
                }
                lattice.aliases.insert(squeeze_lower(&name), key);
            }
        }

        // (c) Hardcoded subtype hierarchy among well-known types.
        lattice.parents = vec![Vec::new(); lattice.display.len()];
        for &(sub_key, sup_key) in WELL_KNOWN_SUBTYPE_EDGES {
            if let (Some(&sub), Some(&sup)) =
                (lattice.names.get(sub_key), lattice.names.get(sup_key))
            {
                if sub != sup && !lattice.parents[sub].contains(&sup) {
                    lattice.parents[sub].push(sup);
                }
            }
        }

        lattice
    }

    /// Tolerant lookup: case-insensitive, singular/plural-insensitive,
    /// alias-aware. Returns the type id.
    pub fn lookup(&self, name: &str) -> Option<TypeId> {
        // 1) Direct normalized key (handles case, whitespace, %-wrappers,
        //    plurals).
        let key = normalize(name);
        if let Some(&id) = self.names.get(&key) {
            return Some(id);
        }
        // 2) The raw (pre-singularization) form via the alias map — catches
        //    spellings that were registered under a different key.
        let raw = squeeze_lower(name);
        if let Some(canonical) = self.aliases.get(&raw) {
            if let Some(&id) = self.names.get(canonical) {
                return Some(id);
            }
        }
        // 3) The normalized form via the alias map.
        if let Some(canonical) = self.aliases.get(&key) {
            if let Some(&id) = self.names.get(canonical) {
                return Some(id);
            }
        }
        None
    }

    /// Canonical display name of a type id.
    pub fn display_name(&self, id: TypeId) -> &str {
        &self.display[id]
    }

    /// Whether `sub` is a (transitive) subtype of `sup` (reflexive).
    pub fn is_subtype(&self, sub: TypeId, sup: TypeId) -> bool {
        if sub >= self.parents.len() || sup >= self.parents.len() {
            return false;
        }
        if sub == sup {
            return true;
        }
        // Walk up the parent edges. The hardcoded hierarchy is tiny and
        // acyclic, but the visited-set guards against cycles anyway.
        let mut seen: HashSet<TypeId> = HashSet::new();
        let mut stack = vec![sub];
        seen.insert(sub);
        while let Some(current) = stack.pop() {
            for &parent in &self.parents[current] {
                if parent == sup {
                    return true;
                }
                if seen.insert(parent) {
                    stack.push(parent);
                }
            }
        }
        false
    }

    /// Number of known types.
    pub fn known_types(&self) -> usize {
        self.display.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;

    /// Full-corpus lattice (the 6.8 MB embedded dump; loads in ~1s).
    fn lattice() -> TypeLattice {
        TypeLattice::build(&Registry::embedded().expect("embedded corpus must load"))
    }

    // ------------------------------------------------------------------
    // Pure-function tests (no corpus needed)
    // ------------------------------------------------------------------

    #[test]
    fn singularize_rules() {
        // Task-mandated examples.
        assert_eq!(singularize("entities"), "entity");
        assert_eq!(singularize("objects"), "object");
        assert_eq!(singularize("locations"), "location");
        // Irregulars table.
        assert_eq!(singularize("indices"), "index");
        assert_eq!(singularize("children"), "child");
        // ies -> y.
        assert_eq!(singularize("difficulties"), "difficulty");
        assert_eq!(singularize("inventories"), "inventory");
        // es after s/x/z/ch/sh.
        assert_eq!(singularize("classes"), "class");
        assert_eq!(singularize("boxes"), "box");
        assert_eq!(singularize("matches"), "match");
        assert_eq!(singularize("bushes"), "bush");
        // Protected endings.
        assert_eq!(singularize("glass"), "glass");
        assert_eq!(singularize("status"), "status");
        // Plain s.
        assert_eq!(singularize("players"), "player");
        assert_eq!(singularize("guis"), "gui");
        // Already singular / unchanged.
        assert_eq!(singularize("entity"), "entity");
        assert_eq!(singularize("text"), "text");
    }

    #[test]
    fn normalize_strips_noise() {
        assert_eq!(normalize("  Offline Player "), "offlineplayer");
        assert_eq!(normalize("Item Types"), "itemtype");
        assert_eq!(normalize("%conversationmessage%"), "conversationmessage");
        assert_eq!(normalize("LIVING  Entities"), "livingentity");
    }

    #[test]
    fn expand_pattern_variants() {
        // Top-level alternatives, order preserved (first = canonical).
        assert_eq!(expand_pattern("folder|dir|box"), ["folder", "dir", "box"]);
        // Optional groups expand absent-first per variant (variant order
        // is preserved, so the first candidate is the all-absent form).
        assert_eq!(
            expand_pattern("[the] text[s]"),
            ["text", "texts", "the text", "the texts"]
        );
        assert_eq!(expand_pattern("biome[s]"), ["biome", "biomes"]);
        // Choice groups keep alternative order (mid-word suffixes work).
        assert_eq!(expand_pattern("entit(y|ies)"), ["entity", "entities"]);
        assert_eq!(expand_pattern("difficult(y|ies)"), ["difficulty", "difficulties"]);
        // Nested optionals + explicit space group.
        assert_eq!(
            expand_pattern("num[ber][s]"),
            ["num", "nums", "number", "numbers"]
        );
        assert_eq!(
            expand_pattern("living[ ]entit(y|ies)").first().map(String::as_str),
            Some("livingentity")
        );
        // Regex regions are stripped (the Character type's whole pattern
        // is `'˂.˃'`, which leaves only the quotes — and no candidates).
        assert_eq!(expand_pattern("'˂.˃'"), ["''"]);
        // %...% slots contribute their inner code names (plus absent).
        let v = expand_pattern("%primitivetype/javatype%");
        assert!(v.contains(&"primitivetype".to_string()));
        assert!(v.contains(&"javatype".to_string()));
        assert!(v.contains(&String::new()));
    }

    #[test]
    fn clean_candidates_drops_junk() {
        let out = clean_candidates([
            "the text".to_string(),
            ",,".to_string(),
            "a.b.c".to_string(),
            "x:y".to_string(),
            "an object".to_string(),
            "!!".to_string(),
        ]);
        assert_eq!(out, ["text", "object"]);
    }

    #[test]
    fn split_return_type_shapes() {
        assert_eq!(split_return_type("Item / Material"), ["Item", "Material"]);
        assert_eq!(
            split_return_type("String & Item Stack"),
            ["String", "Item Stack"]
        );
        assert_eq!(split_return_type("%conversationmessage%"), ["conversationmessage"]);
        assert_eq!(split_return_type("[Navigation] Type"), ["Navigation Type"]);
        // Cut at the first '('.
        assert_eq!(split_return_type(r#"String ("PRIVATE", "BOT")"#), ["String"]);
        assert_eq!(split_return_type("object (protected region)"), ["object"]);
    }

    // ------------------------------------------------------------------
    // Corpus-backed tests
    // ------------------------------------------------------------------

    #[test]
    fn lookup_is_case_and_plural_insensitive() {
        let l = lattice();
        // Task-mandated assertions.
        assert_eq!(l.lookup("Entities"), l.lookup("entity"));
        assert_eq!(l.lookup("Text"), l.lookup("texts"));
        // Skript's own Text entry is `(text|string)[s]`, so "string" is an
        // alternative of the SAME type.
        assert_eq!(l.lookup("String"), l.lookup("text"));
        // Other well-known forms.
        assert_eq!(l.lookup("Players"), l.lookup("player"));
        assert_eq!(l.lookup("material"), l.lookup("item type")); // Item Type entry alt
        assert_eq!(l.lookup("num"), l.lookup("number")); // num[ber][s]
        assert_eq!(l.lookup("livingentities"), l.lookup("living entity"));
        assert_eq!(l.lookup("item stacks"), l.lookup("item"));
        // Return-type-only spellings resolve too.
        assert!(l.lookup("Offline Player").is_some());
        assert!(l.lookup("Timespan").is_some());
        assert!(l.lookup("long").is_some());
    }

    #[test]
    fn lookup_misses_are_none() {
        let l = lattice();
        assert_eq!(l.lookup(""), None);
        assert_eq!(l.lookup("   "), None);
        assert_eq!(l.lookup("definitely-not-a-type-xyzzy"), None);
    }

    #[test]
    fn known_types_exceeds_500() {
        assert!(
            lattice().known_types() > 500,
            "expected a lattice of well over 500 types from 660 type entries + return types"
        );
    }

    #[test]
    fn hardcoded_subtype_edges() {
        let l = lattice();
        let player = l.lookup("player").expect("player type in corpus");
        let entity = l.lookup("entity").expect("entity type in corpus");
        let living = l.lookup("living entity").expect("living entity type in corpus");
        let itemtype = l.lookup("item type").expect("item type in corpus");
        let item = l.lookup("item").expect("item in corpus");
        let integer = l.lookup("integer").expect("integer in corpus");
        let number = l.lookup("number").expect("number in corpus");
        let location = l.lookup("location").expect("location in corpus");

        // Task-mandated assertions.
        assert!(l.is_subtype(player, entity));
        assert!(!l.is_subtype(entity, player));
        // Transitivity through the hardcoded edges.
        assert!(l.is_subtype(player, living));
        assert!(l.is_subtype(living, entity));
        assert!(!l.is_subtype(entity, living));
        // The other edges.
        assert!(l.is_subtype(itemtype, item));
        assert!(l.is_subtype(integer, number));
        assert!(!l.is_subtype(number, integer));
        assert!(!l.is_subtype(item, itemtype));
        // Unrelated types.
        assert!(!l.is_subtype(location, entity));
        assert!(!l.is_subtype(item, entity));
        assert!(!l.is_subtype(location, player));
        // Reflexive.
        assert!(l.is_subtype(entity, entity));
        assert!(l.is_subtype(player, player));
    }

    #[test]
    fn display_names_are_human_readable() {
        let l = lattice();
        assert_eq!(l.display_name(l.lookup("entities").unwrap()), "Entity");
        assert_eq!(l.display_name(l.lookup("texts").unwrap()), "Text");
        assert_eq!(l.display_name(l.lookup("material").unwrap()), "Item Type");
        assert_eq!(l.display_name(l.lookup("num").unwrap()), "Number");
        // Out-of-range ids are not subtype-related to anything.
        assert!(!l.is_subtype(usize::MAX, 0));
        assert!(!l.is_subtype(0, usize::MAX));
    }
}

