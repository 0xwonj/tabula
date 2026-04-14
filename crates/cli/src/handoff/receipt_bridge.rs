//! Internal versioned execution receipt bridge.

use anyhow::Context as _;
use tabula_sdk::interop::{
    CapabilityEffect, ContextExt, ExecutionJournal, ExecutionReceiptExt, FailedTxExecution,
    RelationEffect, RelationEffectKind, StateEffectKind, StatePropertyEffect,
    SuccessfulTxExecution, TransactionBatchExt, TxExecutionOutcome, TypedEventEffect,
    TypedStateEffect, TypedStateSnapshot, TypedStateWrite,
};

const RECEIPT_MAGIC: &[u8] = b"tabula.receipt.v1";
const RECEIPT_VERSION: u32 = 1;

/// Versioned binary receipt bridge written by `execute --receipt-out`.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct ReceiptBridge {
    /// Canonical artifact digest.
    pub(crate) program_digest: String,
    /// Exact logical pre-state authored through the SDK surface.
    pub(crate) state_before: tabula_sdk::State,
    /// Exact committed pre-state snapshot encoded for handoff.
    pub(crate) snapshot: CommittedStateSnapshotBridge,
    /// Exact portable transaction batch.
    pub(crate) batch: tabula_sdk::interop::EntryBatch,
    /// Exact portable public context input.
    pub(crate) context: tabula_sdk::interop::ContextInput,
    /// Exact logical post-state projected through the SDK surface.
    pub(crate) state_after: tabula_sdk::State,
    /// Exact committed post-state snapshot encoded for handoff.
    pub(crate) state_after_snapshot: CommittedStateSnapshotBridge,
    /// Bridge execution journal required to reconstruct prove input later.
    pub(crate) journal: ExecutionJournalBridge,
}

/// Bridge committed snapshot payload validated through the runtime on load.
#[derive(Debug, Clone, PartialEq, Eq, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct CommittedStateSnapshotBridge {
    /// Canonical committed cells in `(table, field, committed_key)` order.
    pub(crate) cells: Vec<CommittedStateCellBridge>,
}

/// Bridge committed snapshot cell.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct CommittedStateCellBridge {
    /// Target state table id.
    pub(crate) table: tabula_sdk::interop::TableId,
    /// Canonical committed key bytes.
    pub(crate) key: Vec<u8>,
    /// Target state field id.
    pub(crate) field: tabula_sdk::interop::FieldId,
    /// Portable field value.
    pub(crate) value: tabula_sdk::interop::PortableValue,
}

/// Bridge journal equivalent for future proof reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct ExecutionJournalBridge {
    /// Aggregate state summary across the whole batch.
    pub(crate) state_summary: ExecutionStateSummaryBridge,
    /// Per-transaction outcomes in batch order.
    pub(crate) txs: Vec<TxExecutionOutcomeBridge>,
}

/// Bridge aggregate state read/write summary.
#[derive(Debug, Clone, PartialEq, Eq, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct ExecutionStateSummaryBridge {
    /// Snapshot of old values for distinct reads.
    pub(crate) read_set_old: Vec<StateSnapshotBridge>,
    /// Snapshot of final values for distinct writes.
    pub(crate) write_set_final: Vec<StateWriteBridge>,
}

/// Bridge outcome of one transaction execution.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) enum TxExecutionOutcomeBridge {
    /// Success case carrying exact effects.
    Success(SuccessfulTxExecutionBridge),
    /// Failure case carrying diagnostic metadata.
    Failed(FailedTxExecutionBridge),
}

/// Bridge successful transaction journal.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct SuccessfulTxExecutionBridge {
    /// Zero-based transaction index within the batch.
    pub(crate) tx_index: u32,
    /// Executed entry id.
    pub(crate) entry_id: tabula_sdk::interop::EntryId,
    /// State read/write/delete effects in order.
    pub(crate) state_effects: Vec<StateEffectBridge>,
    /// Property read effects.
    pub(crate) property_effects: Vec<StatePropertyEffectBridge>,
    /// Relation lookup effects.
    pub(crate) relation_effects: Vec<RelationEffectBridge>,
    /// Native capability invocation effects.
    pub(crate) capability_effects: Vec<CapabilityEffectBridge>,
    /// Event emission effects.
    pub(crate) event_effects: Vec<EventEffectBridge>,
}

/// Bridge failed transaction journal.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct FailedTxExecutionBridge {
    /// Zero-based transaction index within the batch.
    pub(crate) tx_index: u32,
    /// Executed entry id.
    pub(crate) entry_id: tabula_sdk::interop::EntryId,
    /// Human-readable failure reason.
    pub(crate) reason: String,
    /// Failing operation index when known.
    pub(crate) failed_op_index: Option<usize>,
}

/// Bridge snapshot of one state cell value.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct StateSnapshotBridge {
    /// Cell identity.
    pub(crate) key: tabula_sdk::interop::CommittedCellKey,
    /// Value type identifier.
    pub(crate) type_id: tabula_sdk::interop::TypeRef,
    /// Bridge old value, or `None` if the cell was absent.
    pub(crate) value: Option<tabula_sdk::interop::PortableValue>,
}

/// Bridge final write for one state cell.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct StateWriteBridge {
    /// Cell identity.
    pub(crate) key: tabula_sdk::interop::CommittedCellKey,
    /// Value type identifier.
    pub(crate) type_id: tabula_sdk::interop::TypeRef,
    /// Final written value, or `None` if the cell was deleted.
    pub(crate) value: Option<tabula_sdk::interop::PortableValue>,
}

/// Bridge classification of one state effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) enum StateEffectKindBridge {
    /// A state read.
    Read,
    /// A state write.
    Write,
    /// A state delete.
    Delete,
}

/// Bridge state access effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct StateEffectBridge {
    /// Cell identity.
    pub(crate) key: tabula_sdk::interop::CommittedCellKey,
    /// Value type identifier.
    pub(crate) type_id: tabula_sdk::interop::TypeRef,
    /// Effect kind.
    pub(crate) kind: StateEffectKindBridge,
    /// Bridge value, or `None` for deletes.
    pub(crate) value: Option<tabula_sdk::interop::PortableValue>,
    /// Logical execution timestamp.
    pub(crate) logical_time: u64,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Bridge committed-key-native property-query result.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PropertyQueryResultBridge {
    /// Bridge typed value produced by the query.
    pub(crate) value: tabula_sdk::interop::PortableValue,
    /// Committed key returned by the query when one matched.
    pub(crate) key: Option<tabula_sdk::interop::CommittedKey>,
    /// Whether the query resolved to null.
    pub(crate) is_null: bool,
}

/// Bridge state property read effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct StatePropertyEffectBridge {
    /// Target table id.
    pub(crate) table: tabula_sdk::interop::TableId,
    /// Target field id.
    pub(crate) field: tabula_sdk::interop::FieldId,
    /// Resolved committed-key structural query.
    pub(crate) query: tabula_sdk::interop::CommittedPropertyQuery,
    /// Bridge committed-key-native query result.
    pub(crate) result: PropertyQueryResultBridge,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Bridge static relation effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct RelationEffectBridge {
    /// Target relation id.
    pub(crate) relation: tabula_sdk::interop::RelationId,
    /// Whether this was an assertion or evaluation.
    pub(crate) kind: RelationEffectKindBridge,
    /// Bridge relation inputs.
    pub(crate) inputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Bridge relation outputs.
    pub(crate) outputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Bridge relation effect kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) enum RelationEffectKindBridge {
    /// Membership assertion.
    Assert,
    /// Output-producing evaluation.
    Eval,
}

/// Bridge native capability effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct CapabilityEffectBridge {
    /// Target capability id.
    pub(crate) capability: tabula_sdk::interop::CapabilityId,
    /// Bridge inputs.
    pub(crate) inputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Bridge outputs.
    pub(crate) outputs: Vec<tabula_sdk::interop::PortableValue>,
    /// Producing IR operation index.
    pub(crate) op_index: usize,
    /// Effect ordinal within the entry.
    pub(crate) effect_ordinal_in_entry: u32,
}

/// Bridge application event effect.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct EventEffectBridge {
    /// Event id.
    pub(crate) event: tabula_sdk::interop::EventId,
    /// Bridge event payload.
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
) -> ReceiptBridge {
    ReceiptBridge {
        program_digest: program_digest.to_string(),
        state_before: receipt.state_before(),
        snapshot: portable_committed_snapshot(ExecutionReceiptExt::snapshot(receipt)),
        batch: TransactionBatchExt::batch(&receipt.batch()).clone(),
        context: ContextExt::input(&receipt.context()).clone(),
        state_after: receipt.state_after(),
        state_after_snapshot: portable_committed_snapshot(
            ExecutionReceiptExt::state_after_snapshot(receipt),
        ),
        journal: portable_journal(ExecutionReceiptExt::journal(receipt)),
    }
}

/// Serialize one receipt bridge into the canonical binary file format.
pub(crate) fn encode_receipt_bridge(bridge: &ReceiptBridge) -> anyhow::Result<Vec<u8>> {
    let mut bytes = RECEIPT_MAGIC.to_vec();
    bytes.extend_from_slice(&RECEIPT_VERSION.to_le_bytes());
    bytes.extend(borsh::to_vec(bridge).context("failed to encode execution receipt bridge")?);
    Ok(bytes)
}

#[cfg(any(test, feature = "prove"))]
pub(crate) fn decode_receipt_bridge(bytes: &[u8]) -> anyhow::Result<ReceiptBridge> {
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
    ReceiptBridge::try_from_slice(payload)
        .context("failed to decode execution receipt bridge payload")
}

#[cfg(feature = "prove")]
pub(crate) fn sdk_receipt_from_bridge(
    runtime: &tabula_sdk::interop::TabulaRuntime,
    bridge: ReceiptBridge,
) -> anyhow::Result<tabula_sdk::ExecutionReceipt> {
    Ok(tabula_sdk::interop::execution_receipt_from_raw_parts(
        tabula_sdk::interop::RawExecutionReceiptParts {
            #[cfg(feature = "prove")]
            program_digest: bridge.program_digest,
            state_before: bridge.state_before,
            snapshot: committed_snapshot_from_portable(runtime, &bridge.snapshot)?,
            batch: bridge.batch,
            context: bridge.context,
            state_after: bridge.state_after,
            state_after_snapshot: committed_snapshot_from_portable(
                runtime,
                &bridge.state_after_snapshot,
            )?,
            journal: execution_journal_from_portable(&bridge.journal),
        },
    ))
}

fn portable_committed_snapshot(
    snapshot: &tabula_sdk::interop::CommittedStateSnapshot,
) -> CommittedStateSnapshotBridge {
    CommittedStateSnapshotBridge {
        cells: snapshot
            .cells()
            .map(|(key, value)| CommittedStateCellBridge {
                table: tabula_sdk::interop::TableId(key.table.0),
                key: key.key.0.clone(),
                field: tabula_sdk::interop::FieldId(key.col.0),
                value: value.clone(),
            })
            .collect(),
    }
}

#[cfg(feature = "prove")]
fn committed_snapshot_from_portable(
    runtime: &tabula_sdk::interop::TabulaRuntime,
    snapshot: &CommittedStateSnapshotBridge,
) -> anyhow::Result<tabula_sdk::interop::CommittedStateSnapshot> {
    runtime
        .decode_committed_snapshot(
            snapshot
                .cells
                .iter()
                .cloned()
                .map(|cell| (cell.table, cell.key, cell.field, cell.value)),
        )
        .context("failed to validate committed snapshot against the sealed runtime contract")
}

fn portable_journal(journal: &ExecutionJournal) -> ExecutionJournalBridge {
    ExecutionJournalBridge {
        state_summary: portable_state_summary(&journal.state_summary),
        txs: journal.txs.iter().map(portable_tx_outcome).collect(),
    }
}

#[cfg(feature = "prove")]
fn execution_journal_from_portable(journal: &ExecutionJournalBridge) -> ExecutionJournal {
    ExecutionJournal {
        state_summary: execution_state_summary_from_portable(&journal.state_summary),
        txs: journal.txs.iter().map(tx_outcome_from_portable).collect(),
    }
}

#[cfg(feature = "prove")]
fn execution_state_summary_from_portable(
    summary: &ExecutionStateSummaryBridge,
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
fn tx_outcome_from_portable(outcome: &TxExecutionOutcomeBridge) -> TxExecutionOutcome {
    match outcome {
        TxExecutionOutcomeBridge::Success(success) => {
            TxExecutionOutcome::Success(success_from_portable(success))
        }
        TxExecutionOutcomeBridge::Failed(failure) => {
            TxExecutionOutcome::Failed(failure_from_portable(failure))
        }
    }
}

#[cfg(feature = "prove")]
fn success_from_portable(success: &SuccessfulTxExecutionBridge) -> SuccessfulTxExecution {
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
fn failure_from_portable(failure: &FailedTxExecutionBridge) -> FailedTxExecution {
    FailedTxExecution {
        tx_index: failure.tx_index,
        entry_id: failure.entry_id,
        reason: failure.reason.clone(),
        failed_op_index: failure.failed_op_index,
    }
}

#[cfg(feature = "prove")]
fn state_snapshot_from_portable(
    snapshot: &StateSnapshotBridge,
) -> tabula_sdk::interop::TypedStateSnapshot {
    tabula_sdk::interop::TypedStateSnapshot {
        key: snapshot.key.clone(),
        type_id: snapshot.type_id,
        value: snapshot
            .value
            .clone()
            .map(|value| portable_to_typed(&value)),
    }
}

#[cfg(feature = "prove")]
fn state_write_from_portable(write: &StateWriteBridge) -> TypedStateWrite {
    TypedStateWrite {
        key: write.key.clone(),
        type_id: write.type_id,
        value: write.value.clone().map(|value| portable_to_typed(&value)),
    }
}

#[cfg(feature = "prove")]
fn state_effect_from_portable(effect: &StateEffectBridge) -> TypedStateEffect {
    TypedStateEffect {
        key: effect.key.clone(),
        type_id: effect.type_id,
        kind: match effect.kind {
            StateEffectKindBridge::Read => StateEffectKind::Read,
            StateEffectKindBridge::Write => StateEffectKind::Write,
            StateEffectKindBridge::Delete => StateEffectKind::Delete,
        },
        value: effect.value.clone().map(|value| portable_to_typed(&value)),
        logical_time: effect.logical_time,
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn property_effect_from_portable(effect: &StatePropertyEffectBridge) -> StatePropertyEffect {
    StatePropertyEffect {
        table: effect.table,
        field: effect.field,
        query: effect.query.clone(),
        result: property_query_result_from_portable(&effect.result),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn relation_effect_from_portable(effect: &RelationEffectBridge) -> RelationEffect {
    RelationEffect {
        relation: effect.relation,
        kind: match effect.kind {
            RelationEffectKindBridge::Assert => RelationEffectKind::Assert,
            RelationEffectKindBridge::Eval => RelationEffectKind::Eval,
        },
        inputs: effect.inputs.iter().map(portable_to_typed).collect(),
        outputs: effect.outputs.iter().map(portable_to_typed).collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn capability_effect_from_portable(effect: &CapabilityEffectBridge) -> CapabilityEffect {
    CapabilityEffect {
        capability: effect.capability,
        inputs: effect.inputs.iter().map(portable_to_typed).collect(),
        outputs: effect.outputs.iter().map(portable_to_typed).collect(),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn event_effect_from_portable(effect: &EventEffectBridge) -> TypedEventEffect {
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
) -> ExecutionStateSummaryBridge {
    ExecutionStateSummaryBridge {
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

fn portable_state_snapshot(snapshot: &TypedStateSnapshot) -> StateSnapshotBridge {
    StateSnapshotBridge {
        key: snapshot.key.clone(),
        type_id: snapshot.type_id,
        value: snapshot
            .value
            .clone()
            .map(tabula_sdk::interop::TypedValue::into_portable),
    }
}

fn portable_state_write(write: &TypedStateWrite) -> StateWriteBridge {
    StateWriteBridge {
        key: write.key.clone(),
        type_id: write.type_id,
        value: write
            .value
            .clone()
            .map(tabula_sdk::interop::TypedValue::into_portable),
    }
}

fn portable_tx_outcome(outcome: &TxExecutionOutcome) -> TxExecutionOutcomeBridge {
    match outcome {
        TxExecutionOutcome::Success(success) => {
            TxExecutionOutcomeBridge::Success(portable_success(success))
        }
        TxExecutionOutcome::Failed(failure) => {
            TxExecutionOutcomeBridge::Failed(portable_failure(failure))
        }
    }
}

fn portable_success(success: &SuccessfulTxExecution) -> SuccessfulTxExecutionBridge {
    SuccessfulTxExecutionBridge {
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

fn portable_failure(failure: &FailedTxExecution) -> FailedTxExecutionBridge {
    FailedTxExecutionBridge {
        tx_index: failure.tx_index,
        entry_id: failure.entry_id,
        reason: failure.reason.clone(),
        failed_op_index: failure.failed_op_index,
    }
}

fn portable_state_effect(effect: &TypedStateEffect) -> StateEffectBridge {
    StateEffectBridge {
        key: effect.key.clone(),
        type_id: effect.type_id,
        kind: match effect.kind {
            StateEffectKind::Read => StateEffectKindBridge::Read,
            StateEffectKind::Write => StateEffectKindBridge::Write,
            StateEffectKind::Delete => StateEffectKindBridge::Delete,
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

fn portable_property_effect(effect: &StatePropertyEffect) -> StatePropertyEffectBridge {
    StatePropertyEffectBridge {
        table: effect.table,
        field: effect.field,
        query: effect.query.clone(),
        result: portable_property_query_result(&effect.result),
        op_index: effect.op_index,
        effect_ordinal_in_entry: effect.effect_ordinal_in_entry,
    }
}

#[cfg(feature = "prove")]
fn property_query_result_from_portable(
    result: &PropertyQueryResultBridge,
) -> tabula_sdk::interop::TypedCommittedPropertyQueryResult {
    tabula_sdk::interop::TypedCommittedPropertyQueryResult {
        value: portable_to_typed(&result.value),
        key: result.key.clone(),
        is_null: result.is_null,
    }
}

fn portable_property_query_result(
    result: &tabula_sdk::interop::TypedCommittedPropertyQueryResult,
) -> PropertyQueryResultBridge {
    PropertyQueryResultBridge {
        value: result.value.clone().into_portable(),
        key: result.key.clone(),
        is_null: result.is_null,
    }
}

fn portable_relation_effect(effect: &RelationEffect) -> RelationEffectBridge {
    RelationEffectBridge {
        relation: effect.relation,
        kind: match effect.kind {
            RelationEffectKind::Assert => RelationEffectKindBridge::Assert,
            RelationEffectKind::Eval => RelationEffectKindBridge::Eval,
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

fn portable_capability_effect(effect: &CapabilityEffect) -> CapabilityEffectBridge {
    CapabilityEffectBridge {
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

fn portable_event_effect(effect: &TypedEventEffect) -> EventEffectBridge {
    EventEffectBridge {
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
    use super::{
        CommittedStateSnapshotBridge, ExecutionJournalBridge, ExecutionStateSummaryBridge,
    };
    use super::{ReceiptBridge, decode_receipt_bridge, encode_receipt_bridge};

    #[test]
    fn receipt_roundtrip_preserves_payload() {
        let envelope = ReceiptBridge {
            program_digest: "digest".to_string(),
            state_before: tabula_sdk::State::default(),
            snapshot: CommittedStateSnapshotBridge::default(),
            batch: tabula_sdk::interop::EntryBatch::default(),
            context: tabula_sdk::interop::ContextInput::default(),
            state_after: tabula_sdk::State::default(),
            state_after_snapshot: CommittedStateSnapshotBridge::default(),
            journal: ExecutionJournalBridge {
                state_summary: ExecutionStateSummaryBridge::default(),
                txs: Vec::new(),
            },
        };

        let bytes = encode_receipt_bridge(&envelope).unwrap();
        let decoded = decode_receipt_bridge(&bytes).unwrap();
        assert_eq!(decoded, envelope);
    }
}
