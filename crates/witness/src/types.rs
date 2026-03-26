//! Logical shared input types for witness-owned proof preparation.

use tabula_core::{CellKey, EncodingProfileId, LogicalTime, RowKey, TypeId};
use tabula_ir as ir;
use tabula_ir::StatePropertyQuery;
use tabula_types::{TypedPropertyQueryResult, TypedValue};

/// Logical committed-state entry for one row of a committed column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedEntry {
    /// Row key.
    pub row: RowKey,
    /// Logical cell value.
    pub value: TypedValue,
    /// Whether the entry is absent in committed state.
    pub is_null: bool,
}

/// Base-state seed for one committed cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitCell {
    /// The cell address.
    pub key: CellKey,
    /// The logical cell value.
    pub value: TypedValue,
    /// Whether the cell is absent in committed state.
    pub is_null: bool,
}

/// Execution-time access event grouped per column for proof preparation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessEvent {
    /// The cell address.
    pub key: CellKey,
    /// Logical time of this access.
    pub time: LogicalTime,
    /// Whether this event is a write.
    pub is_write: bool,
    /// The logical cell value observed or written by the executor.
    pub value: TypedValue,
    /// Whether the value is null.
    pub is_null: bool,
    /// Transaction index within the batch.
    pub tx_index: u32,
    /// Effect ordinal within the transaction.
    pub effect_ordinal_in_tx: u32,
}

/// Final coalesced write for one row of a committed column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnWrite {
    /// Target row key.
    pub row: RowKey,
    /// Final logical value. `None` means delete / write null.
    pub value: Option<TypedValue>,
}

/// Logical property-read claim extracted from execution for proof preparation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyReadClaim {
    /// Original structural query.
    pub query: StatePropertyQuery,
    /// Execution result claimed for this query.
    pub result: TypedPropertyQueryResult,
}

/// Relation proof claim kind extracted from execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationClaimKind {
    /// `assert relation ...`
    Assert,
    /// `eval relation ...`
    Eval,
}

/// Logical relation claim extracted from execution for proof preparation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationClaim {
    /// Relation identifier.
    pub relation: ir::RelationId,
    /// Proof-visible relation claim kind.
    pub kind: RelationClaimKind,
    /// Relation input tuple in execution order.
    pub inputs: Vec<TypedValue>,
    /// Canonical transcript digest for the input tuple.
    pub input_digest: [u32; 8],
    /// Relation output tuple in execution order.
    pub outputs: Vec<TypedValue>,
    /// Canonical transcript digest for the output tuple.
    pub output_digest: [u32; 8],
    /// Transaction index within the batch.
    pub tx_index: u32,
    /// Effect ordinal within the transaction.
    pub effect_ordinal_in_tx: u32,
    /// Canonical op index within the entry body.
    pub op_index: usize,
}

/// Sealed per-column profile/runtime identity used by proof preparation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnValueProfile {
    /// Column semantic type id.
    pub type_id: TypeId,
    /// Column encoding profile id.
    pub encoding_profile_id: EncodingProfileId,
}
