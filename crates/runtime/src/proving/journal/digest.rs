#[cfg(test)]
use blake3::Hasher as Blake3Hasher;
#[cfg(test)]
use borsh::to_vec;
#[cfg(test)]
use p3_field::PrimeField32;

#[cfg(test)]
use super::types::ProofJournal;

#[cfg(test)]
pub(super) fn journal_digest(journal: &ProofJournal) -> [u8; 32] {
    let mut hasher = Blake3Hasher::new();
    hash_proof_journal(&mut hasher, journal);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
fn update_len(hasher: &mut Blake3Hasher, len: usize) {
    hasher.update(&(len as u64).to_le_bytes());
}

#[cfg(test)]
fn update_bool(hasher: &mut Blake3Hasher, value: bool) {
    hasher.update(&[u8::from(value)]);
}

#[cfg(test)]
fn update_u8(hasher: &mut Blake3Hasher, value: u8) {
    hasher.update(&[value]);
}

#[cfg(test)]
fn update_u16(hasher: &mut Blake3Hasher, value: u16) {
    hasher.update(&value.to_le_bytes());
}

#[cfg(test)]
fn update_u32(hasher: &mut Blake3Hasher, value: u32) {
    hasher.update(&value.to_le_bytes());
}

#[cfg(test)]
fn update_u64(hasher: &mut Blake3Hasher, value: u64) {
    hasher.update(&value.to_le_bytes());
}

#[cfg(test)]
fn update_bytes(hasher: &mut Blake3Hasher, bytes: &[u8]) {
    update_len(hasher, bytes.len());
    hasher.update(bytes);
}

#[cfg(test)]
fn hash_typed_value(hasher: &mut Blake3Hasher, value: &tabula_types::TypedValue) {
    update_u32(hasher, value.type_id().0);
    update_bytes(hasher, value.payload());
}

#[cfg(test)]
fn hash_optional_typed_value(hasher: &mut Blake3Hasher, value: &Option<tabula_types::TypedValue>) {
    update_bool(hasher, value.is_some());
    if let Some(value) = value {
        hash_typed_value(hasher, value);
    }
}

#[cfg(test)]
fn hash_portable_value(hasher: &mut Blake3Hasher, value: &tabula_core::PortableValue) {
    update_u32(hasher, value.type_id().0);
    update_bytes(hasher, value.payload());
}

#[cfg(test)]
fn hash_koalabear_vec(hasher: &mut Blake3Hasher, values: &[p3_koala_bear::KoalaBear]) {
    update_len(hasher, values.len());
    for value in values {
        update_u32(hasher, value.as_canonical_u32());
    }
}

#[cfg(test)]
fn hash_instruction_record(
    hasher: &mut Blake3Hasher,
    record: &tabula_chips::execution::trace::InstructionRecord,
) {
    use tabula_chips::execution::trace::{CmpOp, Opcode};

    let opcode_tag = match record.opcode {
        Opcode::Read => 0u8,
        Opcode::Write => 1,
        Opcode::Add => 2,
        Opcode::Sub => 3,
        Opcode::Mul => 4,
        Opcode::DivMod => 5,
        Opcode::Cmp(op) => {
            update_u8(
                hasher,
                match op {
                    CmpOp::Eq => 0,
                    CmpOp::Ne => 1,
                    CmpOp::Lt => 2,
                    CmpOp::Lte => 3,
                    CmpOp::Gt => 4,
                    CmpOp::Gte => 5,
                },
            );
            6
        }
        Opcode::Not => 7,
        Opcode::And => 8,
        Opcode::Or => 9,
        Opcode::Assert => 10,
        Opcode::Select => 11,
        Opcode::Hash => 12,
        Opcode::Lookup => 13,
        Opcode::Precompile => 14,
        Opcode::PropertyRead => 15,
    };
    update_u8(hasher, opcode_tag);
    update_u32(hasher, record.tx_index);
    update_u32(hasher, record.effect_ordinal_in_tx);
    update_len(hasher, record.written_slots.len());
    for slot in &record.written_slots {
        update_u64(hasher, *slot as u64);
    }
    hash_koalabear_vec(hasher, &record.src1_val);
    hash_koalabear_vec(hasher, &record.src2_val);
    update_bool(hasher, record.cond_val);
    for slot in [
        record.src1_slot_idx,
        record.src2_slot_idx,
        record.cond_slot_idx,
    ] {
        update_bool(hasher, slot.is_some());
        if let Some(slot) = slot {
            update_u64(hasher, slot as u64);
        }
    }
    for access in [
        record.access_t.map(u64::from),
        record.access_c.map(u64::from),
        record.access_r,
    ] {
        update_bool(hasher, access.is_some());
        if let Some(access) = access {
            update_u64(hasher, access);
        }
    }
    update_bool(hasher, record.access_val.is_some());
    if let Some(access_val) = &record.access_val {
        hash_koalabear_vec(hasher, access_val);
    }
    update_bool(hasher, record.access_is_null.unwrap_or(false));
    update_len(hasher, record.writes.len());
    for (slot, value, is_null) in &record.writes {
        update_u64(hasher, *slot as u64);
        hash_koalabear_vec(hasher, value);
        update_bool(hasher, *is_null);
    }
    update_bool(hasher, record.hash_digest.is_some());
    if let Some(digest) = &record.hash_digest {
        for value in digest {
            update_u32(hasher, value.as_canonical_u32());
        }
    }
    update_bool(hasher, record.is_empty_col);
    for field in [
        record.precompile_id.map(u32::from),
        record.instruction_index,
        record.precompile_input_count,
        record.precompile_output_count,
        record.property_query_type.map(u32::from),
    ] {
        update_bool(hasher, field.is_some());
        if let Some(field) = field {
            update_u32(hasher, field);
        }
    }
    update_bool(hasher, record.precompile_event_digest.is_some());
    if let Some(digest) = &record.precompile_event_digest {
        for limb in digest {
            update_u32(hasher, limb.as_canonical_u32());
        }
    }
    hash_koalabear_vec(hasher, &record.property_query_arg0);
    hash_koalabear_vec(hasher, &record.property_query_arg1);
    hash_koalabear_vec(hasher, &record.property_result_val);
    hash_koalabear_vec(hasher, &record.property_result_key);
    update_bool(hasher, record.property_result_is_null);
}

#[cfg(test)]
fn hash_proof_journal(hasher: &mut Blake3Hasher, journal: &ProofJournal) {
    update_len(hasher, journal.lowering.instruction_records.len());
    for record in &journal.lowering.instruction_records {
        hash_instruction_record(hasher, record);
    }
    update_len(hasher, journal.lowering.static_table_rows.len());
    for row in &journal.lowering.static_table_rows {
        update_u32(hasher, row.table_id);
        update_u16(hasher, row.col_id);
        update_u64(hasher, row.row_key);
        hash_koalabear_vec(hasher, &row.value);
        update_u32(hasher, row.lookup_mult);
    }
    update_len(hasher, journal.lowering.ir_hash_calls.len());
    for call in &journal.lowering.ir_hash_calls {
        update_u32(hasher, call.tx_index);
        update_u32(hasher, call.instruction_index);
        update_bytes(hasher, &call.payload);
        for limb in &call.digest {
            update_u32(hasher, *limb);
        }
    }

    update_len(hasher, journal.columns.len());
    for column in &journal.columns {
        update_u32(hasher, column.table.0);
        update_u16(hasher, column.col.0);
        update_u32(hasher, column.type_id.0);
        update_u32(hasher, column.encoding_profile_id.0);
        update_len(hasher, column.old_entries.len());
        for entry in &column.old_entries {
            update_u64(hasher, entry.row.0);
            hash_typed_value(hasher, &entry.value);
            update_bool(hasher, entry.is_null);
        }
        update_len(hasher, column.init_cells.len());
        for cell in &column.init_cells {
            update_u32(hasher, cell.key.table.0);
            update_u16(hasher, cell.key.col.0);
            update_u64(hasher, cell.key.row.0);
            hash_typed_value(hasher, &cell.value);
            update_bool(hasher, cell.is_null);
        }
        update_len(hasher, column.access_events.len());
        for event in &column.access_events {
            update_u32(hasher, event.key.table.0);
            update_u16(hasher, event.key.col.0);
            update_u64(hasher, event.key.row.0);
            update_u64(hasher, event.time);
            update_bool(hasher, event.is_write);
            hash_typed_value(hasher, &event.value);
            update_bool(hasher, event.is_null);
            update_u32(hasher, event.tx_index);
            update_u32(hasher, event.effect_ordinal_in_tx);
        }
        update_len(hasher, column.writes.len());
        for write in &column.writes {
            update_u64(hasher, write.row.0);
            hash_optional_typed_value(hasher, &write.value);
        }
        update_len(hasher, column.property_reads.len());
        for claim in &column.property_reads {
            update_bytes(hasher, &to_vec(&claim.query).expect("borsh property query"));
            hash_typed_value(hasher, &claim.result.value);
            update_bool(hasher, claim.result.key.is_some());
            if let Some(key) = claim.result.key {
                update_u64(hasher, key.0);
            }
            update_bool(hasher, claim.result.is_null);
        }
    }

    update_len(hasher, journal.precompile_calls_by_slot.len());
    for calls in &journal.precompile_calls_by_slot {
        update_len(hasher, calls.len());
        for call in calls {
            update_u64(hasher, call.event.tx_index as u64);
            update_u64(hasher, call.event.instruction_index as u64);
            update_u16(hasher, call.event.precompile_id);
            update_len(hasher, call.event.inputs.len());
            for value in &call.event.inputs {
                hash_portable_value(hasher, value);
            }
            update_len(hasher, call.event.outputs.len());
            for value in &call.event.outputs {
                hash_portable_value(hasher, value);
            }
            update_u32(hasher, call.header.tx_index);
            update_u32(hasher, call.header.instruction_index);
            update_u16(hasher, call.header.precompile_id);
            update_u32(hasher, call.header.input_count);
            update_u32(hasher, call.header.output_count);
            for limb in &call.header.event_digest {
                update_u32(hasher, *limb);
            }
        }
    }

    update_len(hasher, journal.precompile_transcript_calls.len());
    for call in &journal.precompile_transcript_calls {
        update_u64(hasher, call.event.tx_index as u64);
        update_u64(hasher, call.event.instruction_index as u64);
        update_u16(hasher, call.event.precompile_id);
        update_len(hasher, call.event.inputs.len());
        for value in &call.event.inputs {
            hash_portable_value(hasher, value);
        }
        update_len(hasher, call.event.outputs.len());
        for value in &call.event.outputs {
            hash_portable_value(hasher, value);
        }
        update_u32(hasher, call.header.tx_index);
        update_u32(hasher, call.header.instruction_index);
        update_u16(hasher, call.header.precompile_id);
        update_u32(hasher, call.header.input_count);
        update_u32(hasher, call.header.output_count);
        for limb in &call.header.event_digest {
            update_u32(hasher, *limb);
        }
        hash_koalabear_vec(hasher, &call.payload);
    }
}
