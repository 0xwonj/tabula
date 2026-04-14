use tabula_core::traits::StateView;
use tabula_ir as ir;
use tabula_types::bool_typed;

use crate::machine::entry::{EntryMachineCore, OpFailure, fatal, semantic};
use crate::surface::StateEffectKind;

pub(in crate::machine) fn execute<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    op_index: usize,
    op: &ir::Op,
) -> Result<(), OpFailure> {
    match op {
        ir::Op::ReadState {
            guard,
            dst_value,
            dst_present,
            table,
            key,
            field,
        } => {
            let field_ty = fatal(machine.exec.state_runtime.column_type(*table, *field))?;
            if !semantic(machine.guard_active(*guard))? {
                fatal(
                    machine.assign_local(*dst_value, semantic(machine.inactive_default(field_ty))?),
                )?;
                fatal(machine.assign_local(*dst_present, bool_typed(false)))?;
            } else {
                let key = fatal(machine.resolve_cell_key(*table, *field, key))?;
                let value = semantic(machine.overlay.read(&key, field_ty))?;
                machine.effects.record_state(
                    op_index,
                    key,
                    field_ty,
                    StateEffectKind::Read,
                    value.clone(),
                );
                match value {
                    Some(value) => {
                        fatal(machine.assign_local(*dst_value, value))?;
                        fatal(machine.assign_local(*dst_present, bool_typed(true)))?;
                    }
                    None => {
                        fatal(machine.assign_local(
                            *dst_value,
                            semantic(machine.inactive_default(field_ty))?,
                        ))?;
                        fatal(machine.assign_local(*dst_present, bool_typed(false)))?;
                    }
                }
            }
        }
        ir::Op::WriteState {
            guard,
            table,
            key,
            field,
            value,
        } => {
            if semantic(machine.guard_active(*guard))? {
                let field_ty = fatal(machine.exec.state_runtime.column_type(*table, *field))?;
                let key = fatal(machine.resolve_cell_key(*table, *field, key))?;
                let value = semantic(machine.eval_value(value))?;
                machine.effects.record_state(
                    op_index,
                    key.clone(),
                    field_ty,
                    StateEffectKind::Write,
                    Some(value.clone()),
                );
                semantic(machine.overlay.write(&key, Some(value), field_ty))?;
            }
        }
        ir::Op::DeleteState {
            guard,
            table,
            key,
            field,
        } => {
            if semantic(machine.guard_active(*guard))? {
                let field_ty = fatal(machine.exec.state_runtime.column_type(*table, *field))?;
                let key = fatal(machine.resolve_cell_key(*table, *field, key))?;
                machine.effects.record_state(
                    op_index,
                    key.clone(),
                    field_ty,
                    StateEffectKind::Delete,
                    None,
                );
                semantic(machine.overlay.write(&key, None, field_ty))?;
            }
        }
        _ => unreachable!("state::execute only handles state ops"),
    }
    Ok(())
}
