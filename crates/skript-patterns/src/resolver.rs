//! The TypeResolver seam.
//!
//! Java's `TypePatternElement.match` delegates substring parsing to
//! `new SkriptParser(substring, flags, context).parseExpression(exprInfo)`
//! — the full recursive expression parser backed by the live Bukkit type
//! registry. The static LSP cannot run that, so this seam lets the host
//! application plug in whatever expression acceptance strategy it has:
//!
//! - [`PermissiveResolver`] accepts any non-empty substring (good enough
//!   for "does this line match any pattern at all" diagnostics and for
//!   corpus validation).
//! - A future typed resolver will consult the `skript-registry` type
//!   lattice for actual type checking.
//!
//! The resolver is stored in a thread-local, mirroring how Java reaches
//! the parser through the `ParserInstance.get()` thread-local.

use std::cell::RefCell;
use std::rc::Rc;

use crate::match_result::{ParseFlags, SlotMatch, TypeSpec};
use crate::parse_context::ParseContext;

/// Decides whether a `%type%` slot can consume a substring.
pub trait TypeResolver {
    /// Attempt to parse `substring` as an expression acceptable for the
    /// given type specs. Return `Some(slot)` on success.
    fn parse_expression(
        &self,
        substring: &str,
        specs: &[TypeSpec],
        effective_flags: ParseFlags,
        context: ParseContext,
    ) -> Option<SlotMatch>;
}

/// Accepts any non-empty (after trim) substring — the v1 static-analysis
/// strategy.
pub struct PermissiveResolver;

impl TypeResolver for PermissiveResolver {
    fn parse_expression(
        &self,
        substring: &str,
        specs: &[TypeSpec],
        _effective_flags: ParseFlags,
        _context: ParseContext,
    ) -> Option<SlotMatch> {
        if substring.trim().is_empty() {
            return None;
        }
        Some(SlotMatch {
            text: substring.to_string(),
            type_names: specs.iter().map(|s| s.code_name.clone()).collect(),
        })
    }
}

thread_local! {
    static TYPE_RESOLVER: RefCell<Option<Rc<dyn TypeResolver>>> = const { RefCell::new(None) };
}

/// Installs the resolver for the current thread.
pub fn set_type_resolver(resolver: Rc<dyn TypeResolver>) {
    TYPE_RESOLVER.with(|cell| *cell.borrow_mut() = Some(resolver));
}

/// Runs `f` with the active resolver (falls back to
/// [`PermissiveResolver`] when none was installed).
pub fn with_type_resolver<T>(f: impl FnOnce(&dyn TypeResolver) -> T) -> T {
    TYPE_RESOLVER.with(|cell| {
        let borrow = cell.borrow();
        match borrow.as_ref() {
            Some(resolver) => f(resolver.as_ref()),
            None => f(&PermissiveResolver),
        }
    })
}
