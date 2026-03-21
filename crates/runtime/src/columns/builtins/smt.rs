use std::sync::Arc;

use tabula_artifact::SchemeDescriptor;
use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::smt_state::SmtStateShardChip;
use tabula_commitment::schemes::tags;
use tabula_core::{ColumnLayoutKind, PropertyQueryResult, RowKey, SchemeId, Value};
use tabula_ext::ExtError;
use tabula_ir::{PropertyQuery, PropertyQueryKind};
use tabula_machine::SetupError;
use tabula_machine::backend::{AnyRap, ColumnChipSet, ProofColumn};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;
#[cfg(feature = "prove")]
use tabula_witness::stark::schemes::smt::{PreparedSmtProof, SmtProofInput, prepare_smt_proof};

use crate::columns::{ColumnSchemeFactory, ResolvedColumnPlan, RuntimeColumn};
use crate::proof_extensions::ProofSchemeFactory;
#[cfg(feature = "prove")]
use crate::proof_extensions::{ColumnProofContext, ColumnProofPreparer, PreparedColumnProof};

/// SMT commitment scheme factory.
pub struct SmtScheme<const W: usize>;

impl<const W: usize> ColumnSchemeFactory for SmtScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        SchemeDescriptor::builtin_smt()
    }

    fn name(&self) -> &str {
        "smt"
    }

    fn build_runtime_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        validate_runtime_plan(plan, ColumnSchemeFactory::name(self))?;
        Ok(Arc::new(SmtRuntimeColumn { plan: plan.clone() }))
    }
}

impl<const W: usize> ProofSchemeFactory for SmtScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        SchemeDescriptor::builtin_smt()
    }

    fn name(&self) -> &str {
        "smt"
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        validate_proof_plan(plan, ProofSchemeFactory::name(self))?;
        Ok(Arc::new(SmtProofColumn::<W> { plan: plan.clone() }))
    }

    #[cfg(feature = "prove")]
    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        validate_proof_plan(plan, ProofSchemeFactory::name(self))?;
        Ok(Arc::new(SmtProofPreparer::<W> { plan: plan.clone() }))
    }
}

fn validate_plan_detail(plan: &ResolvedColumnPlan, scheme_name: &str) -> Result<(), String> {
    if plan.scheme_descriptor.layout_kind != ColumnLayoutKind::SMT_V1 {
        return Err(format!(
            "scheme factory '{scheme_name}' cannot prepare column layout {}",
            plan.scheme_descriptor.layout_kind.0,
        ));
    }
    if let Some(kind) = plan.required_property_query_kinds.iter().next() {
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
struct SmtRuntimeColumn {
    plan: ResolvedColumnPlan,
}

impl RuntimeColumn for SmtRuntimeColumn {
    fn name(&self) -> &str {
        "smt"
    }

    fn resolve_property(
        &self,
        query: &PropertyQuery,
        _state: &[(RowKey, Value, bool)],
    ) -> Result<PropertyQueryResult, tabula_core::error::TabulaError> {
        Err(tabula_core::error::TabulaError::InvalidIr(format!(
            "column scheme '{}' does not implement property query {:?} for table {} col {}",
            self.name(),
            query.kind(),
            self.plan.table_id.0,
            self.plan.col_id.0,
        )))
    }

    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        &[]
    }
}

#[derive(Debug)]
struct SmtProofColumn<const W: usize> {
    plan: ResolvedColumnPlan,
}

impl<const W: usize> ProofColumn for SmtProofColumn<W> {
    fn name(&self) -> &str {
        "smt"
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
        let state = SmtStateShardChip::<W>::new(state_id, t, c);
        let meta = MetaShardChip::new(meta_id, t, c, tags::SMT, self.plan.receives_commitment);

        Ok(ColumnChipSet {
            airs: vec![
                Box::new(mem.clone()) as Box<dyn AnyRap>,
                Box::new(state.clone()),
                Box::new(meta.clone()),
            ],
            dyn_chips: vec![
                Box::new(mem) as Box<dyn DynChip>,
                Box::new(state),
                Box::new(meta),
            ],
            bus_consumers: vec![],
        })
    }
}

#[cfg(feature = "prove")]
#[derive(Debug)]
struct SmtProofPreparer<const W: usize> {
    plan: ResolvedColumnPlan,
}

#[cfg(feature = "prove")]
impl<const W: usize> ColumnProofPreparer for SmtProofPreparer<W> {
    fn name(&self) -> &str {
        "smt"
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

        if !property_reads.is_empty() {
            return Err(ExtError::proof_preparation(
                tabula_core::error::TabulaError::ProofError {
                    phase: "smt_proof",
                    detail: format!(
                        "SMT column ({}, {}) received unexpected property reads",
                        table.0, col.0
                    ),
                },
            ));
        }

        let PreparedSmtProof { meta, store } = prepare_smt_proof::<W>(SmtProofInput {
            table,
            col,
            value_type: column.value_type,
            old_entries: &old_entries,
            init_cells: &column.init_cells,
            access_events: &column.access_events,
            writes: &column.writes,
            is_touched: column.is_touched(),
        })
        .map_err(ExtError::proof_preparation)?;

        Ok(PreparedColumnProof { meta, store })
    }
}
