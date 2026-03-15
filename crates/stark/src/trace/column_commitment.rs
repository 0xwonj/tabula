//! Pluggable per-column commitment scheme traits and proof plan types.
//!
//! The [`ColumnCommitment`] trait defines the proof-side interface for a column's
//! state commitment. Each implementation produces AIR shard chips and trace data
//! for a particular strategy (SSMC, SMT, or custom).
//!
//! The [`BusConsumer`] trait enables bus-driven dependent chip collection,
//! replacing hardcoded Poseidon/RangeCheck interaction scanning.
//!
//! # Architecture
//!
//! Bus protocol is the interface boundary:
//! - Column commitment schemes **receive** from the Memory bus
//! - Column commitment schemes **send** on the CommitVerif bus
//! - Bus consumers collect interactions emitted by other chips
//!
//! ```text
//! Layer 0: Execution (fixed)
//!   └─ sends on Memory, PoseidonPerm, RangeCheck buses
//! Layer 1: ColumnCommitment (pluggable, per-column)
//!   └─ receives Memory bus, sends CommitVerif bus
//! Layer 2: Root proof (fixed)
//!   └─ ColumnMeta, SmtTablePath
//! Layer 3: BusConsumer (bus-driven)
//!   └─ Poseidon, RangeCheck, ...custom
//! ```

use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::air::interaction::BusId;
use crate::chips::ChipId;
use crate::debug::RecordedInteraction;

use super::contributor::WitnessStore;
use super::trace_map::TraceEntry;

// ── Column Plan ──────────────────────────────────────────────────

/// Value encoding width in field elements.
///
/// Determines the `W` const generic for parameterized chips.
/// This is an open integer — application types can define custom widths
/// (e.g., `EncodingWidth(5)`) beyond the well-known constants.
///
/// Well-known widths match the value encoding spec:
/// `w(Bool)=1, w(U64)=w(I64)=3, w(Bytes32)=8`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EncodingWidth(pub usize);

impl EncodingWidth {
    /// 1 field element (Bool).
    pub const BOOL: Self = Self(1);
    /// 3 field elements (U64, I64).
    pub const STANDARD: Self = Self(3);
    /// 8 field elements (Bytes32, Digest).
    pub const WIDE: Self = Self(8);

    /// The inner width value.
    pub const fn width(self) -> usize {
        self.0
    }
}

/// Proof plan metadata for a single column.
///
/// Describes which commitment scheme and encoding width a column uses.
/// Generated during [`ProofPlan::generate()`].
#[derive(Clone, Debug)]
pub struct ColumnPlan {
    /// Table this column belongs to.
    pub table: TableId,
    /// Column within the table.
    pub col: ColId,
    /// Value encoding width in field elements.
    pub encoding_width: EncodingWidth,
    /// Name of the commitment scheme (e.g., `"ssmc"`, `"smt"`).
    pub scheme_name: String,
}

// ── Proof Plan ───────────────────────────────────────────────────

/// Per-batch proof plan mapping each column to a commitment scheme.
///
/// Generated from `BatchWitness` + schema metadata. The plan drives:
/// - Which shard chips are registered
/// - How witness data is routed per-column
/// - Which columns can be built in parallel
#[derive(Clone, Debug)]
pub struct ProofPlan {
    /// Per-column plans, in deterministic order.
    columns: Vec<ColumnPlan>,
}

impl ProofPlan {
    /// Create a proof plan from a list of column plans.
    pub fn new(columns: Vec<ColumnPlan>) -> Self {
        Self { columns }
    }

    /// All column plans.
    pub fn columns(&self) -> &[ColumnPlan] {
        &self.columns
    }

    /// Find the plan for a specific column.
    pub fn get(&self, table: TableId, col: ColId) -> Option<&ColumnPlan> {
        self.columns
            .iter()
            .find(|p| p.table == table && p.col == col)
    }

    /// Number of columns in the plan.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Whether the plan is empty (no columns).
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Override the commitment scheme for a specific column.
    ///
    /// Used by `MachineBuilder::with_proof_plan_override()` to allow
    /// per-column customization.
    pub fn set_scheme(&mut self, table: TableId, col: ColId, scheme: &str) {
        if let Some(plan) = self
            .columns
            .iter_mut()
            .find(|p| p.table == table && p.col == col)
        {
            plan.scheme_name = scheme.to_string();
        }
    }
}

// ── Column Commitment Trait ──────────────────────────────────────

/// Pluggable column commitment scheme (batch API).
///
/// **Note**: For production use, prefer [`ColumnScheme`](tabula_machine::ColumnScheme)
/// which separates chip instantiation from trace building. This trait
/// bundles both responsibilities and is retained for test-level usage.
///
/// Each implementation knows how to produce AIR chips and trace data
/// for a particular commitment strategy (SSMC, SMT, or custom).
///
/// The batch API receives **all columns of this scheme at once**, enabling both:
/// - **Global-style**: One chip processes all columns (fixed width, optimal proof size)
/// - **Shard-style**: Per-column chips (width scales with C, but parallelizable)
///
/// Implementations are object-safe — [`WitnessStore`] is used for data exchange
/// instead of associated types.
///
/// # Bus protocol
///
/// All implementations must:
/// - **Receive** from the Memory bus (access records for the column)
/// - **Send** on the CommitVerif bus (old_com, new_com, is_touched)
///
/// Internal bus usage (e.g., PoseidonPerm for hash chain) is free.
pub trait ColumnCommitment: Send + Sync {
    /// Human-readable name (e.g., `"ssmc"`, `"smt"`).
    fn name(&self) -> &str;

    /// All chip IDs that this scheme produces across all its columns.
    ///
    /// For global-style implementations, this is a small fixed set.
    /// For shard-style, this is N chips per column.
    fn chip_ids(&self) -> Vec<ChipId>;

    /// Build traces for all columns of this scheme.
    ///
    /// `cols` contains all [`ColumnPlan`]s assigned to this scheme.
    /// Returns `(ChipId, TraceEntry)` pairs that get merged into the global trace map.
    ///
    /// Global-style implementations process all columns in one trace.
    /// Shard-style implementations iterate and build per-column traces.
    fn build_traces(
        &self,
        cols: &[ColumnPlan],
        store: &WitnessStore,
    ) -> Result<Vec<(ChipId, TraceEntry)>, TabulaError>;

    /// Buses this scheme sends on (for downstream [`BusConsumer`] resolution).
    ///
    /// Must include at least `COMMITMENT_VERIF`. May also include
    /// `POSEIDON_PERM`, `RANGE_CHECK`, etc.
    fn output_buses(&self) -> Vec<BusId>;
}

// ── Bus Consumer Trait ───────────────────────────────────────────

/// Bus-driven dependent chip: collects interaction data from upstream chips.
///
/// Replaces hardcoded `collect_poseidon_inputs` / `collect_range_check_multiplicities`
/// in orchestration. Any chip (core or extension) can implement this to declare
/// which buses it depends on and how to collect the data.
///
/// # Lifecycle
///
/// 1. Layer 0+1 chip traces are built
/// 2. Orchestrator evaluates those traces to extract [`RecordedInteraction`]s
/// 3. For each `BusConsumer`, interactions matching [`consumed_buses`](Self::consumed_buses)
///    are filtered and passed to [`collect`](Self::collect)
/// 4. Consumer stores its collected data in [`WitnessStore`]
/// 5. Consumer's trace is built from the stored data
pub trait BusConsumer: Send + Sync {
    /// Which buses this consumer reads from.
    fn consumed_buses(&self) -> Vec<BusId>;

    /// Collect interaction records and populate the [`WitnessStore`].
    ///
    /// `interactions` contains all send-direction interactions on the consumed buses,
    /// aggregated from all upstream chip traces.
    fn collect(
        &self,
        interactions: &[RecordedInteraction<KoalaBear>],
        store: &mut WitnessStore,
    ) -> Result<(), TabulaError>;
}
