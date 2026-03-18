//! Reference interpreter for the Tabula IR instruction set.
//!
//! Walks `&[Instruction]` against an `Overlay`, maintaining a `Vec<Value>` slot
//! environment. Records execution events and emitted events.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, StateSnapshot, StaticTableProvider};
use tabula_core::{
    CellKey, ColId, EmittedEvent, PrecompileIo, PropertyReadResult, TableId, TableSchema, Value,
    ValueType, zero_value,
};
use tabula_ir::{Instruction, Slot};

use crate::overlay::Overlay;
use crate::precompile::PrecompileRegistry;
use crate::property::{CommittedStateProvider, PropertyQueryRegistry};
use crate::resolve::{resolve_row_expr, resolve_value_expr};

/// Output of executing a single transaction's instruction body.
#[derive(Debug, Clone)]
pub struct TxExecutionOutput {
    /// Application events emitted during execution.
    pub emitted: Vec<EmittedEvent>,
    /// Precompile I/O pairs recorded during execution.
    pub precompile_ios: Vec<PrecompileIo>,
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
    /// Table schemas for column type resolution.
    pub schemas: &'a BTreeMap<TableId, TableSchema>,
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
pub fn execute<S: StateSnapshot>(
    instructions: &[Instruction],
    params: &[Value],
    overlay: &mut Overlay<'_, S>,
    ctx: &ExecContext<'_>,
) -> Result<TxExecutionOutput, InterpreterError> {
    let mut slots: Vec<Value> = Vec::new();
    let mut emitted: Vec<EmittedEvent> = Vec::new();
    let mut precompile_ios: Vec<PrecompileIo> = Vec::new();
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
                    let row_key = resolve_row_expr(row, &slots, params)?;
                    let key = CellKey {
                        table: *table,
                        col: *col,
                        row: row_key,
                    };
                    let col_type = lookup_col_type(ctx.schemas, *table, *col)?;
                    let opt = overlay.read(&key, col_type)?;
                    match opt {
                        Some(v) => {
                            set_slot(&mut slots, *dst_val, v)?;
                            set_slot(&mut slots, *dst_is_null, Value::Bool(false))?;
                        }
                        None => {
                            set_slot(&mut slots, *dst_val, zero_value(col_type))?;
                            set_slot(&mut slots, *dst_is_null, Value::Bool(true))?;
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
                    let row_key = resolve_row_expr(row, &slots, params)?;
                    let value = resolve_value_expr(src_val, &slots, params)?;
                    let is_null = resolve_value_expr(src_is_null, &slots, params)?;
                    let key = CellKey {
                        table: *table,
                        col: *col,
                        row: row_key,
                    };
                    let col_type = lookup_col_type(ctx.schemas, *table, *col)?;
                    let opt = match is_null {
                        Value::Bool(true) => None,
                        Value::Bool(false) => Some(value),
                        _ => {
                            return Err(TabulaError::TypeMismatch {
                                expected: "Bool",
                                actual: is_null.type_name(),
                            });
                        }
                    };
                    overlay.write(&key, opt, col_type);
                }

                Instruction::Lookup {
                    dst,
                    static_table,
                    col,
                    row,
                } => {
                    let row_key = resolve_row_expr(row, &slots, params)?;
                    let value = ctx.static_tables.lookup(*static_table, row_key, *col)?;
                    set_slot(&mut slots, *dst, value)?;
                }

                Instruction::Arith { dst, op, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params)?;
                    let r = resolve_value_expr(rhs, &slots, params)?;
                    set_slot(&mut slots, *dst, op.apply(&l, &r)?)?;
                }

                Instruction::DivMod {
                    dst_q,
                    dst_r,
                    lhs,
                    rhs,
                } => {
                    let l = resolve_value_expr(lhs, &slots, params)?;
                    let r = resolve_value_expr(rhs, &slots, params)?;
                    let (q, rem) = l.checked_divmod(&r)?;
                    set_slot(&mut slots, *dst_q, q)?;
                    set_slot(&mut slots, *dst_r, rem)?;
                }

                Instruction::Cmp { dst, op, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params)?;
                    let r = resolve_value_expr(rhs, &slots, params)?;
                    set_slot(&mut slots, *dst, op.apply(&l, &r)?)?;
                }

                Instruction::Not { dst, src } => {
                    let v = resolve_value_expr(src, &slots, params)?;
                    match v {
                        Value::Bool(b) => set_slot(&mut slots, *dst, Value::Bool(!b))?,
                        _ => {
                            return Err(TabulaError::TypeMismatch {
                                expected: "Bool",
                                actual: v.type_name(),
                            });
                        }
                    }
                }

                Instruction::And { dst, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params)?;
                    let r = resolve_value_expr(rhs, &slots, params)?;
                    match (&l, &r) {
                        (Value::Bool(a), Value::Bool(b)) => {
                            set_slot(&mut slots, *dst, Value::Bool(*a && *b))?;
                        }
                        (Value::Bool(_), _) => {
                            return Err(TabulaError::TypeMismatch {
                                expected: "Bool",
                                actual: r.type_name(),
                            });
                        }
                        _ => {
                            return Err(TabulaError::TypeMismatch {
                                expected: "Bool",
                                actual: l.type_name(),
                            });
                        }
                    }
                }

                Instruction::Or { dst, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params)?;
                    let r = resolve_value_expr(rhs, &slots, params)?;
                    match (&l, &r) {
                        (Value::Bool(a), Value::Bool(b)) => {
                            set_slot(&mut slots, *dst, Value::Bool(*a || *b))?;
                        }
                        (Value::Bool(_), _) => {
                            return Err(TabulaError::TypeMismatch {
                                expected: "Bool",
                                actual: r.type_name(),
                            });
                        }
                        _ => {
                            return Err(TabulaError::TypeMismatch {
                                expected: "Bool",
                                actual: l.type_name(),
                            });
                        }
                    }
                }

                Instruction::Assert { cond } => {
                    let v = resolve_value_expr(cond, &slots, params)?;
                    match v {
                        Value::Bool(true) => {}
                        Value::Bool(false) => {
                            return Err(TabulaError::AssertionFailed(format!("{cond:?}")));
                        }
                        _ => {
                            return Err(TabulaError::TypeMismatch {
                                expected: "Bool",
                                actual: v.type_name(),
                            });
                        }
                    }
                }

                Instruction::Hash { dst, inputs } => {
                    let values: Vec<Value> = inputs
                        .iter()
                        .map(|input| resolve_value_expr(input, &slots, params))
                        .collect::<Result<_, _>>()?;
                    let digest = ctx.hasher.hash_ir(&values);
                    set_slot(&mut slots, *dst, Value::Bytes32(digest))?;
                }

                Instruction::Select {
                    dst,
                    cond,
                    if_true,
                    if_false,
                } => {
                    let c = resolve_value_expr(cond, &slots, params)?;
                    let t = resolve_value_expr(if_true, &slots, params)?;
                    let f = resolve_value_expr(if_false, &slots, params)?;
                    let selected = match c {
                        Value::Bool(true) => t,
                        Value::Bool(false) => f,
                        _ => {
                            return Err(TabulaError::TypeMismatch {
                                expected: "Bool",
                                actual: c.type_name(),
                            });
                        }
                    };
                    set_slot(&mut slots, *dst, selected)?;
                }

                Instruction::Emit { topic, data } => {
                    let mut values = Vec::new();
                    for d in data {
                        values.push(resolve_value_expr(d, &slots, params)?);
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
                    let args: Vec<Value> = inputs
                        .iter()
                        .map(|inp| resolve_value_expr(inp, &slots, params))
                        .collect::<Result<_, _>>()?;
                    let results = handler.execute(&args)?;
                    if results.len() != dst_slots.len() {
                        return Err(TabulaError::InvalidIr(format!(
                            "precompile 0x{:04x} returned {} values but {} dst_slots declared",
                            id.0,
                            results.len(),
                            dst_slots.len(),
                        )));
                    }
                    precompile_ios.push(PrecompileIo {
                        instruction_index: idx,
                        precompile_id: id.0,
                        inputs: args,
                        outputs: results.clone(),
                    });
                    for (dst, val) in dst_slots.iter().zip(results) {
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
                        value: result.value,
                        key: result.key,
                        is_null: result.is_null,
                    });
                    set_slot(&mut slots, *dst_val, result.value)?;
                    set_slot(
                        &mut slots,
                        *dst_key,
                        result.key.map_or(Value::U64(0), |k| Value::U64(k.0)),
                    )?;
                    set_slot(&mut slots, *dst_is_null, Value::Bool(result.is_null))?;
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
        precompile_ios,
        property_reads,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn set_slot(slots: &mut Vec<Value>, idx: Slot, value: Value) -> Result<(), TabulaError> {
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
    table: TableId,
    col: ColId,
) -> Result<ValueType, TabulaError> {
    let schema = schemas
        .get(&table)
        .ok_or(TabulaError::TableNotFound(table))?;
    schema
        .columns
        .iter()
        .find(|c| c.id == col)
        .map(|c| c.value_type)
        .ok_or_else(|| {
            TabulaError::InvalidIr(format!(
                "column {col:?} not found in schema for table {table:?}"
            ))
        })
}
