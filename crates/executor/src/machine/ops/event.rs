use tabula_core::traits::StateView;
use tabula_ir as ir;

use crate::machine::entry::{EntryMachineCore, OpFailure, fatal, semantic};

pub(in crate::machine) fn execute<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    op_index: usize,
    op: &ir::Op,
) -> Result<(), OpFailure> {
    match op {
        ir::Op::EmitEvent { guard, event, args } => {
            fatal(machine.program.event(*event).map(|_| ()))?;
            if semantic(machine.guard_active(*guard))? {
                let args = semantic(machine.eval_tuple(args))?;
                machine.effects.record_event(op_index, *event, args);
            }
        }
        _ => unreachable!("event::execute only handles event ops"),
    }
    Ok(())
}
