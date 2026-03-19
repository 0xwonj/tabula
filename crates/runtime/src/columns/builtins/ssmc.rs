use std::sync::Arc;

#[cfg(feature = "prove")]
use std::collections::BTreeMap;

#[cfg(feature = "prove")]
use p3_koala_bear::KoalaBear;
use tabula_artifact::SchemeDescriptor;
use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::property::SsmcPropertyChip;
#[cfg(feature = "prove")]
use tabula_chips::shards::property::trace::{PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord};
#[cfg(feature = "prove")]
use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
#[cfg(feature = "prove")]
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};
use tabula_chips::shards::state::StateShardChip;
#[cfg(feature = "prove")]
use tabula_commitment::{
    ColumnMeta, ColumnState, PoseidonHasher, proof_column_commitment, scheme_tags,
};
#[cfg(feature = "prove")]
use tabula_core::error::TabulaError;
use tabula_core::{ColumnLayoutKind, PropertyQueryResult, RowKey, SchemeId, Value, zero_value};
use tabula_ir::{PropertyQuery, PropertyQueryKind};
use tabula_machine::AnyRap;
use tabula_machine::{ColumnChipSet, ProofColumn, SetupError};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;
#[cfg(feature = "prove")]
use tabula_stark::trace::WitnessStore;
#[cfg(feature = "prove")]
use tabula_witness::trace::builtin::memory::{
    SsmcColumnWitnessParts, prepare_ssmc_column_witness_from_parts,
};

use crate::columns::{ColumnPlan, ColumnSchemeFactory, ColumnViews, RuntimeColumn};
#[cfg(feature = "prove")]
use crate::columns::{ColumnProofInput, ColumnTransitionBackend, ColumnTransitionInput};

/// SSMC commitment scheme factory.
pub struct SsmcScheme<const W: usize>;

/// Query kinds currently implemented for SSMC's ordered committed state.
const SSMC_SUPPORTED_QUERY_KINDS: &[PropertyQueryKind] =
    &[PropertyQueryKind::Successor, PropertyQueryKind::Predecessor];

impl<const W: usize> ColumnSchemeFactory for SsmcScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        SchemeDescriptor::builtin_ssmc()
    }

    fn name(&self) -> &str {
        "ssmc"
    }

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        if plan.scheme_descriptor.layout_kind != ColumnLayoutKind::SSMC_V1 {
            return Err(SetupError::SetupFailed(format!(
                "scheme factory '{}' cannot prepare column layout {}",
                self.name(),
                plan.scheme_descriptor.layout_kind.0,
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
                Arc::new(SsmcTransitionBackend::<W>::new(plan)?),
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
    ) -> Result<PropertyQueryResult, tabula_core::error::TabulaError> {
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
                return Err(tabula_core::error::TabulaError::InvalidIr(format!(
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
            scheme_tags::SSMC,
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

        Ok(ColumnChipSet {
            airs,
            dyn_chips,
            bus_consumers: vec![],
        })
    }
}

#[cfg(feature = "prove")]
#[derive(Debug)]
struct SsmcTransitionBackend<const W: usize> {
    plan: ColumnPlan,
}

#[cfg(feature = "prove")]
impl<const W: usize> SsmcTransitionBackend<W> {
    fn new(plan: ColumnPlan) -> Result<Self, SetupError> {
        if plan.scheme_descriptor.layout_kind != ColumnLayoutKind::SSMC_V1 {
            return Err(SetupError::SetupFailed(format!(
                "SSMC transition backend cannot prepare column layout {}",
                plan.scheme_descriptor.layout_kind.0,
            )));
        }
        Ok(Self { plan })
    }
}

#[cfg(feature = "prove")]
impl<const W: usize> ColumnTransitionBackend for SsmcTransitionBackend<W> {
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

    fn build_proof_input(
        &self,
        input: ColumnTransitionInput,
        property_reads: &[PropertyReadRecord],
    ) -> Result<ColumnProofInput, TabulaError> {
        debug_assert_eq!(input.table, self.plan.table_id);
        debug_assert_eq!(input.col, self.plan.col_id);

        let hasher = PoseidonHasher::new();
        let (old_state, _) = ColumnState::commit(
            &hasher,
            input.table,
            input.col,
            input.old_entries,
            scheme_tags::SSMC,
        )?;
        let com_old = proof_column_commitment(input.table, input.col, &old_state)?;
        let is_empty_old = old_state.is_empty();
        let (new_state, _runtime_com_new, _merge_trace) = if input.is_touched {
            old_state.apply_writes(&hasher, input.table, input.col, &input.writes)
        } else {
            (old_state.clone(), com_old, None)
        };
        let meta = ColumnMeta {
            table: input.table,
            col: input.col,
            tag: scheme_tags::SSMC,
            com_old,
            com_new: proof_column_commitment(input.table, input.col, &new_state)?,
            is_empty_old,
            is_empty_new: new_state.is_empty(),
            is_touched: input.is_touched,
        };

        let old_entries = ssmc_entries(&old_state)?;
        let new_entries = ssmc_entries(&new_state)?;
        let col_witness = prepare_ssmc_column_witness_from_parts::<W>(&SsmcColumnWitnessParts {
            column: (input.table, input.col),
            init_rows: &input.init_rows,
            access_rows: &input.access_rows,
            old_entries: &old_entries,
            new_entries: &new_entries,
            meta: &meta,
            has_commitment_proof: true,
        })?;

        let mut store = WitnessStore::new();
        store.put(
            SHARED_COLUMN_WITNESS_LABEL,
            SharedColumnWitness {
                memory_rows: col_witness.memory_rows.clone(),
                meta_row: col_witness.meta_row.clone(),
            },
        );
        let mut single_witness = SsmcWitness::default();
        single_witness.insert(self.plan.table_id, self.plan.col_id, col_witness);
        store.put(SSMC_WITNESS_LABEL, single_witness);

        if !property_reads.is_empty() {
            store.put(PROPERTY_READ_WITNESS_LABEL, property_reads.to_vec());
        }

        Ok(ColumnProofInput {
            table: input.table,
            col: input.col,
            meta,
            witness_store: store,
        })
    }
}

#[cfg(feature = "prove")]
fn ssmc_entries(
    state: &ColumnState<PoseidonHasher>,
) -> Result<BTreeMap<RowKey, Vec<KoalaBear>>, TabulaError> {
    match state {
        ColumnState::Ssmc(list) => Ok(list
            .entries()
            .iter()
            .map(|entry| (entry.key, entry.value.clone()))
            .collect()),
        ColumnState::Smt(_) => Err(TabulaError::ProofError {
            phase: "ssmc_transition",
            detail: "only SSMC-backed columns are supported".to_string(),
        }),
    }
}
