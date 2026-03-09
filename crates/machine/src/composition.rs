//! Composition traits for Tabula's proof system layers.
//!
//! These traits enable customization of how Tabula's proof system is assembled.
//! Custom implementations can swap memory consistency strategies, root-proof
//! schemes, or commitment methods without modifying the machine layer.
//!
//! # Layered architecture
//!
//! ```text
//! Layer 0: Core (fixed — Tabula's identity)
//!   Execution:      ExecutionChip, StaticTableChip
//!   Memory:         InterTxOrderChip  (MemoryModel)
//!   Root Proof:     ColumnMetaChip, SmtColPathChip, SmtTablePathChip (RootProof)
//!   Bus Consumers:  PoseidonChip, RangeCheckChip
//!
//! Layer 1: Column Commitment (pluggable via CommitmentScheme)
//!   "ssmc" → SsmcScheme → StateColumnChip (global)
//!   "smt"  → SmtScheme  → (no extra chip)
//!   custom → impl CommitmentScheme → custom chips
//! ```
//!
//! All traits expose two parallel methods:
//! - `airs()` — `Vec<Box<dyn AnyRap>>` for proving/verifying
//! - `dyn_chips()` — `Vec<Box<dyn DynChip>>` for trace building

use tabula_chips::column_meta::ColumnMetaChip;
use tabula_chips::execution::ExecutionChip;
use tabula_chips::inter_tx_order::InterTxOrderChip;
use tabula_chips::smt_path::{SmtColPathChip, SmtTablePathChip};
use tabula_chips::state_column::StateColumnChip;
use tabula_chips::static_table::StaticTableChip;
use tabula_stark::air::interaction::BusId;
use tabula_stark::chips::DEFAULT_VALUE_WIDTH;
use tabula_stark::trace::DynChip;

use crate::AnyRap;

// ── Memory Model ────────────────────────────────────────────────

/// How inter-transaction memory consistency is enforced.
///
/// The memory model chip(s) receive access records from the execution layer
/// and prove that all reads and writes are consistent with the state transition.
///
/// Current implementation: [`GlobalSortedMemory`] (InterTxOrderChip — sorted
/// access log with LogUp multiset argument).
///
/// Potential alternatives (future):
/// - Permutation-based memory (Cairo-style)
/// - Per-table memory partitioning
/// - GKR/sum-check based (Jolt-style)
pub trait MemoryModel: Send + Sync {
    /// Produce the AIR(s) that implement this memory model (for proving/verifying).
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;

    /// Produce the chip(s) that implement this memory model (for trace building
    /// and debug validation).
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;

    /// Buses this model's chips interact with.
    ///
    /// Custom implementations should declare any non-core buses here so the
    /// machine builder includes them in validation automatically.
    fn buses(&self) -> Vec<BusId> {
        vec![]
    }
}

/// Global sorted memory: single InterTxOrderChip processing all columns.
///
/// All memory accesses are sorted by (table, col, row, tx_index) in one chip.
/// Consistency is proven via LogUp multiset argument against execution sends.
#[derive(Debug)]
pub struct GlobalSortedMemory;

impl MemoryModel for GlobalSortedMemory {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(InterTxOrderChip::<DEFAULT_VALUE_WIDTH>)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![Box::new(InterTxOrderChip::<DEFAULT_VALUE_WIDTH>)]
    }
}

// ── Root Proof ──────────────────────────────────────────────────

/// How column commitments are aggregated into a state root.
///
/// The root proof chip(s) take per-column commitment values (via CommitVerif bus)
/// and prove they aggregate into the public `old_root → new_root` transition.
///
/// Current implementation: [`SmtRootProof`] (two-level SMT with column and table
/// path chips).
///
/// Potential alternatives (future):
/// - Accumulator-based root (when D2+D3 algebraic accumulator lands)
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

/// SMT root proof: ColumnMetaChip + SmtColPathChip + SmtTablePathChip.
///
/// Proves `old_root → new_root` via Sparse Merkle Tree inclusion/update proofs
/// at both the column level (per table) and the table level (global).
#[derive(Debug)]
pub struct SmtRootProof;

impl RootProof for SmtRootProof {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![
            Box::new(ColumnMetaChip),
            Box::new(SmtColPathChip),
            Box::new(SmtTablePathChip),
        ]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![
            Box::new(ColumnMetaChip),
            Box::new(SmtColPathChip),
            Box::new(SmtTablePathChip),
        ]
    }
}

// ── Commitment Scheme ─────────────────────────────────────────

/// How a column's state transitions are committed and proven.
///
/// Each scheme provides AIR chips (for proving/verifying) and DynChips
/// (for trace building). The scheme's chips interact via the bus protocol:
/// - **Receive** from the Memory bus (access records for the column)
/// - **Send** on the CommitVerif bus (old_com, new_com, is_touched)
///
/// Current implementations:
/// - [`SsmcScheme`]: Global StateColumnChip with sorted-list + hash chain
/// - [`SmtScheme`]: No additional chips — root verification in Layer 0
///
/// Custom schemes can implement this trait to plug into the builder via
/// [`MachineBuilder::with_commitment()`](crate::MachineBuilder::with_commitment).
pub trait CommitmentScheme: Send + Sync {
    /// Produce the AIR(s) that implement this commitment scheme.
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;

    /// Produce the chip(s) for trace building and debug validation.
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;

    /// Buses this scheme's chips interact with.
    ///
    /// Custom schemes should declare any non-core buses here so the
    /// machine builder includes them in validation automatically.
    fn buses(&self) -> Vec<BusId> {
        vec![]
    }
}

/// SSMC commitment scheme: global StateColumnChip with sorted-list hash chains.
///
/// This is the default commitment for all columns. StateColumnChip processes
/// all SSMC columns in a single chip (fixed width, optimal proof size).
#[derive(Debug)]
pub struct SsmcScheme;

impl CommitmentScheme for SsmcScheme {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(StateColumnChip::<DEFAULT_VALUE_WIDTH>)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![Box::new(StateColumnChip::<DEFAULT_VALUE_WIDTH>)]
    }
}

/// SMT commitment scheme: no additional chips.
///
/// For SMT-committed columns, root verification is handled by the Layer 0
/// root proof chips (ColumnMetaChip, SmtColPathChip, SmtTablePathChip).
/// No extra commitment-layer chip is needed.
#[derive(Debug)]
pub struct SmtScheme;

impl CommitmentScheme for SmtScheme {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![]
    }
}

// ── Execution Layer ─────────────────────────────────────────────

/// Fixed execution-layer AIRs for proving/verifying (not behind a trait — these
/// ARE Tabula).
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
