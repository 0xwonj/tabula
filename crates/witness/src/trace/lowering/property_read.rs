//! Witness lowering for the PropertyRead opcode.
//!
//! Calls the property reader callback to resolve a structural query against
//! committed column state, then encodes the result (value, key, is_null)
//! into three destination slots and builds an `InstructionRecord`.

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_ir::PropertyQuery;

use tabula_chips::execution::trace::Opcode;

use super::context::LoweringContext;

/// Map a `PropertyQuery` variant to its ordinal for the AIR witness.
///
/// Ordinals: Minimum=0, Maximum=1, Successor=2, Predecessor=3,
/// NonExistenceRange=4, Aggregate=5.
fn query_kind_ordinal(query: &PropertyQuery) -> u8 {
    match query {
        PropertyQuery::Minimum => 0,
        PropertyQuery::Maximum => 1,
        PropertyQuery::Successor { .. } => 2,
        PropertyQuery::Predecessor { .. } => 3,
        PropertyQuery::NonExistenceRange { .. } => 4,
        PropertyQuery::Aggregate { .. } => 5,
    }
}

pub(super) fn lower_property_read<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst_val: u16,
    dst_key: u16,
    dst_is_null: u16,
    table: TableId,
    col: ColId,
    query: &PropertyQuery,
) -> Result<(), TabulaError> {
    let reader = ctx.property_reader.ok_or_else(|| TabulaError::ProofError {
        phase: "trace_lowering",
        detail: "PropertyRead instruction encountered but no property reader provided".into(),
    })?;

    // 1. Call the property reader to resolve the query.
    let (value, key_opt, is_null) = reader(table, col, query)?;

    // 2. Encode the value to W field elements.
    let val_enc = ctx.encode_padded(&value)?;

    // 3. Encode the key as U64 → W field elements.
    //    When is_null (no matching key), use RowKey(0) as placeholder.
    let key_u64 = key_opt.unwrap_or(RowKey(0)).0;
    let key_val = Value::U64(key_u64);
    let key_enc = ctx.encode_padded(&key_val)?;

    // 4. Encode the is_null flag as Bool → W field elements.
    let null_val = Value::Bool(is_null);
    let null_enc = ctx.encode_padded(&null_val)?;

    // 5. Update the three destination slots.
    let dst_val_idx = dst_val as usize;
    let dst_key_idx = dst_key as usize;
    let dst_is_null_idx = dst_is_null as usize;

    ctx.update_slot(dst_val_idx, value, val_enc.clone(), false)?;
    ctx.update_slot(dst_key_idx, key_val, key_enc.clone(), false)?;
    ctx.update_slot(dst_is_null_idx, null_val, null_enc.clone(), false)?;

    // 6. Build instruction record.
    let mut rec = ctx.empty_record(Opcode::PropertyRead);
    rec.written_slots = vec![dst_val_idx, dst_key_idx, dst_is_null_idx];
    rec.access_t = Some(table.0);
    rec.access_c = Some(col.0);
    rec.property_query_type = Some(query_kind_ordinal(query));
    rec.property_result_val = val_enc.clone();
    rec.property_result_key = key_enc;
    rec.property_result_is_null = is_null;
    rec.dst_val = val_enc;
    rec.dst_is_null = false;

    ctx.push_record(rec);

    Ok(())
}
