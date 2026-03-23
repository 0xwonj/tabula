//! Reference interpreter for the Tabula IR instruction set.
//!
//! Walks `&[Instruction]` against an `Overlay`, maintaining a `Vec<TypedValue>` slot
//! environment. Records execution events and emitted events.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, StateView, StaticTableProvider};
use tabula_core::{
    CellKey, ColId, EmittedEvent, PrecompileEvent, PropertyReadResult, TableId, TableSchema, TypeId,
};
use tabula_ir::{Instruction, Slot};
use tabula_profile::ProfileCatalog;
use tabula_types::{
    ArithmeticOp, TypeRuntimeRegistry, TypedValue, bool_typed, bytes32_typed, typed_bool, u64_typed,
};

use crate::overlay::Overlay;
use crate::precompile::PrecompileRegistry;
use crate::property::{CommittedStateProvider, PropertyQueryRegistry};
use crate::resolve::{resolve_row_expr, resolve_value_expr};

/// Output of executing a single transaction's instruction body.
#[derive(Debug, Clone)]
pub struct TxExecutionOutput {
    /// Application events emitted during execution.
    pub emitted: Vec<EmittedEvent>,
    /// Precompile events recorded during execution.
    pub precompile_events: Vec<PrecompileEvent>,
    /// Property read results recorded during execution.
    pub property_reads: Vec<PropertyReadResult>,
}

/// Error produced by the interpreter, wrapping the underlying error with
/// the instruction index at which execution failed.
#[derive(Debug, Clone, thiserror::Error)]
#[error("instruction {instruction_index}: {error}")]
pub struct InterpreterError {
    /// The underlying execution error.
    #[source]
    pub error: TabulaError,
    /// Zero-based index of the instruction that failed.
    pub instruction_index: usize,
}

/// Read-only execution context: resources needed by the interpreter
/// that don't change across instructions.
pub struct ExecContext<'a> {
    /// Cryptographic hash function.
    pub hasher: &'a dyn Hasher,
    /// Static (read-only) table lookups.
    pub static_tables: &'a dyn StaticTableProvider,
    /// Runtime type registry used for typed execution.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Table schemas for column type resolution.
    pub schemas: &'a BTreeMap<TableId, TableSchema>,
    /// Canonical profile catalog for type resolution from sealed column profiles.
    pub profile_catalog: &'a ProfileCatalog,
    /// Optional precompile handlers for custom instructions.
    pub precompiles: Option<&'a PrecompileRegistry>,
    /// Optional committed state for PropertyRead instructions.
    pub committed_state: Option<&'a dyn CommittedStateProvider>,
    /// Property query registry for PropertyRead resolution.
    pub property_queries: &'a PropertyQueryRegistry,
}

/// Execute a transaction body against an overlay.
///
/// # Arguments
/// - `instructions`: the Tabula IR body of the transaction type
/// - `params`: concrete parameter values for this transaction
/// - `overlay`: the mutable overlay for state reads/writes
/// - `ctx`: read-only execution context (hasher, static tables, schemas)
pub fn execute<S: StateView>(
    tx_index: u32,
    instructions: &[Instruction],
    params: &[TypedValue],
    overlay: &mut Overlay<'_, S>,
    ctx: &ExecContext<'_>,
) -> Result<TxExecutionOutput, InterpreterError> {
    let mut slots: Vec<TypedValue> = Vec::new();
    let mut emitted: Vec<EmittedEvent> = Vec::new();
    let mut precompile_events: Vec<PrecompileEvent> = Vec::new();
    let mut property_reads: Vec<PropertyReadResult> = Vec::new();

    for (idx, instr) in instructions.iter().enumerate() {
        let step: Result<(), TabulaError> = (|| {
            match instr {
                Instruction::Read {
                    dst_val,
                    dst_is_null,
                    table,
                    col,
                    row,
                } => {
                    let row_key = resolve_row_expr(row, &slots, params, ctx.type_runtimes)?;
                    let key = CellKey {
                        table: *table,
                        col: *col,
                        row: row_key,
                    };
                    let col_type = lookup_col_type(ctx.schemas, ctx.profile_catalog, *table, *col)?;
                    let opt = overlay.read(&key, col_type)?;
                    match opt {
                        Some(v) => {
                            set_slot(&mut slots, *dst_val, v)?;
                            set_slot(&mut slots, *dst_is_null, bool_typed(false))?;
                        }
                        None => {
                            set_slot(&mut slots, *dst_val, ctx.type_runtimes.zero_of(col_type)?)?;
                            set_slot(&mut slots, *dst_is_null, bool_typed(true))?;
                        }
                    }
                }

                Instruction::Write {
                    table,
                    col,
                    row,
                    src_val,
                    src_is_null,
                } => {
                    let row_key = resolve_row_expr(row, &slots, params, ctx.type_runtimes)?;
                    let value = resolve_value_expr(src_val, &slots, params, ctx.type_runtimes)?;
                    let is_null =
                        resolve_value_expr(src_is_null, &slots, params, ctx.type_runtimes)?;
                    let key = CellKey {
                        table: *table,
                        col: *col,
                        row: row_key,
                    };
                    let col_type = lookup_col_type(ctx.schemas, ctx.profile_catalog, *table, *col)?;
                    let opt = if typed_bool(&is_null, ctx.type_runtimes)? {
                        None
                    } else {
                        Some(value)
                    };
                    overlay.write(&key, opt, col_type)?;
                }

                Instruction::Lookup {
                    dst,
                    static_table,
                    col,
                    row,
                } => {
                    let row_key = resolve_row_expr(row, &slots, params, ctx.type_runtimes)?;
                    let value = ctx
                        .type_runtimes
                        .decode_portable(&ctx.static_tables.lookup(
                            *static_table,
                            row_key,
                            *col,
                        )?)?;
                    set_slot(&mut slots, *dst, value)?;
                }

                Instruction::Arith { dst, op, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params, ctx.type_runtimes)?;
                    let r = resolve_value_expr(rhs, &slots, params, ctx.type_runtimes)?;
                    let runtime = ctx.type_runtimes.resolve(l.type_id())?;
                    let op = match op {
                        tabula_ir::ArithOp::Add => ArithmeticOp::Add,
                        tabula_ir::ArithOp::Sub => ArithmeticOp::Sub,
                        tabula_ir::ArithOp::Mul => ArithmeticOp::Mul,
                    };
                    set_slot(&mut slots, *dst, runtime.apply_arithmetic(op, &l, &r)?)?;
                }

                Instruction::DivMod {
                    dst_q,
                    dst_r,
                    lhs,
                    rhs,
                } => {
                    let l = resolve_value_expr(lhs, &slots, params, ctx.type_runtimes)?;
                    let r = resolve_value_expr(rhs, &slots, params, ctx.type_runtimes)?;
                    let runtime = ctx.type_runtimes.resolve(l.type_id())?;
                    let (q, rem) = runtime.divmod(&l, &r)?;
                    set_slot(&mut slots, *dst_q, q)?;
                    set_slot(&mut slots, *dst_r, rem)?;
                }

                Instruction::Cmp { dst, op, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params, ctx.type_runtimes)?;
                    let r = resolve_value_expr(rhs, &slots, params, ctx.type_runtimes)?;
                    let runtime = ctx.type_runtimes.resolve(l.type_id())?;
                    let result = match op {
                        tabula_ir::CmpOp::Eq => runtime.eq_value(&l, &r)?,
                        tabula_ir::CmpOp::Ne => !runtime.eq_value(&l, &r)?,
                        tabula_ir::CmpOp::Lt => runtime.cmp_value(&l, &r)? == Ordering::Less,
                        tabula_ir::CmpOp::Lte => runtime.cmp_value(&l, &r)? != Ordering::Greater,
                        tabula_ir::CmpOp::Gt => runtime.cmp_value(&l, &r)? == Ordering::Greater,
                        tabula_ir::CmpOp::Gte => runtime.cmp_value(&l, &r)? != Ordering::Less,
                    };
                    set_slot(&mut slots, *dst, bool_typed(result))?;
                }

                Instruction::Not { dst, src } => {
                    let v = resolve_value_expr(src, &slots, params, ctx.type_runtimes)?;
                    set_slot(
                        &mut slots,
                        *dst,
                        bool_typed(!typed_bool(&v, ctx.type_runtimes)?),
                    )?;
                }

                Instruction::And { dst, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params, ctx.type_runtimes)?;
                    let r = resolve_value_expr(rhs, &slots, params, ctx.type_runtimes)?;
                    set_slot(
                        &mut slots,
                        *dst,
                        bool_typed(
                            typed_bool(&l, ctx.type_runtimes)?
                                && typed_bool(&r, ctx.type_runtimes)?,
                        ),
                    )?;
                }

                Instruction::Or { dst, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params, ctx.type_runtimes)?;
                    let r = resolve_value_expr(rhs, &slots, params, ctx.type_runtimes)?;
                    set_slot(
                        &mut slots,
                        *dst,
                        bool_typed(
                            typed_bool(&l, ctx.type_runtimes)?
                                || typed_bool(&r, ctx.type_runtimes)?,
                        ),
                    )?;
                }

                Instruction::Assert { cond } => {
                    let v = resolve_value_expr(cond, &slots, params, ctx.type_runtimes)?;
                    if !typed_bool(&v, ctx.type_runtimes)? {
                        return Err(TabulaError::AssertionFailed(format!("{cond:?}")));
                    }
                }

                Instruction::Hash { dst, inputs } => {
                    let values = inputs
                        .iter()
                        .map(|input| resolve_value_expr(input, &slots, params, ctx.type_runtimes))
                        .collect::<Result<Vec<_>, _>>()?;
                    let encoded = values
                        .iter()
                        .map(|value| ctx.type_runtimes.encode_typed(value))
                        .collect::<Result<Vec<_>, _>>()?;
                    let digest = ctx.hasher.hash_ir(&encoded);
                    set_slot(&mut slots, *dst, bytes32_typed(digest))?;
                }

                Instruction::Select {
                    dst,
                    cond,
                    if_true,
                    if_false,
                } => {
                    let c = resolve_value_expr(cond, &slots, params, ctx.type_runtimes)?;
                    let t = resolve_value_expr(if_true, &slots, params, ctx.type_runtimes)?;
                    let f = resolve_value_expr(if_false, &slots, params, ctx.type_runtimes)?;
                    let selected = if typed_bool(&c, ctx.type_runtimes)? {
                        t
                    } else {
                        f
                    };
                    set_slot(&mut slots, *dst, selected)?;
                }

                Instruction::Emit { topic, data } => {
                    let mut values = Vec::new();
                    for d in data {
                        let value = resolve_value_expr(d, &slots, params, ctx.type_runtimes)?;
                        values.push(ctx.type_runtimes.encode_typed(&value)?);
                    }
                    emitted.push(EmittedEvent {
                        topic: topic.clone(),
                        data: values,
                    });
                }

                Instruction::Precompile {
                    id,
                    dst_slots,
                    inputs,
                } => {
                    let registry = ctx.precompiles.ok_or_else(|| {
                        TabulaError::InvalidIr(
                            "precompile instruction encountered but no PrecompileRegistry provided"
                                .into(),
                        )
                    })?;
                    let handler = registry.get(*id)?;
                    let signature = handler.signature();
                    let args = inputs
                        .iter()
                        .map(|inp| resolve_value_expr(inp, &slots, params, ctx.type_runtimes))
                        .collect::<Result<Vec<_>, _>>()?;
                    if args.len() != signature.inputs.len() {
                        return Err(TabulaError::InvalidIr(format!(
                            "precompile 0x{:04x} expects {} inputs but IR provided {}",
                            id.0,
                            signature.inputs.len(),
                            args.len(),
                        )));
                    }
                    for (idx, (arg, expected)) in args.iter().zip(&signature.inputs).enumerate() {
                        if arg.type_id() != expected.type_id {
                            return Err(TabulaError::InvalidIr(format!(
                                "precompile 0x{:04x} input {} expects type {} but got {}",
                                id.0,
                                idx,
                                expected.type_id.0,
                                arg.type_id().0,
                            )));
                        }
                    }
                    let results = handler.execute(&args)?;
                    if results.len() != signature.outputs.len() {
                        return Err(TabulaError::InvalidIr(format!(
                            "precompile 0x{:04x} returned {} values but signature declares {} outputs",
                            id.0,
                            results.len(),
                            signature.outputs.len(),
                        )));
                    }
                    if results.len() != dst_slots.len() {
                        return Err(TabulaError::InvalidIr(format!(
                            "precompile 0x{:04x} signature declares {} outputs but IR has {} dst_slots",
                            id.0,
                            results.len(),
                            dst_slots.len(),
                        )));
                    }
                    for (idx, (value, expected)) in
                        results.iter().zip(&signature.outputs).enumerate()
                    {
                        if value.type_id() != expected.type_id {
                            return Err(TabulaError::InvalidIr(format!(
                                "precompile 0x{:04x} output {} expects type {} but handler returned {}",
                                id.0,
                                idx,
                                expected.type_id.0,
                                value.type_id().0,
                            )));
                        }
                    }
                    precompile_events.push(PrecompileEvent {
                        tx_index: tx_index as usize,
                        instruction_index: idx,
                        precompile_id: id.0,
                        inputs: args
                            .iter()
                            .map(|value| ctx.type_runtimes.encode_typed(value))
                            .collect::<Result<Vec<_>, _>>()?,
                        outputs: results
                            .iter()
                            .map(|value| ctx.type_runtimes.encode_typed(value))
                            .collect::<Result<Vec<_>, _>>()?,
                    });
                    for (dst, val) in dst_slots.iter().zip(results.into_iter()) {
                        set_slot(&mut slots, *dst, val)?;
                    }
                }

                Instruction::PropertyRead {
                    dst_val,
                    dst_key,
                    dst_is_null,
                    table,
                    col,
                    query,
                } => {
                    let provider = ctx.committed_state.ok_or_else(|| {
                        TabulaError::InvalidIr(
                            "PropertyRead encountered but no CommittedStateProvider".into(),
                        )
                    })?;
                    let result = ctx
                        .property_queries
                        .resolve(*table, *col, query, provider)?;
                    property_reads.push(PropertyReadResult {
                        instruction_index: idx,
                        value: ctx.type_runtimes.encode_typed(&result.value)?,
                        key: result.key,
                        is_null: result.is_null,
                    });
                    set_slot(&mut slots, *dst_val, result.value)?;
                    set_slot(
                        &mut slots,
                        *dst_key,
                        u64_typed(result.key.map_or(0, |k| k.0)),
                    )?;
                    set_slot(&mut slots, *dst_is_null, bool_typed(result.is_null))?;
                }
            }
            Ok(())
        })();
        step.map_err(|error| InterpreterError {
            error,
            instruction_index: idx,
        })?;
    }

    Ok(TxExecutionOutput {
        emitted,
        precompile_events,
        property_reads,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn set_slot(slots: &mut Vec<TypedValue>, idx: Slot, value: TypedValue) -> Result<(), TabulaError> {
    let i = idx as usize;
    if i < slots.len() {
        slots[i] = value;
    } else if i == slots.len() {
        slots.push(value);
    } else {
        return Err(TabulaError::InvalidIr(format!(
            "slot gap: index {i}, len {}",
            slots.len()
        )));
    }
    Ok(())
}

fn lookup_col_type(
    schemas: &BTreeMap<TableId, TableSchema>,
    profile_catalog: &ProfileCatalog,
    table: TableId,
    col: ColId,
) -> Result<TypeId, TabulaError> {
    let schema = schemas
        .get(&table)
        .ok_or(TabulaError::TableNotFound(table))?;
    schema
        .columns
        .iter()
        .find(|c| c.id == col)
        .ok_or_else(|| {
            TabulaError::InvalidIr(format!(
                "column {col:?} not found in schema for table {table:?}"
            ))
        })
        .and_then(|column| {
            let resolved = profile_catalog
                .resolve_column_profile(column.column_profile_id)
                .map_err(|err| {
                    TabulaError::InvalidIr(format!(
                        "column profile {} for table {:?} col {:?} is invalid: {err}",
                        column.column_profile_id.0, table, col
                    ))
                })?;
            Ok(resolved.type_descriptor.type_id)
        })
}
