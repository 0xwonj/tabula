//! Witness lowering for the PropertyRead opcode.
//!
//! Reads stored property read results from execution, then encodes the
//! result (value, key, is_null) into three destination slots and builds
//! an `InstructionRecord`.

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId};
use tabula_ir::PropertyQuery;
use tabula_types::{bool_typed, u64_typed};

use tabula_chips::execution::trace::Opcode;

use super::context::LoweringContext;

pub(super) fn lower_property_read<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst_val: u16,
    dst_key: u16,
    dst_is_null: u16,
    table: TableId,
    col: ColId,
    query: &PropertyQuery,
) -> Result<(), TabulaError> {
    // 1. Read stored result from execution.
    let stored = ctx
        .property_reads_stored
        .get(ctx.property_read_idx)
        .ok_or_else(|| TabulaError::ProofError {
            phase: "trace_lowering",
            detail: format!(
                "PropertyRead instruction encountered but no stored result at index {}",
                ctx.property_read_idx
            ),
        })?;
    ctx.property_read_idx += 1;

    let value = ctx.decode_column_portable(table, col, &stored.value)?;
    let key_opt = stored.key;
    let is_null = stored.is_null;

    // 2. Encode the value to W field elements.
    let val_enc = ctx.encode_padded(&value)?;

    // 3. Encode the key as U64 → W field elements.
    //    When is_null (no matching key), use RowKey(0) as placeholder.
    let key_u64 = key_opt.unwrap_or(RowKey(0)).0;
    let key_val = u64_typed(key_u64);
    let key_enc = ctx.encode_padded(&key_val)?;

    // 4. Encode the is_null flag as Bool → W field elements.
    let null_val = bool_typed(is_null);
    let null_enc = ctx.encode_padded(&null_val)?;

    // 5. Canonical query operand encoding for the proof claim.
    let (query_arg0, query_arg1) = query.encoded_args();
    let query_arg0_enc = ctx.encode_u64_padded(query_arg0)?;
    let query_arg1_enc = ctx.encode_u64_padded(query_arg1)?;

    // 6. Update the three destination slots.
    let dst_val_idx = dst_val as usize;
    let dst_key_idx = dst_key as usize;
    let dst_is_null_idx = dst_is_null as usize;

    ctx.update_slot(dst_val_idx, value, val_enc.clone(), false)?;
    ctx.update_slot(dst_key_idx, key_val, key_enc.clone(), false)?;
    ctx.update_slot(dst_is_null_idx, null_val, null_enc.clone(), false)?;

    // 7. Build instruction record.
    let mut rec = ctx.empty_record(Opcode::PropertyRead);
    rec.written_slots = vec![dst_val_idx, dst_key_idx, dst_is_null_idx];
    rec.access_t = Some(table.0);
    rec.access_c = Some(col.0);
    rec.property_query_type = Some(query.kind_ordinal());
    rec.property_query_arg0 = query_arg0_enc;
    rec.property_query_arg1 = query_arg1_enc;
    rec.property_result_val = val_enc.clone();
    rec.property_result_key = key_enc.clone();
    rec.property_result_is_null = is_null;
    rec.writes.push((dst_val_idx, val_enc, false));
    rec.writes.push((dst_key_idx, key_enc, false));
    rec.writes.push((dst_is_null_idx, null_enc, is_null));

    ctx.push_record(rec);

    Ok(())
}
