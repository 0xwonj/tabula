use tabula_core::{CellKey, TypeId};
use tabula_ir as ir;
use tabula_types::TypedValue;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub op_index: usize,
    pub effect_ordinal_in_entry: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatePropertyEffect {
    pub table: ir::TableId,
    pub field: ir::FieldId,
    pub query: ir::StatePropertyQuery,
    pub outputs: Vec<TypedValue>,
    pub op_index: usize,
    pub effect_ordinal_in_entry: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub op_index: usize,
    pub effect_ordinal_in_entry: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityEffect {
    pub capability: ir::CapabilityId,
    pub inputs: Vec<TypedValue>,
    pub outputs: Vec<TypedValue>,
    pub op_index: usize,
    pub effect_ordinal_in_entry: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedEventEffect {
    pub event: ir::EventId,
    pub args: Vec<TypedValue>,
    pub op_index: usize,
    pub effect_ordinal_in_entry: u32,
}
