use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{ColId, TableId};
use tabula_ir::RowExpr;

use tabula_chips::execution::trace::Opcode;
use tabula_chips::static_table::trace::StaticTableRow;

use super::context::LoweringContext;

pub(super) fn lower_lookup<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    static_table: TableId,
    col: ColId,
    row: &RowExpr,
) -> Result<(), TabulaError> {
    let row_key = ctx.resolve_row(row)?;
    let value = ctx.static_tables.lookup(static_table, row_key, col)?;
    let dst_enc = ctx.encode_padded(&value)?;

    let slot = dst as usize;
    ctx.update_slot(slot, value, dst_enc.clone(), false)?;

    let mut rec = ctx.empty_record(Opcode::Lookup);
    rec.written_slots = vec![slot];
    rec.access_t = Some(static_table.0);
    rec.access_c = Some(col.0);
    rec.access_r = Some(row_key.0);
    rec.access_val = Some(dst_enc.clone());
    rec.access_is_null = Some(false);
    rec.dst_val = dst_enc;
    rec.dst_is_null = false;
    ctx.push_record(rec);

    ctx.push_static_row(StaticTableRow {
        table_id: static_table.0,
        col_id: col.0,
        row_key: row_key.0,
        value: ctx.codec.encode(&value)?,
        lookup_mult: 1,
    });

    Ok(())
}
