use tabula_core::PortableValue;
use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_ir as ir;
use tabula_types::{RelationEffectKind, TypeRuntimeRegistry, TypedValue};

use crate::machine::entry::{EntryMachineCore, OpFailure, fatal, semantic};

pub(in crate::machine) fn execute<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    op_index: usize,
    op: &ir::Op,
) -> Result<(), OpFailure> {
    match op {
        ir::Op::AssertRelation {
            guard,
            relation,
            args,
        } => {
            if semantic(machine.guard_active(*guard))? {
                let inputs = semantic(machine.eval_tuple(args))?;
                let relation_entry = fatal(machine.program.relation(*relation).cloned())?;
                let matched = semantic(relation_matches(
                    &relation_entry,
                    &inputs,
                    machine.exec.type_runtimes,
                ))?;
                if !matched {
                    return Err(OpFailure::Semantic(TabulaError::AssertionFailed(format!(
                        "relation assertion failed for {}",
                        relation_entry.descriptor.symbol
                    ))));
                }
                machine.effects.record_relation(
                    op_index,
                    *relation,
                    RelationEffectKind::Assert,
                    inputs,
                    vec![],
                );
            }
        }
        ir::Op::EvalRelation {
            guard,
            relation,
            inputs,
            dsts,
        } => {
            let relation_entry = fatal(machine.program.relation(*relation).cloned())?;
            if !semantic(machine.guard_active(*guard))? {
                for dst in dsts {
                    let ty = fatal(machine.entry.local_type(*dst))?;
                    fatal(machine.assign_local(*dst, semantic(machine.inactive_default(ty))?))?;
                }
            } else {
                let inputs_typed = semantic(machine.eval_tuple(inputs))?;
                let outputs = semantic(relation_eval(
                    &relation_entry,
                    &inputs_typed,
                    machine.exec.type_runtimes,
                ))?;
                if outputs.len() != dsts.len() {
                    return Err(OpFailure::Fatal(TabulaError::InvalidIr(format!(
                        "relation {} output arity mismatch",
                        relation_entry.descriptor.symbol
                    ))));
                }
                for (dst, output) in dsts.iter().zip(outputs.iter().cloned()) {
                    fatal(machine.assign_local(*dst, output))?;
                }
                machine.effects.record_relation(
                    op_index,
                    *relation,
                    RelationEffectKind::Eval,
                    inputs_typed,
                    outputs,
                );
            }
        }
        _ => unreachable!("relation::execute only handles relation ops"),
    }
    Ok(())
}

fn relation_matches(
    relation: &ir::RelationManifestEntry,
    inputs: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<bool, TabulaError> {
    let portable_inputs = encode_typed_values(inputs, type_runtimes)?;
    Ok(match &relation.binding {
        ir::RelationBinding::EnumSet { values } => {
            portable_inputs.len() == 1 && values.contains(&portable_inputs[0])
        }
        ir::RelationBinding::Map { rows } => rows.iter().any(|row| row.inputs == portable_inputs),
    })
}

fn relation_eval(
    relation: &ir::RelationManifestEntry,
    inputs: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<Vec<TypedValue>, TabulaError> {
    let portable_inputs = encode_typed_values(inputs, type_runtimes)?;
    match &relation.binding {
        ir::RelationBinding::EnumSet { .. } => Ok(Vec::new()),
        ir::RelationBinding::Map { rows } => rows
            .iter()
            .find(|row| row.inputs == portable_inputs)
            .ok_or_else(|| {
                TabulaError::AssertionFailed(format!(
                    "no relation row matched {}",
                    relation.descriptor.symbol
                ))
            })?
            .outputs
            .iter()
            .map(|value| type_runtimes.decode_portable(value))
            .collect(),
    }
}

fn encode_typed_values(
    values: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<Vec<PortableValue>, TabulaError> {
    values
        .iter()
        .map(|value| type_runtimes.encode_typed(value))
        .collect()
}
