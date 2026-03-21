use std::sync::Arc;

use tabula_artifact::SchemeDescriptor;
use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::property::SsmcPropertyChip;
use tabula_chips::shards::state::StateShardChip;
use tabula_commitment::schemes::tags;
use tabula_core::{ColumnLayoutKind, PropertyQueryResult, RowKey, SchemeId, Value, zero_value};
use tabula_ext::ExtError;
use tabula_ir::{PropertyQuery, PropertyQueryKind};
use tabula_machine::SetupError;
use tabula_machine::backend::{AnyRap, ColumnChipSet, ProofColumn};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;
#[cfg(feature = "prove")]
use tabula_witness::stark::schemes::ssmc::{PreparedSsmcProof, SsmcProofInput, prepare_ssmc_proof};

use crate::columns::{ColumnSchemeFactory, ResolvedColumnPlan, RuntimeColumn};
use crate::proof_extensions::ProofSchemeFactory;
#[cfg(feature = "prove")]
use crate::proof_extensions::{ColumnProofContext, ColumnProofPreparer, PreparedColumnProof};

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

    fn build_runtime_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        validate_runtime_plan(plan, ColumnSchemeFactory::name(self))?;
        Ok(Arc::new(SsmcRuntimeColumn { plan: plan.clone() }))
    }
}

impl<const W: usize> ProofSchemeFactory for SsmcScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        SchemeDescriptor::builtin_ssmc()
    }

    fn name(&self) -> &str {
        "ssmc"
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        validate_proof_plan(plan, ProofSchemeFactory::name(self))?;
        Ok(Arc::new(SsmcProofColumn::<W> { plan: plan.clone() }))
    }

    #[cfg(feature = "prove")]
    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        validate_proof_plan(plan, ProofSchemeFactory::name(self))?;
        Ok(Arc::new(SsmcProofPreparer::<W> { plan: plan.clone() }))
    }
}

fn validate_plan_detail(plan: &ResolvedColumnPlan, scheme_name: &str) -> Result<(), String> {
    if plan.scheme_descriptor.layout_kind != ColumnLayoutKind::SSMC_V1 {
        return Err(format!(
            "scheme factory '{scheme_name}' cannot prepare column layout {}",
            plan.scheme_descriptor.layout_kind.0,
        ));
    }
    if let Some(kind) = plan
        .required_property_query_kinds
        .iter()
        .find(|kind| !SSMC_SUPPORTED_QUERY_KINDS.contains(kind))
    {
        return Err(format!(
            "scheme '{scheme_name}' does not support property query {:?} for table {} col {}",
            kind, plan.table_id.0, plan.col_id.0,
        ));
    }
    Ok(())
}

fn validate_runtime_plan(plan: &ResolvedColumnPlan, scheme_name: &str) -> Result<(), ExtError> {
    validate_plan_detail(plan, scheme_name).map_err(ExtError::validation)
}

fn validate_proof_plan(plan: &ResolvedColumnPlan, scheme_name: &str) -> Result<(), ExtError> {
    validate_plan_detail(plan, scheme_name).map_err(ExtError::validation)
}

#[derive(Debug)]
struct SsmcRuntimeColumn {
    plan: ResolvedColumnPlan,
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
    plan: ResolvedColumnPlan,
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
        let meta = MetaShardChip::new(meta_id, t, c, tags::SSMC, self.plan.receives_commitment);

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
struct SsmcProofPreparer<const W: usize> {
    plan: ResolvedColumnPlan,
}

#[cfg(feature = "prove")]
impl<const W: usize> ColumnProofPreparer for SsmcProofPreparer<W> {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn prepare_column(&self, context: ColumnProofContext) -> Result<PreparedColumnProof, ExtError> {
        let ColumnProofContext {
            column,
            old_entries,
            property_reads,
        } = context;
        let table = column.table;
        let col = column.col;
        debug_assert_eq!(table, self.plan.table_id);
        debug_assert_eq!(col, self.plan.col_id);

        let PreparedSsmcProof { meta, store } = prepare_ssmc_proof::<W>(SsmcProofInput {
            table,
            col,
            value_type: column.value_type,
            old_entries: &old_entries,
            init_cells: &column.init_cells,
            access_events: &column.access_events,
            writes: &column.writes,
            is_touched: column.is_touched(),
            property_reads: &property_reads,
        })
        .map_err(ExtError::proof_preparation)?;

        Ok(PreparedColumnProof { meta, store })
    }
}
