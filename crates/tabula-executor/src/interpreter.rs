//! Reference interpreter for the DB-IR instruction set.
//!
//! Walks `&[Instruction]` against an `Overlay`, maintaining a `Vec<Value>` slot
//! environment. Records execution events and emitted events.

use tabula_core::error::TabulaError;
use tabula_core::event::{EmittedEvent, LogicalTime};
use tabula_core::ir::{Instruction, Slot};
use tabula_core::traits::{Hasher, StateSnapshot, StaticTableProvider};
use tabula_core::types::{CellKey, Value};

use crate::overlay::Overlay;
use crate::resolve::{evaluate_predicate, resolve_row_expr, resolve_value_expr};

/// Output of executing a single transaction's instruction body.
#[derive(Debug, Clone)]
pub struct TxExecutionOutput {
    /// Application events emitted during execution.
    pub emitted: Vec<EmittedEvent>,
}

/// Error produced by the interpreter, wrapping the underlying error with
/// the instruction index at which execution failed.
#[derive(Debug, Clone)]
pub struct InterpreterError {
    /// The underlying execution error.
    pub error: TabulaError,
    /// Zero-based index of the instruction that failed.
    pub instruction_index: usize,
}

/// Execute a transaction body against an overlay.
///
/// # Arguments
/// - `instructions`: the DB-IR body of the transaction type
/// - `params`: concrete parameter values for this transaction
/// - `overlay`: the mutable overlay for state reads/writes
/// - `hasher`: cryptographic hash function
/// - `static_tables`: provider for static/fixed table lookups
/// - `_time`: base logical time (currently tracked by overlay internally)
pub fn execute<S: StateSnapshot>(
    instructions: &[Instruction],
    params: &[Value],
    overlay: &mut Overlay<'_, S>,
    hasher: &dyn Hasher,
    static_tables: &dyn StaticTableProvider,
    _time: LogicalTime,
) -> Result<TxExecutionOutput, InterpreterError> {
    let mut slots: Vec<Value> = Vec::new();
    let mut emitted: Vec<EmittedEvent> = Vec::new();

    for (idx, instr) in instructions.iter().enumerate() {
        let step: Result<(), TabulaError> = (|| {
            match instr {
                Instruction::Read {
                    dst,
                    table,
                    row,
                    col,
                } => {
                    let row_key = resolve_row_expr(row, &slots, params)?;
                    let key = CellKey {
                        table: *table,
                        col: *col,
                        row: row_key,
                    };
                    let value = overlay.read(&key)?;
                    set_slot(&mut slots, *dst, value)?;
                }

                Instruction::Write {
                    table,
                    row,
                    col,
                    src,
                } => {
                    let row_key = resolve_row_expr(row, &slots, params)?;
                    let value = resolve_value_expr(src, &slots, params)?;
                    let key = CellKey {
                        table: *table,
                        col: *col,
                        row: row_key,
                    };
                    overlay.write(&key, value);
                }

                Instruction::Lookup {
                    dst,
                    static_table,
                    col,
                    row,
                } => {
                    let row_key = resolve_row_expr(row, &slots, params)?;
                    let value = static_tables.lookup(*static_table, row_key, *col)?;
                    set_slot(&mut slots, *dst, value)?;
                }

                Instruction::Add { dst, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params)?;
                    let r = resolve_value_expr(rhs, &slots, params)?;
                    set_slot(&mut slots, *dst, l.checked_add(&r)?)?;
                }

                Instruction::Sub { dst, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params)?;
                    let r = resolve_value_expr(rhs, &slots, params)?;
                    set_slot(&mut slots, *dst, l.checked_sub(&r)?)?;
                }

                Instruction::Mul { dst, lhs, rhs } => {
                    let l = resolve_value_expr(lhs, &slots, params)?;
                    let r = resolve_value_expr(rhs, &slots, params)?;
                    set_slot(&mut slots, *dst, l.checked_mul(&r)?)?;
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

                Instruction::Assert { predicate } => {
                    let result = evaluate_predicate(predicate, &slots, params)?;
                    if !result {
                        return Err(TabulaError::AssertionFailed(format!("{predicate:?}")));
                    }
                }

                Instruction::Hash { dst, inputs } => {
                    let values: Vec<Value> = inputs
                        .iter()
                        .map(|input| resolve_value_expr(input, &slots, params))
                        .collect::<Result<_, _>>()?;
                    let digest = hasher.hash_ir(&values);
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
            }
            Ok(())
        })();
        step.map_err(|error| InterpreterError {
            error,
            instruction_index: idx,
        })?;
    }

    Ok(TxExecutionOutput { emitted })
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
        // Fill gaps with Null
        slots.resize(i, Value::Null);
        slots.push(value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_core::ir::{Predicate, RowExpr, ValueExpr};
    use tabula_core::types::{ColId, RowKey, TableId};

    use crate::test_fixtures::*;

    fn make_snapshot(entries: Vec<(CellKey, Value)>) -> TestSnapshot {
        TestSnapshot(entries.into_iter().collect())
    }

    // --- Tests ---

    #[test]
    fn test_read_populates_slot() {
        let snap = make_snapshot(vec![(cell(1, 0, 0), Value::U64(100))]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Read {
            dst: 0,
            table: TableId(1),
            row: RowExpr::Literal(RowKey(0)),
            col: ColId(0),
        }];
        let out = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
        assert!(out.emitted.is_empty());
    }

    #[test]
    fn test_write_updates_overlay() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(RowKey(0)),
                col: ColId(0),
                src: ValueExpr::Literal(Value::U64(42)),
            },
            Instruction::Read {
                dst: 0,
                table: TableId(1),
                row: RowExpr::Literal(RowKey(0)),
                col: ColId(0),
            },
        ];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
        let result = ov.into_result();
        assert_eq!(
            result.write_set_final,
            vec![(cell(1, 0, 0), Value::U64(42))]
        );
    }

    #[test]
    fn test_add_correct() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Add {
            dst: 0,
            lhs: ValueExpr::Literal(Value::U64(10)),
            rhs: ValueExpr::Literal(Value::U64(20)),
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_add_overflow() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Add {
            dst: 0,
            lhs: ValueExpr::Literal(Value::U64(u64::MAX)),
            rhs: ValueExpr::Literal(Value::U64(1)),
        }];
        let err = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap_err();
        assert_eq!(err.error, TabulaError::ArithmeticOverflow);
    }

    #[test]
    fn test_sub_correct() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Sub {
            dst: 0,
            lhs: ValueExpr::Literal(Value::U64(30)),
            rhs: ValueExpr::Literal(Value::U64(10)),
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_mul_correct() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Mul {
            dst: 0,
            lhs: ValueExpr::Literal(Value::U64(5)),
            rhs: ValueExpr::Literal(Value::U64(7)),
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_divmod_correct() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::DivMod {
            dst_q: 0,
            dst_r: 1,
            lhs: ValueExpr::Literal(Value::U64(17)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_divmod_by_zero() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::DivMod {
            dst_q: 0,
            dst_r: 1,
            lhs: ValueExpr::Literal(Value::U64(10)),
            rhs: ValueExpr::Literal(Value::U64(0)),
        }];
        let err = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap_err();
        assert_eq!(err.error, TabulaError::DivisionByZero);
    }

    #[test]
    fn test_assert_passing() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Assert {
            predicate: Predicate::Eq(
                ValueExpr::Literal(Value::U64(1)),
                ValueExpr::Literal(Value::U64(1)),
            ),
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_assert_failing() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Assert {
            predicate: Predicate::Eq(
                ValueExpr::Literal(Value::U64(1)),
                ValueExpr::Literal(Value::U64(2)),
            ),
        }];
        let err = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap_err();
        assert!(matches!(err.error, TabulaError::AssertionFailed(_)));
    }

    #[test]
    fn test_predicate_lt() {
        assert!(
            evaluate_predicate(
                &Predicate::Lt(
                    ValueExpr::Literal(Value::U64(1)),
                    ValueExpr::Literal(Value::U64(2))
                ),
                &[],
                &[],
            )
            .unwrap()
        );
    }

    #[test]
    fn test_predicate_gte() {
        assert!(
            evaluate_predicate(
                &Predicate::Gte(
                    ValueExpr::Literal(Value::U64(5)),
                    ValueExpr::Literal(Value::U64(5))
                ),
                &[],
                &[],
            )
            .unwrap()
        );
    }

    #[test]
    fn test_predicate_not_null() {
        assert!(
            evaluate_predicate(
                &Predicate::NotNull(ValueExpr::Literal(Value::U64(1))),
                &[],
                &[],
            )
            .unwrap()
        );
        assert!(
            !evaluate_predicate(
                &Predicate::NotNull(ValueExpr::Literal(Value::Null)),
                &[],
                &[],
            )
            .unwrap()
        );
    }

    #[test]
    fn test_predicate_and_or_not() {
        let t = Predicate::Eq(
            ValueExpr::Literal(Value::U64(1)),
            ValueExpr::Literal(Value::U64(1)),
        );
        let f = Predicate::Eq(
            ValueExpr::Literal(Value::U64(1)),
            ValueExpr::Literal(Value::U64(2)),
        );

        assert!(
            evaluate_predicate(
                &Predicate::And(Box::new(t.clone()), Box::new(t.clone())),
                &[],
                &[]
            )
            .unwrap()
        );
        assert!(
            !evaluate_predicate(
                &Predicate::And(Box::new(t.clone()), Box::new(f.clone())),
                &[],
                &[]
            )
            .unwrap()
        );
        assert!(
            evaluate_predicate(
                &Predicate::Or(Box::new(t.clone()), Box::new(f.clone())),
                &[],
                &[]
            )
            .unwrap()
        );
        assert!(evaluate_predicate(&Predicate::Not(Box::new(f.clone())), &[], &[]).unwrap());
    }

    #[test]
    fn test_hash_produces_bytes32() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Hash {
            dst: 0,
            inputs: vec![ValueExpr::Literal(Value::U64(42))],
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_emit_captures_event() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Emit {
            topic: b"transfer".to_vec(),
            data: vec![ValueExpr::Literal(Value::U64(100))],
        }];
        let out = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
        assert_eq!(out.emitted.len(), 1);
        assert_eq!(out.emitted[0].topic, b"transfer");
    }

    #[test]
    fn test_lookup_delegates() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Lookup {
            dst: 0,
            static_table: TableId(99),
            col: ColId(0),
            row: RowExpr::Literal(RowKey(7)),
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_select_true_branch() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Select {
            dst: 0,
            cond: ValueExpr::Literal(Value::Bool(true)),
            if_true: ValueExpr::Literal(Value::U64(10)),
            if_false: ValueExpr::Literal(Value::U64(20)),
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_select_false_branch() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Select {
            dst: 0,
            cond: ValueExpr::Literal(Value::Bool(false)),
            if_true: ValueExpr::Literal(Value::U64(10)),
            if_false: ValueExpr::Literal(Value::U64(20)),
        }];
        execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap();
    }

    #[test]
    fn test_select_non_bool_cond_fails() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Select {
            dst: 0,
            cond: ValueExpr::Literal(Value::U64(1)),
            if_true: ValueExpr::Literal(Value::U64(10)),
            if_false: ValueExpr::Literal(Value::U64(20)),
        }];
        let err = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap_err();
        assert!(matches!(err.error, TabulaError::TypeMismatch { .. }));
    }

    #[test]
    fn test_null_arithmetic() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Add {
            dst: 0,
            lhs: ValueExpr::Literal(Value::Null),
            rhs: ValueExpr::Literal(Value::U64(1)),
        }];
        let err = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap_err();
        assert_eq!(err.error, TabulaError::NullValue);
    }

    #[test]
    fn test_slot_out_of_bounds() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        // Try to use slot 5 as input without ever setting it
        let instrs = vec![Instruction::Add {
            dst: 0,
            lhs: ValueExpr::Slot(5),
            rhs: ValueExpr::Literal(Value::U64(1)),
        }];
        let err = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap_err();
        assert!(matches!(err.error, TabulaError::SlotOutOfBounds { .. }));
    }

    #[test]
    fn test_param_out_of_bounds() {
        let snap = make_snapshot(vec![]);
        let mut ov = Overlay::new(&snap);
        let instrs = vec![Instruction::Add {
            dst: 0,
            lhs: ValueExpr::Param(10),
            rhs: ValueExpr::Literal(Value::U64(1)),
        }];
        let err = execute(&instrs, &[], &mut ov, &XorHasher, &TestStaticTables, 0).unwrap_err();
        assert!(matches!(err.error, TabulaError::ParamOutOfBounds { .. }));
    }
}
