use tabula_ext::root::RootBackendBundle;
use tabula_machine::ColumnSlotKey;

use crate::error::RuntimeError;
use crate::program::ResolvedProofProgram;

/// Runtime-owned batch-local proof plan for one proving request.
pub(crate) struct BatchProofPlan {
    pub(crate) columns: Vec<ColumnTierPlan>,
    pub(crate) root: RootTierPlan,
}

/// Column-tier plan for one machine-managed column slot.
#[derive(Debug)]
pub(crate) struct ColumnTierPlan {
    pub(crate) key: ColumnSlotKey,
}

/// Root-tier plan for the current proving request.
pub(crate) struct RootTierPlan {
    pub(crate) backend: RootBackendBundle,
}

impl std::fmt::Debug for BatchProofPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchProofPlan")
            .field("columns", &self.columns)
            .field("root", &self.root)
            .finish()
    }
}

impl std::fmt::Debug for RootTierPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootTierPlan")
            .field("backend", &self.backend.name())
            .finish()
    }
}

pub(crate) fn build_batch_proof_plan(
    proof_program: &ResolvedProofProgram,
    root_backend_bundle: &RootBackendBundle,
) -> Result<BatchProofPlan, RuntimeError> {
    Ok(BatchProofPlan {
        columns: proof_program
            .proof_plan()
            .column_slots()
            .iter()
            .map(|slot| ColumnTierPlan {
                key: ColumnSlotKey {
                    table: slot.table,
                    col: slot.col,
                },
            })
            .collect(),
        root: RootTierPlan {
            backend: root_backend_bundle.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use p3_field::PrimeCharacteristicRing;
    use tabula_core::{ColId, RootProfileId, TableId, TxTypeId};
    use tabula_ext::root::{
        PreparedRootWitness, RootBackend, RootBackendBundle, RootWitnessContext,
        RootWitnessPreparer,
    };
    use tabula_machine::PublicStatement;
    use tabula_stark::trace::WitnessStore;
    use tabula_testing::exec::compiled_program_from_definition;
    use tabula_testing::fixtures::schema::single_u64_column_schema;

    use super::build_batch_proof_plan;
    use crate::TabulaRuntime;
    #[derive(Debug)]
    struct RecordingRootWitnessPreparer;

    impl RootWitnessPreparer for RecordingRootWitnessPreparer {
        fn name(&self) -> &str {
            "recording_root"
        }

        fn prepare_root_witness(
            &self,
            _context: RootWitnessContext<'_>,
        ) -> tabula_ext::ExtResult<PreparedRootWitness> {
            Ok(PreparedRootWitness::new(
                PublicStatement {
                    old_root: tabula_commitment::NativeDigest([p3_koala_bear::KoalaBear::ZERO; 8]),
                    new_root: tabula_commitment::NativeDigest([p3_koala_bear::KoalaBear::ZERO; 8]),
                },
                WitnessStore::new(),
            ))
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct DelegatingRootProofBackend;

    impl tabula_machine::RootProofBackend for DelegatingRootProofBackend {
        fn name(&self) -> &str {
            "delegating_root_proof"
        }

        fn supported_root_binding_families(&self) -> &'static [RootProfileId] {
            tabula_machine::SmtRootProofBackend.supported_root_binding_families()
        }

        fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
            tabula_machine::SmtRootProofBackend.airs()
        }

        fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
            tabula_machine::SmtRootProofBackend.dyn_chips()
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct RecordingRootBackend;

    impl RootBackend for RecordingRootBackend {
        fn name(&self) -> &str {
            "recording_root_backend"
        }

        fn proof_backend(&self) -> std::sync::Arc<dyn tabula_machine::RootProofBackend> {
            std::sync::Arc::new(DelegatingRootProofBackend)
        }

        fn witness_preparer(&self) -> std::sync::Arc<dyn RootWitnessPreparer> {
            std::sync::Arc::new(RecordingRootWitnessPreparer)
        }
    }

    fn single_column_runtime() -> TabulaRuntime {
        let schema = single_u64_column_schema(TableId(1), ColId(0), "accounts", "balance");
        let tx = tabula_ir::TxTypeDef {
            id: TxTypeId(1),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        };
        TabulaRuntime::builder(compiled_program_from_definition(vec![schema], vec![tx]))
            .with_root_backend_bundle(RootBackendBundle::new(RecordingRootBackend))
            .build()
            .expect("runtime")
    }

    #[test]
    fn batch_plan_preserves_proof_plan_column_order() {
        let runtime = single_column_runtime();
        let plan = build_batch_proof_plan(runtime.proof_program(), runtime.root_backend_bundle())
            .expect("batch proof plan");

        let expected: Vec<_> = runtime
            .proof_program()
            .proof_plan()
            .column_slots()
            .iter()
            .map(|slot| (slot.table, slot.col))
            .collect();
        let actual: Vec<_> = plan
            .columns
            .iter()
            .map(|plan| (plan.key.table, plan.key.col))
            .collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn batch_plan_carries_runtime_root_backend_bundle() {
        let runtime = single_column_runtime();
        let plan = build_batch_proof_plan(runtime.proof_program(), runtime.root_backend_bundle())
            .expect("batch proof plan");

        assert_eq!(plan.root.backend.name(), "recording_root_backend");
    }
}
