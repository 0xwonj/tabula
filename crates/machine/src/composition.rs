//! Composition traits for Tabula's proof system layers.
//!
//! These traits enable customization of how the proof system is assembled.
//! Custom implementations can swap root-proof schemes or commitment methods
//! without modifying the machine layer.

use tabula_chips::execution::ExecutionChip;
use tabula_chips::smt_path::{SmtColPathChip, SmtTablePathChip};
use tabula_chips::static_table::StaticTableChip;
use tabula_stark::air::interaction::BusId;
use tabula_stark::chips::DEFAULT_VALUE_WIDTH;
use tabula_stark::trace::DynChip;

use crate::AnyRap;

// ── Root Proof ──────────────────────────────────────────────────

/// How column commitments are aggregated into a state root.
///
/// The root proof chip(s) take per-column commitment values (via CommitVerif bus)
/// and prove they aggregate into the public `old_root → new_root` transition.
///
/// Current implementation: [`SmtRootProof`] (two-level SMT with column and
/// table path chips).
///
/// Potential alternatives (future):
/// - Accumulator-based root (when algebraic accumulator lands)
/// - Direct commitment list (for small column counts)
pub trait RootProof: Send + Sync {
    /// Produce the AIR(s) that implement this root proof (for proving/verifying).
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;

    /// Produce the chip(s) that implement this root proof (for trace building
    /// and debug validation).
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;

    /// Buses this root proof's chips interact with.
    ///
    /// Custom implementations should declare any non-core buses here.
    fn buses(&self) -> Vec<BusId> {
        vec![]
    }
}

/// SMT root proof (standard per-tier architecture).
///
/// Only includes SmtColPathChip + SmtTablePathChip. ColumnMetaChip is NOT
/// needed — its responsibilities (commitment verification + leaf digest
/// computation) are handled by MetaShardChip within each column proof.
///
/// Leaf digests reach the root tier via the LEAF_DIGEST bus (C15).
#[derive(Debug)]
pub struct SmtRootProof;

impl RootProof for SmtRootProof {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(SmtColPathChip), Box::new(SmtTablePathChip)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![Box::new(SmtColPathChip), Box::new(SmtTablePathChip)]
    }
}

// ── Execution Layer ─────────────────────────────────────────────

/// Fixed execution-layer AIRs for proving/verifying.
pub(crate) fn execution_airs() -> Vec<Box<dyn AnyRap>> {
    vec![
        Box::new(ExecutionChip::<DEFAULT_VALUE_WIDTH>),
        Box::new(StaticTableChip::<DEFAULT_VALUE_WIDTH>),
    ]
}

/// Fixed execution-layer chips for trace building and debug validation.
pub(crate) fn execution_dyn_chips() -> Vec<Box<dyn DynChip>> {
    vec![
        Box::new(ExecutionChip::<DEFAULT_VALUE_WIDTH>),
        Box::new(StaticTableChip::<DEFAULT_VALUE_WIDTH>),
    ]
}
