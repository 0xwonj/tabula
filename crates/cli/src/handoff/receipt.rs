//! Versioned binary execution receipt handoff.

use anyhow::Context as _;
use tabula_sdk::interop::{
    CapabilityEffect, ContextExt, ExecutionJournal, ExecutionReceiptExt, FailedTxExecution,
    RelationEffect, RelationEffectKind, StateEffectKind, StateExt, StatePropertyEffect,
    SuccessfulTxExecution, TransactionBatchExt, TxExecutionOutcome, TypedEventEffect,
    TypedStateEffect, TypedStateSnapshot, TypedStateWrite,
};

const RECEIPT_MAGIC: &[u8] = b"tabula.receipt.v1";
const RECEIPT_VERSION: u32 = 1;

/// Versioned binary receipt bridge written by `execute --receipt-out`.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct ExecutionReceiptBridgeV1 {
    /// Canonical artifact digest.
    pub(crate) program_digest: String,
    /// Exact committed pre-state snapshot.
    pub(crate) snapshot: tabula_sdk::interop::StateSnapshot,
    /// Exact portable transaction batch.
    pub(crate) batch: tabula_sdk::interop::EntryBatch,
    /// Exact portable public context input.
    pub(crate) context: tabula_sdk::interop::ContextInput,
    /// Exact committed post-state snapshot.
    pub(crate) state_after: tabula_sdk::interop::StateSnapshot,
    /// Portable execution journal required to reconstruct prove input later.
    pub(crate) journal: PortableExecutionJournalV1,
}

/// Portable journal equivalent for future proof reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableExecutionJournalV1 {
    /// Aggregate state summary across the whole batch.
    pub(crate) state_summary: PortableExecutionStateSummaryV1,
    /// Per-transaction outcomes in batch order.
    pub(crate) txs: Vec<PortableTxExecutionOutcomeV1>,
}

/// Portable aggregate state read/write summary.
#[derive(Debug, Clone, PartialEq, Eq, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableExecutionStateSummaryV1 {
    /// Snapshot of old values for distinct reads.
    pub(crate) read_set_old: Vec<PortableStateSnapshotV1>,
    /// Snapshot of final values for distinct writes.
    pub(crate) write_set_final: Vec<PortableStateWriteV1>,
}

/// Portable outcome of one transaction execution.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) enum PortableTxExecutionOutcomeV1 {
    /// Success case carrying exact effects.
    Success(PortableSuccessfulTxExecutionV1),
    /// Failure case carrying diagnostic metadata.
    Failed(PortableFailedTxExecutionV1),
}

/// Portable successful transaction journal.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableSuccessfulTxExecutionV1 {
    /// Zero-based transaction index within the batch.
    pub(crate) tx_index: u32,
    /// Executed entry id.
    pub(crate) entry_id: tabula_sdk::interop::EntryId,
    /// State read/write/delete effects in order.
    pub(crate) state_effects: Vec<PortableStateEffectV1>,
    /// Property read effects.
    pub(crate) property_effects: Vec<PortableStatePropertyEffectV1>,
    /// Relation lookup effects.
    pub(crate) relation_effects: Vec<PortableRelationEffectV1>,
    /// Native capability invocation effects.
    pub(crate) capability_effects: Vec<PortableCapabilityEffectV1>,
    /// Event emission effects.
    pub(crate) event_effects: Vec<PortableEventEffectV1>,
}

/// Portable failed transaction journal.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableFailedTxExecutionV1 {
    /// Zero-based transaction index within the batch.
    pub(crate) tx_index: u32,
    /// Executed entry id.
    pub(crate) entry_id: tabula_sdk::interop::EntryId,
    /// Human-readable failure reason.
    pub(crate) reason: String,
    /// Failing operation index when known.
    pub(crate) failed_op_index: Option<usize>,
}

/// Portable snapshot of one state cell value.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableStateSnapshotV1 {
    /// Cell identity.
    pub(crate) key: tabula_sdk::interop::CellKey,
    /// Value type identifier.
    pub(crate) type_id: tabula_sdk::interop::TypeRef,
    /// Portable old value, or `None` if the cell was absent.
    pub(crate) value: Option<tabula_sdk::interop::PortableValue>,
}

/// Portable final write for one state cell.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableStateWriteV1 {
    /// Cell identity.
    pub(crate) key: tabula_sdk::interop::CellKey,
    /// Value type identifier.
    pub(crate) type_id: tabula_sdk::interop::TypeRef,
    /// Final written value, or `None` if the cell was deleted.
    pub(crate) value: Option<tabula_sdk::interop::PortableValue>,
}

/// Portable classification of one state effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) enum PortableStateEffectKindV1 {
    /// A state read.
    Read,
    /// A state write.
    Write,
    /// A state delete.
    Delete,
}

/// Portable state access effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableStateEffectV1 {
    /// Cell identity.
    pub(crate) key: tabula_sdk::interop::CellKey,
    /// Value type identifier.
    pub(crate) type_id: tabula_sdk::interop::TypeRef,
    /// Effect kind.
    pub(crate) kind: PortableStateEffectKindV1,
    /// Portable value, or `None` for deletes.
    pub(crate) value: Option<tabula_sdk::interop::PortableValue>,
    /// Logical execution timestamp.
    pub(crate) logical_time: u64,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Portable state property read effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableStatePropertyEffectV1 {
    /// Target table id.
    pub(crate) table: tabula_sdk::interop::TableId,
    /// Target field id.
    pub(crate) field: tabula_sdk::interop::FieldId,
    /// Resolved structural query.
    pub(crate) query: tabula_sdk::interop::StatePropertyQuery,
    /// Portable outputs.
    pub(crate) outputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Portable static relation effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableRelationEffectV1 {
    /// Target relation id.
    pub(crate) relation: tabula_sdk::interop::RelationId,
    /// Whether this was an assertion or evaluation.
    pub(crate) kind: PortableRelationEffectKindV1,
    /// Portable relation inputs.
    pub(crate) inputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Portable relation outputs.
    pub(crate) outputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Portable relation effect kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) enum PortableRelationEffectKindV1 {
    /// Membership assertion.
    Assert,
    /// Output-producing evaluation.
    Eval,
}

/// Portable native capability effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableCapabilityEffectV1 {
    /// Target capability id.
    pub(crate) capability: tabula_sdk::interop::CapabilityId,
    /// Portable inputs.
    pub(crate) inputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Portable outputs.
    pub(crate) outputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Portable application event effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PortableEventEffectV1 {
    /// Event id.
    pub(crate) event: tabula_sdk::interop::EventId,
    /// Portable event payload.
    pub(crate) args: Vec<tabula_sdk::interop::PortableValue>,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Convert one SDK receipt into the CLI-owned portable receipt envelope.
pub(crate) fn bridge_from_receipt(
    program_digest: &str,
    receipt: &tabula_sdk::ExecutionReceipt,
) -> ExecutionReceiptBridgeV1 {
    ExecutionReceiptBridgeV1 {
        program_digest: program_digest.to_string(),
        snapshot: StateExt::snapshot(&receipt.state_before()).clone(),
        batch: TransactionBatchExt::batch(&receipt.batch()).clone(),
        context: ContextExt::input(&receipt.context()).clone(),
        state_after: StateExt::snapshot(&receipt.state_after()).clone(),
        journal: portable_journal(ExecutionReceiptExt::journal(receipt)),
    }
}

/// Serialize one receipt bridge into the canonical binary file format.
pub(crate) fn encode_receipt_bridge(bridge: &ExecutionReceiptBridgeV1) -> anyhow::Result<Vec<u8>> {
    let mut bytes = RECEIPT_MAGIC.to_vec();
    bytes.extend_from_slice(&RECEIPT_VERSION.to_le_bytes());
    bytes.extend(borsh::to_vec(bridge).context("failed to encode execution receipt bridge")?);
    Ok(bytes)
}

#[cfg(any(test, feature = "prove"))]
pub(crate) fn decode_receipt_bridge(bytes: &[u8]) -> anyhow::Result<ExecutionReceiptBridgeV1> {
    use anyhow::bail;
    use borsh::BorshDeserialize as _;

    if bytes.len() < RECEIPT_MAGIC.len() + std::mem::size_of::<u32>() {
        bail!("receipt file is too short");
    }
    let (magic, rest) = bytes.split_at(RECEIPT_MAGIC.len());
    if magic != RECEIPT_MAGIC {
        bail!("receipt file does not use the tabula.receipt magic header");
    }
    let (version_bytes, payload) = rest.split_at(std::mem::size_of::<u32>());
    let version = u32::from_le_bytes(version_bytes.try_into().expect("version bytes"));
    if version != RECEIPT_VERSION {
        bail!("unsupported receipt version {version}");
    }
    ExecutionReceiptBridgeV1::try_from_slice(payload)
        .context("failed to decode execution receipt bridge payload")
}

#[cfg(feature = "prove")]
pub(crate) fn sdk_receipt_from_bridge(
    bridge: ExecutionReceiptBridgeV1,
) -> anyhow::Result<tabula_sdk::ExecutionReceipt> {
    Ok(tabula_sdk::interop::execution_receipt_from_raw_parts(
        #[cfg(feature = "prove")]
        bridge.program_digest,
        bridge.snapshot,
        bridge.batch,
        bridge.context,
        bridge.state_after,
        execution_journal_from_portable(&bridge.journal),
    ))
}

fn portable_journal(journal: &ExecutionJournal) -> PortableExecutionJournalV1 {
    PortableExecutionJournalV1 {
        state_summary: portable_state_summary(&journal.state_summary),
        txs: journal.txs.iter().map(portable_tx_outcome).collect(),
    }
}

#[cfg(feature = "prove")]
fn execution_journal_from_portable(journal: &PortableExecutionJournalV1) -> ExecutionJournal {
    ExecutionJournal {
        state_summary: execution_state_summary_from_portable(&journal.state_summary),
        txs: journal.txs.iter().map(tx_outcome_from_portable).collect(),
    }
}

#[cfg(feature = "prove")]
fn execution_state_summary_from_portable(
    summary: &PortableExecutionStateSummaryV1,
) -> tabula_sdk::interop::ExecutionStateSummary {
    tabula_sdk::interop::ExecutionStateSummary {
        read_set_old: summary
            .read_set_old
            .iter()
            .map(state_snapshot_from_portable)
            .collect(),
        write_set_final: summary
            .write_set_final
            .iter()
            .map(state_write_from_portable)
            .collect(),
    }
}

#[cfg(feature = "prove")]
fn tx_outcome_from_portable(outcome: &PortableTxExecutionOutcomeV1) -> TxExecutionOutcome {
    match outcome {
        PortableTxExecutionOutcomeV1::Success(success) => {
            TxExecutionOutcome::Success(success_from_portable(success))
        }
        PortableTxExecutionOutcomeV1::Failed(failure) => {
            TxExecutionOutcome::Failed(failure_from_portable(failure))
        }
    }
}

#[cfg(feature = "prove")]
fn success_from_portable(success: &PortableSuccessfulTxExecutionV1) -> SuccessfulTxExecution {
    SuccessfulTxExecution {
        tx_index: success.tx_index,
        entry_id: success.entry_id,
        state_effects: success
            .state_effects
            .iter()
            .map(state_effect_from_portable)
            .collect(),
        property_effects: success
            .property_effects
            .iter()
            .map(property_effect_from_portable)
            .collect(),
        relation_effects: success
            .relation_effects
            .iter()
            .map(relation_effect_from_portable)
            .collect(),
        capability_effects: success
            .capability_effects
            .iter()
            .map(capability_effect_from_portable)
            .collect(),
        event_effects: success
            .event_effects
            .iter()
            .map(event_effect_from_portable)
            .collect(),
    }
}

#[cfg(feature = "prove")]
fn failure_from_portable(failure: &PortableFailedTxExecutionV1) -> FailedTxExecution {
    FailedTxExecution {
        tx_index: failure.tx_index,
        entry_id: failure.entry_id,
        reason: failure.reason.clone(),
        failed_op_index: failure.failed_op_index,
    }
}

#[cfg(feature = "prove")]
fn state_snapshot_from_portable(
    snapshot: &PortableStateSnapshotV1,
) -> tabula_sdk::interop::TypedStateSnapshot {
    tabula_sdk::interop::TypedStateSnapshot {
        key: snapshot.key,
        type_id: snapshot.type_id,
        value: snapshot
            .value
            .clone()
            .map(|value| portable_to_typed(&value)),
    }
}

#[cfg(feature = "prove")]
fn state_write_from_portable(write: &PortableStateWriteV1) -> TypedStateWrite {
    TypedStateWrite {
        key: write.key,
        type_id: write.type_id,
        value: write.value.clone().map(|value| portable_to_typed(&value)),
    }
}

#[cfg(feature = "prove")]
fn state_effect_from_portable(effect: &PortableStateEffectV1) -> TypedStateEffect {
    TypedStateEffect {
        key: effect.key,
        type_id: effect.type_id,
        kind: match effect.kind {
            PortableStateEffectKindV1::Read => StateEffectKind::Read,
            PortableStateEffectKindV1::Write => StateEffectKind::Write,
            PortableStateEffectKindV1::Delete => StateEffectKind::Delete,
        },
        value: effect.value.clone().map(|value| portable_to_typed(&value)),
        logical_time: effect.logical_time,
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn property_effect_from_portable(effect: &PortableStatePropertyEffectV1) -> StatePropertyEffect {
    StatePropertyEffect {
        table: effect.table,
        field: effect.field,
        query: effect.query.clone(),
        outputs: effect.outputs.iter().map(portable_to_typed).collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn relation_effect_from_portable(effect: &PortableRelationEffectV1) -> RelationEffect {
    RelationEffect {
        relation: effect.relation,
        kind: match effect.kind {
            PortableRelationEffectKindV1::Assert => RelationEffectKind::Assert,
            PortableRelationEffectKindV1::Eval => RelationEffectKind::Eval,
        },
        inputs: effect.inputs.iter().map(portable_to_typed).collect(),
        outputs: effect.outputs.iter().map(portable_to_typed).collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn capability_effect_from_portable(effect: &PortableCapabilityEffectV1) -> CapabilityEffect {
    CapabilityEffect {
        capability: effect.capability,
        inputs: effect.inputs.iter().map(portable_to_typed).collect(),
        outputs: effect.outputs.iter().map(portable_to_typed).collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn event_effect_from_portable(effect: &PortableEventEffectV1) -> TypedEventEffect {
    TypedEventEffect {
        event: effect.event,
        args: effect.args.iter().map(portable_to_typed).collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn portable_to_typed(
    value: &tabula_sdk::interop::PortableValue,
) -> tabula_sdk::interop::TypedValue {
    tabula_sdk::interop::TypedValue::new(value.type_id(), value.payload().to_vec())
}

fn portable_state_summary(
    summary: &tabula_sdk::interop::ExecutionStateSummary,
) -> PortableExecutionStateSummaryV1 {
    PortableExecutionStateSummaryV1 {
        read_set_old: summary
            .read_set_old
            .iter()
            .map(portable_state_snapshot)
            .collect(),
        write_set_final: summary
            .write_set_final
            .iter()
            .map(portable_state_write)
            .collect(),
    }
}

fn portable_state_snapshot(snapshot: &TypedStateSnapshot) -> PortableStateSnapshotV1 {
    PortableStateSnapshotV1 {
        key: snapshot.key,
        type_id: snapshot.type_id,
        value: snapshot
            .value
            .clone()
            .map(tabula_sdk::interop::TypedValue::into_portable),
    }
}

fn portable_state_write(write: &TypedStateWrite) -> PortableStateWriteV1 {
    PortableStateWriteV1 {
        key: write.key,
        type_id: write.type_id,
        value: write
            .value
            .clone()
            .map(tabula_sdk::interop::TypedValue::into_portable),
    }
}

fn portable_tx_outcome(outcome: &TxExecutionOutcome) -> PortableTxExecutionOutcomeV1 {
    match outcome {
        TxExecutionOutcome::Success(success) => {
            PortableTxExecutionOutcomeV1::Success(portable_success(success))
        }
        TxExecutionOutcome::Failed(failure) => {
            PortableTxExecutionOutcomeV1::Failed(portable_failure(failure))
        }
    }
}

fn portable_success(success: &SuccessfulTxExecution) -> PortableSuccessfulTxExecutionV1 {
    PortableSuccessfulTxExecutionV1 {
        tx_index: success.tx_index,
        entry_id: success.entry_id,
        state_effects: success
            .state_effects
            .iter()
            .map(portable_state_effect)
            .collect(),
        property_effects: success
            .property_effects
            .iter()
            .map(portable_property_effect)
            .collect(),
        relation_effects: success
            .relation_effects
            .iter()
            .map(portable_relation_effect)
            .collect(),
        capability_effects: success
            .capability_effects
            .iter()
            .map(portable_capability_effect)
            .collect(),
        event_effects: success
            .event_effects
            .iter()
            .map(portable_event_effect)
            .collect(),
    }
}

fn portable_failure(failure: &FailedTxExecution) -> PortableFailedTxExecutionV1 {
    PortableFailedTxExecutionV1 {
        tx_index: failure.tx_index,
        entry_id: failure.entry_id,
        reason: failure.reason.clone(),
        failed_op_index: failure.failed_op_index,
    }
}

fn portable_state_effect(effect: &TypedStateEffect) -> PortableStateEffectV1 {
    PortableStateEffectV1 {
        key: effect.key,
        type_id: effect.type_id,
        kind: match effect.kind {
            StateEffectKind::Read => PortableStateEffectKindV1::Read,
            StateEffectKind::Write => PortableStateEffectKindV1::Write,
            StateEffectKind::Delete => PortableStateEffectKindV1::Delete,
        },
        value: effect
            .value
            .clone()
            .map(tabula_sdk::interop::TypedValue::into_portable),
        logical_time: effect.logical_time,
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

fn portable_property_effect(effect: &StatePropertyEffect) -> PortableStatePropertyEffectV1 {
    PortableStatePropertyEffectV1 {
        table: effect.table,
        field: effect.field,
        query: effect.query.clone(),
        outputs: effect
            .outputs
            .iter()
            .cloned()
            .map(tabula_sdk::interop::TypedValue::into_portable)
            .collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

fn portable_relation_effect(effect: &RelationEffect) -> PortableRelationEffectV1 {
    PortableRelationEffectV1 {
        relation: effect.relation,
        kind: match effect.kind {
            RelationEffectKind::Assert => PortableRelationEffectKindV1::Assert,
            RelationEffectKind::Eval => PortableRelationEffectKindV1::Eval,
        },
        inputs: effect
            .inputs
            .iter()
            .cloned()
            .map(tabula_sdk::interop::TypedValue::into_portable)
            .collect(),
        outputs: effect
            .outputs
            .iter()
            .cloned()
            .map(tabula_sdk::interop::TypedValue::into_portable)
            .collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

fn portable_capability_effect(effect: &CapabilityEffect) -> PortableCapabilityEffectV1 {
    PortableCapabilityEffectV1 {
        capability: effect.capability,
        inputs: effect
            .inputs
            .iter()
            .cloned()
            .map(tabula_sdk::interop::TypedValue::into_portable)
            .collect(),
        outputs: effect
            .outputs
            .iter()
            .cloned()
            .map(tabula_sdk::interop::TypedValue::into_portable)
            .collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

fn portable_event_effect(effect: &TypedEventEffect) -> PortableEventEffectV1 {
    PortableEventEffectV1 {
        event: effect.event,
        args: effect
            .args
            .iter()
            .cloned()
            .map(tabula_sdk::interop::TypedValue::into_portable)
            .collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionReceiptBridgeV1, decode_receipt_bridge, encode_receipt_bridge};
    use super::{PortableExecutionJournalV1, PortableExecutionStateSummaryV1};

    #[test]
    fn receipt_roundtrip_preserves_payload() {
        let envelope = ExecutionReceiptBridgeV1 {
            program_digest: "digest".to_string(),
            snapshot: tabula_sdk::interop::StateSnapshot::default(),
            batch: tabula_sdk::interop::EntryBatch::default(),
            context: tabula_sdk::interop::ContextInput::default(),
            state_after: tabula_sdk::interop::StateSnapshot::default(),
            journal: PortableExecutionJournalV1 {
                state_summary: PortableExecutionStateSummaryV1::default(),
                txs: Vec::new(),
            },
        };

        let bytes = encode_receipt_bridge(&envelope).unwrap();
        let decoded = decode_receipt_bridge(&bytes).unwrap();
        assert_eq!(decoded, envelope);
    }
}
