//! Executor adapter for the next-generation canonical IR.

use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, StateView};
use tabula_core::{CellKey, RowKey, TypeId};
use tabula_ir_next as ir;
use tabula_profile::TYPE_U64_ID;
use tabula_types::{
    TypeRuntimeRegistry, TypedColumnEntry, TypedValue, bool_typed, bytes32_typed, typed_bool,
};

use crate::overlay::Overlay;

#[derive(Debug, Clone, thiserror::Error)]
#[error("op {op_index}: {error}")]
pub struct ExecuteError {
    #[source]
    pub error: TabulaError,
    pub op_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrapKind {
    Semantic,
    Fatal,
}

#[derive(Debug)]
struct EntryTrap {
    kind: TrapKind,
    op_index: usize,
    error: TabulaError,
}

#[derive(Debug)]
enum OpFailure {
    Semantic(TabulaError),
    Fatal(TabulaError),
}

impl OpFailure {
    fn semantic(error: TabulaError) -> Self {
        Self::Semantic(error)
    }

    fn fatal(error: TabulaError) -> Self {
        Self::Fatal(error)
    }

    fn at(self, op_index: usize) -> EntryTrap {
        match self {
            Self::Semantic(error) => EntryTrap {
                kind: TrapKind::Semantic,
                op_index,
                error,
            },
            Self::Fatal(error) => EntryTrap {
                kind: TrapKind::Fatal,
                op_index,
                error,
            },
        }
    }
}

fn semantic<T>(result: Result<T, TabulaError>) -> Result<T, OpFailure> {
    result.map_err(OpFailure::semantic)
}

fn fatal<T>(result: Result<T, TabulaError>) -> Result<T, OpFailure> {
    result.map_err(OpFailure::fatal)
}

pub struct ExecContext<'a> {
    pub hasher: &'a dyn Hasher,
    pub type_runtimes: &'a TypeRuntimeRegistry,
    pub capabilities: Option<&'a CapabilityRegistry>,
    pub committed_columns: Option<&'a dyn CommittedColumnProvider>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextValues {
    pub fields: BTreeMap<ir::ContextFieldId, TypedValue>,
}

impl ContextValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: ir::ContextFieldId, value: TypedValue) {
        self.fields.insert(id, value);
    }
}

pub trait CapabilityHandler: Send + Sync {
    fn id(&self) -> ir::CapabilityId;
    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError>;
}

pub trait CommittedColumnProvider: Send + Sync {
    fn get_column(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<Vec<TypedColumnEntry>, TabulaError>;
}

#[derive(Default)]
pub struct CapabilityRegistry {
    handlers: Vec<Box<dyn CapabilityHandler>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        handler: impl CapabilityHandler + 'static,
    ) -> Result<(), TabulaError> {
        let id = handler.id();
        if self.contains(id) {
            return Err(TabulaError::InvalidIr(format!(
                "duplicate capability ID {}",
                id.0
            )));
        }
        self.handlers.push(Box::new(handler));
        Ok(())
    }

    pub fn get(&self, id: ir::CapabilityId) -> Result<&dyn CapabilityHandler, TabulaError> {
        self.handlers
            .iter()
            .find(|handler| handler.id() == id)
            .map(AsRef::as_ref)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown capability ID {}", id.0)))
    }

    pub fn contains(&self, id: ir::CapabilityId) -> bool {
        self.handlers.iter().any(|handler| handler.id() == id)
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedEntry {
    pub index: usize,
    pub definition: ir::Entry,
    params_by_id: BTreeMap<ir::ParamId, usize>,
    locals_by_id: BTreeMap<ir::LocalId, TypeId>,
}

impl ResolvedEntry {
    fn param_value(
        &self,
        id: ir::ParamId,
        params: &[TypedValue],
    ) -> Result<TypedValue, TabulaError> {
        let index = self.params_by_id.get(&id).ok_or_else(|| {
            TabulaError::InvalidIr(format!(
                "entry {} references unknown param {}",
                self.definition.symbol, id.0
            ))
        })?;
        Ok(params[*index].clone())
    }

    fn local_type(&self, id: ir::LocalId) -> Result<TypeId, TabulaError> {
        self.locals_by_id.get(&id).copied().ok_or_else(|| {
            TabulaError::InvalidIr(format!(
                "entry {} references unknown local {}",
                self.definition.symbol, id.0
            ))
        })
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedTable {
    pub schema: ir::TableSchema,
    fields: BTreeMap<ir::FieldId, ir::FieldSchema>,
}

#[derive(Debug, Clone)]
pub struct ResolvedExecutionProgram {
    program: Arc<ir::ValidatedProgram>,
    context_fields: BTreeMap<ir::ContextFieldId, ir::ContextField>,
    consts: BTreeMap<ir::ConstId, ir::ConstantEntry>,
    entries: BTreeMap<ir::EntryId, ResolvedEntry>,
    tables: BTreeMap<ir::TableId, ResolvedTable>,
    relations: BTreeMap<ir::RelationId, ir::RelationManifestEntry>,
    capabilities: BTreeMap<ir::CapabilityId, ir::CapabilityDescriptor>,
    events: BTreeMap<ir::EventId, ir::EventDescriptor>,
}

impl ResolvedExecutionProgram {
    pub fn from_validated_program(program: ir::ValidatedProgram) -> Result<Self, TabulaError> {
        Self::from_shared_program(Arc::new(program))
    }

    pub fn from_shared_program(program: Arc<ir::ValidatedProgram>) -> Result<Self, TabulaError> {
        let raw = program.as_program();
        let context_fields = raw
            .context
            .fields
            .iter()
            .cloned()
            .map(|field| (field.id, field))
            .collect();
        let consts = raw
            .const_pool
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();
        let entries = raw
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let params_by_id = entry
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| (param.id, index))
                    .collect();
                let locals_by_id = entry
                    .body
                    .locals
                    .iter()
                    .map(|local| (local.id, local.ty))
                    .collect();
                (
                    entry.id,
                    ResolvedEntry {
                        index,
                        definition: entry.clone(),
                        params_by_id,
                        locals_by_id,
                    },
                )
            })
            .collect();
        let tables = raw
            .state
            .tables
            .iter()
            .cloned()
            .map(|schema| {
                let fields = schema
                    .fields
                    .iter()
                    .cloned()
                    .map(|field| (field.id, field))
                    .collect();
                (schema.id, ResolvedTable { schema, fields })
            })
            .collect();
        let relations = raw
            .relation_manifest
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();
        let capabilities = raw
            .capability_manifest
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();
        let events = raw
            .event_manifest
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();
        Ok(Self {
            program,
            context_fields,
            consts,
            entries,
            tables,
            relations,
            capabilities,
            events,
        })
    }

    pub fn validated_program(&self) -> &ir::ValidatedProgram {
        self.program.as_ref()
    }

    pub fn program(&self) -> &ir::Program {
        self.program.as_program()
    }

    pub fn entry(&self, id: ir::EntryId) -> Result<&ResolvedEntry, TabulaError> {
        self.entries
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown entry ID {}", id.0)))
    }

    pub fn table(&self, id: ir::TableId) -> Result<&ResolvedTable, TabulaError> {
        self.tables
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown table {}", id.0)))
    }

    pub fn field_type(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<TypeId, TabulaError> {
        self.table(table)?
            .fields
            .get(&field)
            .map(|field| field.ty)
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown table/field {}.{}", table.0, field.0))
            })
    }

    pub fn relation(&self, id: ir::RelationId) -> Result<&ir::RelationManifestEntry, TabulaError> {
        self.relations
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown relation {}", id.0)))
    }

    pub fn capability(
        &self,
        id: ir::CapabilityId,
    ) -> Result<&ir::CapabilityDescriptor, TabulaError> {
        self.capabilities
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown capability {}", id.0)))
    }

    pub fn event(&self, id: ir::EventId) -> Result<&ir::EventDescriptor, TabulaError> {
        self.events
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown event {}", id.0)))
    }

    pub fn const_entry(&self, id: ir::ConstId) -> Result<&ir::ConstantEntry, TabulaError> {
        self.consts
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown const {}", id.0)))
    }

    pub fn context_field(&self, id: ir::ContextFieldId) -> Result<&ir::ContextField, TabulaError> {
        self.context_fields
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown context field {}", id.0)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryExecutionResult {
    pub returns: Vec<TypedValue>,
    pub state_summary: ExecutionStateSummary,
    pub state_effects: Vec<TypedStateEffect>,
    pub property_effects: Vec<StatePropertyEffect>,
    pub relation_effects: Vec<RelationEffect>,
    pub capability_effects: Vec<CapabilityEffect>,
    pub event_effects: Vec<TypedEventEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxCall {
    pub entry_id: ir::EntryId,
    pub params: Vec<TypedValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionJournal {
    pub state_summary: ExecutionStateSummary,
    pub txs: Vec<TxExecutionOutcome>,
}

impl ExecutionJournal {
    pub fn successful_txs(&self) -> impl Iterator<Item = &SuccessfulTxExecution> + '_ {
        self.txs.iter().filter_map(TxExecutionOutcome::success)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxExecutionOutcome {
    Success(SuccessfulTxExecution),
    Failed(FailedTxExecution),
}

impl TxExecutionOutcome {
    pub fn success(&self) -> Option<&SuccessfulTxExecution> {
        match self {
            Self::Success(success) => Some(success),
            Self::Failed(_) => None,
        }
    }

    pub fn failure(&self) -> Option<&FailedTxExecution> {
        match self {
            Self::Success(_) => None,
            Self::Failed(failure) => Some(failure),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuccessfulTxExecution {
    pub tx_index: u32,
    pub entry_id: ir::EntryId,
    pub state_effects: Vec<TypedStateEffect>,
    pub property_effects: Vec<StatePropertyEffect>,
    pub relation_effects: Vec<RelationEffect>,
    pub capability_effects: Vec<CapabilityEffect>,
    pub event_effects: Vec<TypedEventEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedTxExecution {
    pub tx_index: u32,
    pub entry_id: ir::EntryId,
    pub reason: String,
    pub failed_op_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionStateSummary {
    pub read_set_old: Vec<TypedStateSnapshot>,
    pub write_set_final: Vec<TypedStateWrite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStateSnapshot {
    pub key: CellKey,
    pub type_id: TypeId,
    pub value: Option<TypedValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStateWrite {
    pub key: CellKey,
    pub type_id: TypeId,
    pub value: Option<TypedValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateEffectKind {
    Read,
    Write,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStateEffect {
    pub key: CellKey,
    pub type_id: TypeId,
    pub kind: StateEffectKind,
    pub value: Option<TypedValue>,
    pub logical_time: u64,
    pub effect_ordinal_in_entry: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatePropertyEffect {
    pub table: ir::TableId,
    pub field: ir::FieldId,
    pub query: ir::StatePropertyQuery,
    pub outputs: Vec<TypedValue>,
    pub effect_ordinal_in_entry: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationEffectKind {
    Assert,
    Eval,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationEffect {
    pub relation: ir::RelationId,
    pub kind: RelationEffectKind,
    pub inputs: Vec<TypedValue>,
    pub outputs: Vec<TypedValue>,
    pub effect_ordinal_in_entry: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityEffect {
    pub capability: ir::CapabilityId,
    pub inputs: Vec<TypedValue>,
    pub outputs: Vec<TypedValue>,
    pub effect_ordinal_in_entry: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedEventEffect {
    pub event: ir::EventId,
    pub args: Vec<TypedValue>,
    pub effect_ordinal_in_entry: u32,
}

pub fn execute_query<S: StateView>(
    program: &ResolvedExecutionProgram,
    entry_id: ir::EntryId,
    params: &[TypedValue],
    context: &ContextValues,
    snapshot: &S,
    exec: &ExecContext<'_>,
) -> Result<QueryExecutionResult, ExecuteError> {
    validate_context(program, context, exec.type_runtimes)
        .map_err(|error| ExecuteError { error, op_index: 0 })?;
    let entry = program
        .entry(entry_id)
        .map_err(|error| ExecuteError { error, op_index: 0 })?;
    if entry.definition.kind != ir::EntryKind::Query {
        return Err(ExecuteError {
            error: TabulaError::InvalidIr(format!(
                "entry {} is not a query",
                entry.definition.symbol
            )),
            op_index: 0,
        });
    }
    validate_params(entry, params, exec.type_runtimes)
        .map_err(|error| ExecuteError { error, op_index: 0 })?;

    let mut overlay = Overlay::new(snapshot, exec.type_runtimes);
    let machine = EntryMachineCore::new(program, entry, params, context, &mut overlay, exec, 0);
    let result = machine.execute().map_err(|trap| ExecuteError {
        error: trap.error,
        op_index: trap.op_index,
    })?;
    let overlay = overlay
        .into_result()
        .map_err(|error| ExecuteError { error, op_index: 0 })?;

    Ok(QueryExecutionResult {
        returns: result.returns,
        state_summary: map_overlay_summary(overlay),
        state_effects: result.state_effects,
        property_effects: result.property_effects,
        relation_effects: result.relation_effects,
        capability_effects: result.capability_effects,
        event_effects: result.event_effects,
    })
}

pub fn execute_batch<S: StateView>(
    program: &ResolvedExecutionProgram,
    txs: &[TxCall],
    context: &ContextValues,
    snapshot: &S,
    exec: &ExecContext<'_>,
) -> Result<ExecutionJournal, TabulaError> {
    validate_context(program, context, exec.type_runtimes)?;

    let mut overlay = Overlay::new(snapshot, exec.type_runtimes);
    let mut outcomes = Vec::with_capacity(txs.len());
    let mut next_logical_time = 0u64;

    for (tx_index, tx) in txs.iter().enumerate() {
        let tx_index = tx_index as u32;
        let entry = match program.entry(tx.entry_id) {
            Ok(entry) => entry,
            Err(error) => {
                outcomes.push(TxExecutionOutcome::Failed(FailedTxExecution {
                    tx_index,
                    entry_id: tx.entry_id,
                    reason: error.to_string(),
                    failed_op_index: None,
                }));
                continue;
            }
        };
        if entry.definition.kind != ir::EntryKind::Tx {
            outcomes.push(TxExecutionOutcome::Failed(FailedTxExecution {
                tx_index,
                entry_id: tx.entry_id,
                reason: format!("entry {} is not a tx", entry.definition.symbol),
                failed_op_index: None,
            }));
            continue;
        }
        if let Err(error) = validate_params(entry, &tx.params, exec.type_runtimes) {
            outcomes.push(TxExecutionOutcome::Failed(FailedTxExecution {
                tx_index,
                entry_id: tx.entry_id,
                reason: error.to_string(),
                failed_op_index: None,
            }));
            continue;
        }

        overlay.checkpoint();
        let machine = EntryMachineCore::new(
            program,
            entry,
            &tx.params,
            context,
            &mut overlay,
            exec,
            next_logical_time,
        );
        match machine.execute() {
            Ok(result) => {
                overlay.discard_checkpoint();
                next_logical_time = result.next_logical_time;
                outcomes.push(TxExecutionOutcome::Success(SuccessfulTxExecution {
                    tx_index,
                    entry_id: tx.entry_id,
                    state_effects: result.state_effects,
                    property_effects: result.property_effects,
                    relation_effects: result.relation_effects,
                    capability_effects: result.capability_effects,
                    event_effects: result.event_effects,
                }));
            }
            Err(trap) => {
                overlay.rollback();
                match trap.kind {
                    TrapKind::Semantic => {
                        outcomes.push(TxExecutionOutcome::Failed(FailedTxExecution {
                            tx_index,
                            entry_id: tx.entry_id,
                            reason: trap.error.to_string(),
                            failed_op_index: Some(trap.op_index),
                        }));
                    }
                    TrapKind::Fatal => return Err(trap.error),
                }
            }
        }
    }

    let overlay = overlay.into_result()?;
    Ok(ExecutionJournal {
        state_summary: map_overlay_summary(overlay),
        txs: outcomes,
    })
}

struct EntryExecution {
    returns: Vec<TypedValue>,
    state_effects: Vec<TypedStateEffect>,
    property_effects: Vec<StatePropertyEffect>,
    relation_effects: Vec<RelationEffect>,
    capability_effects: Vec<CapabilityEffect>,
    event_effects: Vec<TypedEventEffect>,
    next_logical_time: u64,
}

struct EntryMachineCore<'a, 'snap, 'exec, S: StateView> {
    program: &'a ResolvedExecutionProgram,
    entry: &'a ResolvedEntry,
    params: &'a [TypedValue],
    context: &'a ContextValues,
    overlay: &'a mut Overlay<'snap, S>,
    exec: &'exec ExecContext<'exec>,
    locals: BTreeMap<ir::LocalId, TypedValue>,
    state_effects: Vec<TypedStateEffect>,
    relation_effects: Vec<RelationEffect>,
    property_effects: Vec<StatePropertyEffect>,
    capability_effects: Vec<CapabilityEffect>,
    event_effects: Vec<TypedEventEffect>,
    logical_time: u64,
    next_effect_ordinal: u32,
}

impl<'a, 'snap, 'exec, S: StateView> EntryMachineCore<'a, 'snap, 'exec, S> {
    fn new(
        program: &'a ResolvedExecutionProgram,
        entry: &'a ResolvedEntry,
        params: &'a [TypedValue],
        context: &'a ContextValues,
        overlay: &'a mut Overlay<'snap, S>,
        exec: &'exec ExecContext<'exec>,
        start_logical_time: u64,
    ) -> Self {
        Self {
            program,
            entry,
            params,
            context,
            overlay,
            exec,
            locals: BTreeMap::new(),
            state_effects: Vec::new(),
            relation_effects: Vec::new(),
            property_effects: Vec::new(),
            capability_effects: Vec::new(),
            event_effects: Vec::new(),
            logical_time: start_logical_time,
            next_effect_ordinal: 0,
        }
    }

    fn execute(mut self) -> Result<EntryExecution, EntryTrap> {
        let mut returns = Vec::new();
        for (op_index, op) in self.entry.definition.body.ops.iter().enumerate() {
            match op {
                ir::Op::Return { values } => {
                    returns = self
                        .eval_tuple(values)
                        .map_err(OpFailure::semantic)
                        .map_err(|failure| failure.at(op_index))?;
                }
                _ => self
                    .execute_op(op)
                    .map_err(|failure| failure.at(op_index))?,
            }
        }

        Ok(EntryExecution {
            returns,
            state_effects: self.state_effects,
            property_effects: self.property_effects,
            relation_effects: self.relation_effects,
            capability_effects: self.capability_effects,
            event_effects: self.event_effects,
            next_logical_time: self.logical_time,
        })
    }

    fn execute_op(&mut self, op: &ir::Op) -> Result<(), OpFailure> {
        match op {
            ir::Op::Arith { dst, op, lhs, rhs } => {
                let lhs = semantic(self.eval_value(lhs))?;
                let rhs = semantic(self.eval_value(rhs))?;
                let runtime = semantic(self.exec.type_runtimes.resolve(lhs.type_id()))?;
                let value = semantic(runtime.apply_arithmetic(map_arith(*op), &lhs, &rhs))?;
                self.assign_local(*dst, value);
            }
            ir::Op::Cmp { dst, op, lhs, rhs } => {
                let lhs = semantic(self.eval_value(lhs))?;
                let rhs = semantic(self.eval_value(rhs))?;
                let runtime = semantic(self.exec.type_runtimes.resolve(lhs.type_id()))?;
                let result = match op {
                    ir::CmpOp::Eq => semantic(runtime.eq_value(&lhs, &rhs))?,
                    ir::CmpOp::Ne => !semantic(runtime.eq_value(&lhs, &rhs))?,
                    ir::CmpOp::Lt => {
                        semantic(runtime.cmp_value(&lhs, &rhs))? == std::cmp::Ordering::Less
                    }
                    ir::CmpOp::Lte => {
                        semantic(runtime.cmp_value(&lhs, &rhs))? != std::cmp::Ordering::Greater
                    }
                    ir::CmpOp::Gt => {
                        semantic(runtime.cmp_value(&lhs, &rhs))? == std::cmp::Ordering::Greater
                    }
                    ir::CmpOp::Gte => {
                        semantic(runtime.cmp_value(&lhs, &rhs))? != std::cmp::Ordering::Less
                    }
                };
                self.assign_local(*dst, bool_typed(result));
            }
            ir::Op::Not { dst, src } => {
                let src = semantic(self.eval_value(src))?;
                self.assign_local(
                    *dst,
                    bool_typed(!semantic(typed_bool(&src, self.exec.type_runtimes))?),
                );
            }
            ir::Op::And { dst, lhs, rhs } => {
                let lhs = semantic(self.eval_value(lhs))?;
                let rhs = semantic(self.eval_value(rhs))?;
                self.assign_local(
                    *dst,
                    bool_typed(
                        semantic(typed_bool(&lhs, self.exec.type_runtimes))?
                            && semantic(typed_bool(&rhs, self.exec.type_runtimes))?,
                    ),
                );
            }
            ir::Op::Or { dst, lhs, rhs } => {
                let lhs = semantic(self.eval_value(lhs))?;
                let rhs = semantic(self.eval_value(rhs))?;
                self.assign_local(
                    *dst,
                    bool_typed(
                        semantic(typed_bool(&lhs, self.exec.type_runtimes))?
                            || semantic(typed_bool(&rhs, self.exec.type_runtimes))?,
                    ),
                );
            }
            ir::Op::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => {
                let cond = semantic(self.eval_value(cond))?;
                let selected = if semantic(typed_bool(&cond, self.exec.type_runtimes))? {
                    semantic(self.eval_value(if_true))?
                } else {
                    semantic(self.eval_value(if_false))?
                };
                self.assign_local(*dst, selected);
            }
            ir::Op::Hash {
                dst,
                family: ir::HashFamily::Poseidon,
                inputs,
            } => {
                let portable_inputs = semantic(self.eval_tuple_portable(inputs))?;
                self.assign_local(
                    *dst,
                    bytes32_typed(self.exec.hasher.hash_ir(&portable_inputs)),
                );
            }
            ir::Op::DivMod {
                guard,
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => {
                let lhs = semantic(self.eval_value(lhs))?;
                if !semantic(self.guard_active(*guard))? {
                    let zero = semantic(self.inactive_default(lhs.type_id()))?;
                    self.assign_local(*dst_q, zero.clone());
                    self.assign_local(*dst_r, zero);
                } else {
                    let rhs = semantic(self.eval_value(rhs))?;
                    let runtime = semantic(self.exec.type_runtimes.resolve(lhs.type_id()))?;
                    let (q, r) = semantic(runtime.divmod(&lhs, &rhs))?;
                    self.assign_local(*dst_q, q);
                    self.assign_local(*dst_r, r);
                }
            }
            ir::Op::ReadState {
                guard,
                dst_value,
                dst_present,
                table,
                key,
                field,
            } => {
                let field_ty = fatal(self.program.field_type(*table, *field))?;
                if !semantic(self.guard_active(*guard))? {
                    self.assign_local(*dst_value, semantic(self.inactive_default(field_ty))?);
                    self.assign_local(*dst_present, bool_typed(false));
                } else {
                    let key = fatal(self.resolve_cell_key(*table, *field, key))?;
                    let value = semantic(self.overlay.read(&key, field_ty))?;
                    self.record_state_effect(key, field_ty, StateEffectKind::Read, value.clone());
                    match value {
                        Some(value) => {
                            self.assign_local(*dst_value, value);
                            self.assign_local(*dst_present, bool_typed(true));
                        }
                        None => {
                            self.assign_local(
                                *dst_value,
                                semantic(self.inactive_default(field_ty))?,
                            );
                            self.assign_local(*dst_present, bool_typed(false));
                        }
                    }
                }
            }
            ir::Op::WriteState {
                guard,
                table,
                key,
                field,
                value,
            } => {
                if semantic(self.guard_active(*guard))? {
                    let field_ty = fatal(self.program.field_type(*table, *field))?;
                    let key = fatal(self.resolve_cell_key(*table, *field, key))?;
                    let value = semantic(self.eval_value(value))?;
                    self.record_state_effect(
                        key,
                        field_ty,
                        StateEffectKind::Write,
                        Some(value.clone()),
                    );
                    semantic(self.overlay.write(&key, Some(value), field_ty))?;
                }
            }
            ir::Op::DeleteState {
                guard,
                table,
                key,
                field,
            } => {
                if semantic(self.guard_active(*guard))? {
                    let field_ty = fatal(self.program.field_type(*table, *field))?;
                    let key = fatal(self.resolve_cell_key(*table, *field, key))?;
                    self.record_state_effect(key, field_ty, StateEffectKind::Delete, None);
                    semantic(self.overlay.write(&key, None, field_ty))?;
                }
            }
            ir::Op::ReadStateProperty {
                guard,
                dsts,
                table,
                field,
                query,
            } => {
                if !semantic(self.guard_active(*guard))? {
                    for dst in dsts {
                        let ty = fatal(self.entry.local_type(*dst))?;
                        self.assign_local(*dst, semantic(self.inactive_default(ty))?);
                    }
                } else {
                    let outputs = fatal(self.execute_state_property_read(
                        *table,
                        *field,
                        query.clone(),
                        dsts,
                    ))?;
                    self.record_property_effect(*table, *field, query.clone(), outputs);
                }
            }
            ir::Op::Assert { guard, cond } => {
                if semantic(self.guard_active(*guard))? {
                    let cond = semantic(self.eval_value(cond))?;
                    if !semantic(typed_bool(&cond, self.exec.type_runtimes))? {
                        return Err(OpFailure::semantic(TabulaError::AssertionFailed(
                            "assert".into(),
                        )));
                    }
                }
            }
            ir::Op::AssertRelation {
                guard,
                relation,
                args,
            } => {
                if semantic(self.guard_active(*guard))? {
                    let inputs = semantic(self.eval_tuple(args))?;
                    let relation_entry = fatal(self.program.relation(*relation).cloned())?;
                    let matched = semantic(relation_matches(
                        &relation_entry,
                        &inputs,
                        self.exec.type_runtimes,
                    ))?;
                    if !matched {
                        return Err(OpFailure::semantic(TabulaError::AssertionFailed(format!(
                            "relation assertion failed for {}",
                            relation_entry.descriptor.symbol
                        ))));
                    }
                    self.record_relation_effect(
                        *relation,
                        RelationEffectKind::Assert,
                        inputs,
                        vec![],
                    );
                }
            }
            ir::Op::EvalRelation {
                guard,
                relation,
                inputs,
                dsts,
            } => {
                let relation_entry = fatal(self.program.relation(*relation).cloned())?;
                if !semantic(self.guard_active(*guard))? {
                    for dst in dsts {
                        let ty = fatal(self.entry.local_type(*dst))?;
                        self.assign_local(*dst, semantic(self.inactive_default(ty))?);
                    }
                } else {
                    let inputs_typed = semantic(self.eval_tuple(inputs))?;
                    let outputs = semantic(relation_eval(
                        &relation_entry,
                        &inputs_typed,
                        self.exec.type_runtimes,
                    ))?;
                    if outputs.len() != dsts.len() {
                        return Err(OpFailure::fatal(TabulaError::InvalidIr(format!(
                            "relation {} output arity mismatch",
                            relation_entry.descriptor.symbol
                        ))));
                    }
                    for (dst, output) in dsts.iter().zip(outputs.iter().cloned()) {
                        self.assign_local(*dst, output);
                    }
                    self.record_relation_effect(
                        *relation,
                        RelationEffectKind::Eval,
                        inputs_typed,
                        outputs,
                    );
                }
            }
            ir::Op::CallCapability {
                guard,
                capability,
                inputs,
                dsts,
            } => {
                let capability_desc = fatal(self.program.capability(*capability).cloned())?;
                if !semantic(self.guard_active(*guard))? {
                    for dst in dsts {
                        let ty = fatal(self.entry.local_type(*dst))?;
                        self.assign_local(*dst, semantic(self.inactive_default(ty))?);
                    }
                } else {
                    let registry = self.exec.capabilities.ok_or_else(|| {
                        OpFailure::fatal(TabulaError::InvalidIr(
                            "capability call encountered but no CapabilityRegistry provided".into(),
                        ))
                    })?;
                    let handler = fatal(registry.get(*capability))?;
                    let inputs_typed = semantic(self.eval_tuple(inputs))?;
                    let outputs = match capability_desc.totality {
                        ir::CapabilityTotality::Total => fatal(handler.execute(&inputs_typed))?,
                        ir::CapabilityTotality::Checked => {
                            semantic(handler.execute(&inputs_typed))?
                        }
                    };
                    if outputs.len() != capability_desc.outputs.len() {
                        return Err(OpFailure::fatal(TabulaError::InvalidIr(format!(
                            "capability {} returned {} values but descriptor declares {}",
                            capability_desc.symbol,
                            outputs.len(),
                            capability_desc.outputs.len()
                        ))));
                    }
                    for (output, expected_ty) in outputs.iter().zip(&capability_desc.outputs) {
                        if output.type_id() != *expected_ty {
                            return Err(OpFailure::fatal(TabulaError::InvalidIr(format!(
                                "capability {} returned wrong output type",
                                capability_desc.symbol
                            ))));
                        }
                    }
                    for (dst, output) in dsts.iter().zip(outputs.iter().cloned()) {
                        self.assign_local(*dst, output);
                    }
                    let effect_ordinal_in_entry = self.next_effect_ordinal();
                    self.capability_effects.push(CapabilityEffect {
                        capability: *capability,
                        inputs: inputs_typed,
                        outputs,
                        effect_ordinal_in_entry,
                    });
                }
            }
            ir::Op::EmitEvent { guard, event, args } => {
                fatal(self.program.event(*event).map(|_| ()))?;
                if semantic(self.guard_active(*guard))? {
                    let args = semantic(self.eval_tuple(args))?;
                    let effect_ordinal_in_entry = self.next_effect_ordinal();
                    self.event_effects.push(TypedEventEffect {
                        event: *event,
                        args,
                        effect_ordinal_in_entry,
                    });
                }
            }
            ir::Op::Return { .. } => {}
        }
        Ok(())
    }

    fn eval_value(&self, value: &ir::ValueRef) -> Result<TypedValue, TabulaError> {
        match value {
            ir::ValueRef::Literal(value) => self.exec.type_runtimes.decode_portable(value),
            ir::ValueRef::Param(id) => self.entry.param_value(*id, self.params),
            ir::ValueRef::Context(id) => {
                self.context.fields.get(id).cloned().ok_or_else(|| {
                    TabulaError::InvalidIr(format!("missing context field {}", id.0))
                })
            }
            ir::ValueRef::Local(id) => self
                .locals
                .get(id)
                .cloned()
                .ok_or_else(|| TabulaError::InvalidIr(format!("unassigned local {}", id.0))),
            ir::ValueRef::Const(id) => {
                let entry = self.program.const_entry(*id)?;
                self.exec.type_runtimes.decode_portable(&entry.value)
            }
        }
    }

    fn eval_tuple(&self, values: &ir::ValueTupleRef) -> Result<Vec<TypedValue>, TabulaError> {
        values
            .0
            .iter()
            .map(|value| self.eval_value(value))
            .collect()
    }

    fn eval_tuple_portable(
        &self,
        values: &ir::ValueTupleRef,
    ) -> Result<Vec<tabula_core::PortableValue>, TabulaError> {
        self.eval_tuple(values)?
            .iter()
            .map(|value| self.exec.type_runtimes.encode_typed(value))
            .collect()
    }

    fn guard_active(&self, guard: Option<ir::GuardRef>) -> Result<bool, TabulaError> {
        match guard {
            Some(guard) => typed_bool(
                self.locals.get(&guard.0).ok_or_else(|| {
                    TabulaError::InvalidIr(format!("unassigned guard {}", guard.0.0))
                })?,
                self.exec.type_runtimes,
            ),
            None => Ok(true),
        }
    }

    fn assign_local(&mut self, id: ir::LocalId, value: TypedValue) {
        self.locals.insert(id, value);
    }

    fn resolve_cell_key(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        key: &ir::ValueTupleRef,
    ) -> Result<CellKey, TabulaError> {
        let values = self.eval_tuple(key)?;
        let row = self.decode_table_row_key(table, &values)?;
        Ok(CellKey {
            table: table.into(),
            col: field.into(),
            row,
        })
    }

    fn inactive_default(&self, ty: TypeId) -> Result<TypedValue, TabulaError> {
        self.exec.type_runtimes.zero_of(ty)
    }

    fn record_state_effect(
        &mut self,
        key: CellKey,
        type_id: TypeId,
        kind: StateEffectKind,
        value: Option<TypedValue>,
    ) {
        let effect_ordinal_in_entry = self.next_effect_ordinal();
        self.state_effects.push(TypedStateEffect {
            key,
            type_id,
            kind,
            value,
            logical_time: self.logical_time,
            effect_ordinal_in_entry,
        });
        self.logical_time += 1;
    }

    fn record_relation_effect(
        &mut self,
        relation: ir::RelationId,
        kind: RelationEffectKind,
        inputs: Vec<TypedValue>,
        outputs: Vec<TypedValue>,
    ) {
        let effect_ordinal_in_entry = self.next_effect_ordinal();
        self.relation_effects.push(RelationEffect {
            relation,
            kind,
            inputs,
            outputs,
            effect_ordinal_in_entry,
        });
    }

    fn record_property_effect(
        &mut self,
        table: ir::TableId,
        field: ir::FieldId,
        query: ir::StatePropertyQuery,
        outputs: Vec<TypedValue>,
    ) {
        let effect_ordinal_in_entry = self.next_effect_ordinal();
        self.property_effects.push(StatePropertyEffect {
            table,
            field,
            query,
            outputs,
            effect_ordinal_in_entry,
        });
    }

    fn execute_state_property_read(
        &mut self,
        table: ir::TableId,
        field: ir::FieldId,
        query: ir::StatePropertyQuery,
        dsts: &[ir::LocalId],
    ) -> Result<Vec<TypedValue>, TabulaError> {
        match query {
            ir::StatePropertyQuery::Minimum => self.execute_row_subset_property_query(
                table,
                field,
                None,
                RowSubsetKind::Minimum,
                dsts,
            ),
            ir::StatePropertyQuery::Maximum => self.execute_row_subset_property_query(
                table,
                field,
                None,
                RowSubsetKind::Maximum,
                dsts,
            ),
            ir::StatePropertyQuery::Successor { key } => {
                let key_value = self.eval_single_key_component(table, &key)?;
                self.execute_row_subset_property_query(
                    table,
                    field,
                    Some(key_value),
                    RowSubsetKind::Successor,
                    dsts,
                )
            }
            ir::StatePropertyQuery::Predecessor { key } => {
                let key_value = self.eval_single_key_component(table, &key)?;
                self.execute_row_subset_property_query(
                    table,
                    field,
                    Some(key_value),
                    RowSubsetKind::Predecessor,
                    dsts,
                )
            }
            ir::StatePropertyQuery::Aggregate { .. } => Err(TabulaError::InvalidIr(
                "ReadStateProperty Aggregate is not yet supported in V1 adapter".into(),
            )),
            ir::StatePropertyQuery::NonExistenceRange { .. } => Err(TabulaError::InvalidIr(
                "ReadStateProperty NonExistenceRange is not yet supported in V1 adapter".into(),
            )),
        }
    }

    fn execute_row_subset_property_query(
        &mut self,
        table: ir::TableId,
        field: ir::FieldId,
        pivot: Option<RowKey>,
        kind: RowSubsetKind,
        dsts: &[ir::LocalId],
    ) -> Result<Vec<TypedValue>, TabulaError> {
        if dsts.len() != 3 {
            return Err(TabulaError::InvalidIr(
                "row-oriented property reads require exactly 3 destinations".into(),
            ));
        }
        let provider = self.exec.committed_columns.ok_or_else(|| {
            TabulaError::InvalidIr(
                "ReadStateProperty encountered but no CommittedColumnProvider was provided".into(),
            )
        })?;
        let key_ty = self.v1_single_key_type(table)?;
        let field_ty = self.program.field_type(table, field)?;
        let entries = provider.get_column(table, field)?;
        for entry in &entries {
            if entry.value.type_id() != field_ty {
                return Err(TabulaError::InvalidIr(format!(
                    "committed column {}.{} yielded value type {} but field type is {}",
                    table.0,
                    field.0,
                    entry.value.type_id().0,
                    field_ty.0
                )));
            }
        }
        let selected = match kind {
            RowSubsetKind::Minimum => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .min_by_key(|entry| entry.row_key.0),
            RowSubsetKind::Maximum => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .max_by_key(|entry| entry.row_key.0),
            RowSubsetKind::Successor => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .filter(|entry| Some(entry.row_key) > pivot)
                .min_by_key(|entry| entry.row_key.0),
            RowSubsetKind::Predecessor => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .filter(|entry| Some(entry.row_key) < pivot)
                .max_by_key(|entry| entry.row_key.0),
        };

        let outputs = if let Some(entry) = selected {
            vec![
                entry.value.clone(),
                self.row_key_typed(entry.row_key, key_ty)?,
                bool_typed(false),
            ]
        } else {
            vec![
                self.inactive_default(field_ty)?,
                self.inactive_default(key_ty)?,
                bool_typed(true),
            ]
        };

        for (dst, output) in dsts.iter().zip(outputs.iter().cloned()) {
            self.assign_local(*dst, output);
        }
        Ok(outputs)
    }

    fn eval_single_key_component(
        &self,
        table: ir::TableId,
        key: &ir::ValueTupleRef,
    ) -> Result<RowKey, TabulaError> {
        let values = self.eval_tuple(key)?;
        self.decode_table_row_key(table, &values)
    }

    fn decode_table_row_key(
        &self,
        table: ir::TableId,
        values: &[TypedValue],
    ) -> Result<RowKey, TabulaError> {
        let key_ty = self.v1_single_key_type(table)?;
        if values.len() != 1 {
            return Err(TabulaError::InvalidIr(
                "V1 canonical executor only supports single-component state keys".into(),
            ));
        }
        decode_row_key(&values[0], key_ty, self.exec.type_runtimes)
    }

    fn v1_single_key_type(&self, table: ir::TableId) -> Result<TypeId, TabulaError> {
        let schema = &self.program.table(table)?.schema;
        if schema.key_tys.as_slice() != [TYPE_U64_ID] {
            return Err(TabulaError::InvalidIr(format!(
                "V1 canonical executor only supports [u64] key schema, table {} declared {:?}",
                table.0,
                schema.key_tys.iter().map(|ty| ty.0).collect::<Vec<_>>()
            )));
        }
        Ok(schema.key_tys[0])
    }

    fn row_key_typed(&self, row: RowKey, key_ty: TypeId) -> Result<TypedValue, TabulaError> {
        if key_ty != TYPE_U64_ID {
            return Err(TabulaError::InvalidIr(format!(
                "V1 canonical executor only supports u64 key outputs, got {}",
                key_ty.0
            )));
        }
        self.exec
            .type_runtimes
            .decode_portable(&tabula_core::PortableValue::new(
                TYPE_U64_ID,
                borsh::to_vec(&row.0)
                    .map_err(|error| TabulaError::BorshEncodingError(error.to_string()))?,
            ))
    }

    fn next_effect_ordinal(&mut self) -> u32 {
        let current = self.next_effect_ordinal;
        self.next_effect_ordinal += 1;
        current
    }
}

#[derive(Debug, Clone, Copy)]
enum RowSubsetKind {
    Minimum,
    Maximum,
    Successor,
    Predecessor,
}

fn validate_context(
    program: &ResolvedExecutionProgram,
    context: &ContextValues,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<(), TabulaError> {
    if context.fields.len() != program.program().context.fields.len() {
        return Err(TabulaError::ParamSchemaMismatch(format!(
            "program expects {} context values but got {}",
            program.program().context.fields.len(),
            context.fields.len()
        )));
    }
    for field in &program.program().context.fields {
        let value = context.fields.get(&field.id).ok_or_else(|| {
            TabulaError::ParamSchemaMismatch(format!("missing context field {}", field.symbol))
        })?;
        if field.ty != value.type_id() {
            return Err(TabulaError::ParamSchemaMismatch(format!(
                "context field {} expects type {} but got {}",
                field.symbol,
                field.ty.0,
                value.type_id().0
            )));
        }
        type_runtimes.resolve(value.type_id())?.validate(value)?;
    }
    Ok(())
}

fn validate_params(
    entry: &ResolvedEntry,
    params: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<(), TabulaError> {
    if entry.definition.params.len() != params.len() {
        return Err(TabulaError::ParamSchemaMismatch(format!(
            "entry {} expects {} params but got {}",
            entry.definition.symbol,
            entry.definition.params.len(),
            params.len()
        )));
    }
    for (param, value) in entry.definition.params.iter().zip(params) {
        if param.ty != value.type_id() {
            return Err(TabulaError::ParamSchemaMismatch(format!(
                "param {} expects type {} but got {}",
                param.symbol,
                param.ty.0,
                value.type_id().0
            )));
        }
        type_runtimes.resolve(value.type_id())?.validate(value)?;
    }
    Ok(())
}

fn map_arith(op: ir::ArithOp) -> tabula_types::ArithmeticOp {
    match op {
        ir::ArithOp::Add => tabula_types::ArithmeticOp::Add,
        ir::ArithOp::Sub => tabula_types::ArithmeticOp::Sub,
        ir::ArithOp::Mul => tabula_types::ArithmeticOp::Mul,
    }
}

fn relation_matches(
    relation: &ir::RelationManifestEntry,
    inputs: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<bool, TabulaError> {
    let portable_inputs = encode_typed_values(inputs, type_runtimes)?;
    Ok(match &relation.binding {
        ir::RelationBinding::EnumSet { values } => {
            portable_inputs.len() == 1 && values.contains(&portable_inputs[0])
        }
        ir::RelationBinding::Map { rows } => rows.iter().any(|row| row.inputs == portable_inputs),
    })
}

fn relation_eval(
    relation: &ir::RelationManifestEntry,
    inputs: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<Vec<TypedValue>, TabulaError> {
    let portable_inputs = encode_typed_values(inputs, type_runtimes)?;
    match &relation.binding {
        ir::RelationBinding::EnumSet { .. } => Ok(Vec::new()),
        ir::RelationBinding::Map { rows } => rows
            .iter()
            .find(|row| row.inputs == portable_inputs)
            .ok_or_else(|| {
                TabulaError::AssertionFailed(format!(
                    "no relation row matched {}",
                    relation.descriptor.symbol
                ))
            })?
            .outputs
            .iter()
            .map(|value| type_runtimes.decode_portable(value))
            .collect(),
    }
}

fn encode_typed_values(
    values: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<Vec<tabula_core::PortableValue>, TabulaError> {
    values
        .iter()
        .map(|value| type_runtimes.encode_typed(value))
        .collect()
}

fn decode_row_key(
    value: &TypedValue,
    key_ty: TypeId,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<RowKey, TabulaError> {
    if key_ty != TYPE_U64_ID || value.type_id() != TYPE_U64_ID {
        return Err(TabulaError::InvalidIr(format!(
            "V1 canonical executor expects state keys to be u64, got {}",
            value.type_id().0
        )));
    }
    let portable = type_runtimes.encode_typed(value)?;
    let raw = borsh::from_slice::<u64>(portable.payload())
        .map_err(|error| TabulaError::BorshEncodingError(error.to_string()))?;
    Ok(RowKey(raw))
}

fn map_overlay_summary(overlay: crate::overlay::OverlayResult) -> ExecutionStateSummary {
    ExecutionStateSummary {
        read_set_old: overlay
            .read_set_old
            .into_iter()
            .map(|entry| TypedStateSnapshot {
                key: entry.key,
                type_id: entry.type_id,
                value: entry.value,
            })
            .collect(),
        write_set_final: overlay
            .write_set_final
            .into_iter()
            .map(|entry| TypedStateWrite {
                key: entry.key,
                type_id: entry.type_id,
                value: entry.value,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use borsh::to_vec;
    use tabula_core::PortableValue;
    use tabula_core::traits::Hasher;
    use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_U64_ID};
    use tabula_types::{TypeRuntimeRegistry, TypedColumnEntry, TypedValue, u64_typed};

    use super::*;

    struct XorHasher;

    impl Hasher for XorHasher {
        fn hash(&self, data: &[u8]) -> tabula_core::Digest {
            let mut out = [0u8; 32];
            for (index, byte) in data.iter().enumerate() {
                out[index % 32] ^= byte;
            }
            out
        }

        fn hash_pair(
            &self,
            left: &tabula_core::Digest,
            right: &tabula_core::Digest,
        ) -> tabula_core::Digest {
            let mut data = Vec::new();
            data.extend_from_slice(left);
            data.extend_from_slice(right);
            self.hash(&data)
        }
    }

    struct AddOneCapability;

    impl CapabilityHandler for AddOneCapability {
        fn id(&self) -> ir::CapabilityId {
            ir::CapabilityId(7)
        }

        fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
            let value = &inputs[0];
            let portable = PortableValue::new(
                value.type_id(),
                to_vec(&(borsh::from_slice::<u64>(value.payload()).unwrap() + 1)).unwrap(),
            );
            Ok(vec![
                TypeRuntimeRegistry::seeded()
                    .unwrap()
                    .decode_portable(&portable)
                    .unwrap(),
            ])
        }
    }

    struct FailOnInputCapability {
        fail_on: u64,
    }

    impl CapabilityHandler for FailOnInputCapability {
        fn id(&self) -> ir::CapabilityId {
            ir::CapabilityId(7)
        }

        fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
            let raw = borsh::from_slice::<u64>(inputs[0].payload())
                .map_err(|error| TabulaError::BorshEncodingError(error.to_string()))?;
            if raw == self.fail_on {
                return Err(TabulaError::AssertionFailed(format!(
                    "capability rejected input {raw}"
                )));
            }
            Ok(vec![u64_typed(raw + 1)])
        }
    }

    struct WrongArityCapability;

    impl CapabilityHandler for WrongArityCapability {
        fn id(&self) -> ir::CapabilityId {
            ir::CapabilityId(7)
        }

        fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
            Ok(vec![])
        }
    }

    struct WrongTypeCapability;

    impl CapabilityHandler for WrongTypeCapability {
        fn id(&self) -> ir::CapabilityId {
            ir::CapabilityId(7)
        }

        fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
            Ok(vec![bool_typed(true)])
        }
    }

    fn type_runtimes() -> TypeRuntimeRegistry {
        TypeRuntimeRegistry::seeded().expect("seeded runtimes")
    }

    fn portable_u64(value: u64) -> PortableValue {
        PortableValue::new(TYPE_U64_ID, to_vec(&value).unwrap())
    }

    fn raw_program() -> ir::Program {
        ir::Program {
            program_id: ir::ProgramId(0),
            state: ir::StateSchema {
                tables: vec![ir::TableSchema {
                    id: ir::TableId(1),
                    symbol: "accounts".into(),
                    key_tys: vec![TYPE_U64_ID],
                    fields: vec![ir::FieldSchema {
                        id: ir::FieldId(0),
                        symbol: "balance".into(),
                        ty: TYPE_U64_ID,
                    }],
                }],
            },
            context: ir::ContextSchema {
                fields: vec![ir::ContextField {
                    id: ir::ContextFieldId(0),
                    symbol: "epoch".into(),
                    ty: TYPE_U64_ID,
                }],
            },
            const_pool: ir::ConstantPool {
                entries: vec![ir::ConstantEntry {
                    id: ir::ConstId(0),
                    ty: TYPE_U64_ID,
                    value: portable_u64(5),
                }],
            },
            relation_manifest: ir::RelationManifest {
                entries: vec![
                    ir::RelationManifestEntry {
                        id: ir::RelationId(0),
                        descriptor: ir::RelationDescriptor {
                            symbol: "AllowedTier".into(),
                            inputs: vec![TYPE_U64_ID],
                            outputs: vec![],
                        },
                        binding: ir::RelationBinding::EnumSet {
                            values: vec![portable_u64(1), portable_u64(2)],
                        },
                    },
                    ir::RelationManifestEntry {
                        id: ir::RelationId(1),
                        descriptor: ir::RelationDescriptor {
                            symbol: "FeeForTier".into(),
                            inputs: vec![TYPE_U64_ID],
                            outputs: vec![TYPE_U64_ID],
                        },
                        binding: ir::RelationBinding::Map {
                            rows: vec![
                                ir::RelationRow {
                                    inputs: vec![portable_u64(1)],
                                    outputs: vec![portable_u64(10)],
                                },
                                ir::RelationRow {
                                    inputs: vec![portable_u64(2)],
                                    outputs: vec![portable_u64(20)],
                                },
                            ],
                        },
                    },
                ],
            },
            capability_manifest: ir::CapabilityManifest {
                entries: vec![ir::CapabilityDescriptor {
                    id: ir::CapabilityId(7),
                    symbol: "add_one".into(),
                    inputs: vec![TYPE_U64_ID],
                    outputs: vec![TYPE_U64_ID],
                    totality: ir::CapabilityTotality::Total,
                    query_policy: ir::CapabilityQueryPolicy::QuerySafe,
                    proof_visibility: ir::CapabilityProofVisibility::Journaled,
                }],
            },
            event_manifest: ir::EventManifest {
                entries: vec![ir::EventDescriptor {
                    id: ir::EventId(0),
                    symbol: "Transfer".into(),
                    fields: vec![TYPE_U64_ID, TYPE_U64_ID],
                }],
            },
            entries: vec![
                ir::Entry {
                    id: ir::EntryId(0),
                    symbol: "balance_of".into(),
                    kind: ir::EntryKind::Query,
                    params: vec![ir::ParamDecl {
                        id: ir::ParamId(0),
                        symbol: "owner".into(),
                        ty: TYPE_U64_ID,
                    }],
                    returns: vec![TYPE_U64_ID, TYPE_BYTES32_ID],
                    return_policy: ir::ReturnPolicy::Explicit,
                    body: ir::Body {
                        locals: vec![
                            ir::LocalDecl {
                                id: ir::LocalId(0),
                                ty: TYPE_U64_ID,
                            },
                            ir::LocalDecl {
                                id: ir::LocalId(1),
                                ty: TYPE_BOOL_ID,
                            },
                            ir::LocalDecl {
                                id: ir::LocalId(2),
                                ty: TYPE_BYTES32_ID,
                            },
                        ],
                        ops: vec![
                            ir::Op::ReadState {
                                guard: None,
                                dst_value: ir::LocalId(0),
                                dst_present: ir::LocalId(1),
                                table: ir::TableId(1),
                                key: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                                field: ir::FieldId(0),
                            },
                            ir::Op::Hash {
                                dst: ir::LocalId(2),
                                family: ir::HashFamily::Poseidon,
                                inputs: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(
                                    0,
                                ))]),
                            },
                            ir::Op::Return {
                                values: ir::ValueTupleRef(vec![
                                    ir::ValueRef::Local(ir::LocalId(0)),
                                    ir::ValueRef::Local(ir::LocalId(2)),
                                ]),
                            },
                        ],
                    },
                },
                ir::Entry {
                    id: ir::EntryId(1),
                    symbol: "transfer".into(),
                    kind: ir::EntryKind::Tx,
                    params: vec![
                        ir::ParamDecl {
                            id: ir::ParamId(0),
                            symbol: "from".into(),
                            ty: TYPE_U64_ID,
                        },
                        ir::ParamDecl {
                            id: ir::ParamId(1),
                            symbol: "to".into(),
                            ty: TYPE_U64_ID,
                        },
                        ir::ParamDecl {
                            id: ir::ParamId(2),
                            symbol: "tier".into(),
                            ty: TYPE_U64_ID,
                        },
                    ],
                    returns: vec![],
                    return_policy: ir::ReturnPolicy::Unit,
                    body: ir::Body {
                        locals: vec![
                            ir::LocalDecl {
                                id: ir::LocalId(0),
                                ty: TYPE_U64_ID,
                            },
                            ir::LocalDecl {
                                id: ir::LocalId(1),
                                ty: TYPE_BOOL_ID,
                            },
                            ir::LocalDecl {
                                id: ir::LocalId(2),
                                ty: TYPE_U64_ID,
                            },
                        ],
                        ops: vec![
                            ir::Op::AssertRelation {
                                guard: None,
                                relation: ir::RelationId(0),
                                args: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(2))]),
                            },
                            ir::Op::EvalRelation {
                                guard: None,
                                relation: ir::RelationId(1),
                                inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(
                                    2,
                                ))]),
                                dsts: vec![ir::LocalId(0)],
                            },
                            ir::Op::CallCapability {
                                guard: None,
                                capability: ir::CapabilityId(7),
                                inputs: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(
                                    0,
                                ))]),
                                dsts: vec![ir::LocalId(2)],
                            },
                            ir::Op::WriteState {
                                guard: None,
                                table: ir::TableId(1),
                                key: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(1))]),
                                field: ir::FieldId(0),
                                value: ir::ValueRef::Local(ir::LocalId(2)),
                            },
                            ir::Op::EmitEvent {
                                guard: None,
                                event: ir::EventId(0),
                                args: ir::ValueTupleRef(vec![
                                    ir::ValueRef::Param(ir::ParamId(0)),
                                    ir::ValueRef::Local(ir::LocalId(2)),
                                ]),
                            },
                            ir::Op::Return {
                                values: ir::ValueTupleRef(vec![]),
                            },
                        ],
                    },
                },
            ],
        }
    }

    fn validated_program() -> ir::ValidatedProgram {
        ir::ValidatedProgram::try_from(raw_program()).expect("valid canonical program")
    }

    fn resolved_program() -> ResolvedExecutionProgram {
        ResolvedExecutionProgram::from_validated_program(validated_program())
            .expect("resolved execution program")
    }

    fn resolved_program_with_capability(
        totality: ir::CapabilityTotality,
        proof_visibility: ir::CapabilityProofVisibility,
    ) -> ResolvedExecutionProgram {
        let mut raw = raw_program();
        raw.capability_manifest.entries[0].totality = totality;
        raw.capability_manifest.entries[0].proof_visibility = proof_visibility;
        ResolvedExecutionProgram::from_validated_program(
            ir::ValidatedProgram::try_from(raw).expect("valid modified program"),
        )
        .expect("resolved execution program")
    }

    fn capability_query_program(totality: ir::CapabilityTotality) -> ResolvedExecutionProgram {
        ResolvedExecutionProgram::from_validated_program(
            ir::ValidatedProgram::try_from(ir::Program {
                program_id: ir::ProgramId(2),
                state: ir::StateSchema { tables: vec![] },
                context: ir::ContextSchema { fields: vec![] },
                const_pool: ir::ConstantPool { entries: vec![] },
                relation_manifest: ir::RelationManifest { entries: vec![] },
                capability_manifest: ir::CapabilityManifest {
                    entries: vec![ir::CapabilityDescriptor {
                        id: ir::CapabilityId(7),
                        symbol: "maybe_fail".into(),
                        inputs: vec![TYPE_U64_ID],
                        outputs: vec![TYPE_U64_ID],
                        totality,
                        query_policy: ir::CapabilityQueryPolicy::QuerySafe,
                        proof_visibility: ir::CapabilityProofVisibility::Journaled,
                    }],
                },
                event_manifest: ir::EventManifest { entries: vec![] },
                entries: vec![ir::Entry {
                    id: ir::EntryId(0),
                    symbol: "check".into(),
                    kind: ir::EntryKind::Query,
                    params: vec![ir::ParamDecl {
                        id: ir::ParamId(0),
                        symbol: "value".into(),
                        ty: TYPE_U64_ID,
                    }],
                    returns: vec![TYPE_U64_ID],
                    return_policy: ir::ReturnPolicy::Explicit,
                    body: ir::Body {
                        locals: vec![ir::LocalDecl {
                            id: ir::LocalId(0),
                            ty: TYPE_U64_ID,
                        }],
                        ops: vec![
                            ir::Op::CallCapability {
                                guard: None,
                                capability: ir::CapabilityId(7),
                                inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(
                                    0,
                                ))]),
                                dsts: vec![ir::LocalId(0)],
                            },
                            ir::Op::Return {
                                values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(
                                    0,
                                ))]),
                            },
                        ],
                    },
                }],
            })
            .expect("valid capability query program"),
        )
        .expect("resolved capability query program")
    }

    #[derive(Default)]
    struct MockCommittedColumns {
        columns: BTreeMap<(ir::TableId, ir::FieldId), Vec<TypedColumnEntry>>,
    }

    impl MockCommittedColumns {
        fn with_u64_column(
            mut self,
            table: ir::TableId,
            field: ir::FieldId,
            rows: &[(u64, u64, bool)],
        ) -> Self {
            self.columns.insert(
                (table, field),
                rows.iter()
                    .map(|(row_key, value, is_null)| TypedColumnEntry {
                        row_key: RowKey(*row_key),
                        value: u64_typed(*value),
                        is_null: *is_null,
                    })
                    .collect(),
            );
            self
        }
    }

    impl CommittedColumnProvider for MockCommittedColumns {
        fn get_column(
            &self,
            table: ir::TableId,
            field: ir::FieldId,
        ) -> Result<Vec<TypedColumnEntry>, TabulaError> {
            self.columns.get(&(table, field)).cloned().ok_or_else(|| {
                TabulaError::InvalidIr(format!("missing committed column {}.{}", table.0, field.0))
            })
        }
    }

    fn property_program(query: ir::StatePropertyQuery, key_ty: TypeId) -> ResolvedExecutionProgram {
        ResolvedExecutionProgram::from_validated_program(
            ir::ValidatedProgram::try_from(ir::Program {
                program_id: ir::ProgramId(1),
                state: ir::StateSchema {
                    tables: vec![ir::TableSchema {
                        id: ir::TableId(1),
                        symbol: "accounts".into(),
                        key_tys: vec![key_ty],
                        fields: vec![ir::FieldSchema {
                            id: ir::FieldId(0),
                            symbol: "balance".into(),
                            ty: TYPE_U64_ID,
                        }],
                    }],
                },
                context: ir::ContextSchema { fields: vec![] },
                const_pool: ir::ConstantPool { entries: vec![] },
                relation_manifest: ir::RelationManifest { entries: vec![] },
                capability_manifest: ir::CapabilityManifest { entries: vec![] },
                event_manifest: ir::EventManifest { entries: vec![] },
                entries: vec![ir::Entry {
                    id: ir::EntryId(0),
                    symbol: "property".into(),
                    kind: ir::EntryKind::Query,
                    params: vec![],
                    returns: vec![TYPE_U64_ID, key_ty, TYPE_BOOL_ID],
                    return_policy: ir::ReturnPolicy::Explicit,
                    body: ir::Body {
                        locals: vec![
                            ir::LocalDecl {
                                id: ir::LocalId(0),
                                ty: TYPE_U64_ID,
                            },
                            ir::LocalDecl {
                                id: ir::LocalId(1),
                                ty: key_ty,
                            },
                            ir::LocalDecl {
                                id: ir::LocalId(2),
                                ty: TYPE_BOOL_ID,
                            },
                        ],
                        ops: vec![
                            ir::Op::ReadStateProperty {
                                guard: None,
                                dsts: vec![ir::LocalId(0), ir::LocalId(1), ir::LocalId(2)],
                                table: ir::TableId(1),
                                field: ir::FieldId(0),
                                query,
                            },
                            ir::Op::Return {
                                values: ir::ValueTupleRef(vec![
                                    ir::ValueRef::Local(ir::LocalId(0)),
                                    ir::ValueRef::Local(ir::LocalId(1)),
                                    ir::ValueRef::Local(ir::LocalId(2)),
                                ]),
                            },
                        ],
                    },
                }],
            })
            .expect("valid property program"),
        )
        .expect("resolved property program")
    }

    #[test]
    fn query_reads_state_and_hashes_result() {
        let runtimes = type_runtimes();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: None,
        };
        let mut state = tabula_core::InMemoryState::new();
        state.set(
            CellKey {
                table: tabula_core::TableId(1),
                col: tabula_core::ColId(0),
                row: RowKey(9),
            },
            portable_u64(42),
        );
        let mut context = ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let result = execute_query(
            &resolved_program(),
            ir::EntryId(0),
            &[u64_typed(9)],
            &context,
            &state,
            &exec,
        )
        .expect("query succeeds");

        assert_eq!(result.returns[0], u64_typed(42));
        assert_eq!(result.returns[1].type_id(), TYPE_BYTES32_ID);
        assert_eq!(result.state_effects.len(), 1);
        assert!(result.state_summary.write_set_final.is_empty());
    }

    #[test]
    fn batch_tx_writes_state_records_relation_capability_and_event_effects() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities.register(AddOneCapability).unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let mut context = ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let journal = execute_batch(
            &resolved_program(),
            &[TxCall {
                entry_id: ir::EntryId(1),
                params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
            }],
            &context,
            &state,
            &exec,
        )
        .expect("batch succeeds");

        let tx = journal.successful_txs().next().expect("successful tx");
        assert_eq!(tx.relation_effects.len(), 2);
        assert_eq!(tx.capability_effects.len(), 1);
        assert_eq!(tx.event_effects.len(), 1);
        assert_eq!(journal.state_summary.write_set_final.len(), 1);
    }

    #[test]
    fn batch_keeps_failed_txs_separate_from_success_effects() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities.register(AddOneCapability).unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let mut context = ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let journal = execute_batch(
            &resolved_program(),
            &[
                TxCall {
                    entry_id: ir::EntryId(999),
                    params: vec![],
                },
                TxCall {
                    entry_id: ir::EntryId(1),
                    params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
                },
            ],
            &context,
            &state,
            &exec,
        )
        .expect("batch completes");

        assert_eq!(journal.txs.len(), 2);
        assert!(matches!(journal.txs[0], TxExecutionOutcome::Failed(_)));
        assert!(matches!(journal.txs[1], TxExecutionOutcome::Success(_)));
        assert_eq!(journal.state_summary.write_set_final.len(), 1);
    }

    #[test]
    fn opaque_capability_effects_stay_in_execution_journal() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities.register(AddOneCapability).unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let mut context = ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let journal = execute_batch(
            &resolved_program_with_capability(
                ir::CapabilityTotality::Total,
                ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
            ),
            &[TxCall {
                entry_id: ir::EntryId(1),
                params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
            }],
            &context,
            &state,
            &exec,
        )
        .expect("batch succeeds");

        let tx = journal.successful_txs().next().expect("successful tx");
        assert_eq!(tx.capability_effects.len(), 1);
    }

    #[test]
    fn checked_capability_failure_rolls_back_only_one_tx() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities
            .register(FailOnInputCapability { fail_on: 10 })
            .unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let mut context = ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let journal = execute_batch(
            &resolved_program_with_capability(
                ir::CapabilityTotality::Checked,
                ir::CapabilityProofVisibility::Journaled,
            ),
            &[
                TxCall {
                    entry_id: ir::EntryId(1),
                    params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
                },
                TxCall {
                    entry_id: ir::EntryId(1),
                    params: vec![u64_typed(3), u64_typed(4), u64_typed(2)],
                },
            ],
            &context,
            &state,
            &exec,
        )
        .expect("checked failure should not abort batch");

        assert!(matches!(journal.txs[0], TxExecutionOutcome::Failed(_)));
        assert!(matches!(journal.txs[1], TxExecutionOutcome::Success(_)));
        assert_eq!(journal.state_summary.write_set_final.len(), 1);
    }

    #[test]
    fn total_capability_failure_aborts_batch() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities
            .register(FailOnInputCapability { fail_on: 10 })
            .unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let mut context = ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let error = execute_batch(
            &resolved_program_with_capability(
                ir::CapabilityTotality::Total,
                ir::CapabilityProofVisibility::Journaled,
            ),
            &[TxCall {
                entry_id: ir::EntryId(1),
                params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
            }],
            &context,
            &state,
            &exec,
        )
        .expect_err("total capability failure should abort batch");

        assert!(error.to_string().contains("capability rejected input 10"));
    }

    #[test]
    fn query_checked_capability_failure_surfaces_execute_error() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities
            .register(FailOnInputCapability { fail_on: 0 })
            .unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();

        let error = execute_query(
            &capability_query_program(ir::CapabilityTotality::Checked),
            ir::EntryId(0),
            &[u64_typed(0)],
            &context,
            &state,
            &exec,
        )
        .expect_err("checked capability should fail query");

        assert!(
            error
                .error
                .to_string()
                .contains("capability rejected input 0")
        );
    }

    #[test]
    fn query_total_capability_failure_still_surfaces_execute_error() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities
            .register(FailOnInputCapability { fail_on: 0 })
            .unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();

        let error = execute_query(
            &capability_query_program(ir::CapabilityTotality::Total),
            ir::EntryId(0),
            &[u64_typed(0)],
            &context,
            &state,
            &exec,
        )
        .expect_err("total capability failure should fail query");

        assert!(
            error
                .error
                .to_string()
                .contains("capability rejected input 0")
        );
    }

    #[test]
    fn capability_output_arity_mismatch_is_fatal() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities.register(WrongArityCapability).unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let mut context = ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let error = execute_batch(
            &resolved_program(),
            &[TxCall {
                entry_id: ir::EntryId(1),
                params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
            }],
            &context,
            &state,
            &exec,
        )
        .expect_err("wrong output arity should abort batch");

        assert!(error.to_string().contains("returned 0 values"));
    }

    #[test]
    fn capability_output_type_mismatch_is_fatal() {
        let runtimes = type_runtimes();
        let mut capabilities = CapabilityRegistry::new();
        capabilities.register(WrongTypeCapability).unwrap();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = tabula_core::InMemoryState::new();
        let mut context = ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let error = execute_batch(
            &resolved_program(),
            &[TxCall {
                entry_id: ir::EntryId(1),
                params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
            }],
            &context,
            &state,
            &exec,
        )
        .expect_err("wrong output type should abort batch");

        assert!(error.to_string().contains("returned wrong output type"));
    }

    #[test]
    fn property_read_minimum_records_effect_and_returns_row_tuple() {
        let runtimes = type_runtimes();
        let committed = MockCommittedColumns::default().with_u64_column(
            ir::TableId(1),
            ir::FieldId(0),
            &[(10, 100, false), (5, 50, false), (20, 200, false)],
        );
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: Some(&committed),
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();
        let program = property_program(ir::StatePropertyQuery::Minimum, TYPE_U64_ID);

        let result = execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
            .expect("property read succeeds");

        assert_eq!(
            result.returns,
            vec![u64_typed(50), u64_typed(5), bool_typed(false)]
        );
        assert_eq!(result.property_effects.len(), 1);
        assert_eq!(result.property_effects[0].outputs, result.returns);
    }

    #[test]
    fn property_read_maximum_returns_greatest_row_key() {
        let runtimes = type_runtimes();
        let committed = MockCommittedColumns::default().with_u64_column(
            ir::TableId(1),
            ir::FieldId(0),
            &[(10, 100, false), (5, 50, false), (20, 200, false)],
        );
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: Some(&committed),
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();
        let program = property_program(ir::StatePropertyQuery::Maximum, TYPE_U64_ID);

        let result = execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
            .expect("property read succeeds");

        assert_eq!(
            result.returns,
            vec![u64_typed(200), u64_typed(20), bool_typed(false)]
        );
    }

    #[test]
    fn property_read_successor_and_predecessor_are_structural() {
        let runtimes = type_runtimes();
        let committed = MockCommittedColumns::default().with_u64_column(
            ir::TableId(1),
            ir::FieldId(0),
            &[(10, 100, false), (5, 50, false), (20, 200, false)],
        );
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: Some(&committed),
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();
        let successor = property_program(
            ir::StatePropertyQuery::Successor {
                key: ir::ValueTupleRef(vec![ir::ValueRef::Literal(portable_u64(10))]),
            },
            TYPE_U64_ID,
        );
        let predecessor = property_program(
            ir::StatePropertyQuery::Predecessor {
                key: ir::ValueTupleRef(vec![ir::ValueRef::Literal(portable_u64(10))]),
            },
            TYPE_U64_ID,
        );

        let successor_result =
            execute_query(&successor, ir::EntryId(0), &[], &context, &state, &exec)
                .expect("successor succeeds");
        let predecessor_result =
            execute_query(&predecessor, ir::EntryId(0), &[], &context, &state, &exec)
                .expect("predecessor succeeds");

        assert_eq!(
            successor_result.returns,
            vec![u64_typed(200), u64_typed(20), bool_typed(false)]
        );
        assert_eq!(
            predecessor_result.returns,
            vec![u64_typed(50), u64_typed(5), bool_typed(false)]
        );
    }

    #[test]
    fn property_read_no_match_returns_defaults_and_true_null_flag() {
        let runtimes = type_runtimes();
        let committed = MockCommittedColumns::default().with_u64_column(
            ir::TableId(1),
            ir::FieldId(0),
            &[(10, 100, false)],
        );
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: Some(&committed),
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();
        let program = property_program(
            ir::StatePropertyQuery::Successor {
                key: ir::ValueTupleRef(vec![ir::ValueRef::Literal(portable_u64(10))]),
            },
            TYPE_U64_ID,
        );

        let result = execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
            .expect("property read succeeds");

        assert_eq!(
            result.returns,
            vec![u64_typed(0), u64_typed(0), bool_typed(true)]
        );
    }

    #[test]
    fn property_read_aggregate_is_unsupported_in_v1_adapter() {
        let runtimes = type_runtimes();
        let committed = MockCommittedColumns::default();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: Some(&committed),
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();
        let program = property_program(
            ir::StatePropertyQuery::Aggregate {
                kind: ir::AggregateKind::Sum,
            },
            TYPE_U64_ID,
        );

        let error = execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
            .expect_err("aggregate should be unsupported");
        assert!(
            error
                .error
                .to_string()
                .contains("Aggregate is not yet supported in V1 adapter")
        );
    }

    #[test]
    fn property_read_non_existence_range_is_unsupported_in_v1_adapter() {
        let runtimes = type_runtimes();
        let committed = MockCommittedColumns::default();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: Some(&committed),
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();
        let program = property_program(
            ir::StatePropertyQuery::NonExistenceRange {
                lower: ir::ValueTupleRef(vec![ir::ValueRef::Literal(portable_u64(1))]),
                upper: ir::ValueTupleRef(vec![ir::ValueRef::Literal(portable_u64(2))]),
            },
            TYPE_U64_ID,
        );

        let error = execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
            .expect_err("non-existence range should be unsupported");
        assert!(
            error
                .error
                .to_string()
                .contains("NonExistenceRange is not yet supported in V1 adapter")
        );
    }

    #[test]
    fn property_read_rejects_non_u64_key_schema_in_v1_executor() {
        let runtimes = type_runtimes();
        let committed = MockCommittedColumns::default();
        let exec = ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: Some(&committed),
        };
        let state = tabula_core::InMemoryState::new();
        let context = ContextValues::new();
        let program = property_program(ir::StatePropertyQuery::Minimum, TYPE_BYTES32_ID);

        let error = execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
            .expect_err("non-u64 key schema should fail in V1");
        assert!(
            error
                .error
                .to_string()
                .contains("only supports [u64] key schema")
        );
    }
}
