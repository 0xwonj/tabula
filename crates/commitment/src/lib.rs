//! Native commitment and state-root computation for the Tabula proof stack.
//!
//! This crate owns out-of-circuit commitment semantics:
//! - Poseidon2 over KoalaBear
//! - Sparse Merkle Trees (SMT)
//! - Small Sparse Map Commitments (SSMC)
//! - canonical per-column root bindings and global state-root computation
//!
//! `tabula-runtime` and `tabula-witness` prepare proving inputs around these
//! native commitment products. `tabula-chips` and `tabula-machine` constrain
//! and prove the same computations in-circuit.
//!
//! Public module layout:
//! - [`primitives`]: shared commitment primitives: digests, hashers, codecs, and domain/depth constants
//! - [`schemes`]: scheme-specific native commitment implementations (`smt`, `ssmc`, `tags`)
//! - `roots`: private column/table/global root-binding helpers exposed through
//!   the crate-level re-exports
//!
//! All meaningful native APIs are behind the `stark` feature flag. Without it,
//! the crate remains only a minimal shell.
//!
//! SMT internal nodes intentionally use the same plain 2-to-1 Poseidon compression
//! checked by the current proof chips. Tree/domain separation comes
//! from domain-specific empty-leaf seeding and distinct leaf/table/column
//! bindings, not from an additional per-node domain tag.

#[cfg(feature = "stark")]
mod binding;
#[cfg(feature = "stark")]
/// Shared commitment primitives: digests, hashers, codecs, and constants.
pub mod primitives;
#[cfg(feature = "stark")]
/// Column/table/global root-binding helpers.
mod roots;
#[cfg(feature = "stark")]
/// Scheme-specific native commitment implementations and builtin scheme tags.
pub mod schemes;

#[cfg(feature = "stark")]
pub use binding::{ColumnRootBinding, NormalizedVerifierDigest};
#[cfg(feature = "stark")]
pub use primitives::{FieldHasher, NativeDigest, PoseidonHasher};
#[cfg(feature = "stark")]
pub use roots::compute_column_root_binding_leaf;
#[cfg(feature = "stark")]
pub use roots::compute_column_root_binding_prefix_digest;
#[cfg(feature = "stark")]
pub use roots::compute_state_roots_from_bindings;
