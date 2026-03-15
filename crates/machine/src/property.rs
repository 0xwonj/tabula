//! Structural property queries on committed column state.
//!
//! The [`PropertyOpening`] trait enables provable queries like "minimum key",
//! "successor key", or "non-existence range" on committed columns. Each
//! implementation is paired with a compatible [`ColumnScheme`](crate::ColumnScheme)
//! and provides both the query result and a cryptographic witness.
//!
//! # Architecture
//!
//! PropertyRead is a **cross-tier** operation. The ExecutionChip (Tier 1) sends
//! a query on the `PROPERTY_READ` external bus. A PropertyVerifierChip in
//! **Tier 2** (column proof) receives and verifies the result against the
//! column's pre-batch committed state (com_old).
//!
//! ```text
//! Tier 1 (Execution):
//!   ExecutionChip SENDS on PROPERTY_READ bus
//!     → (table_id, col_id, query_type, result_key[W], result_val[W], is_null)
//!
//! Tier 2 (Column):
//!   PropertyVerifierChip RECEIVES from PROPERTY_READ bus
//!     → Verifies result against column commitment (com_old)
//!
//! Tier 3 (Root):
//!   Verifies PROPERTY_READ bus balance across tiers
//! ```
//!
//! # Extension Integration
//!
//! Register property openings via [`MachineBuilder::with_property_opening()`]:
//!
//! ```ignore
//! TabulaMachine::builder()
//!     .with_property_opening(OrderbookMinOpening)
//!     .build()?;
//! ```

use std::any::Any;

use p3_koala_bear::KoalaBear;
use tabula_core::RowKey;

use crate::extension::ChipExtension;

// ── Query Types (re-exported from IR) ────────────────────────────

pub use tabula_ir::{AggregateKind, PropertyQuery, PropertyQueryKind};

// ── Witness ──────────────────────────────────────────────────────

/// Opaque cryptographic witness for a property query result.
///
/// Implementations carry the query result value plus a proof that the
/// result is correct with respect to the committed column state. The
/// proof is verified by the property chip's AIR constraints in Tier 2.
///
/// Witnesses are stored in [`WitnessStore`](tabula_stark::trace::WitnessStore)
/// and consumed by the extension's property chip during trace building.
pub trait PropertyWitness: Send + Sync {
    /// The query result as field elements.
    ///
    /// Encoding matches the column's [`EncodingWidth`](tabula_stark::trace::EncodingWidth).
    fn value(&self) -> &[KoalaBear];

    /// The key satisfying the property (e.g., the minimum key).
    ///
    /// Returns `None` for aggregate queries (Sum/Count) where no single
    /// key is associated with the result.
    fn key(&self) -> Option<RowKey>;

    /// Whether the result is null (no matching key found).
    fn is_null(&self) -> bool;

    /// Downcast to a concrete witness type for chip-specific processing.
    fn as_any(&self) -> &dyn Any;
}

// ── PropertyOpening Trait ────────────────────────────────────────

/// Pluggable structural query implementation for committed columns.
///
/// Each implementation is paired with a specific [`ColumnScheme`](crate::ColumnScheme)
/// (identified by scheme tag) and knows how to:
/// 1. Answer structural queries (min, max, successor, etc.)
/// 2. Produce a cryptographic witness proving the answer is correct
/// 3. Optionally provide verifier chips via [`column_verifier()`](Self::column_verifier)
///
/// # Cross-Tier Verification
///
/// Verifier chips run in **Tier 2 (column proof)**, not Tier 1 (execution).
/// Column state (sorted chains, Merkle paths, commitments) lives in Tier 2,
/// so verifiers must be placed there. The `PROPERTY_READ` external bus carries
/// query results from Tier 1 to Tier 2 for verification.
///
/// # Soundness
///
/// The [`PropertyWitness`] returned by [`prove()`](Self::prove) **must** contain
/// a cryptographic proof, not just a value. The verifier chips (returned by
/// [`column_verifier()`](Self::column_verifier)) verify this proof against
/// the column commitment digest via AIR constraints in Tier 2.
///
/// If your opening requires custom verification chips, you **must** return
/// them via [`column_verifier()`](Self::column_verifier). The builder
/// registers them in the column tier automatically. Omitting verifier chips
/// for an opening that produces non-trivial witnesses creates an unsound proof.
///
/// # State Semantics
///
/// All queries operate on **pre-batch committed state (com_old)**, providing
/// snapshot isolation. The in-flight overlay state has no commitment and
/// cannot be verified in ZK.
///
/// # Example
///
/// ```ignore
/// use tabula_machine::property::*;
///
/// struct OrderbookMinOpening;
///
/// impl PropertyOpening for OrderbookMinOpening {
///     fn name(&self) -> &str { "orderbook-min" }
///     fn compatible_scheme_tag(&self) -> u16 { scheme_tags::SSMC }
///     fn supported_queries(&self) -> &[PropertyQueryKind] {
///         &[PropertyQueryKind::Minimum, PropertyQueryKind::Maximum]
///     }
///     fn prove(
///         &self,
///         commitment_digest: &[KoalaBear],
///         query: &PropertyQuery,
///         state: &[(RowKey, &[KoalaBear], bool)],
///     ) -> Result<Box<dyn PropertyWitness>, PropertyError> {
///         // ... produce witness proving result against committed state
///     }
///     fn column_verifier(&self) -> Option<Box<dyn ChipExtension>> {
///         Some(Box::new(OrderbookMinVerifier))  // Runs in Tier 2
///     }
/// }
/// ```
pub trait PropertyOpening: Send + Sync {
    /// Human-readable name for this opening type.
    fn name(&self) -> &str;

    /// Scheme tag of the compatible [`ColumnScheme`](crate::ColumnScheme).
    ///
    /// Must match one of the scheme tags registered via
    /// [`MachineBuilder::with_column_scheme()`](crate::MachineBuilder::with_column_scheme).
    /// Uses compile-time constants (e.g., `scheme_tags::SSMC`) for type safety.
    fn compatible_scheme_tag(&self) -> u16;

    /// Which query kinds this implementation supports.
    fn supported_queries(&self) -> &[PropertyQueryKind];

    /// Prove a structural property about the committed column state.
    ///
    /// Queries operate on **pre-batch committed state (com_old)**.
    ///
    /// # Arguments
    ///
    /// - `commitment_digest` — the column's pre-batch commitment (com_old)
    /// - `query` — the structural query to answer
    /// - `state` — pre-batch column state as `(row_key, value_fes, is_null)` tuples
    ///
    /// # Errors
    ///
    /// Returns [`PropertyError`] if the query is unsupported or the state
    /// is inconsistent with the commitment.
    fn prove(
        &self,
        commitment_digest: &[KoalaBear],
        query: &PropertyQuery,
        state: &[(RowKey, &[KoalaBear], bool)],
    ) -> Result<Box<dyn PropertyWitness>, PropertyError>;

    /// Verifier chips for the **column tier** (Tier 2).
    ///
    /// These chips receive from the `PROPERTY_READ` external bus and verify
    /// the query result against the column commitment (com_old). They run in
    /// Tier 2 where column state is accessible, not in Tier 1 (execution).
    ///
    /// If this returns `Some`, the builder automatically registers the
    /// extension's chips in the column tier setup. The extension's AIR
    /// constraints must verify the [`PropertyWitness`] against the column
    /// commitment digest.
    ///
    /// Return `None` if the opening's witnesses are verified by existing core
    /// chips (rare — most non-trivial openings need custom verifier chips).
    fn column_verifier(&self) -> Option<Box<dyn ChipExtension>> {
        None
    }
}

/// Errors from property opening operations.
#[derive(Debug, thiserror::Error)]
pub enum PropertyError {
    /// The query kind is not supported by this implementation.
    #[error("unsupported query kind: {kind:?} (supported: {supported:?})")]
    UnsupportedQuery {
        /// The query kind that was requested.
        kind: PropertyQueryKind,
        /// The query kinds this implementation supports.
        supported: Vec<PropertyQueryKind>,
    },
    /// The commitment scheme doesn't match this implementation.
    #[error("incompatible scheme tag: expected {expected}, got {actual}")]
    IncompatibleSchemeTag {
        /// The scheme tag this implementation expects.
        expected: u16,
        /// The scheme tag that was provided.
        actual: u16,
    },
    /// No property opening registered for the given scheme and query kind.
    #[error("no property opening for scheme tag {scheme_tag} supporting {query_kind:?}")]
    NoOpeningRegistered {
        /// The scheme tag that was queried.
        scheme_tag: u16,
        /// The query kind that was requested.
        query_kind: PropertyQueryKind,
    },
    /// Internal error during proof generation.
    #[error("property proof failed: {detail}")]
    ProofFailed {
        /// Description of the proof failure.
        detail: String,
    },
}
