//! Skript type validation.
//!
//! When a pattern placeholder like `%player%` captures a piece of user code,
//! we sanity-check that the captured text *looks like* that Skript type. This
//! is intentionally fuzzy: Skript is a dynamic, expression-composable language
//! and we can't statically know whether `{_target}` resolves to a player. So
//! the rules here accept anything that *could* be the type, and reject only
//! things that clearly can't (e.g. a bare alphabetic word where a number is
//! required).
//!
//! Unknown types (`%something_weird%`) default to accepting anything, since
//! addons can introduce arbitrary types.

use regex::{Regex, RegexBuilder};
use std::sync::OnceLock;

/// Outcome of validating a captured value against a Skript type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCheck {
    /// The value is consistent with the type.
    Ok,
    /// The value is *very likely* wrong for this type.
    Mismatch(String),
}

/// Validate `value` against a Skript type name (the inner text of `%type%`).
///
/// Type names are matched case-insensitively. Compound names like
/// `entity/location` accept a value if *any* component accepts it. `*` (the
/// catch-all) always accepts.
pub fn check_type(ty: &str, value: &str) -> TypeCheck {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return TypeCheck::Mismatch("expected a value, found nothing".to_owned());
    }

    // Catch-all and unknown types accept anything.
    if ty == "*" || ty.eq_ignore_ascii_case("object") || ty.eq_ignore_ascii_case("objects") {
        return TypeCheck::Ok;
    }

    // Compound types: accept if any alternative accepts.
    for part in ty.split('/') {
        if let TypeCheck::Ok = check_single_type(part.trim(), trimmed) {
            return TypeCheck::Ok;
        }
    }
    TypeCheck::Mismatch(format!("value {trimmed:?} does not look like a {ty}"))
}

fn check_single_type(ty: &str, value: &str) -> TypeCheck {
    if ty.is_empty() || ty == "*" {
        return TypeCheck::Ok;
    }
    let norm = normalize_type_name(ty);
    match norm.as_str() {
        "number" | "integer" | "long" | "short" | "byte" => check_number(value),
        "boolean" => check_boolean(value),
        // Text-like: anything is fine, but quoted strings must be balanced.
        "text" | "string" => check_text(value),
        // Player/entity-ish: identifiers, quoted names, or variables.
        "player" | "offlineplayer" | "commandsender" | "human" | "humanentity" => check_name(value),
        "entity" | "entitydata" | "entitytype" | "entitytypes" | "livingentity" => check_name(value),
        "location" => check_location(value),
        // Everything else (item, world, material, ...) is too permissive to
        // constrain; accept anything non-empty.
        _ => TypeCheck::Ok,
    }
}

/// Lowercase, strip a trailing `s`, drop `-type`/`s` decorators.
fn normalize_type_name(ty: &str) -> String {
    let lower = ty.trim().to_ascii_lowercase();
    // Plurals and a couple of common decorators.
    let stripped = lower
        .trim_end_matches("s")
        .trim_end_matches("-type")
        .trim_end_matches(" type");
    if stripped.is_empty() {
        lower
    } else {
        stripped.to_owned()
    }
}

fn check_number(value: &str) -> TypeCheck {
    // Accept plain numerics, signed, with optional decimal/exponent.
    if value
        .trim()
        .parse::<f64>()
        .is_ok()
        || number_expr_regex().is_match(value.trim())
    {
        TypeCheck::Ok
    } else {
        TypeCheck::Mismatch(format!("{value:?} is not a number"))
    }
}

fn check_boolean(value: &str) -> TypeCheck {
    let lower = value.trim().to_ascii_lowercase();
    match lower.as_str() {
        "true" | "false" | "yes" | "no" | "on" | "off" => TypeCheck::Ok,
        _ => TypeCheck::Mismatch(format!("{value:?} is not a boolean")),
    }
}

fn check_text(value: &str) -> TypeCheck {
    // Quoted strings must be balanced; bare text is always acceptable as text.
    let t = value.trim();
    if (t.starts_with('"') && !t.ends_with('"'))
        || (t.starts_with("''") && !t.ends_with("''"))
    {
        return TypeCheck::Mismatch("unterminated string literal".to_owned());
    }
    TypeCheck::Ok
}

fn check_name(value: &str) -> TypeCheck {
    let t = value.trim();
    // Variables, expressions in parens, quoted names, plain identifiers.
    if t.starts_with('{') && t.ends_with('}')
        || t.starts_with('(') && t.ends_with(')')
        || t.starts_with('"') && t.ends_with('"')
        || t.starts_with("''") && t.ends_with("''")
        || identifier_regex().is_match(t)
    {
        TypeCheck::Ok
    } else {
        TypeCheck::Mismatch(format!("{value:?} does not look like an identifier or variable"))
    }
}

fn check_location(value: &str) -> TypeCheck {
    let t = value.trim();
    // `(world, x, y, z)`-ish, a variable, or a bare "world x, y, z".
    if t.starts_with('{') && t.ends_with('}')
        || t.starts_with('(') && t.ends_with(')')
        || location_regex().is_match(t)
    {
        TypeCheck::Ok
    } else {
        // Be lenient: many things coerce to a location.
        TypeCheck::Ok
    }
}

// --- compiled regexes, built once and cached -------------------------------

fn number_expr_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        RegexBuilder::new(r"^-?\d+(\.\d+)?(e[+-]?\d+)?$")
            .case_insensitive(true)
            .build()
            .unwrap()
    })
}

fn identifier_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap())
}

fn location_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // e.g. `world named "x" at 1, 2, 3` or `1, 2, 3` or `1 2 3`
        Regex::new(r"^[A-Za-z_][A-Za-z0-9_ ]*[-+]?\d+[, ]+[-+]?\d+[, ]+[-+]?\d+").unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_validate() {
        assert_eq!(check_type("number", "42"), TypeCheck::Ok);
        assert_eq!(check_type("number", "-3.14"), TypeCheck::Ok);
        assert_eq!(check_type("number", "1e10"), TypeCheck::Ok);
        assert!(matches!(check_type("number", "Steve"), TypeCheck::Mismatch(_)));
    }

    #[test]
    fn booleans_validate() {
        assert_eq!(check_type("boolean", "true"), TypeCheck::Ok);
        assert_eq!(check_type("boolean", "no"), TypeCheck::Ok);
        assert!(matches!(check_type("boolean", "maybe"), TypeCheck::Mismatch(_)));
    }

    #[test]
    fn names_validate() {
        assert_eq!(check_type("player", "Steve"), TypeCheck::Ok);
        assert_eq!(check_type("player", "{_p}"), TypeCheck::Ok);
        assert_eq!(check_type("player", "(event-player)"), TypeCheck::Ok);
        assert_eq!(check_type("player", "\"Notch\""), TypeCheck::Ok);
    }

    #[test]
    fn compound_types_accept_either() {
        assert_eq!(check_type("entity/location", "Steve"), TypeCheck::Ok);
        assert_eq!(check_type("entity/location", "1, 2, 3"), TypeCheck::Ok);
    }

    #[test]
    fn catchall_accepts_anything() {
        assert_eq!(check_type("*", "anything at all"), TypeCheck::Ok);
        assert_eq!(check_type("object", "anything at all"), TypeCheck::Ok);
    }

    #[test]
    fn unknown_type_is_permissive() {
        assert_eq!(check_type("madeuptype", "whatever"), TypeCheck::Ok);
    }

    #[test]
    fn unterminated_string_is_mismatch_for_text() {
        assert!(matches!(check_type("text", "\"oops"), TypeCheck::Mismatch(_)));
        assert_eq!(check_type("text", "\"fine\""), TypeCheck::Ok);
    }

    #[test]
    fn empty_value_is_mismatch() {
        assert!(matches!(check_type("number", "   "), TypeCheck::Mismatch(_)));
    }
}
