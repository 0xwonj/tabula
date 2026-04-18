//! Runtime-owned user-state view consumed by the executor and witness lowering.

use tabula_core::error::TabulaError;
use tabula_core::{CommittedCellKey, CommittedKey, CommittedPropertyQuery, TypeId};
use tabula_ir as ir;

use crate::{
    CommittedColumnEntry, NativeKeyPayload, TypedCommittedPropertyQueryResult, TypedValue,
};

/// Runtime-owned user-state services consumed by the executor.
pub trait StateRuntimeView: Send + Sync {
    /// Encode a logical key tuple for one state cell access.
    fn encode_cell_key(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        key: &[TypedValue],
    ) -> Result<CommittedCellKey, TabulaError>;

    /// Encode a logical key tuple without a field binding.
    fn encode_committed_key(
        &self,
        table: ir::TableId,
        key: &[TypedValue],
    ) -> Result<CommittedKey, TabulaError>;

    /// Decode one committed key into logical key components.
    fn decode_committed_key(
        &self,
        table: ir::TableId,
        key: &CommittedKey,
    ) -> Result<Vec<TypedValue>, TabulaError>;

    /// Encode one committed key into the native proof payload for the table.
    fn encode_key_payload(
        &self,
        table: ir::TableId,
        key: &CommittedKey,
    ) -> Result<NativeKeyPayload, TabulaError>;

    /// Compare two committed keys using the sealed table-key ordering.
    fn compare_keys(
        &self,
        table: ir::TableId,
        lhs: &CommittedKey,
        rhs: &CommittedKey,
    ) -> Result<std::cmp::Ordering, TabulaError>;

    /// Borrow the logical key component type ids for one state table.
    fn key_component_types(&self, table: ir::TableId) -> Result<Vec<TypeId>, TabulaError>;

    /// Resolve the field type for one user-state column from the sealed runtime contract.
    fn column_type(&self, table: ir::TableId, field: ir::FieldId) -> Result<TypeId, TabulaError>;

    /// Execute a structural property read over one committed column state snapshot.
    fn resolve_property(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        query: &CommittedPropertyQuery,
        state: &[CommittedColumnEntry],
    ) -> Result<TypedCommittedPropertyQueryResult, TabulaError>;
}
