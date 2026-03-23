use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::{BusConsumer, DynChip};

use crate::backend::{ColumnChipSet, ProofColumn};
use crate::setup::execution::execution_dyn_chips;
use crate::setup::registry::{ChipRegistry, SetupError};
use crate::setup::root::RootProof;
use crate::setup::types::TierSetup;
use crate::{TabulaProvingKey, TabulaVerifyingKey};

pub(crate) fn execution_tier_setup() -> Result<TierSetup, SetupError> {
    let mut registry = ChipRegistry::new();
    registry.register_execution();
    registry.register(RangeCheckChip);
    registry.validate()?;

    let mut dyn_chips: Vec<Box<dyn DynChip>> = execution_dyn_chips();
    dyn_chips.push(Box::new(RangeCheckChip));

    let bus_consumers: Vec<Box<dyn BusConsumer>> = vec![Box::new(RangeCheckChip)];

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

pub(crate) fn column_tier_setup(column: &dyn ProofColumn) -> Result<TierSetup, SetupError> {
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
    registry.validate()?;

    dyn_chips.push(Box::new(PoseidonChip));
    dyn_chips.push(Box::new(RangeCheckChip));

    bus_consumers.push(Box::new(PoseidonChip));
    bus_consumers.push(Box::new(RangeCheckChip));

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
    use crate::setup::root::SmtRootProof;
    use crate::testing::TestSsmcProofColumn;

    use super::{TierSetup, column_tier_setup, execution_tier_setup, root_tier_setup};

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
        let setup = execution_tier_setup().unwrap();
        let ids = setup.registry.chip_ids();

        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&core_chips::EXECUTION));
        assert!(ids.contains(&core_chips::STATIC_TABLE));
        assert!(ids.contains(&core_chips::RANGE_CHECK));

        assert_eq!(setup.dyn_chips.len(), 3);
        assert_eq!(setup.bus_consumers.len(), 1);
    }

    #[test]
    fn column_tier_has_correct_chips() {
        let column = TestSsmcProofColumn {
            table_id: TableId(1),
            col_id: ColId(2),
            receives_commitment: true,
        };
        let setup = column_tier_setup(&column).unwrap();
        let ids = setup.registry.chip_ids();

        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&core_chips::POSEIDON));
        assert!(ids.contains(&core_chips::RANGE_CHECK));

        assert_eq!(setup.dyn_chips.len(), 5);
        assert_eq!(setup.bus_consumers.len(), 2);
    }

    #[test]
    fn column_tier_accepts_scheme_owned_bus_consumers() {
        let setup = column_tier_setup(&TestConsumerProofColumn).unwrap();
        assert_eq!(setup.bus_consumers.len(), 3);
        assert_eq!(setup.dyn_chips.len(), 2);
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

        let setup1 = column_tier_setup(&column1).unwrap();
        let setup2 = column_tier_setup(&column2).unwrap();

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

    #[test]
    fn setup_auto_traits() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TierSetup>();
    }
}
