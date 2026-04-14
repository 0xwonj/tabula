use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_core::{CommittedPropertyQuery, PropertyAggregateKind};
use tabula_ir as ir;
use tabula_types::{TypedCommittedPropertyQueryResult, TypedValue};

use crate::machine::entry::{EntryMachineCore, OpFailure, fatal, semantic};

pub(in crate::machine) fn execute<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    op_index: usize,
    op: &ir::Op,
) -> Result<(), OpFailure> {
    match op {
        ir::Op::ReadStateProperty {
            guard,
            dst_value,
            dst_key_components,
            dst_is_null,
            table,
            field,
            query,
        } => {
            if !semantic(machine.guard_active(*guard))? {
                for dst in std::iter::once(dst_value)
                    .chain(dst_key_components.iter())
                    .chain(std::iter::once(dst_is_null))
                {
                    let ty = fatal(machine.entry.local_type(*dst))?;
                    fatal(machine.assign_local(*dst, semantic(machine.inactive_default(ty))?))?;
                }
            } else {
                let (query, result, _outputs) = fatal(execute_state_property_read(
                    machine,
                    *table,
                    *field,
                    query,
                    *dst_value,
                    dst_key_components,
                    *dst_is_null,
                ))?;
                machine
                    .effects
                    .record_property(op_index, *table, *field, query, result);
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
    dst_value: ir::LocalId,
    dst_key_components: &[ir::LocalId],
    dst_is_null: ir::LocalId,
) -> Result<
    (
        CommittedPropertyQuery,
        TypedCommittedPropertyQueryResult,
        Vec<TypedValue>,
    ),
    TabulaError,
> {
    let (query, result) = execute_state_property_read_committed(machine, table, field, query)?;
    let outputs = std::iter::once(result.value.clone())
        .chain(if let Some(ref key) = result.key {
            machine
                .exec
                .state_runtime
                .decode_committed_key(table, key)?
                .into_iter()
        } else {
            machine
                .exec
                .state_runtime
                .key_component_types(table)?
                .into_iter()
                .map(|ty| machine.exec.type_runtimes.zero_of(ty))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
        })
        .chain(std::iter::once(tabula_types::bool_typed(result.is_null)))
        .collect::<Vec<_>>();
    let dsts = std::iter::once(dst_value)
        .chain(dst_key_components.iter().copied())
        .chain(std::iter::once(dst_is_null))
        .collect::<Vec<_>>();
    for (dst, output) in dsts.iter().zip(outputs.iter().cloned()) {
        machine.assign_local(*dst, output)?;
    }
    Ok((query, result, outputs))
}

fn execute_state_property_read_committed<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    table: ir::TableId,
    field: ir::FieldId,
    query: &ir::StatePropertyQuery,
) -> Result<(CommittedPropertyQuery, TypedCommittedPropertyQueryResult), TabulaError> {
    let evaluated_key = match &query {
        ir::StatePropertyQuery::Successor { key } | ir::StatePropertyQuery::Predecessor { key } => {
            Some(
                machine
                    .exec
                    .state_runtime
                    .encode_committed_key(table, &machine.eval_tuple(key)?)?,
            )
        }
        _ => None,
    };
    let evaluated_range = match &query {
        ir::StatePropertyQuery::NonExistenceRange { lower, upper } => Some((
            machine
                .exec
                .state_runtime
                .encode_committed_key(table, &machine.eval_tuple(lower)?)?,
            machine
                .exec
                .state_runtime
                .encode_committed_key(table, &machine.eval_tuple(upper)?)?,
        )),
        _ => None,
    };
    let committed_query = build_committed_property_query(query, evaluated_key, evaluated_range)?;
    let field_type = machine.exec.state_runtime.column_type(table, field)?;
    let live_column_state =
        machine
            .overlay
            .committed_column_entries(table.into(), field.into(), field_type)?;
    let result = machine.exec.state_runtime.resolve_property(
        table,
        field,
        &committed_query,
        &live_column_state,
    )?;
    Ok((committed_query, result))
}

pub(crate) fn build_committed_property_query(
    query: &ir::StatePropertyQuery,
    evaluated_key: Option<tabula_core::CommittedKey>,
    evaluated_range: Option<(tabula_core::CommittedKey, tabula_core::CommittedKey)>,
) -> Result<CommittedPropertyQuery, TabulaError> {
    Ok(match query {
        ir::StatePropertyQuery::Minimum => CommittedPropertyQuery::Minimum,
        ir::StatePropertyQuery::Maximum => CommittedPropertyQuery::Maximum,
        ir::StatePropertyQuery::Successor { .. } => CommittedPropertyQuery::Successor {
            key: evaluated_key.ok_or_else(|| {
                TabulaError::InvalidIr("missing evaluated property-read successor key".into())
            })?,
        },
        ir::StatePropertyQuery::Predecessor { .. } => CommittedPropertyQuery::Predecessor {
            key: evaluated_key.ok_or_else(|| {
                TabulaError::InvalidIr("missing evaluated property-read predecessor key".into())
            })?,
        },
        ir::StatePropertyQuery::Aggregate { kind } => CommittedPropertyQuery::Aggregate {
            kind: match kind {
                ir::AggregateKind::Sum => PropertyAggregateKind::Sum,
                ir::AggregateKind::Count => PropertyAggregateKind::Count,
            },
        },
        ir::StatePropertyQuery::NonExistenceRange { .. } => {
            let (lower, upper) = evaluated_range.ok_or_else(|| {
                TabulaError::InvalidIr("missing evaluated property-read range bounds".into())
            })?;
            CommittedPropertyQuery::NonExistenceRange { lower, upper }
        }
    })
}
