use std::collections::{BTreeMap, BTreeSet};

use tabula_artifact::State;
use tabula_core::{ColId, PortableValue, TableId};
use tabula_executor::ExecutionJournal;
use tabula_types::{TypeRuntimeRegistry, TypedValue};
use tabula_witness::{ColumnValueProfile, ColumnWrite, CommittedEntry, InitCell};

use crate::error::RuntimeError;
use crate::program::{ColumnProofSlot, PrecompileProofSlot, ResolvedProofProgram};

use super::types::{
    ColumnPlanIndex, PrecompilePlanIndex, PreparedBatchPlanContext, ProofColumnSlot,
};

pub(super) fn build_column_plan_index(
    column_slots: &[ColumnProofSlot],
) -> Result<ColumnPlanIndex, RuntimeError> {
    let mut index = BTreeMap::new();
    for (slot_idx, slot) in column_slots.iter().enumerate() {
        if index.insert((slot.table, slot.col), slot_idx).is_some() {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate proof column slot for ({}, {})",
                    slot.table.0, slot.col.0,
                ),
            });
        }
    }
    Ok(index)
}

pub(super) fn build_precompile_plan_index(
    precompile_slots: &[PrecompileProofSlot],
) -> Result<PrecompilePlanIndex, RuntimeError> {
    let mut index = BTreeMap::new();
    for (slot_idx, slot) in precompile_slots.iter().enumerate() {
        if index
            .insert(slot.descriptor.precompile_id, slot_idx)
            .is_some()
        {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate proof precompile slot for 0x{:04x}",
                    slot.descriptor.precompile_id.0,
                ),
            });
        }
    }
    Ok(index)
}

pub(super) fn build_column_profile_map(
    resolved_program: &ResolvedProofProgram,
    column_slots: &[ColumnProofSlot],
) -> Result<BTreeMap<(TableId, ColId), ColumnValueProfile>, RuntimeError> {
    let mut profile_map = BTreeMap::new();
    for slot in column_slots {
        let schema = resolved_program
            .schemas_by_id()
            .get(&slot.table)
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!("no schema for table {}", slot.table.0),
            })?;
        let column = schema
            .columns
            .iter()
            .find(|column| column.id == slot.col)
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!("no column {} in table {}", slot.col.0, slot.table.0,),
            })?;
        let resolved = resolved_program
            .program()
            .profile_catalog()
            .resolve_column_profile(column.column_profile_id)
            .map_err(|err| RuntimeError::ValidationFailed {
                detail: format!(
                    "column profile {} for table {} col {} is invalid: {err}",
                    column.column_profile_id.0, slot.table.0, slot.col.0,
                ),
            })?;
        resolved_program
            .type_runtimes()
            .resolve(resolved.type_descriptor.type_id)
            .map_err(|err| RuntimeError::ValidationFailed {
                detail: format!(
                    "missing type runtime {} for table {} col {}: {err}",
                    resolved.type_descriptor.type_id.0, slot.table.0, slot.col.0,
                ),
            })?;
        resolved_program
            .encoding_runtimes()
            .resolve(resolved.encoding_profile.encoding_profile_id)
            .map_err(|err| RuntimeError::ValidationFailed {
                detail: format!(
                    "missing encoding runtime {} for table {} col {}: {err}",
                    resolved.encoding_profile.encoding_profile_id.0, slot.table.0, slot.col.0,
                ),
            })?;
        profile_map.insert(
            (slot.table, slot.col),
            ColumnValueProfile {
                type_id: resolved.type_descriptor.type_id,
                encoding_profile_id: resolved.encoding_profile.encoding_profile_id,
            },
        );
    }
    Ok(profile_map)
}

pub(super) fn collect_old_entries_by_slot(
    ctx: &PreparedBatchPlanContext<'_>,
    resolved_program: &ResolvedProofProgram,
    state_file: &State,
) -> Result<Vec<Vec<CommittedEntry>>, RuntimeError> {
    let mut entries_by_slot = vec![Vec::new(); ctx.column_slots.len()];

    for cell in &state_file.cells {
        let key = (
            tabula_core::TableId(cell.table),
            tabula_core::ColId(cell.col),
        );
        let slot_idx =
            ctx.column_index
                .get(&key)
                .copied()
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!(
                        "prevalidated state cell ({}, {}) is missing from the proof slot index",
                        cell.table, cell.col,
                    ),
                })?;
        let slot = &ctx.column_slots[slot_idx];
        let profile = ctx
            .column_profiles
            .get(&(slot.table, slot.col))
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "missing sealed type/encoding profile for proof slot ({}, {})",
                    slot.table.0, slot.col.0,
                ),
            })?;
        let value = if let Some(portable) = &cell.value {
            decode_column_portable(
                resolved_program.type_runtimes(),
                profile,
                slot.table,
                slot.col,
                portable,
                "committed-state entry",
            )?
        } else {
            resolved_program
                .type_runtimes()
                .zero_of(profile.type_id)
                .map_err(|err| RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing canonical zero value for type {} while collecting committed entries for table {} col {}: {err}",
                        profile.type_id.0, slot.table.0, slot.col.0,
                    ),
                })?
        };
        entries_by_slot[slot_idx].push(CommittedEntry {
            row: tabula_core::RowKey(cell.row),
            value,
            is_null: cell.value.is_none(),
        });
    }

    for (slot_idx, slot) in ctx.column_slots.iter().enumerate() {
        if !resolved_program
            .column_backends()
            .contains_key(&(slot.table, slot.col))
        {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "missing materialized backend for table {} col {}",
                    slot.table.0, slot.col.0,
                ),
            });
        }
        entries_by_slot[slot_idx].sort_by_key(|entry| entry.row);
    }

    Ok(entries_by_slot)
}

pub(super) fn collect_empty_columns(
    column_slots: &[ColumnProofSlot],
    old_entries_by_slot: &[Vec<CommittedEntry>],
) -> BTreeSet<(TableId, ColId)> {
    column_slots
        .iter()
        .zip(old_entries_by_slot.iter())
        .filter_map(|(slot, entries)| {
            entries
                .iter()
                .all(|entry| entry.is_null)
                .then_some((slot.table, slot.col))
        })
        .collect()
}

pub(super) fn reduce_init_cells(
    columns: &mut [ProofColumnSlot],
    column_index: &ColumnPlanIndex,
    journal: &ExecutionJournal,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<(), RuntimeError> {
    for entry in &journal.state_summary.read_set_old {
        let slot_idx = *column_index
            .get(&(entry.key.table, entry.key.col))
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "read-set column ({}, {}) missing from proof plan",
                    entry.key.table.0, entry.key.col.0,
                ),
            })?;
        let value = match &entry.value {
            Some(value) => value.clone(),
            None => type_runtimes.zero_of(entry.type_id).map_err(|source| {
                RuntimeError::WitnessGeneration {
                    detail: source.to_string(),
                }
            })?,
        };
        columns[slot_idx].init_cells.push(InitCell {
            key: entry.key,
            value,
            is_null: entry.value.is_none(),
        });
    }
    Ok(())
}

pub(super) fn reduce_writes(
    columns: &mut [ProofColumnSlot],
    column_index: &ColumnPlanIndex,
    journal: &ExecutionJournal,
) -> Result<(), RuntimeError> {
    for entry in &journal.state_summary.write_set_final {
        let slot_idx = *column_index
            .get(&(entry.key.table, entry.key.col))
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "write-set column ({}, {}) missing from proof plan",
                    entry.key.table.0, entry.key.col.0,
                ),
            })?;
        columns[slot_idx].writes.push(ColumnWrite {
            row: entry.key.row,
            value: entry.value.clone(),
        });
    }
    Ok(())
}

pub(super) fn decode_column_portable(
    type_runtimes: &TypeRuntimeRegistry,
    profile: &ColumnValueProfile,
    table: TableId,
    col: ColId,
    portable: &PortableValue,
    label: &str,
) -> Result<TypedValue, RuntimeError> {
    if portable.type_id() != profile.type_id {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "{label} type {} does not match sealed column type {} for table {} col {}",
                portable.type_id().0,
                profile.type_id.0,
                table.0,
                col.0,
            ),
        });
    }
    type_runtimes
        .decode_portable(portable)
        .map_err(|err| RuntimeError::ValidationFailed {
            detail: format!(
                "failed to decode {label} for table {} col {}: {err}",
                table.0, col.0,
            ),
        })
}
