//! Pluggable per-column commitment schemes for the proof pipeline.
//!
//! [`ColumnScheme`] determines which shard chips are created for each column
//! proof tier. Core implementations ([`SsmcScheme`], [`SmtScheme`]) wrap the
//! existing shard chips; custom schemes can add application-specific chips.
//!
//! The scheme controls **chip instantiation** at setup time, not trace
//! generation at prove time. Trace generation uses the standard [`DynChip`]
//! orchestration pipeline.

use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::property::PropertyVerifierChip;
use tabula_chips::shards::state::StateShardChip;
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;

use crate::AnyRap;
use crate::property::PropertyQueryKind;
use crate::registry::SetupError;
use crate::setup::ColumnSetupConfig;

/// Chips produced by a [`ColumnScheme`] for a single column.
///
/// The setup layer adds PoseidonChip and RangeCheckChip as bus consumers
/// automatically; the scheme only provides domain-specific chips.
pub struct ColumnChipSet {
    /// AIR implementations for proving/verification (registered in ChipRegistry).
    pub airs: Vec<Box<dyn AnyRap>>,
    /// Dynamic chips for phase-ordered trace generation.
    pub dyn_chips: Vec<Box<dyn DynChip>>,
}

/// Pluggable column commitment scheme for chip instantiation.
///
/// Each scheme creates the shard chips needed for a particular commitment
/// strategy. The proof pipeline calls [`create_chips()`](Self::create_chips)
/// once per column during machine setup.
///
/// This trait supersedes [`ColumnCommitment`](tabula_stark::trace::ColumnCommitment)
/// for production use. `ColumnCommitment` bundles chip creation with trace
/// building; this trait handles only chip instantiation, delegating trace
/// building to the standard [`DynChip`] pipeline.
///
/// # Built-in schemes
///
/// - [`SsmcScheme`] — Sorted State Merkle Commitment (4 chips: Memory, State, Meta, PropertyVerifier)
/// - [`SmtScheme`] — Sparse Merkle Tree (2 chips: Memory, Meta)
///
/// # Custom schemes
///
/// Implement this trait to add custom commitment strategies. Custom chips
/// must implement [`AnyRap`] and [`DynChip`] (both via blanket impls).
/// Store witness data in the [`WitnessStore`](tabula_stark::trace::WitnessStore)
/// under custom labels.
pub trait ColumnScheme: Send + Sync {
    /// Human-readable name (e.g., `"ssmc"`, `"smt"`).
    fn name(&self) -> &str;

    /// Structural property queries this scheme can support.
    ///
    /// Returns the set of [`PropertyQueryKind`]s that are structurally
    /// feasible given this commitment strategy. A [`PropertyOpening`]
    /// registered for this scheme must only claim queries within this set.
    ///
    /// The builder validates at setup time:
    /// `opening.supported_queries() ⊆ scheme.supported_property_queries()`
    ///
    /// Default: empty (no structural queries supported).
    fn supported_property_queries(&self) -> &[PropertyQueryKind] {
        &[]
    }

    /// Create shard chips for a single column.
    ///
    /// Called once per column during machine setup. Chip IDs are allocated
    /// sequentially from `alloc` (starts at 100 for shard chips).
    ///
    /// # Errors
    ///
    /// Returns [`SetupError`] if chip creation fails (e.g., invalid config).
    fn create_chips(
        &self,
        config: &ColumnSetupConfig,
        alloc: &mut ChipIdAllocator,
    ) -> Result<ColumnChipSet, SetupError>;
}

/// SSMC commitment scheme: Sorted State Merkle Commitment.
///
/// Creates four shard chips per column:
/// - [`MemoryShardChip<W>`] — inter-tx memory access ordering
/// - [`StateShardChip<W>`] — old/new hash chain state commitments
/// - [`MetaShardChip`] — commitment metadata and leaf digest
/// - [`PropertyVerifierChip<W>`] — structural property query verification
///
/// This is the default scheme for all columns.
pub struct SsmcScheme<const W: usize>;

/// All query kinds structurally supported by SSMC's sorted hash chain.
const SSMC_SUPPORTED_QUERIES: &[PropertyQueryKind] = &[
    PropertyQueryKind::Minimum,
    PropertyQueryKind::Maximum,
    PropertyQueryKind::Successor,
    PropertyQueryKind::Predecessor,
    PropertyQueryKind::NonExistenceRange,
    PropertyQueryKind::Aggregate,
];

impl<const W: usize> ColumnScheme for SsmcScheme<W> {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn supported_property_queries(&self) -> &[PropertyQueryKind] {
        SSMC_SUPPORTED_QUERIES
    }

    fn create_chips(
        &self,
        config: &ColumnSetupConfig,
        alloc: &mut ChipIdAllocator,
    ) -> Result<ColumnChipSet, SetupError> {
        let t = config.table_id.0;
        let c = config.col_id.0;

        let mem_id = alloc.next();
        let state_id = alloc.next();
        let meta_id = alloc.next();
        let prop_id = alloc.next();

        let mem = MemoryShardChip::<W>::new(mem_id, t, c);
        let state = StateShardChip::<W>::new(state_id, t, c);
        let meta = MetaShardChip::new(meta_id, t, c, config.scheme_tag, config.receives_commitment);
        let prop = PropertyVerifierChip::<W>::new(prop_id, t, c);

        Ok(ColumnChipSet {
            airs: vec![
                Box::new(mem.clone()),
                Box::new(state.clone()),
                Box::new(meta.clone()),
                Box::new(prop.clone()),
            ],
            dyn_chips: vec![
                Box::new(mem),
                Box::new(state),
                Box::new(meta),
                Box::new(prop),
            ],
        })
    }
}

/// SMT commitment scheme: Sparse Merkle Tree.
///
/// Creates two shard chips per column:
/// - [`MemoryShardChip<W>`] — inter-tx memory access ordering
/// - [`MetaShardChip`] — commitment metadata and leaf digest
///
/// Unlike SSMC, SMT does NOT include a [`StateShardChip`] — root verification
/// is delegated to the root tier's SmtColPathChip + SmtTablePathChip.
pub struct SmtScheme<const W: usize>;

impl<const W: usize> ColumnScheme for SmtScheme<W> {
    fn name(&self) -> &str {
        "smt"
    }

    // SMT does not override supported_property_queries() — returns empty.
    // SMT keys are hashed, so ordering is lost. Structural queries (min,
    // max, successor) require full tree scan and are not feasible in-circuit.
    // Future: Indexed Merkle Tree variant could support O(log N) successor.

    fn create_chips(
        &self,
        config: &ColumnSetupConfig,
        alloc: &mut ChipIdAllocator,
    ) -> Result<ColumnChipSet, SetupError> {
        let t = config.table_id.0;
        let c = config.col_id.0;

        let mem_id = alloc.next();
        let meta_id = alloc.next();

        let mem = MemoryShardChip::<W>::new(mem_id, t, c);
        let meta = MetaShardChip::new(meta_id, t, c, config.scheme_tag, config.receives_commitment);

        Ok(ColumnChipSet {
            airs: vec![Box::new(mem.clone()), Box::new(meta.clone())],
            dyn_chips: vec![Box::new(mem), Box::new(meta)],
        })
    }
}
