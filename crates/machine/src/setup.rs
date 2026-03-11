//! Per-tier proof setup for the multi-proof architecture.
//!
//! Creates [`ChipRegistry`], keys, and chip sets for each tier in the C+2
//! proof architecture (1 execution + C column + 1 root).
//!
//! Each tier receives a self-contained [`TierSetup`] with:
//! - A [`ChipRegistry`] of AIR implementations for proving/verification
//! - [`TabulaProvingKey`] and [`TabulaVerifyingKey`] (keygen cached)
//! - [`DynChip`] list for phase-ordered trace building
//! - [`BusConsumer`] list for interaction-driven trace building
//!
//! # Per-tier chip sets
//!
//! | Tier | Chips |
//! |------|-------|
//! | Execution | ExecutionChip, StaticTableChip, PoseidonChip, RangeCheckChip |
//! | Column | MemoryShardChip, StateShardChip, MetaShardChip, PoseidonChip, RangeCheckChip |
//! | Root | SmtColPathChip, SmtTablePathChip, PoseidonChip, RangeCheckChip |
//!
//! PoseidonChip and RangeCheckChip are included in every tier because bus
//! interactions (Poseidon permutation requests, range checks) must balance
//! within each independent proof.

use std::collections::BTreeMap;

use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::state::StateShardChip;
use tabula_core::{ColId, TableId};
use tabula_stark::chips::{ChipIdAllocator, DEFAULT_VALUE_WIDTH};
use tabula_stark::trace::{BusConsumer, DynChip};

use crate::composition::{RootProof, SmtRootProof, execution_dyn_chips};
use crate::keys::{TabulaProvingKey, TabulaVerifyingKey};
use crate::registry::{ChipRegistry, SetupError};

/// Per-tier chip setup for one proof instance.
///
/// Contains everything needed to build traces and prove for a single tier.
pub struct TierSetup {
    /// Chip registry (AIR implementations for proving/verification).
    pub registry: ChipRegistry,
    /// Proving key (keygen info cached from registry).
    pub proving_key: TabulaProvingKey,
    /// Verifying key (minimal verification metadata).
    pub verifying_key: TabulaVerifyingKey,
    /// Dynamic chips for trace building (phase-ordered dispatch).
    pub(crate) dyn_chips: Vec<Box<dyn DynChip>>,
    /// Bus consumers for interaction-driven trace building.
    pub(crate) bus_consumers: Vec<Box<dyn BusConsumer>>,
}

/// Build setup for the execution proof tier.
///
/// Chips: ExecutionChip<3>, StaticTableChip<3>, PoseidonChip, RangeCheckChip.
pub(crate) fn execution_tier_setup() -> Result<TierSetup, SetupError> {
    let mut registry = ChipRegistry::new();
    registry.register_execution();
    registry.register_bus_consumers();
    registry.validate()?;

    let mut dyn_chips: Vec<Box<dyn DynChip>> = execution_dyn_chips();
    dyn_chips.push(Box::new(PoseidonChip));
    dyn_chips.push(Box::new(RangeCheckChip));

    let bus_consumers: Vec<Box<dyn BusConsumer>> =
        vec![Box::new(PoseidonChip), Box::new(RangeCheckChip)];

    let proving_key = TabulaProvingKey::from_registry(&registry);
    let verifying_key = TabulaVerifyingKey::from_proving_key(&proving_key);

    Ok(TierSetup {
        registry,
        proving_key,
        verifying_key,
        dyn_chips,
        bus_consumers,
    })
}

/// Build setup for a column proof tier.
///
/// Chips: MemoryShardChip\<W\>, StateShardChip\<W\>, MetaShardChip, PoseidonChip, RangeCheckChip.
///
/// Each column proof operates on a single `(table_id, col_id)` pair.
/// Shard chip IDs are allocated locally (starting at 100) and are independent
/// across column proofs.
pub(crate) fn column_tier_setup<const W: usize>(
    table_id: TableId,
    col_id: ColId,
    scheme_tag: u16,
    receives_commitment: bool,
) -> Result<TierSetup, SetupError> {
    let t = table_id.0;
    let c = col_id.0;

    let mut alloc = ChipIdAllocator::for_shards();
    let mem_id = alloc.next();
    let state_id = alloc.next();
    let meta_id = alloc.next();

    let mem_chip = MemoryShardChip::<W>::new(mem_id, t, c);
    let state_chip = StateShardChip::<W>::new(state_id, t, c);
    let meta_chip = MetaShardChip::new(meta_id, t, c, scheme_tag, receives_commitment);

    let mut registry = ChipRegistry::new();
    registry.register(mem_chip.clone());
    registry.register(state_chip.clone());
    registry.register(meta_chip.clone());
    registry.register(PoseidonChip);
    registry.register(RangeCheckChip);
    registry.validate()?;

    let dyn_chips: Vec<Box<dyn DynChip>> = vec![
        Box::new(mem_chip),
        Box::new(state_chip),
        Box::new(meta_chip),
        Box::new(PoseidonChip),
        Box::new(RangeCheckChip),
    ];

    let bus_consumers: Vec<Box<dyn BusConsumer>> =
        vec![Box::new(PoseidonChip), Box::new(RangeCheckChip)];

    let proving_key = TabulaProvingKey::from_registry(&registry);
    let verifying_key = TabulaVerifyingKey::from_proving_key(&proving_key);

    Ok(TierSetup {
        registry,
        proving_key,
        verifying_key,
        dyn_chips,
        bus_consumers,
    })
}

/// Build setup for the root proof tier.
///
/// Accepts any [`RootProof`] implementation. PoseidonChip and RangeCheckChip
/// are always included as bus consumers.
///
/// The default root proof is [`SmtRootProof`], which provides
/// SmtColPathChip + SmtTablePathChip for two-level SMT path verification.
pub(crate) fn root_tier_setup(root_proof: &dyn RootProof) -> Result<TierSetup, SetupError> {
    let mut registry = ChipRegistry::new();
    registry.register_boxed(root_proof.airs());
    registry.register_bus_consumers();
    registry.validate()?;

    let mut dyn_chips: Vec<Box<dyn DynChip>> = root_proof.dyn_chips();
    dyn_chips.push(Box::new(PoseidonChip));
    dyn_chips.push(Box::new(RangeCheckChip));

    let bus_consumers: Vec<Box<dyn BusConsumer>> =
        vec![Box::new(PoseidonChip), Box::new(RangeCheckChip)];

    let proving_key = TabulaProvingKey::from_registry(&registry);
    let verifying_key = TabulaVerifyingKey::from_proving_key(&proving_key);

    Ok(TierSetup {
        registry,
        proving_key,
        verifying_key,
        dyn_chips,
        bus_consumers,
    })
}

// ── Trace building ───────────────────────────────────────────────────────────

use rayon::prelude::*;
use tabula_core::error::TabulaError;
use tabula_stark::trace::WitnessStore;
use tabula_witness::trace::{TraceMap, build_all_traces};

impl TierSetup {
    /// Build chip traces for this tier from a populated [`WitnessStore`].
    pub fn build_traces(&self, store: WitnessStore) -> Result<TraceMap, TabulaError> {
        build_all_traces(&self.dyn_chips, &self.bus_consumers, store)
    }
}

/// Per-tier trace maps for the proof architecture.
#[derive(Clone)]
pub struct ProofTraces {
    /// Execution tier traces.
    pub execution: TraceMap,
    /// Column tier traces: per-(table, col) trace maps.
    pub columns: Vec<((TableId, ColId), TraceMap)>,
    /// Root tier traces.
    pub root: TraceMap,
}

/// All per-tier setups for the proof architecture.
pub struct ProofSetups {
    /// Execution tier setup.
    pub execution: TierSetup,
    /// Column tier setups: per-(table, col).
    pub columns: Vec<((TableId, ColId), TierSetup)>,
    /// Root tier setup.
    pub root: TierSetup,
}

/// Per-column configuration for setup creation.
pub struct ColumnSetupConfig {
    /// Table identifier.
    pub table_id: TableId,
    /// Column identifier.
    pub col_id: ColId,
    /// Commitment scheme tag (e.g., `scheme_tags::SSMC` or `scheme_tags::SMT`).
    pub scheme_tag: u16,
    /// Whether MetaShardChip receives on the CommitmentVerification bus.
    pub receives_commitment: bool,
}

/// Create all per-tier setups (registries + keys).
pub(crate) fn create_proof_setups(
    columns: &[ColumnSetupConfig],
) -> Result<ProofSetups, TabulaError> {
    let exec_setup = execution_tier_setup().map_err(|e| setup_to_tabula(&e))?;

    let col_setups: Vec<((TableId, ColId), TierSetup)> = columns
        .iter()
        .map(|cfg| {
            let setup = column_tier_setup::<DEFAULT_VALUE_WIDTH>(
                cfg.table_id,
                cfg.col_id,
                cfg.scheme_tag,
                cfg.receives_commitment,
            )
            .map_err(|e| setup_to_tabula(&e))?;
            Ok(((cfg.table_id, cfg.col_id), setup))
        })
        .collect::<Result<Vec<_>, TabulaError>>()?;

    let root_setup = root_tier_setup(&SmtRootProof).map_err(|e| setup_to_tabula(&e))?;

    Ok(ProofSetups {
        execution: exec_setup,
        columns: col_setups,
        root: root_setup,
    })
}

/// Build all per-tier trace maps from setups and partitioned witness stores.
pub(crate) fn build_proof_traces(
    setups: &ProofSetups,
    stores: tabula_witness::trace::PartitionedStores,
) -> Result<ProofTraces, TabulaError> {
    let exec_traces = setups.execution.build_traces(stores.execution)?;

    // O(n) index for column setup lookup (avoids O(n²) linear scan).
    let setup_index: BTreeMap<(TableId, ColId), usize> = setups
        .columns
        .iter()
        .enumerate()
        .map(|(i, ((t, c), _))| ((*t, *c), i))
        .collect();

    // Build column traces in parallel (each column is independent).
    let col_traces: Vec<_> = stores
        .columns
        .into_par_iter()
        .map(|((table, col), col_store)| {
            let idx = setup_index
                .get(&(table, col))
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "trace_build",
                    detail: format!("no setup for column ({}, {})", table.0, col.0),
                })?;
            let traces = setups.columns[*idx].1.build_traces(col_store)?;
            Ok(((table, col), traces))
        })
        .collect::<Result<Vec<_>, TabulaError>>()?;

    // Validate column ordering: trace keys must match setup keys.
    debug_assert!(
        col_traces
            .iter()
            .zip(setups.columns.iter())
            .all(|(((t1, c1), _), ((t2, c2), _))| t1 == t2 && c1 == c2),
        "column trace ordering must match setup ordering"
    );

    let root_traces = setups.root.build_traces(stores.root)?;

    Ok(ProofTraces {
        execution: exec_traces,
        columns: col_traces,
        root: root_traces,
    })
}

/// Convert [`SetupError`] to [`TabulaError`] for ergonomic chaining.
fn setup_to_tabula(e: &SetupError) -> TabulaError {
    TabulaError::ProofError {
        phase: "setup",
        detail: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_commitment::scheme_tags;
    use tabula_stark::chips::core_chips;

    #[test]
    fn execution_tier_has_correct_chips() {
        let setup = execution_tier_setup().unwrap();
        let ids = setup.registry.chip_ids();

        assert_eq!(ids.len(), 4);
        assert!(ids.contains(&core_chips::EXECUTION));
        assert!(ids.contains(&core_chips::STATIC_TABLE));
        assert!(ids.contains(&core_chips::POSEIDON));
        assert!(ids.contains(&core_chips::RANGE_CHECK));

        assert_eq!(setup.dyn_chips.len(), 4);
        assert_eq!(setup.bus_consumers.len(), 2);
    }

    #[test]
    fn column_tier_has_correct_chips() {
        let setup = column_tier_setup::<3>(TableId(1), ColId(2), scheme_tags::SSMC, true).unwrap();
        let ids = setup.registry.chip_ids();

        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&core_chips::POSEIDON));
        assert!(ids.contains(&core_chips::RANGE_CHECK));

        assert_eq!(setup.dyn_chips.len(), 5);
        assert_eq!(setup.bus_consumers.len(), 2);
    }

    #[test]
    fn column_tiers_have_independent_ids() {
        let setup1 = column_tier_setup::<3>(TableId(1), ColId(1), scheme_tags::SSMC, true).unwrap();
        let setup2 = column_tier_setup::<3>(TableId(1), ColId(2), scheme_tags::SSMC, true).unwrap();

        assert_eq!(setup1.registry.chip_ids(), setup2.registry.chip_ids());
    }

    #[test]
    fn root_tier_has_correct_chips() {
        let setup = root_tier_setup(&SmtRootProof).unwrap();
        let ids = setup.registry.chip_ids();

        assert_eq!(ids.len(), 4);
        assert!(ids.contains(&core_chips::SMT_COL_PATH));
        assert!(ids.contains(&core_chips::SMT_TABLE_PATH));
        assert!(ids.contains(&core_chips::POSEIDON));
        assert!(ids.contains(&core_chips::RANGE_CHECK));

        assert_eq!(setup.dyn_chips.len(), 4);
        assert_eq!(setup.bus_consumers.len(), 2);
    }

    #[test]
    fn tier_keys_match_registry() {
        let setup = execution_tier_setup().unwrap();
        let pk_ids = setup.proving_key.chip_ids();
        let vk_ids = setup.verifying_key.chip_ids();
        let reg_ids = setup.registry.chip_ids();

        assert_eq!(pk_ids.len(), reg_ids.len());
        assert_eq!(vk_ids.len(), reg_ids.len());
        for id in &reg_ids {
            assert!(setup.proving_key.get(*id).is_some());
            assert!(setup.verifying_key.get(*id).is_some());
        }
    }
}
