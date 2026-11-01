//! The type lattice derived from the corpus.
//!
//! OWNER: Agent 8.
//!
//! Built from (a) the 660 `syntax_type == "type"` entries (their patterns
//! are plain type names like `folder|dir|box` or `[the] text[s]`), and
//! (b) the `return_type` strings of expression entries. Provides tolerant
//! lookup (case-insensitive, singular/plural) and a small hardcoded
//! hierarchy for subtype checks.

use crate::registry::Registry;

/// Opaque type id (index into the lattice's type table).
pub type TypeId = usize;

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
}

impl TypeLattice {
    /// Build the lattice from a loaded (not necessarily compiled) registry.
    pub fn build(registry: &Registry) -> TypeLattice {
        todo!("Agent 8: harvest type names + return types, normalize, index")
    }

    /// Tolerant lookup: case-insensitive, singular/plural-insensitive,
    /// alias-aware. Returns the type id.
    pub fn lookup(&self, name: &str) -> Option<TypeId> {
        todo!("Agent 8")
    }

    /// Canonical display name of a type id.
    pub fn display_name(&self, id: TypeId) -> &str {
        &self.display[id]
    }

    /// Whether `sub` is a (transitive) subtype of `sup` (reflexive).
    pub fn is_subtype(&self, sub: TypeId, sup: TypeId) -> bool {
        todo!("Agent 8: hardcoded hierarchy + reflexive closure")
    }

    /// Number of known types.
    pub fn known_types(&self) -> usize {
        self.display.len()
    }
}
