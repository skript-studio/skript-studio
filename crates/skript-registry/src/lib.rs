//! SkriptHub syntax corpus -> compiled pattern registry.
//!
//! Input: the projected SkriptHub `/api/v1/addonsyntaxlist/` dump
//! (see `scripts/prepare_corpus.py`; 8712 entries / 174 addons / 13661
//! split patterns as of 2026-11).

pub mod lattice;
pub mod model;
pub mod registry;

pub use lattice::TypeLattice;
pub use model::{AddonInfo, PluginInfo, SyntaxEntry, SyntaxType};
pub use registry::{CompileFailure, CompiledSyntax, Registry, RegistryError};
