//! Prove-path artifact assembly: column slots, witness kits,
//! public-statement slot layout, machine input.
#![cfg(feature = "prove")]

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use tabula_chips::execution::MAX_SLOTS;
use tabula_commitment::PoseidonHasher;
#[cfg(all(test, feature = "prove"))]
use tabula_contract::BoundStatement;
use tabula_contract::PublicStatement;
use tabula_core::{ColId, TableId};
use tabula_executor as exec;
use tabula_ext::backend::column::{ColumnProofContext, PreparedColumnDelta, PreparedColumnProof};
use tabula_ext::root::{RootBackendBundle, RootWitnessContext};
use tabula_ir as ir;
use tabula_machine::{ColumnSlotKey, PreparedColumnInput, PreparedMachineInput, PreparedTierInput};
use tabula_types::StateEffectKind;
use tabula_types::{ContextValues, TxCall};
use tabula_witness::stark::prepare_execution_store;
use tabula_witness::stark::{
    ChipKitRegistry, ContextPreludeSlot, LowerSuccessfulTxInput, lower_successful_tx,
    merge_lowering_outputs,
};
use tabula_witness::{
    AccessEvent, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim, prepare_relation_proof,
};

use crate::error::{ProveError, RuntimeError, VerifyError};
use crate::prepared_state::{ColumnProofSlot, PreparedRuntimeState};
use crate::semantics as runtime_ir;
use crate::snapshot::CommittedStateSnapshot;

#[derive(Clone)]
pub(crate) struct PreparedColumnSlot {
    table: TableId,
    col: ColId,
    old_entries: Vec<CommittedEntry>,
    init_cells: Vec<InitCell>,
    access_events: Vec<AccessEvent>,
    writes: Vec<ColumnWrite>,
    property_reads: Vec<PropertyReadClaim>,
}

pub(crate) struct PreparedColumnArtifacts {
    input: PreparedColumnInput,
}

pub(crate) struct PreparedArtifacts {
    pub(crate) public_statement: PublicStatement,
    pub(crate) execution: PreparedTierInput,
    pub(crate) columns: Vec<PreparedColumnArtifacts>,
    pub(crate) root: PreparedTierInput,
}

impl PreparedArtifacts {
    pub(crate) fn into_prepared_machine_input(
        self,
        binding_digest: [u8; 32],
    ) -> PreparedMachineInput {
        PreparedMachineInput {
            execution: self.execution,
            columns: self
                .columns
                .into_iter()
                .map(|column| column.input)
                .collect(),
            root: self.root,
            binding_digest,
        }
    }
}

pub(crate) struct PublicStatementSlotLayout {
    pub(crate) aux_slot_limit: usize,
    pub(crate) context_slots: Vec<(ir::ContextFieldId, usize)>,
    pub(crate) param_slot_base: usize,
}

pub(crate) type ContextPreludeArtifacts = (
    Vec<ContextPreludeSlot>,
    Vec<tabula_stark::witness_kit::LogicalExecutionPrelude>,
    Vec<[p3_koala_bear::KoalaBear; 8]>,
);

pub(crate) fn build_public_statement_slot_layout(
    context_field_ids: &[ir::ContextFieldId],
    max_param_count: usize,
) -> Result<PublicStatementSlotLayout, RuntimeError> {
    let reserved_slots = context_field_ids
        .len()
        .checked_add(max_param_count)
        .ok_or_else(|| VerifyError::Validation {
            detail: "reserved public-statement slot count overflowed usize".to_string(),
        })?;
    if reserved_slots > MAX_SLOTS {
        return Err(VerifyError::Validation {
            detail: format!(
                "proof-visible public-statement prelude requires {reserved_slots} reserved slots, exceeding the machine ceiling of {MAX_SLOTS}"
            ),
        }.into());
    }
    let aux_slot_limit = MAX_SLOTS - reserved_slots;
    let context_slots = context_field_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(index, field_id)| (field_id, aux_slot_limit + index))
        .collect::<Vec<_>>();
    Ok(PublicStatementSlotLayout {
        aux_slot_limit,
        context_slots,
        param_slot_base: aux_slot_limit + context_field_ids.len(),
    })
}

pub(crate) fn context_public_statement_bindings(
    runtime_program: &PreparedRuntimeState,
    context: &ContextValues,
) -> Result<Vec<runtime_ir::PublicContextBinding>, RuntimeError> {
    runtime_ir::encode_public_context(
        runtime_program.semantic.proof(),
        context,
        &runtime_program.type_runtimes,
    )
    .map_err(|error| {
        RuntimeError::from(VerifyError::StatementBuild {
            detail: error.to_string(),
        })
    })
}

pub(crate) fn prepare_proof_artifacts(
    runtime_program: &PreparedRuntimeState,
    root_backend_bundle: &RootBackendBundle,
    kit_registry: &ChipKitRegistry,
    snapshot: &CommittedStateSnapshot,
    txs: &[TxCall],
    context: &ContextValues,
    executed: &exec::ExecutionJournal,
) -> Result<PreparedArtifacts, RuntimeError> {
    let mut column_slots = Vec::with_capacity(runtime_program.column_slots.len());
    for slot in &runtime_program.column_slots {
        column_slots.push(PreparedColumnSlot {
            table: slot.table,
            col: slot.col,
            old_entries: snapshot.committed_entries(
                slot.table,
                slot.col,
                &runtime_program.type_runtimes,
            )?,
            init_cells: Vec::new(),
            access_events: Vec::new(),
            writes: Vec::new(),
            property_reads: Vec::new(),
        });
    }
    let column_index = runtime_program
        .column_slots
        .iter()
        .enumerate()
        .map(|(index, slot)| ((slot.table, slot.col), index))
        .collect::<BTreeMap<_, _>>();
    let empty_columns = runtime_program
        .column_slots
        .iter()
        .zip(column_slots.iter())
        .filter_map(|(slot, prepared)| {
            prepared
                .old_entries
                .is_empty()
                .then_some((ir::TableId(slot.table.0), ir::FieldId(slot.col.0)))
        })
        .collect::<BTreeSet<_>>();

    for entry in &executed.state_summary.read_set_old {
        let slot = *column_index
            .get(&(entry.key.table, entry.key.col))
            .ok_or_else(|| ProveError::WitnessGeneration {
                detail: format!(
                    "read-set column ({}, {}) missing from the proof plan",
                    entry.key.table.0, entry.key.col.0
                ),
            })?;
        let value = match &entry.value {
            Some(value) => value.clone(),
            None => runtime_program
                .type_runtimes
                .zero_of(entry.type_id)
                .map_err(|source| ProveError::WitnessGeneration {
                    detail: source.to_string(),
                })?,
        };
        column_slots[slot].init_cells.push(InitCell {
            key: entry.key.clone(),
            value,
            is_null: entry.value.is_none(),
        });
    }
    for entry in &executed.state_summary.write_set_final {
        let slot = *column_index
            .get(&(entry.key.table, entry.key.col))
            .ok_or_else(|| ProveError::WitnessGeneration {
                detail: format!(
                    "write-set column ({}, {}) missing from the proof plan",
                    entry.key.table.0, entry.key.col.0
                ),
            })?;
        column_slots[slot].writes.push(ColumnWrite {
            key: entry.key.key.clone(),
            value: entry.value.clone(),
        });
    }

    let context_bindings = context_public_statement_bindings(runtime_program, context)?;
    let canonical_context_ids = runtime_ir::canonical_public_context(&context_bindings)
        .map_err(|error| VerifyError::StatementBuild {
            detail: error.to_string(),
        })?
        .into_iter()
        .map(|binding| binding.field)
        .collect::<Vec<_>>();
    let max_param_count = txs.iter().map(|call| call.params.len()).max().unwrap_or(0);
    let statement_slot_layout =
        build_public_statement_slot_layout(&canonical_context_ids, max_param_count)?;
    let (context_slots, context_records, public_context_transcript_items) =
        crate::prelude::build_context_prelude(
            runtime_program,
            &context_bindings,
            &statement_slot_layout,
        )?;

    let (event_item_bases_by_tx, proof_events) = crate::prelude::build_event_item_bases(executed);
    let event_transcript_items = runtime_ir::canonical_event_log_payload(
        &proof_events,
        &runtime_program.encoding_runtimes,
        &runtime_program.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?
    .into_iter()
    .skip(1)
    .collect::<Vec<_>>();

    let portable_batch = ir::EntryBatch {
        calls: txs
            .iter()
            .map(|call| {
                let params = call
                    .params
                    .iter()
                    .map(|value| runtime_program.type_runtimes.encode_typed(value))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|source| VerifyError::StatementBuild {
                        detail: source.to_string(),
                    })?;
                Ok(ir::EntryCall {
                    entry_id: call.entry_id,
                    params,
                })
            })
            .collect::<Result<Vec<_>, RuntimeError>>()?,
    };
    let tx_batch_transcript_items = runtime_ir::canonical_batch_payload(
        &portable_batch,
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

    let mut tx_prelude_by_index = BTreeMap::new();
    let mut next_tx_item_index = 0u32;
    for (tx_index, call) in txs.iter().enumerate() {
        let entry = runtime_program
            .semantic
            .execution()
            .entry_definition(call.entry_id)
            .map_err(|error| ProveError::WitnessGeneration {
                detail: error.to_string(),
            })?;
        let (param_slots, records) = crate::prelude::build_param_prelude(
            runtime_program,
            &statement_slot_layout,
            entry,
            call,
            next_tx_item_index,
            tx_index as u32,
        )?;
        next_tx_item_index += 1 + call.params.len() as u32;
        tx_prelude_by_index.insert(tx_index as u32, (param_slots, records));
    }

    let mut lowered_txs = BTreeMap::new();
    let empty_event_item_bases = BTreeMap::new();
    let mut kit_scratch = tabula_stark::witness_kit::KitScratch::new();
    for tx in executed.successful_txs() {
        for effect in &tx.state_effects {
            let slot = *column_index
                .get(&(effect.key.table, effect.key.col))
                .ok_or_else(|| ProveError::WitnessGeneration {
                    detail: format!(
                        "state effect column ({}, {}) missing from the proof plan",
                        effect.key.table.0, effect.key.col.0
                    ),
                })?;
            let value = match &effect.value {
                Some(value) => value.clone(),
                None => runtime_program
                    .type_runtimes
                    .zero_of(effect.type_id)
                    .map_err(|source| ProveError::WitnessGeneration {
                        detail: source.to_string(),
                    })?,
            };
            column_slots[slot].access_events.push(AccessEvent {
                key: effect.key.clone(),
                time: effect.logical_time,
                is_write: matches!(
                    effect.kind,
                    StateEffectKind::Write | StateEffectKind::Delete
                ),
                value,
                is_null: effect.value.is_none(),
                tx_index: tx.tx_index,
                effect_ordinal_in_tx: effect.effect_ordinal_in_entry,
            });
        }
        for effect in &tx.property_effects {
            let slot = *column_index
                .get(&(effect.table.into(), effect.field.into()))
                .ok_or_else(|| ProveError::WitnessGeneration {
                    detail: format!(
                        "property effect column ({}, {}) missing from the proof plan",
                        effect.table.0, effect.field.0
                    ),
                })?;
            column_slots[slot].property_reads.push(PropertyReadClaim {
                query: effect.query.clone(),
                result: effect.result.clone(),
            });
        }

        let call = txs
            .get(tx.tx_index as usize)
            .ok_or_else(|| ProveError::WitnessGeneration {
                detail: format!("missing tx call {} during witness lowering", tx.tx_index),
            })?;
        let entry = runtime_program
            .semantic
            .execution()
            .entry_definition(tx.entry_id)
            .map_err(|error| ProveError::WitnessGeneration {
                detail: error.to_string(),
            })?;
        let (param_slots, _) =
            tx_prelude_by_index
                .get(&tx.tx_index)
                .ok_or_else(|| ProveError::WitnessGeneration {
                    detail: format!(
                        "missing reserved parameter prelude for tx {} during witness lowering",
                        tx.tx_index
                    ),
                })?;
        lowered_txs.insert(
            tx.tx_index,
            lower_successful_tx::<3>(
                LowerSuccessfulTxInput {
                    tx_index: tx.tx_index,
                    program: runtime_program.semantic.execution().program(),
                    call,
                    entry,
                    context,
                    state_effects: &tx.state_effects,
                    event_effects: &tx.event_effects,
                    property_effects: &tx.property_effects,
                    relation_effects: &tx.relation_effects,
                    empty_columns: &empty_columns,
                    type_runtimes: &runtime_program.type_runtimes,
                    encoding_runtimes: &runtime_program.encoding_runtimes,
                    tuple_encoding_defaults: &runtime_program.tuple_encoding_defaults,
                    hasher: &PoseidonHasher::new(),
                    state_runtime: &runtime_program.state,
                    context_slots: context_slots.as_slice(),
                    param_slots: param_slots.as_slice(),
                    aux_slot_limit: statement_slot_layout.aux_slot_limit,
                    event_item_bases: event_item_bases_by_tx
                        .get(&tx.tx_index)
                        .unwrap_or(&empty_event_item_bases),
                },
                &mut kit_scratch,
            )
            .map_err(ProveError::TraceBuild)?,
        );
    }

    let mut lowered = merge_lowering_outputs(lowered_txs.values(), kit_scratch);
    let mut instruction_records = Vec::new();
    instruction_records.extend(context_records.into_iter().map(Into::into));
    for tx_index in 0..txs.len() {
        let (_, prelude_records) =
            tx_prelude_by_index.get(&(tx_index as u32)).ok_or_else(|| {
                ProveError::WitnessGeneration {
                    detail: format!("missing tx prelude for tx {tx_index}"),
                }
            })?;
        instruction_records.extend(prelude_records.iter().cloned().map(Into::into));
        if let Some(lowered_tx) = lowered_txs.get(&(tx_index as u32)) {
            instruction_records.extend(lowered_tx.instruction_records.iter().cloned());
        }
    }
    lowered.instruction_records = instruction_records;
    tabula_chips::public_context_transcript::PublicContextTranscriptKit::insert_items(
        &mut lowered.kit_scratch,
        public_context_transcript_items,
    );
    tabula_chips::tx_batch_transcript::TxBatchTranscriptKit::insert_items(
        &mut lowered.kit_scratch,
        tx_batch_transcript_items,
    );
    tabula_chips::event_transcript::EventTranscriptKit::insert_items(
        &mut lowered.kit_scratch,
        event_transcript_items,
    );
    let relation_proof = prepare_relation_proof(
        runtime_program.semantic.execution().program(),
        &runtime_program.static_table_artifact,
        &lowered.relation_claims,
    )
    .map_err(|source| ProveError::WitnessGeneration {
        detail: source.to_string(),
    })?;
    if relation_proof.root() != runtime_program.static_table_artifact.root {
        return Err(ProveError::WitnessGeneration {
            detail: "prepared relation proof root diverged from the registered static table root"
                .to_string(),
        }
        .into());
    }

    tabula_chips::relation_table::RelationTableKit::insert_rows(
        &mut lowered.kit_scratch,
        relation_proof
            .table_rows()
            .iter()
            .map(|row| {
                tabula_stark::witness_kit::LogicalRelationTableRow {
                    relation_id: row.relation_id,
                    input_digest: row.input_digest,
                    output_digest: row.output_digest,
                    lookup_mult: row.lookup_mult,
                }
                .into()
            })
            .collect(),
    );
    let execution_store =
        prepare_execution_store(&mut lowered, kit_registry).map_err(ProveError::TraceBuild)?;

    let prepared_columns = runtime_program
        .column_slots
        .iter()
        .zip(column_slots.into_iter())
        .map(|(slot, mut prepared)| {
            synthesize_missing_init_cells(runtime_program, slot, &mut prepared)?;
            prepare_column_slot(runtime_program, slot, prepared)
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

    let root_bindings = prepared_columns
        .iter()
        .filter_map(|(_, _, proof)| proof.root_binding.clone())
        .collect::<Vec<_>>();
    let witness_preparer = root_backend_bundle.witness_preparer();
    let prepared_root = witness_preparer
        .prepare_root_witness(RootWitnessContext::new(&root_bindings))
        .map_err(|error| {
            let detail = match error {
                tabula_ext::ExtError::Validation { detail } => detail,
                #[cfg(feature = "verify")]
                tabula_ext::ExtError::Setup(source) => source.to_string(),
                tabula_ext::ExtError::RuntimeHook(source)
                | tabula_ext::ExtError::ProofPreparation(source) => source.to_string(),
            };
            ProveError::WitnessGeneration {
                detail: format!(
                    "root witness preparer '{}': {detail}",
                    witness_preparer.name(),
                ),
            }
        })?;
    let (public_statement, root_store) = prepared_root.into_parts();

    Ok(PreparedArtifacts {
        public_statement,
        execution: PreparedTierInput {
            store: execution_store,
        },
        columns: prepared_columns
            .into_iter()
            .map(|(table, col, proof)| PreparedColumnArtifacts {
                input: PreparedColumnInput {
                    key: ColumnSlotKey { table, col },
                    store: proof.store,
                },
            })
            .collect(),
        root: PreparedTierInput { store: root_store },
    })
}

fn synthesize_missing_init_cells(
    runtime_program: &PreparedRuntimeState,
    slot: &ColumnProofSlot,
    prepared: &mut PreparedColumnSlot,
) -> Result<(), RuntimeError> {
    let mut present_rows = prepared
        .init_cells
        .iter()
        .map(|cell| cell.key.key.clone())
        .collect::<BTreeSet<_>>();
    let touched_rows = prepared
        .access_events
        .iter()
        .map(|event| event.key.key.clone())
        .chain(prepared.writes.iter().map(|write| write.key.clone()))
        .collect::<BTreeSet<_>>();
    let old_entries = prepared
        .old_entries
        .iter()
        .map(|entry| (entry.key.clone(), (entry.value.clone(), entry.is_null)))
        .collect::<BTreeMap<_, _>>();
    let required_rows = old_entries
        .keys()
        .cloned()
        .chain(touched_rows.iter().cloned())
        .collect::<BTreeSet<_>>();
    if required_rows.is_empty() {
        return Ok(());
    }
    let field_ty = runtime_program
        .state
        .column_contract(slot.table, slot.col)?
        .ty;

    for row in required_rows {
        if present_rows.contains(&row) {
            continue;
        }
        let (value, is_null) = match old_entries.get(&row) {
            Some((value, is_null)) => (value.clone(), *is_null),
            None => (
                runtime_program
                    .type_runtimes
                    .zero_of(field_ty)
                    .map_err(|source| ProveError::WitnessGeneration {
                        detail: source.to_string(),
                    })?,
                true,
            ),
        };
        prepared.init_cells.push(InitCell {
            key: tabula_core::CommittedCellKey {
                table: slot.table,
                col: slot.col,
                key: row.clone(),
            },
            value,
            is_null,
        });
        present_rows.insert(row);
    }
    prepared.init_cells.sort_by_key(|cell| cell.key.key.clone());
    Ok(())
}

fn prepare_column_slot(
    runtime_program: &PreparedRuntimeState,
    slot: &ColumnProofSlot,
    prepared: PreparedColumnSlot,
) -> Result<(TableId, ColId, PreparedColumnProof), RuntimeError> {
    let backend = runtime_program.state.backend(slot.table, slot.col)?;
    let proof = slot
        .proof_backend
        .prepare_column(ColumnProofContext {
            column: PreparedColumnDelta {
                table: prepared.table,
                col: prepared.col,
                init_cells: prepared.init_cells,
                access_events: prepared.access_events,
                writes: prepared.writes.clone(),
                is_touched: !prepared.writes.is_empty(),
            },
            old_entries: prepared.old_entries,
            property_reads: prepared.property_reads,
        })
        .map_err(|error| {
            let detail = match error {
                tabula_ext::ExtError::Validation { detail } => detail,
                #[cfg(feature = "verify")]
                tabula_ext::ExtError::Setup(source) => source.to_string(),
                tabula_ext::ExtError::RuntimeHook(source)
                | tabula_ext::ExtError::ProofPreparation(source) => source.to_string(),
            };
            ProveError::WitnessGeneration { detail }
        })?;
    match (
        &proof.root_binding,
        backend.root_binding_contract.receives_commitment,
    ) {
        (Some(binding), true) => {
            if binding.table != slot.table
                || binding.col != slot.col
                || binding.root_binding_family != backend.root_binding_contract.root_binding_family
                || binding.column_profile_hash != backend.root_binding_contract.column_profile_hash
                || binding.binding_digest != backend.root_binding_contract.binding_digest
            {
                return Err(ProveError::WitnessGeneration {
                    detail: format!(
                        "prepared column proof ({}, {}) returned a root binding that does not match the sealed backend contract",
                        slot.table.0, slot.col.0,
                    ),
                }.into());
            }
        }
        (None, true) => {
            return Err(ProveError::WitnessGeneration {
                detail: format!(
                    "prepared column proof ({}, {}) omitted a required root binding",
                    slot.table.0, slot.col.0,
                ),
            }
            .into());
        }
        (Some(_), false) => {
            return Err(ProveError::WitnessGeneration {
                detail: format!(
                    "prepared column proof ({}, {}) returned an unexpected root binding",
                    slot.table.0, slot.col.0,
                ),
            }
            .into());
        }
        (None, false) => {}
    }
    Ok((slot.table, slot.col, proof))
}

/// Prepare the machine input and public statement without running the prover.
///
/// Exposed only for tests that need to tamper with witness store contents
/// before proving. Production code must use
/// [`crate::prover::prepare_proof_request_on_prepared_state`] instead.
#[cfg(all(test, feature = "prove"))]
pub(crate) fn prepare_proof_machine_input(
    state: &PreparedRuntimeState,
    root_backend_bundle: &RootBackendBundle,
    kit_registry: &ChipKitRegistry,
    input: &crate::prover::ProveInput<'_>,
) -> Result<(PreparedMachineInput, PublicStatement), RuntimeError> {
    let typed_context = crate::prelude::decode_context_input_on_state(state, input.context)?;
    let typed_txs = crate::prelude::decode_entry_batch_on_state(state, input.batch)?;
    let applied_tx_digest = runtime_ir::compute_applied_tx_digest(
        input.batch,
        &state.type_runtimes,
        &state.encoding_runtimes,
        &state.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?;
    let proof_artifacts = prepare_proof_artifacts(
        state,
        root_backend_bundle,
        kit_registry,
        input.snapshot,
        &typed_txs,
        &typed_context,
        input.executed,
    )?;
    let public_statement = crate::statement::materialize_public_statement_on_state(
        state,
        &typed_context,
        runtime_ir::PublicStatementMaterialization {
            applied_tx_digest,
            old_state_root: proof_artifacts.public_statement.old_root.to_bytes(),
            new_state_root: proof_artifacts.public_statement.new_root.to_bytes(),
        },
        input.executed,
    )?;
    let binding_digest =
        BoundStatement::new(state.artifact_context.clone(), public_statement.clone())
            .binding_digest()
            .map_err(|error| VerifyError::StatementBuild {
                detail: error.to_string(),
            })?;
    let machine_input = proof_artifacts.into_prepared_machine_input(binding_digest);
    Ok((machine_input, public_statement))
}
