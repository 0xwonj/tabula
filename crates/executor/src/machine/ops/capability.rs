use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_ir as ir;

use crate::machine::entry::{EntryMachineCore, OpFailure, fatal, semantic};

pub(in crate::machine) fn execute<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    op_index: usize,
    op: &ir::Op,
) -> Result<(), OpFailure> {
    match op {
        ir::Op::CallCapability {
            guard,
            capability,
            inputs,
            dsts,
        } => {
            let capability_desc = fatal(machine.program.capability(*capability).cloned())?;
            if !semantic(machine.guard_active(*guard))? {
                for dst in dsts {
                    let ty = fatal(machine.entry.local_type(*dst))?;
                    fatal(machine.assign_local(*dst, semantic(machine.inactive_default(ty))?))?;
                }
            } else {
                let executor = machine.exec.capability_executor.ok_or_else(|| {
                    OpFailure::Fatal(TabulaError::InvalidIr(
                        "capability call encountered but no CapabilityExecutor provided".into(),
                    ))
                })?;
                let inputs_typed = semantic(machine.eval_tuple(inputs))?;
                let outputs = match capability_desc.totality {
                    ir::CapabilityTotality::Total => {
                        fatal(executor.execute(*capability, &inputs_typed))?
                    }
                    ir::CapabilityTotality::Checked => {
                        semantic(executor.execute(*capability, &inputs_typed))?
                    }
                };
                if outputs.len() != capability_desc.outputs.len() {
                    return Err(OpFailure::Fatal(TabulaError::InvalidIr(format!(
                        "capability {} returned {} values but descriptor declares {}",
                        capability_desc.symbol,
                        outputs.len(),
                        capability_desc.outputs.len()
                    ))));
                }
                for (output, expected_ty) in outputs.iter().zip(&capability_desc.outputs) {
                    if output.type_id() != *expected_ty {
                        return Err(OpFailure::Fatal(TabulaError::InvalidIr(format!(
                            "capability {} returned wrong output type",
                            capability_desc.symbol
                        ))));
                    }
                }
                for (dst, output) in dsts.iter().zip(outputs.iter().cloned()) {
                    fatal(machine.assign_local(*dst, output))?;
                }
                machine
                    .effects
                    .record_capability(op_index, *capability, inputs_typed, outputs);
            }
        }
        _ => unreachable!("capability::execute only handles capability ops"),
    }
    Ok(())
}
