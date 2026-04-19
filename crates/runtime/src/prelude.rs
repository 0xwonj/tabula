//! Prove-path typed input decoders and context/param prelude slot
//! loaders.
//!
//! These six helpers form a cohesive surface: they take the prepared
//! runtime state plus raw portable inputs (entry batches, context
//! inputs) and lift them into the slot layouts consumed by the prove
//! pipeline.
//!
//! The four `decode_*_on_state` helpers are un-gated inside the module
//! because they are reused by the verify-surface `TabulaRuntime` to
//! validate query and execution entry calls. The three `build_*_prelude`
//! helpers are prove-only and carry their own `#[cfg(feature = "prove")]`
//! gate along with their imports.

#![cfg(feature = "verify")]

use tabula_core::PortableValue;
use tabula_ir as ir;
use tabula_types::{ContextValues, TxCall, TypedValue};

use crate::prepared_state::PreparedRuntimeState;
use crate::error::{RuntimeError, VerifyError};

#[cfg(feature = "prove")]
use std::collections::BTreeMap;

#[cfg(feature = "prove")]
use tabula_chips::execution::trace::InstructionRecord;
#[cfg(feature = "prove")]
use tabula_executor as exec;
#[cfg(feature = "prove")]
use tabula_witness::stark::{ContextPreludeSlot, ParamPreludeSlot};

#[cfg(feature = "prove")]
use crate::proof_artifacts::{ContextPreludeArtifacts, PublicStatementSlotLayout};
#[cfg(feature = "prove")]
use crate::semantics as runtime_ir;

pub(crate) fn decode_entry_batch_on_state(
    state: &PreparedRuntimeState,
    batch: &ir::EntryBatch,
) -> Result<Vec<TxCall>, RuntimeError> {
    batch
        .calls
        .iter()
        .map(|call| decode_entry_call_on_state(state, call))
        .collect()
}

pub(crate) fn decode_entry_call_on_state(
    state: &PreparedRuntimeState,
    call: &ir::EntryCall,
) -> Result<TxCall, RuntimeError> {
    let entry = state
        .semantic
        .execution()
        .entry_definition(call.entry_id)
        .map_err(|error| VerifyError::Validation {
            detail: error.to_string(),
        })?;
    if entry.kind != ir::EntryKind::Tx {
        return Err(VerifyError::Validation {
            detail: format!("entry {} is not a tx entry", call.entry_id.0),
        }
        .into());
    }
    let params = decode_params_on_state(state, &entry.params, &call.params)?;
    Ok(TxCall {
        entry_id: call.entry_id,
        params,
    })
}

pub(crate) fn decode_params_on_state(
    state: &PreparedRuntimeState,
    expected: &[ir::ParamDecl],
    params: &[PortableValue],
) -> Result<Vec<TypedValue>, RuntimeError> {
    if expected.len() != params.len() {
        return Err(VerifyError::Validation {
            detail: format!(
                "expected {} params but received {}",
                expected.len(),
                params.len()
            ),
        }
        .into());
    }
    expected
        .iter()
        .zip(params)
        .map(|(param, value)| {
            if value.type_id() != param.ty {
                return Err(VerifyError::Validation {
                    detail: format!(
                        "param {} expects type {} but received {}",
                        param.symbol,
                        param.ty.0,
                        value.type_id().0
                    ),
                }
                .into());
            }
            state.type_runtimes.decode_portable(value).map_err(|error| {
                RuntimeError::from(VerifyError::Validation {
                    detail: error.to_string(),
                })
            })
        })
        .collect()
}

pub(crate) fn decode_context_input_on_state(
    state: &PreparedRuntimeState,
    context: &ir::ContextInput,
) -> Result<ContextValues, RuntimeError> {
    let mut typed = ContextValues::new();
    for (field_id, value) in &context.fields {
        let field = state
            .semantic
            .execution()
            .context_field(*field_id)
            .map_err(|error| VerifyError::Validation {
                detail: error.to_string(),
            })?;
        if value.type_id() != field.ty {
            return Err(VerifyError::Validation {
                detail: format!(
                    "context field {} expects type {} but received {}",
                    field.symbol,
                    field.ty.0,
                    value.type_id().0
                ),
            }
            .into());
        }
        let decoded = state
            .type_runtimes
            .decode_portable(value)
            .map_err(|error| VerifyError::Validation {
                detail: error.to_string(),
            })?;
        typed.insert(*field_id, decoded);
    }
    Ok(typed)
}

#[cfg(feature = "prove")]
pub(crate) fn build_context_prelude(
    runtime_program: &PreparedRuntimeState,
    context_bindings: &[runtime_ir::PublicContextBinding],
    layout: &PublicStatementSlotLayout,
) -> Result<ContextPreludeArtifacts, RuntimeError> {
    let canonical_bindings =
        runtime_ir::canonical_public_context(context_bindings).map_err(|error| {
            VerifyError::StatementBuild {
                detail: error.to_string(),
            }
        })?;
    let item_blocks = runtime_ir::canonical_public_context_payload(
        context_bindings,
        &runtime_program.type_runtimes,
        &runtime_program.encoding_runtimes,
        &runtime_program.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?
    .into_iter()
    .skip(1)
    .collect::<Vec<_>>();

    let mut slots = Vec::with_capacity(canonical_bindings.len());
    let mut records = Vec::with_capacity(canonical_bindings.len());
    for (item_index, binding) in canonical_bindings.iter().enumerate() {
        let slot = layout
            .context_slots
            .iter()
            .find_map(|(field_id, slot)| (*field_id == binding.field).then_some(*slot))
            .ok_or_else(|| VerifyError::Validation {
                detail: format!(
                    "missing reserved execution slot for context field {}",
                    binding.field.0
                ),
            })?;
        let typed = runtime_program
            .type_runtimes
            .decode_portable(&binding.value)
            .map_err(|source| VerifyError::StatementBuild {
                detail: source.to_string(),
            })?;
        let encoded = runtime_ir::encode_public_statement_value(
            &typed,
            &runtime_program.encoding_runtimes,
            &runtime_program.tuple_encoding_defaults,
        )
        .map_err(|source| VerifyError::StatementBuild {
            detail: source.to_string(),
        })?;
        slots.push(ContextPreludeSlot {
            field_id: binding.field,
            slot,
            value: typed.clone(),
            encoded: encoded.field_elements.to_vec(),
        });
        records.push(InstructionRecord {
            opcode: tabula_chips::execution::trace::Opcode::LoadContext,
            tx_index: 0,
            proof_meta0: Some(item_index as u32),
            proof_meta1: Some(binding.field.0),
            proof_meta2: Some(encoded.type_id.0),
            written_slots: vec![slot],
            src1_val: encoded.field_elements.to_vec(),
            writes: vec![(slot, encoded.field_elements.to_vec(), false)],
            ..InstructionRecord::default()
        });
    }
    Ok((slots, records, item_blocks))
}

#[cfg(feature = "prove")]
pub(crate) fn build_param_prelude(
    runtime_program: &PreparedRuntimeState,
    layout: &PublicStatementSlotLayout,
    entry: &ir::Entry,
    call: &TxCall,
    tx_item_index_base: u32,
    tx_index: u32,
) -> Result<(Vec<ParamPreludeSlot>, Vec<InstructionRecord>), RuntimeError> {
    let mut slots = Vec::with_capacity(entry.params.len());
    let mut records = Vec::with_capacity(entry.params.len() + 1);

    records.push(InstructionRecord {
        opcode: tabula_chips::execution::trace::Opcode::TxBegin,
        tx_index,
        proof_meta0: Some(tx_item_index_base),
        proof_meta1: Some(call.entry_id.0),
        proof_meta2: Some(entry.params.len() as u32),
        ..InstructionRecord::default()
    });

    for (param_index, param) in entry.params.iter().enumerate() {
        let value =
            call.params
                .get(param_index)
                .cloned()
                .ok_or_else(|| VerifyError::Validation {
                    detail: format!(
                        "tx {tx_index} is missing parameter {} for entry {}",
                        param.symbol, entry.symbol
                    ),
                })?;
        let encoded = runtime_ir::encode_public_statement_value(
            &value,
            &runtime_program.encoding_runtimes,
            &runtime_program.tuple_encoding_defaults,
        )
        .map_err(|source| VerifyError::StatementBuild {
            detail: source.to_string(),
        })?;
        let slot = layout.param_slot_base + param_index;
        slots.push(ParamPreludeSlot {
            param_id: param.id,
            slot,
            value: value.clone(),
            encoded: encoded.field_elements.to_vec(),
        });

        records.push(InstructionRecord {
            opcode: tabula_chips::execution::trace::Opcode::LoadParam,
            tx_index,
            proof_meta0: Some(tx_item_index_base + 1 + param_index as u32),
            proof_meta1: Some(param_index as u32),
            proof_meta2: Some(encoded.type_id.0),
            written_slots: vec![slot],
            src1_val: encoded.field_elements.to_vec(),
            writes: vec![(slot, encoded.field_elements.to_vec(), false)],
            ..InstructionRecord::default()
        });
    }

    Ok((slots, records))
}

#[cfg(feature = "prove")]
pub(crate) fn build_event_item_bases(
    executed: &exec::ExecutionJournal,
) -> (
    BTreeMap<u32, BTreeMap<usize, u32>>,
    Vec<runtime_ir::ProofEventEffect>,
) {
    let mut per_tx = BTreeMap::new();
    let mut events = Vec::new();
    let mut next_item_index = 0u32;

    for tx in executed.successful_txs() {
        let mut per_op = BTreeMap::new();
        for effect in &tx.event_effects {
            per_op.insert(effect.op_index, next_item_index);
            next_item_index += 1 + effect.args.len() as u32;
            events.push(runtime_ir::ProofEventEffect {
                tx_index: tx.tx_index,
                effect: effect.clone(),
            });
        }
        per_tx.insert(tx.tx_index, per_op);
    }

    (per_tx, events)
}
