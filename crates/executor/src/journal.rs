//! Canonical execution journal and reporting projections.
//!
//! `ExecutionJournal` is the canonical internal output of executor batch
//! execution. Portable reporting views such as `BatchReport` are explicitly
//! derived from this typed journal.

use tabula_core::error::TabulaError;
use tabula_core::{
    AccessEvent, BatchReport, CellKey, EmittedEvent, ExecutionConsistencyStatus, LogicalTime,
    OpKind, PortableValue, PropertyReadResult, TxResult, TypeId,
};
use tabula_ir::PrecompileId;
use tabula_types::{TypeRuntimeRegistry, TypedPropertyQueryResult, TypedValue};

/// Canonical internal journal for one executed batch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionJournal {
    /// Derived batch-level state summary computed by the executor.
    pub state_summary: ExecutionStateSummary,
    /// Per-transaction execution outcomes in batch order.
    pub txs: Vec<TxExecutionOutcome>,
}

impl ExecutionJournal {
    /// Iterate successful transaction shards in batch order.
    pub fn successful_txs(&self) -> impl Iterator<Item = &SuccessfulTxExecution> + '_ {
        self.txs.iter().filter_map(TxExecutionOutcome::success)
    }

    /// Iterate successful access effects in execution order.
    pub fn successful_access_effects(&self) -> impl Iterator<Item = &TypedAccessEffect> + '_ {
        self.successful_txs()
            .flat_map(|shard| shard.access_effects.iter())
    }

    /// Iterate successful access effects with their tx index.
    pub fn successful_access_effects_with_tx(
        &self,
    ) -> impl Iterator<Item = (u32, &TypedAccessEffect)> + '_ {
        self.successful_txs().flat_map(|shard| {
            shard
                .access_effects
                .iter()
                .map(move |effect| (shard.tx_index, effect))
        })
    }

    /// Iterate successful emitted events in execution order.
    pub fn successful_emitted(&self) -> impl Iterator<Item = &EmittedEvent> + '_ {
        self.successful_txs()
            .flat_map(|shard| shard.emitted_events.iter())
    }

    /// Iterate successful property reads in execution order.
    pub fn successful_property_reads(&self) -> impl Iterator<Item = &TypedPropertyReadEffect> + '_ {
        self.successful_txs()
            .flat_map(|shard| shard.property_reads.iter())
    }

    /// Iterate successful precompile calls in execution order.
    pub fn successful_precompile_calls(
        &self,
    ) -> impl Iterator<Item = &TypedPrecompileCallEffect> + '_ {
        self.successful_txs()
            .flat_map(|shard| shard.precompile_calls.iter())
    }

    /// Iterate successful IR-hash calls in execution order.
    pub fn successful_ir_hash_calls(&self) -> impl Iterator<Item = &IrHashEffect> + '_ {
        self.successful_txs()
            .flat_map(|shard| shard.ir_hash_calls.iter())
    }
}

/// Derived batch-level state summary nested inside the execution journal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionStateSummary {
    /// Base-state reads observed during execution.
    pub read_set_old: Vec<TypedStateSnapshot>,
    /// Final coalesced writes after execution.
    pub write_set_final: Vec<TypedStateWrite>,
}

/// Portable reporting projection of the journal state summary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PortableStateSummary {
    /// Portable projection of `ExecutionStateSummary.read_set_old`.
    pub read_set_old: Vec<(CellKey, Option<PortableValue>)>,
    /// Portable projection of `ExecutionStateSummary.write_set_final`.
    pub write_set_final: Vec<(CellKey, Option<PortableValue>)>,
}

/// Batch-ordered execution outcome for one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxExecutionOutcome {
    /// Successful transaction execution.
    Success(SuccessfulTxExecution),
    /// Failed transaction execution.
    Failed(FailedTxExecution),
}

impl TxExecutionOutcome {
    /// Transaction index for this outcome.
    #[must_use]
    pub fn tx_index(&self) -> u32 {
        match self {
            Self::Success(shard) => shard.tx_index,
            Self::Failed(failure) => failure.tx_index,
        }
    }

    /// Successful shard view, if this outcome succeeded.
    #[must_use]
    pub fn success(&self) -> Option<&SuccessfulTxExecution> {
        match self {
            Self::Success(shard) => Some(shard),
            Self::Failed(_) => None,
        }
    }

    /// Failed shard view, if this outcome failed.
    #[must_use]
    pub fn failure(&self) -> Option<&FailedTxExecution> {
        match self {
            Self::Success(_) => None,
            Self::Failed(failure) => Some(failure),
        }
    }
}

/// Immutable semantic execution effects for one successful transaction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SuccessfulTxExecution {
    /// Zero-based transaction index within the batch.
    pub tx_index: u32,
    /// Typed state access effects recorded during execution.
    pub access_effects: Vec<TypedAccessEffect>,
    /// Typed property-read effects recorded during execution.
    pub property_reads: Vec<TypedPropertyReadEffect>,
    /// Typed precompile call effects recorded during execution.
    pub precompile_calls: Vec<TypedPrecompileCallEffect>,
    /// Canonical IR-hash effects recorded during execution.
    pub ir_hash_calls: Vec<IrHashEffect>,
    /// Boundary-facing application events emitted during execution.
    pub emitted_events: Vec<EmittedEvent>,
}

/// Failed transaction execution outcome.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FailedTxExecution {
    /// Zero-based transaction index within the batch.
    pub tx_index: u32,
    /// Human-readable failure reason.
    pub reason: String,
    /// Diagnostic access observations recorded before the failure.
    pub partial_accesses: Vec<FailedAccessObservation>,
    /// Zero-based instruction index at which execution failed, if any.
    pub failed_instruction: Option<usize>,
}

/// Typed old-state snapshot entry used by the execution journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStateSnapshot {
    /// Cell key.
    pub key: CellKey,
    /// Declared cell type.
    pub type_id: TypeId,
    /// Decoded typed value, or `None` for absent.
    pub value: Option<TypedValue>,
}

/// Typed final state write used by the execution journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStateWrite {
    /// Cell key.
    pub key: CellKey,
    /// Declared cell type.
    pub type_id: TypeId,
    /// Decoded typed value, or `None` for delete.
    pub value: Option<TypedValue>,
}

/// Typed state access effect recorded during execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedAccessEffect {
    /// Cell key.
    pub key: CellKey,
    /// Declared cell type.
    pub type_id: TypeId,
    /// Access kind.
    pub op: OpKind,
    /// Decoded typed value, or `None` for null.
    pub value: Option<TypedValue>,
    /// Logical time for this canonical success-path access within the batch.
    pub logical_time: LogicalTime,
    /// Ordinal of the access effect within the transaction.
    pub effect_ordinal_in_tx: u32,
}

/// Diagnostic access observation recorded for a failed transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedAccessObservation {
    /// Cell key.
    pub key: CellKey,
    /// Declared cell type.
    pub type_id: TypeId,
    /// Access kind.
    pub op: OpKind,
    /// Decoded typed value, or `None` for null.
    pub value: Option<TypedValue>,
    /// Diagnostic attempt-local time within the failed transaction.
    pub attempt_time: LogicalTime,
    /// Ordinal of the access observation within the transaction.
    pub effect_ordinal_in_tx: u32,
}

/// Typed property-read effect recorded during execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedPropertyReadEffect {
    /// Instruction index within the transaction body.
    pub instruction_index: usize,
    /// Typed property query result.
    pub result: TypedPropertyQueryResult,
}

/// Typed precompile call effect recorded during execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedPrecompileCallEffect {
    /// Instruction index within the transaction body.
    pub instruction_index: usize,
    /// Precompile identifier.
    pub precompile_id: PrecompileId,
    /// Typed input values passed to the precompile.
    pub inputs: Vec<TypedValue>,
    /// Typed output values returned by the precompile.
    pub outputs: Vec<TypedValue>,
}

/// Canonical IR-hash effect recorded during execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrHashEffect {
    /// Instruction index within the transaction body.
    pub instruction_index: usize,
    /// Canonically encoded portable inputs passed to `hash_ir`.
    pub inputs: Vec<PortableValue>,
    /// Final portable bytes32 digest written by the instruction.
    pub digest: PortableValue,
}

/// Tx-local typed effect recorder used by the executor.
#[derive(Debug, Clone)]
pub(crate) struct TxJournalBuilder {
    tx_index: u32,
    next_logical_time: LogicalTime,
    next_effect_ordinal_in_tx: u32,
    access_effects: Vec<TypedAccessEffect>,
    property_reads: Vec<TypedPropertyReadEffect>,
    precompile_calls: Vec<TypedPrecompileCallEffect>,
    ir_hash_calls: Vec<IrHashEffect>,
    emitted_events: Vec<EmittedEvent>,
}

impl TxJournalBuilder {
    pub(crate) fn new(tx_index: u32, base_logical_time: LogicalTime) -> Self {
        Self {
            tx_index,
            next_logical_time: base_logical_time,
            next_effect_ordinal_in_tx: 0,
            access_effects: Vec::new(),
            property_reads: Vec::new(),
            precompile_calls: Vec::new(),
            ir_hash_calls: Vec::new(),
            emitted_events: Vec::new(),
        }
    }

    pub(crate) fn record_access(
        &mut self,
        key: CellKey,
        type_id: TypeId,
        op: OpKind,
        value: Option<TypedValue>,
    ) {
        self.access_effects.push(TypedAccessEffect {
            key,
            type_id,
            op,
            value,
            logical_time: self.next_logical_time,
            effect_ordinal_in_tx: self.next_effect_ordinal_in_tx,
        });
        self.next_logical_time += 1;
        self.next_effect_ordinal_in_tx += 1;
    }

    pub(crate) fn record_property_read(
        &mut self,
        instruction_index: usize,
        result: TypedPropertyQueryResult,
    ) {
        self.property_reads.push(TypedPropertyReadEffect {
            instruction_index,
            result,
        });
    }

    pub(crate) fn record_precompile_call(
        &mut self,
        instruction_index: usize,
        precompile_id: PrecompileId,
        inputs: Vec<TypedValue>,
        outputs: Vec<TypedValue>,
    ) {
        self.precompile_calls.push(TypedPrecompileCallEffect {
            instruction_index,
            precompile_id,
            inputs,
            outputs,
        });
    }

    pub(crate) fn record_ir_hash(
        &mut self,
        instruction_index: usize,
        inputs: Vec<PortableValue>,
        digest: PortableValue,
    ) {
        self.ir_hash_calls.push(IrHashEffect {
            instruction_index,
            inputs,
            digest,
        });
    }

    pub(crate) fn record_emitted(&mut self, event: EmittedEvent) {
        self.emitted_events.push(event);
    }

    pub(crate) fn access_effect_count(&self) -> usize {
        self.access_effects.len()
    }

    pub(crate) fn into_success(self) -> SuccessfulTxExecution {
        SuccessfulTxExecution {
            tx_index: self.tx_index,
            access_effects: self.access_effects,
            property_reads: self.property_reads,
            precompile_calls: self.precompile_calls,
            ir_hash_calls: self.ir_hash_calls,
            emitted_events: self.emitted_events,
        }
    }

    pub(crate) fn into_failure(
        self,
        reason: String,
        failed_instruction: Option<usize>,
    ) -> FailedTxExecution {
        FailedTxExecution {
            tx_index: self.tx_index,
            reason,
            partial_accesses: self
                .access_effects
                .into_iter()
                .map(|effect| FailedAccessObservation {
                    key: effect.key,
                    type_id: effect.type_id,
                    op: effect.op,
                    value: effect.value,
                    attempt_time: effect.logical_time,
                    effect_ordinal_in_tx: effect.effect_ordinal_in_tx,
                })
                .collect(),
            failed_instruction,
        }
    }
}

/// Derive the portable state-summary projection from the canonical journal.
pub fn derive_portable_state_summary(
    summary: &ExecutionStateSummary,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<PortableStateSummary, TabulaError> {
    Ok(PortableStateSummary {
        read_set_old: summary
            .read_set_old
            .iter()
            .map(|entry| {
                Ok((
                    entry.key,
                    entry
                        .value
                        .as_ref()
                        .map(|value| type_runtimes.encode_typed(value))
                        .transpose()?,
                ))
            })
            .collect::<Result<Vec<_>, TabulaError>>()?,
        write_set_final: summary
            .write_set_final
            .iter()
            .map(|entry| {
                Ok((
                    entry.key,
                    entry
                        .value
                        .as_ref()
                        .map(|value| type_runtimes.encode_typed(value))
                        .transpose()?,
                ))
            })
            .collect::<Result<Vec<_>, TabulaError>>()?,
    })
}

/// Derive the public portable reporting projection from the canonical journal.
pub fn derive_batch_report(
    journal: &ExecutionJournal,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<BatchReport, TabulaError> {
    let portable_state = derive_portable_state_summary(&journal.state_summary, type_runtimes)?;
    Ok(BatchReport {
        read_set_old: portable_state.read_set_old,
        write_set_final: portable_state.write_set_final,
        txs: journal
            .txs
            .iter()
            .map(|record| derive_tx_result(record, type_runtimes))
            .collect::<Result<Vec<_>, TabulaError>>()?,
    })
}

/// Derive the public consistency status from the canonical journal.
#[must_use]
pub fn derive_consistency_status(
    journal: &ExecutionJournal,
    _type_runtimes: &TypeRuntimeRegistry,
) -> ExecutionConsistencyStatus {
    crate::consistency::check_journal_consistency_status(journal)
}

fn derive_tx_result(
    record: &TxExecutionOutcome,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<TxResult, TabulaError> {
    match record {
        TxExecutionOutcome::Success(shard) => Ok(TxResult::Success {
            emitted: shard.emitted_events.clone(),
            access_trace: shard
                .access_effects
                .iter()
                .map(|effect| derive_access_event(effect, type_runtimes))
                .collect::<Result<Vec<_>, TabulaError>>()?,
            precompile_events: shard
                .precompile_calls
                .iter()
                .map(|effect| derive_precompile_event(shard.tx_index, effect, type_runtimes))
                .collect::<Result<Vec<_>, TabulaError>>()?,
            property_reads: shard
                .property_reads
                .iter()
                .map(|effect| derive_property_read(effect, type_runtimes))
                .collect::<Result<Vec<_>, TabulaError>>()?,
        }),
        TxExecutionOutcome::Failed(failure) => Ok(TxResult::Failed {
            reason: failure.reason.clone(),
            partial_events: failure
                .partial_accesses
                .iter()
                .map(|effect| derive_failed_access_event(effect, type_runtimes))
                .collect::<Result<Vec<_>, TabulaError>>()?,
            failed_instruction: failure.failed_instruction,
        }),
    }
}

fn derive_access_event(
    effect: &TypedAccessEffect,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<AccessEvent, TabulaError> {
    let (value, val_is_null) = match &effect.value {
        Some(value) => (type_runtimes.encode_typed(value)?, false),
        None => (
            type_runtimes.encode_typed(&type_runtimes.zero_of(effect.type_id)?)?,
            true,
        ),
    };
    Ok(AccessEvent {
        key: effect.key,
        op: effect.op,
        value,
        val_is_null,
        time: effect.logical_time,
        effect_ordinal_in_tx: effect.effect_ordinal_in_tx,
    })
}

fn derive_failed_access_event(
    effect: &FailedAccessObservation,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<AccessEvent, TabulaError> {
    let (value, val_is_null) = match &effect.value {
        Some(value) => (type_runtimes.encode_typed(value)?, false),
        None => (
            type_runtimes.encode_typed(&type_runtimes.zero_of(effect.type_id)?)?,
            true,
        ),
    };
    Ok(AccessEvent {
        key: effect.key,
        op: effect.op,
        value,
        val_is_null,
        time: effect.attempt_time,
        effect_ordinal_in_tx: effect.effect_ordinal_in_tx,
    })
}

fn derive_property_read(
    effect: &TypedPropertyReadEffect,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<PropertyReadResult, TabulaError> {
    Ok(PropertyReadResult {
        instruction_index: effect.instruction_index,
        value: type_runtimes.encode_typed(&effect.result.value)?,
        key: effect.result.key,
        is_null: effect.result.is_null,
    })
}

fn derive_precompile_event(
    tx_index: u32,
    effect: &TypedPrecompileCallEffect,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<tabula_core::PrecompileEvent, TabulaError> {
    Ok(tabula_core::PrecompileEvent {
        tx_index: tx_index as usize,
        instruction_index: effect.instruction_index,
        precompile_id: effect.precompile_id.0,
        inputs: effect
            .inputs
            .iter()
            .map(|value| type_runtimes.encode_typed(value))
            .collect::<Result<Vec<_>, TabulaError>>()?,
        outputs: effect
            .outputs
            .iter()
            .map(|value| type_runtimes.encode_typed(value))
            .collect::<Result<Vec<_>, TabulaError>>()?,
    })
}
