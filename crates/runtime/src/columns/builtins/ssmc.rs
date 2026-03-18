use std::sync::Arc;

use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::property::SsmcPropertyChip;
#[cfg(feature = "prove")]
use tabula_chips::shards::property::trace::{PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord};
#[cfg(feature = "prove")]
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};
use tabula_chips::shards::state::StateShardChip;
#[cfg(feature = "prove")]
use tabula_commitment::PoseidonHasher;
use tabula_core::error::TabulaError;
use tabula_core::{PropertyQueryResult, RowKey, SchemeId, Value, zero_value};
use tabula_ir::{PropertyQuery, PropertyQueryKind};
use tabula_machine::AnyRap;
use tabula_machine::{ColumnChipSet, ProofColumn, SetupError};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;
#[cfg(feature = "prove")]
use tabula_stark::trace::WitnessStore;
#[cfg(feature = "prove")]
use tabula_witness::ColumnWitness;
#[cfg(feature = "prove")]
use tabula_witness::trace::builtin::memory::prepare_ssmc_column_witness;

use crate::columns::{ColumnPlan, ColumnSchemeFactory, ColumnViews, RuntimeColumn};
#[cfg(feature = "prove")]
use crate::columns::ProofInputBuilder;

/// SSMC commitment scheme factory.
pub struct SsmcScheme<const W: usize>;

/// Query kinds currently implemented for SSMC's ordered committed state.
const SSMC_SUPPORTED_QUERY_KINDS: &[PropertyQueryKind] =
    &[PropertyQueryKind::Successor, PropertyQueryKind::Predecessor];

impl<const W: usize> ColumnSchemeFactory for SsmcScheme<W> {
    fn scheme_id(&self) -> SchemeId {
        SchemeId::SSMC
    }

    fn name(&self) -> &str {
        "ssmc"
    }

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        if plan.scheme_id != SchemeId::SSMC {
            return Err(SetupError::SetupFailed(format!(
                "scheme factory '{}' cannot prepare scheme id {}",
                self.name(),
                plan.scheme_id.0,
            )));
        }

        if let Some(kind) = plan
            .required_property_query_kinds
            .iter()
            .find(|kind| !SSMC_SUPPORTED_QUERY_KINDS.contains(kind))
        {
            return Err(SetupError::SetupFailed(format!(
                "scheme '{}' does not support property query {:?} for table {} col {}",
                self.name(),
                kind,
                plan.table_id.0,
                plan.col_id.0,
            )));
        }

        #[cfg(feature = "prove")]
        {
            Ok(ColumnViews::new(
                Arc::new(SsmcRuntimeColumn { plan: plan.clone() }),
                Arc::new(SsmcProofColumn::<W> { plan: plan.clone() }),
                Arc::new(SsmcProofInputBuilder::<W> { plan }),
            ))
        }

        #[cfg(not(feature = "prove"))]
        {
            Ok(ColumnViews::new(
                Arc::new(SsmcRuntimeColumn { plan: plan.clone() }),
                Arc::new(SsmcProofColumn::<W> { plan }),
            ))
        }
    }
}

#[derive(Debug)]
struct SsmcRuntimeColumn {
    plan: ColumnPlan,
}

impl RuntimeColumn for SsmcRuntimeColumn {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        SSMC_SUPPORTED_QUERY_KINDS
    }

    fn resolve_property(
        &self,
        query: &PropertyQuery,
        state: &[(RowKey, Value, bool)],
    ) -> Result<PropertyQueryResult, TabulaError> {
        let non_null = || state.iter().filter(|(_, _, is_null)| !*is_null);

        let resolved = match query {
            PropertyQuery::Successor { key } => non_null()
                .filter(|(candidate, _, _)| *candidate > *key)
                .min_by_key(|(candidate, _, _)| *candidate)
                .map(|(candidate, value, _)| PropertyQueryResult {
                    value: *value,
                    key: Some(*candidate),
                    is_null: false,
                }),
            PropertyQuery::Predecessor { key } => non_null()
                .filter(|(candidate, _, _)| *candidate < *key)
                .max_by_key(|(candidate, _, _)| *candidate)
                .map(|(candidate, value, _)| PropertyQueryResult {
                    value: *value,
                    key: Some(*candidate),
                    is_null: false,
                }),
            PropertyQuery::Minimum
            | PropertyQuery::Maximum
            | PropertyQuery::NonExistenceRange { .. }
            | PropertyQuery::Aggregate { .. } => {
                return Err(TabulaError::InvalidIr(format!(
                    "column scheme '{}' does not implement property query {:?} for table {} col {}",
                    self.name(),
                    query.kind(),
                    self.plan.table_id.0,
                    self.plan.col_id.0,
                )));
            }
        };

        Ok(resolved.unwrap_or(PropertyQueryResult {
            value: zero_value(self.plan.value_type),
            key: None,
            is_null: true,
        }))
    }
}

#[derive(Debug)]
struct SsmcProofColumn<const W: usize> {
    plan: ColumnPlan,
}

impl<const W: usize> ProofColumn for SsmcProofColumn<W> {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn table_id(&self) -> tabula_core::TableId {
        self.plan.table_id
    }

    fn col_id(&self) -> tabula_core::ColId {
        self.plan.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn create_chips(&self, alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
        let t = self.plan.table_id.0;
        let c = self.plan.col_id.0;

        let mem_id = alloc.next();
        let state_id = alloc.next();
        let meta_id = alloc.next();

        let mem = MemoryShardChip::<W>::new(mem_id, t, c);
        let state = StateShardChip::<W>::new(state_id, t, c);
        let meta = MetaShardChip::new(
            meta_id,
            t,
            c,
            self.plan.scheme_id.raw(),
            self.plan.receives_commitment,
        );

        let mut airs: Vec<Box<dyn AnyRap>> = vec![
            Box::new(mem.clone()),
            Box::new(state.clone()),
            Box::new(meta.clone()),
        ];
        let mut dyn_chips: Vec<Box<dyn DynChip>> =
            vec![Box::new(mem), Box::new(state), Box::new(meta)];

        if self.plan.requires_property_support() {
            let prop_id = alloc.next();
            let prop = SsmcPropertyChip::<W>::new(prop_id, t, c);
            airs.push(Box::new(prop.clone()));
            dyn_chips.push(Box::new(prop));
        }

        Ok(ColumnChipSet { airs, dyn_chips })
    }
}

#[cfg(feature = "prove")]
#[derive(Debug)]
struct SsmcProofInputBuilder<const W: usize> {
    plan: ColumnPlan,
}

#[cfg(feature = "prove")]
impl<const W: usize> ProofInputBuilder for SsmcProofInputBuilder<W> {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn table_id(&self) -> tabula_core::TableId {
        self.plan.table_id
    }

    fn col_id(&self) -> tabula_core::ColId {
        self.plan.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn build_witness_store(
        &self,
        column: &ColumnWitness<PoseidonHasher>,
        property_reads: &[PropertyReadRecord],
    ) -> Result<WitnessStore, TabulaError> {
        let col_witness = prepare_ssmc_column_witness::<PoseidonHasher, W>(column)?;

        let mut store = WitnessStore::new();
        let mut single_witness = SsmcWitness::default();
        single_witness.insert(self.plan.table_id, self.plan.col_id, col_witness);
        store.put(SSMC_WITNESS_LABEL, single_witness);

        if !property_reads.is_empty() {
            store.put(PROPERTY_READ_WITNESS_LABEL, property_reads.to_vec());
        }

        Ok(store)
    }
}
