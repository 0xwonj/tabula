use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::{BusConsumer, DynChip};

use crate::backend::{ColumnChipSet, ProofColumn};
use crate::setup::execution::execution_dyn_chips;
use crate::setup::metadata::{TierProvingMetadata, TierVerificationMetadata};
use crate::setup::registry::{ChipRegistry, SetupError};
use crate::setup::root::RootProofBackend;
use crate::setup::topology::TierTopology;

pub(crate) struct TierRecipe {
    pub(crate) registry: ChipRegistry,
    pub(crate) dyn_chips: Vec<Box<dyn DynChip>>,
    pub(crate) bus_consumers: Vec<Box<dyn BusConsumer>>,
}

impl TierRecipe {
    pub(crate) fn finalize(self) -> Result<TierTopology, SetupError> {
        finalize_tier_topology(self)
    }
}

pub(crate) fn execution_tier_topology() -> Result<TierTopology, SetupError> {
    let mut registry = ChipRegistry::new();
    registry.register_execution();
    registry.register(RangeCheckChip);

    let mut dyn_chips: Vec<Box<dyn DynChip>> = execution_dyn_chips();
    dyn_chips.push(Box::new(RangeCheckChip));

    TierRecipe {
        registry,
        dyn_chips,
        bus_consumers: vec![Box::new(RangeCheckChip)],
    }
    .finalize()
}

pub(crate) fn column_tier_topology(column: &dyn ProofColumn) -> Result<TierTopology, SetupError> {
    let mut alloc = ChipIdAllocator::for_shards();
    let ColumnChipSet {
        airs,
        mut dyn_chips,
        mut bus_consumers,
    } = column.create_chips(&mut alloc)?;

    let mut registry = ChipRegistry::new();
    registry.register_boxed(airs);
    registry.register(PoseidonChip);
    registry.register(RangeCheckChip);

    dyn_chips.push(Box::new(PoseidonChip));
    dyn_chips.push(Box::new(RangeCheckChip));

    bus_consumers.push(Box::new(PoseidonChip));
    bus_consumers.push(Box::new(RangeCheckChip));

    TierRecipe {
        registry,
        dyn_chips,
        bus_consumers,
    }
    .finalize()
}

pub(crate) fn root_tier_topology(
    root_proof: &dyn RootProofBackend,
) -> Result<TierTopology, SetupError> {
    let mut registry = ChipRegistry::new();
    registry.register_boxed(root_proof.airs());
    registry.register_bus_consumers();

    let mut dyn_chips: Vec<Box<dyn DynChip>> = root_proof.dyn_chips();
    dyn_chips.push(Box::new(PoseidonChip));
    dyn_chips.push(Box::new(RangeCheckChip));

    let bus_consumers: Vec<Box<dyn BusConsumer>> =
        vec![Box::new(PoseidonChip), Box::new(RangeCheckChip)];

    TierRecipe {
        registry,
        dyn_chips,
        bus_consumers,
    }
    .finalize()
}

pub(crate) fn finalize_tier_topology(recipe: TierRecipe) -> Result<TierTopology, SetupError> {
    let TierRecipe {
        registry,
        dyn_chips,
        bus_consumers,
    } = recipe;
    registry.validate()?;

    let proving_metadata = TierProvingMetadata::from_registry(&registry);
    let verification_metadata = TierVerificationMetadata::from_proving_metadata(&proving_metadata);

    Ok(TierTopology {
        registry,
        proving_metadata,
        verification_metadata,
        dyn_chips,
        bus_consumers,
    })
}

#[cfg(test)]
mod tests {
    use p3_koala_bear::KoalaBear;
    use tabula_core::error::TabulaError;
    use tabula_core::{ColId, TableId};
    use tabula_stark::air::interaction::BusId;
    use tabula_stark::chips::{ChipIdAllocator, core_chips};
    use tabula_stark::debug::RecordedInteraction;
    use tabula_stark::trace::column_commitment::BusConsumer;
    use tabula_stark::trace::contributor::WitnessStore;

    use crate::SetupError;
    use crate::backend::{AnyRap, ColumnChipSet, ProofColumn};
    use crate::setup::root::SmtRootProofBackend;
    use crate::testing::TestSsmcProofColumn;

    use super::{TierTopology, column_tier_topology, execution_tier_topology, root_tier_topology};

    struct DummyConsumer;

    impl BusConsumer for DummyConsumer {
        fn consumed_buses(&self) -> Vec<BusId> {
            vec![]
        }

        fn collect(
            &self,
            _interactions: &[RecordedInteraction<KoalaBear>],
            _store: &mut WitnessStore,
        ) -> Result<(), TabulaError> {
            Ok(())
        }
    }

    struct TestConsumerProofColumn;

    impl ProofColumn for TestConsumerProofColumn {
        fn name(&self) -> &str {
            "test-consumer"
        }

        fn table_id(&self) -> TableId {
            TableId(1)
        }

        fn col_id(&self) -> ColId {
            ColId(9)
        }

        fn scheme_id(&self) -> tabula_core::SchemeId {
            tabula_core::SchemeId::SSMC
        }

        fn create_chips(&self, _alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
            Ok(ColumnChipSet {
                airs: Vec::<Box<dyn AnyRap>>::new(),
                dyn_chips: vec![],
                bus_consumers: vec![Box::new(DummyConsumer)],
            })
        }
    }

    #[test]
    fn execution_tier_has_correct_chips() {
        let topology = execution_tier_topology().unwrap();
        let ids = topology.registry.chip_ids();

        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&core_chips::EXECUTION));
        assert!(ids.contains(&core_chips::STATIC_TABLE));
        assert!(ids.contains(&core_chips::RANGE_CHECK));

        assert_eq!(topology.dyn_chips.len(), 3);
        assert_eq!(topology.bus_consumers.len(), 1);
    }

    #[test]
    fn column_tier_has_correct_chips() {
        let column = TestSsmcProofColumn {
            table_id: TableId(1),
            col_id: ColId(2),
            receives_commitment: true,
        };
        let topology = column_tier_topology(&column).unwrap();
        let ids = topology.registry.chip_ids();

        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&core_chips::POSEIDON));
        assert!(ids.contains(&core_chips::RANGE_CHECK));

        assert_eq!(topology.dyn_chips.len(), 5);
        assert_eq!(topology.bus_consumers.len(), 2);
    }

    #[test]
    fn column_tier_accepts_scheme_owned_bus_consumers() {
        let topology = column_tier_topology(&TestConsumerProofColumn).unwrap();
        assert_eq!(topology.bus_consumers.len(), 3);
        assert_eq!(topology.dyn_chips.len(), 2);
    }

    #[test]
    fn column_tiers_have_independent_ids() {
        let column1 = TestSsmcProofColumn {
            table_id: TableId(1),
            col_id: ColId(1),
            receives_commitment: true,
        };
        let column2 = TestSsmcProofColumn {
            table_id: TableId(1),
            col_id: ColId(2),
            receives_commitment: true,
        };

        let topology1 = column_tier_topology(&column1).unwrap();
        let topology2 = column_tier_topology(&column2).unwrap();

        assert_eq!(topology1.registry.chip_ids(), topology2.registry.chip_ids());
    }

    #[test]
    fn root_tier_has_correct_chips() {
        let topology = root_tier_topology(&SmtRootProofBackend).unwrap();
        let ids = topology.registry.chip_ids();

        assert_eq!(ids.len(), 4);
        assert!(ids.contains(&core_chips::SMT_COL_PATH));
        assert!(ids.contains(&core_chips::SMT_TABLE_PATH));
        assert!(ids.contains(&core_chips::POSEIDON));
        assert!(ids.contains(&core_chips::RANGE_CHECK));

        assert_eq!(topology.dyn_chips.len(), 4);
        assert_eq!(topology.bus_consumers.len(), 2);
    }

    #[test]
    fn tier_metadata_matches_registry() {
        let topology = execution_tier_topology().unwrap();
        let proving_ids = topology.proving_metadata.chip_ids();
        let verification_ids = topology.verification_metadata.chip_ids();
        let reg_ids = topology.registry.chip_ids();

        assert_eq!(proving_ids.len(), reg_ids.len());
        assert_eq!(verification_ids.len(), reg_ids.len());
        for id in &reg_ids {
            assert!(topology.proving_metadata.get(*id).is_some());
            assert!(topology.verification_metadata.get(*id).is_some());
        }
    }

    #[test]
    fn tier_topology_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TierTopology>();
    }
}
