use tabula_chips::precompile_transcript::PrecompileTranscriptCall;
use tabula_core::{OpKind, TableId};
use tabula_executor::{
    SuccessfulTxExecution, TypedAccessEffect, TypedPrecompileCallEffect, TypedPropertyReadEffect,
};
use tabula_ext::backend::precompile::ResolvedPrecompileCall;
use tabula_ir::Instruction;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedPropertyQueryResult};
use tabula_witness::stark::{
    LowerSuccessfulTxInput, LoweringPrecompileCall, LoweringPropertyRead, lower_successful_tx,
};
use tabula_witness::{AccessEvent, PropertyReadClaim};

use crate::error::RuntimeError;

use super::types::{TxProofProjection, TxProofProjectionContext};

pub(super) fn build_tx_proof_shard(
    ctx: &TxProofProjectionContext<'_>,
    success: &SuccessfulTxExecution,
) -> Result<TxProofProjection, RuntimeError> {
    let tx = ctx
        .batch
        .transactions
        .get(success.tx_index as usize)
        .ok_or_else(|| RuntimeError::ValidationFailed {
            detail: format!(
                "missing batch transaction {} while reducing proof shard",
                success.tx_index,
            ),
        })?;
    let tx_def = ctx
        .resolved_program
        .program()
        .resolve(tx.tx_type)
        .map_err(RuntimeError::TraceBuild)?;

    let mut lowering_access_trace = Vec::with_capacity(success.access_effects.len());
    let mut access_slot_indices = Vec::with_capacity(success.access_effects.len());
    for effect in &success.access_effects {
        let slot_idx = *ctx
            .column_index
            .get(&(effect.key.table, effect.key.col))
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "access effect column ({}, {}) missing from proof plan",
                    effect.key.table.0, effect.key.col.0,
                ),
            })?;
        access_slot_indices.push(slot_idx);
        lowering_access_trace.push(logical_access_event(
            success.tx_index,
            effect,
            ctx.resolved_program.type_runtimes(),
        )?);
    }
    let lowering_precompile_calls = success
        .precompile_calls
        .iter()
        .map(logical_precompile_call)
        .collect::<Vec<_>>();
    let lowering_property_reads = success
        .property_reads
        .iter()
        .map(logical_property_read)
        .collect::<Vec<_>>();
    let lowering = lower_successful_tx::<3>(LowerSuccessfulTxInput {
        tx_index: success.tx_index,
        tx,
        tx_def,
        profile_map: ctx.column_profiles,
        type_runtimes: ctx.resolved_program.type_runtimes(),
        encoding_runtimes: ctx.resolved_program.encoding_runtimes(),
        static_tables: ctx.static_tables,
        empty_columns: ctx.empty_columns,
        precompile_signatures: ctx.resolved_program.program().precompiles(),
        access_trace: &lowering_access_trace,
        precompile_calls: &lowering_precompile_calls,
        property_reads: &lowering_property_reads,
    })
    .map_err(RuntimeError::TraceBuild)?;

    let mut access_events_by_slot = vec![Vec::new(); ctx.column_index.len()];
    for (slot_idx, event) in access_slot_indices
        .into_iter()
        .zip(lowering_access_trace.into_iter())
    {
        access_events_by_slot[slot_idx].push(event);
    }

    let mut property_reads_by_slot = vec![Vec::new(); ctx.column_index.len()];
    for effect in &success.property_reads {
        let (table, col, query) = property_read_slot(effect, tx_def, success.tx_index)?;
        let slot_idx =
            *ctx.column_index
                .get(&(table, col))
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!(
                        "property-read column ({}, {}) missing from proof plan",
                        table.0, col.0,
                    ),
                })?;
        property_reads_by_slot[slot_idx].push(PropertyReadClaim {
            query,
            result: TypedPropertyQueryResult {
                value: effect.result.value.clone(),
                key: effect.result.key,
                is_null: effect.result.is_null,
            },
        });
    }

    let mut precompile_calls_by_slot = vec![Vec::new(); ctx.precompile_slots.len()];
    let mut precompile_transcript_calls = Vec::new();
    for effect in &success.precompile_calls {
        let slot_idx = *ctx
            .precompile_index
            .get(&effect.precompile_id)
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "precompile 0x{:04x} missing from proof plan",
                    effect.precompile_id.0,
                ),
            })?;
        let descriptor = &ctx.precompile_slots[slot_idx].descriptor;
        let (resolved_call, transcript_call) = materialize_precompile_call(
            success.tx_index,
            effect,
            descriptor,
            ctx.resolved_program.type_runtimes(),
            ctx.resolved_program.encoding_runtimes(),
        )?;
        precompile_calls_by_slot[slot_idx].push(resolved_call);
        precompile_transcript_calls.push(transcript_call);
    }
    precompile_transcript_calls.sort_by_key(|call| {
        (
            call.header.tx_index,
            call.header.instruction_index,
            call.header.precompile_id,
        )
    });

    Ok(TxProofProjection {
        tx_index: success.tx_index,
        lowering,
        access_events_by_slot,
        property_reads_by_slot,
        precompile_calls_by_slot,
        precompile_transcript_calls,
    })
}

fn property_read_slot(
    effect: &TypedPropertyReadEffect,
    tx_def: &tabula_ir::TxTypeDef,
    tx_index: u32,
) -> Result<(TableId, tabula_core::ColId, tabula_ir::PropertyQuery), RuntimeError> {
    match tx_def.body.get(effect.instruction_index) {
        Some(Instruction::PropertyRead {
            table, col, query, ..
        }) => Ok((*table, *col, query.clone())),
        Some(other) => Err(RuntimeError::ValidationFailed {
            detail: format!(
                "tx {} instruction {} is not PropertyRead during proof reduction: {other:?}",
                tx_index, effect.instruction_index,
            ),
        }),
        None => Err(RuntimeError::ValidationFailed {
            detail: format!(
                "tx {} missing instruction {} during proof reduction",
                tx_index, effect.instruction_index,
            ),
        }),
    }
}

fn logical_access_event(
    tx_index: u32,
    effect: &TypedAccessEffect,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<AccessEvent, RuntimeError> {
    let value = match &effect.value {
        Some(value) => value.clone(),
        None => type_runtimes.zero_of(effect.type_id).map_err(|source| {
            RuntimeError::WitnessGeneration {
                detail: source.to_string(),
            }
        })?,
    };
    Ok(AccessEvent {
        key: effect.key,
        time: effect.logical_time,
        is_write: effect.op == OpKind::Write,
        value,
        is_null: effect.value.is_none(),
        tx_index,
        effect_ordinal_in_tx: effect.effect_ordinal_in_tx,
    })
}

fn logical_property_read(effect: &TypedPropertyReadEffect) -> LoweringPropertyRead {
    LoweringPropertyRead {
        instruction_index: effect.instruction_index,
        result: effect.result.clone(),
    }
}

fn logical_precompile_call(effect: &TypedPrecompileCallEffect) -> LoweringPrecompileCall {
    LoweringPrecompileCall {
        instruction_index: effect.instruction_index,
        precompile_id: effect.precompile_id,
        inputs: effect.inputs.clone(),
        outputs: effect.outputs.clone(),
    }
}

fn resolved_precompile_event(
    tx_index: u32,
    effect: &TypedPrecompileCallEffect,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<tabula_core::PrecompileEvent, RuntimeError> {
    Ok(tabula_core::PrecompileEvent {
        tx_index: tx_index as usize,
        instruction_index: effect.instruction_index,
        precompile_id: effect.precompile_id.0,
        inputs: effect
            .inputs
            .iter()
            .map(|value| {
                type_runtimes.encode_typed(value).map_err(|source| {
                    RuntimeError::WitnessGeneration {
                        detail: source.to_string(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        outputs: effect
            .outputs
            .iter()
            .map(|value| {
                type_runtimes.encode_typed(value).map_err(|source| {
                    RuntimeError::WitnessGeneration {
                        detail: source.to_string(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn materialize_precompile_call(
    tx_index: u32,
    effect: &TypedPrecompileCallEffect,
    descriptor: &tabula_artifact::PrecompileDescriptor,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<(ResolvedPrecompileCall, PrecompileTranscriptCall), RuntimeError> {
    let portable_event = resolved_precompile_event(tx_index, effect, type_runtimes)?;
    let transcript_call = PrecompileTranscriptCall::from_event(
        &portable_event,
        descriptor.precompile_id.0,
        &descriptor.signature,
        type_runtimes,
        encoding_runtimes,
    )
    .map_err(|source| RuntimeError::WitnessGeneration {
        detail: source.to_string(),
    })?;
    let resolved_call = ResolvedPrecompileCall {
        event: transcript_call.event.clone(),
        header: tabula_ext::backend::precompile::PrecompileCallHeader {
            tx_index: transcript_call.header.tx_index,
            instruction_index: transcript_call.header.instruction_index,
            precompile_id: transcript_call.header.precompile_id,
            input_count: transcript_call.header.input_count,
            output_count: transcript_call.header.output_count,
            event_digest: transcript_call.header.event_digest,
        },
    };
    Ok((resolved_call, transcript_call))
}
