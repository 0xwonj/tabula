use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_ir as ir;
use tabula_types::TypedValue;

use crate::host::{PropertyReadQuery, PropertyReadRequest};
use crate::machine::entry::{EntryMachineCore, OpFailure, fatal, semantic};

pub(in crate::machine) fn execute<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    op_index: usize,
    op: &ir::Op,
) -> Result<(), OpFailure> {
    match op {
        ir::Op::ReadStateProperty {
            guard,
            dsts,
            table,
            field,
            query,
        } => {
            if !semantic(machine.guard_active(*guard))? {
                for dst in dsts {
                    let ty = fatal(machine.entry.local_type(*dst))?;
                    fatal(machine.assign_local(*dst, semantic(machine.inactive_default(ty))?))?;
                }
            } else {
                let outputs = fatal(execute_state_property_read(
                    machine, *table, *field, query, dsts,
                ))?;
                machine
                    .effects
                    .record_property(op_index, *table, *field, query.clone(), outputs);
            }
        }
        _ => unreachable!("property::execute only handles property ops"),
    }
    Ok(())
}

fn execute_state_property_read<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    table: ir::TableId,
    field: ir::FieldId,
    query: &ir::StatePropertyQuery,
    dsts: &[ir::LocalId],
) -> Result<Vec<TypedValue>, TabulaError> {
    let property_reads = machine.exec.property_reads.ok_or_else(|| {
        TabulaError::InvalidIr(
            "ReadStateProperty encountered but no PropertyReadExecutor was provided".into(),
        )
    })?;
    let evaluated_key = match &query {
        ir::StatePropertyQuery::Successor { key } | ir::StatePropertyQuery::Predecessor { key } => {
            Some(machine.eval_tuple(key)?)
        }
        _ => None,
    };
    let evaluated_range = match &query {
        ir::StatePropertyQuery::NonExistenceRange { lower, upper } => {
            Some((machine.eval_tuple(lower)?, machine.eval_tuple(upper)?))
        }
        _ => None,
    };
    let request = build_property_read_request(
        machine.program,
        table,
        field,
        query,
        evaluated_key,
        evaluated_range,
        dsts.len(),
    )?;
    let outputs = property_reads.execute(&request, machine.exec.type_runtimes)?;
    for (dst, output) in dsts.iter().zip(outputs.iter().cloned()) {
        machine.assign_local(*dst, output)?;
    }
    Ok(outputs)
}

pub(crate) fn build_property_read_request(
    program: &crate::program::ResolvedExecutionProgram,
    table: ir::TableId,
    field: ir::FieldId,
    query: &ir::StatePropertyQuery,
    evaluated_key: Option<Vec<TypedValue>>,
    evaluated_range: Option<(Vec<TypedValue>, Vec<TypedValue>)>,
    output_arity: usize,
) -> Result<PropertyReadRequest, TabulaError> {
    let schema = &program.table(table)?.schema;
    let key_type = *schema.key_tys.first().ok_or_else(|| {
        TabulaError::InvalidIr(format!(
            "table {} has no key columns for property read",
            table.0
        ))
    })?;
    let field_type = program.field_type(table, field)?;
    let query = match query {
        ir::StatePropertyQuery::Minimum => PropertyReadQuery::Minimum,
        ir::StatePropertyQuery::Maximum => PropertyReadQuery::Maximum,
        ir::StatePropertyQuery::Successor { .. } => PropertyReadQuery::Successor {
            key: evaluated_key.ok_or_else(|| {
                TabulaError::InvalidIr("missing evaluated property-read successor key".into())
            })?,
        },
        ir::StatePropertyQuery::Predecessor { .. } => PropertyReadQuery::Predecessor {
            key: evaluated_key.ok_or_else(|| {
                TabulaError::InvalidIr("missing evaluated property-read predecessor key".into())
            })?,
        },
        ir::StatePropertyQuery::Aggregate { kind } => PropertyReadQuery::Aggregate { kind: *kind },
        ir::StatePropertyQuery::NonExistenceRange { .. } => {
            let (lower, upper) = evaluated_range.ok_or_else(|| {
                TabulaError::InvalidIr("missing evaluated property-read range bounds".into())
            })?;
            PropertyReadQuery::NonExistenceRange { lower, upper }
        }
    };
    Ok(PropertyReadRequest {
        table,
        field,
        key_type,
        key_arity: schema.key_tys.len(),
        field_type,
        query,
        output_arity,
    })
}
