//! Batch executor: iterates transactions, orchestrates interpretation
//! with per-tx rollback on failure.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, NoncePolicy, SigVerifier, StateSnapshot, StaticTableProvider};
use tabula_core::{Batch, EmittedEvent, ExecutionResult, TxOutcome};

use tabula_ir::Program;

use crate::interpreter;
use crate::overlay::Overlay;

/// Pluggable trait implementations needed by the batch executor.
pub struct BatchEnv<'a> {
    /// Cryptographic hash function.
    pub hasher: &'a dyn Hasher,
    /// Signature verification.
    pub sig_verifier: &'a dyn SigVerifier,
    /// Nonce validation and advancement.
    pub nonce_policy: &'a dyn NoncePolicy,
    /// Static (read-only) table lookups.
    pub static_tables: &'a dyn StaticTableProvider,
}

/// Execute a batch of transactions against a state snapshot.
///
/// Returns an `ExecutionResult` containing the read set, write set, events,
/// emitted events, and per-tx outcomes.
pub fn execute_batch<S: StateSnapshot>(
    batch: &Batch,
    program: &Program,
    snapshot: &S,
    env: &BatchEnv<'_>,
    initial_nonces: &BTreeMap<[u8; 32], u64>,
) -> Result<ExecutionResult, TabulaError> {
    let mut overlay = Overlay::new(snapshot);
    let mut tx_outcomes = Vec::new();
    let mut all_emitted: Vec<EmittedEvent> = Vec::new();
    let mut nonces: BTreeMap<[u8; 32], u64> = initial_nonces.clone();

    for (tx_idx, tx) in batch.transactions.iter().enumerate() {
        overlay.set_tx_index(tx_idx as u32);
        // Resolve tx type
        let tx_def = match program.resolve(tx.tx_type) {
            Ok(def) => def,
            Err(e) => {
                tx_outcomes.push(TxOutcome::Failed {
                    reason: e.to_string(),
                    partial_events: vec![],
                    failed_instruction: None,
                });
                continue;
            }
        };

        // Validate param count and types against schema
        if tx.params.len() != tx_def.param_schema.len() {
            tx_outcomes.push(TxOutcome::Failed {
                reason: TabulaError::ParamSchemaMismatch(format!(
                    "expected {} params, got {}",
                    tx_def.param_schema.len(),
                    tx.params.len()
                ))
                .to_string(),
                partial_events: vec![],
                failed_instruction: None,
            });
            continue;
        }
        {
            let mut mismatch = None;
            for (i, (param, schema)) in tx.params.iter().zip(tx_def.param_schema.iter()).enumerate()
            {
                if !param.matches_type(schema.value_type) {
                    mismatch = Some(TabulaError::ParamSchemaMismatch(format!(
                        "param {i}: expected {:?}, got {}",
                        schema.value_type,
                        param.type_name()
                    )));
                    break;
                }
            }
            if let Some(e) = mismatch {
                tx_outcomes.push(TxOutcome::Failed {
                    reason: e.to_string(),
                    partial_events: vec![],
                    failed_instruction: None,
                });
                continue;
            }
        }

        // Verify signature (message excludes the signature field itself)
        let msg = tx.signable_bytes()?;
        if let Err(e) = env.sig_verifier.verify(&tx.sender, &msg, &tx.signature) {
            tx_outcomes.push(TxOutcome::Failed {
                reason: e.to_string(),
                partial_events: vec![],
                failed_instruction: None,
            });
            continue;
        }

        // Verify nonce
        let current_nonce = *nonces.get(&tx.sender).unwrap_or(&0);
        if let Err(e) = env
            .nonce_policy
            .validate(&tx.sender, tx.nonce, current_nonce)
        {
            tx_outcomes.push(TxOutcome::Failed {
                reason: e.to_string(),
                partial_events: vec![],
                failed_instruction: None,
            });
            continue;
        }

        // Checkpoint before execution
        let events_before = overlay.events_len();
        overlay.checkpoint();

        // Execute
        let time = overlay.time();
        match interpreter::execute(
            &tx_def.body,
            &tx.params,
            &mut overlay,
            env.hasher,
            env.static_tables,
            program.schemas(),
            time,
        ) {
            Ok(output) => {
                overlay.discard_checkpoint();
                let next = env.nonce_policy.next_nonce(&tx.sender, current_nonce);
                nonces.insert(tx.sender, next);
                all_emitted.extend(output.emitted);
                tx_outcomes.push(TxOutcome::Success);
            }
            Err(interp_err) => {
                let partial_events = overlay.events_since(events_before);
                overlay.rollback();
                tx_outcomes.push(TxOutcome::Failed {
                    reason: interp_err.error.to_string(),
                    partial_events,
                    failed_instruction: Some(interp_err.instruction_index),
                });
            }
        }
    }

    let result = overlay.into_result();
    Ok(ExecutionResult {
        read_set_old: result.read_set_old,
        write_set_final: result.write_set_final,
        events: result.events,
        emitted: all_emitted,
        tx_outcomes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tabula_core::{ColId, ColumnDef, RowKey, TableId, TableSchema, TxTypeId, Value, ValueType};
    use tabula_ir::{ArithOp, CmpOp, Instruction, ParamDef, RowExpr, TxTypeDef, ValueExpr};

    use crate::test_fixtures::*;

    /// Table schema used by all batch test tx types: TableId(1) with one U64 column.
    fn test_schema() -> TableSchema {
        TableSchema {
            id: TableId(1),
            name: "test".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "val".into(),
                value_type: ValueType::U64,
            }],
        }
    }

    /// A simple "write value to cell" tx type.
    fn write_tx_def() -> TxTypeDef {
        TxTypeDef {
            id: TxTypeId(1),
            name: "write_cell".into(),
            param_schema: vec![
                ParamDef {
                    name: "row".into(),
                    value_type: ValueType::U64,
                },
                ParamDef {
                    name: "value".into(),
                    value_type: ValueType::U64,
                },
            ],
            body: vec![Instruction::Write {
                table: TableId(1),
                row: RowExpr::Param(0),
                col: ColId(0),
                src_val: ValueExpr::Param(1),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            }],
        }
    }

    /// NF-compliant transfer: reads row 0 and row 1 of (table 1, col 0),
    /// transfers `amount` (param 0) from row 0 to row 1.
    fn transfer_tx_def() -> TxTypeDef {
        TxTypeDef {
            id: TxTypeId(2),
            name: "transfer".into(),
            param_schema: vec![ParamDef {
                name: "amount".into(),
                value_type: ValueType::U64,
            }],
            body: vec![
                Instruction::Read {
                    dst_val: 0,
                    dst_is_null: 1,
                    table: TableId(1),
                    row: RowExpr::Literal(RowKey(0)),
                    col: ColId(0),
                },
                Instruction::Read {
                    dst_val: 2,
                    dst_is_null: 3,
                    table: TableId(1),
                    row: RowExpr::Literal(RowKey(1)),
                    col: ColId(0),
                },
                Instruction::Cmp {
                    dst: 4,
                    op: CmpOp::Gte,
                    lhs: ValueExpr::Slot(0),
                    rhs: ValueExpr::Param(0),
                },
                Instruction::Assert {
                    cond: ValueExpr::Slot(4),
                },
                Instruction::Arith {
                    dst: 5,
                    op: ArithOp::Sub,
                    lhs: ValueExpr::Slot(0),
                    rhs: ValueExpr::Param(0),
                },
                Instruction::Arith {
                    dst: 6,
                    op: ArithOp::Add,
                    lhs: ValueExpr::Slot(2),
                    rhs: ValueExpr::Param(0),
                },
                Instruction::Write {
                    table: TableId(1),
                    row: RowExpr::Literal(RowKey(0)),
                    col: ColId(0),
                    src_val: ValueExpr::Slot(5),
                    src_is_null: ValueExpr::Literal(Value::Bool(false)),
                },
                Instruction::Write {
                    table: TableId(1),
                    row: RowExpr::Literal(RowKey(1)),
                    col: ColId(0),
                    src_val: ValueExpr::Slot(6),
                    src_is_null: ValueExpr::Literal(Value::Bool(false)),
                },
            ],
        }
    }

    #[test]
    fn test_single_successful_tx() {
        let snap = TestSnapshot(BTreeMap::new());
        let sender = [1u8; 32];
        let batch = Batch {
            transactions: vec![make_tx(1, vec![Value::U64(0), Value::U64(42)], sender, 0)],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
        assert_eq!(result.tx_outcomes.len(), 1);
        assert_eq!(result.tx_outcomes[0], TxOutcome::Success);
        assert_eq!(
            result.write_set_final,
            vec![(cell(1, 0, 0), Some(Value::U64(42)))]
        );
    }

    #[test]
    fn test_inter_tx_read_your_writes() {
        // tx1 writes 100 to cell(1,0,0), tx2 reads it
        let snap = TestSnapshot(BTreeMap::new());
        let sender = [1u8; 32];
        let batch = Batch {
            transactions: vec![
                make_tx(1, vec![Value::U64(0), Value::U64(100)], sender, 0),
                // tx2: read cell(1,0,0) into slot 0, then write slot 0 to cell(1,1,0)
                make_tx(3, vec![], sender, 1),
            ],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();
        prog.register(TxTypeDef {
            id: TxTypeId(3),
            name: "copy_cell".into(),
            param_schema: vec![],
            body: vec![
                Instruction::Read {
                    dst_val: 0,
                    dst_is_null: 1,
                    table: TableId(1),
                    row: RowExpr::Literal(RowKey(0)),
                    col: ColId(0),
                },
                Instruction::Write {
                    table: TableId(1),
                    row: RowExpr::Literal(RowKey(1)),
                    col: ColId(0),
                    src_val: ValueExpr::Slot(0),
                    src_is_null: ValueExpr::Literal(Value::Bool(false)),
                },
            ],
        })
        .unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
        assert_eq!(
            result.tx_outcomes,
            vec![TxOutcome::Success, TxOutcome::Success]
        );
        // cell(1,1,0) should have the value written by tx1
        assert!(
            result
                .write_set_final
                .contains(&(cell(1, 1, 0), Some(Value::U64(100))))
        );
    }

    #[test]
    fn test_failed_tx_rollback() {
        // Initial state: cell(1,0,0) = 50
        let mut data = BTreeMap::new();
        data.insert(cell(1, 0, 0), Value::U64(50));
        data.insert(cell(1, 1, 0), Value::U64(50));
        let snap = TestSnapshot(data);
        let sender = [1u8; 32];

        let batch = Batch {
            transactions: vec![
                // tx1: transfer 30 from row 0 to row 1 (should succeed)
                make_tx(2, vec![Value::U64(30)], sender, 0),
                // tx2: transfer 100 from row 0 to row 1 (should fail — only 20 left)
                // nonce=1 is valid, but tx fails at assertion so nonce is NOT incremented
                make_tx(2, vec![Value::U64(100)], sender, 1),
                // tx3: transfer 10 from row 0 to row 1 (should succeed — 20 left from tx1)
                // nonce is still 1 because tx2 failed
                make_tx(2, vec![Value::U64(10)], sender, 1),
            ],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(transfer_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
        assert_eq!(result.tx_outcomes[0], TxOutcome::Success);
        assert!(matches!(result.tx_outcomes[1], TxOutcome::Failed { .. }));
        assert_eq!(result.tx_outcomes[2], TxOutcome::Success);

        // Final: row 0 = 50 - 30 - 10 = 10, row 1 = 50 + 30 + 10 = 90
        assert!(
            result
                .write_set_final
                .contains(&(cell(1, 0, 0), Some(Value::U64(10))))
        );
        assert!(
            result
                .write_set_final
                .contains(&(cell(1, 1, 0), Some(Value::U64(90))))
        );
    }

    #[test]
    fn test_invalid_signature() {
        let snap = TestSnapshot(BTreeMap::new());
        let batch = Batch {
            transactions: vec![make_tx(1, vec![Value::U64(0), Value::U64(1)], [1u8; 32], 0)],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();

        let env = BatchEnv {
            sig_verifier: &AlwaysInvalidSig,
            ..test_env()
        };
        let result = execute_batch(&batch, &prog, &snap, &env, &BTreeMap::new()).unwrap();
        assert!(matches!(result.tx_outcomes[0], TxOutcome::Failed { .. }));
    }

    #[test]
    fn test_invalid_nonce() {
        let snap = TestSnapshot(BTreeMap::new());
        let batch = Batch {
            transactions: vec![make_tx(
                1,
                vec![Value::U64(0), Value::U64(1)],
                [1u8; 32],
                999,
            )],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
        assert!(matches!(result.tx_outcomes[0], TxOutcome::Failed { .. }));
    }

    #[test]
    fn test_empty_batch() {
        let snap = TestSnapshot(BTreeMap::new());
        let batch = Batch {
            transactions: vec![],
        };
        let prog = Program::new();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
        assert!(result.tx_outcomes.is_empty());
        assert!(result.read_set_old.is_empty());
        assert!(result.write_set_final.is_empty());
    }

    #[test]
    fn test_tx_outcomes_len_matches_batch() {
        let snap = TestSnapshot(BTreeMap::new());
        let sender = [1u8; 32];
        let batch = Batch {
            transactions: vec![
                make_tx(1, vec![Value::U64(0), Value::U64(1)], sender, 0),
                make_tx(1, vec![Value::U64(1), Value::U64(2)], sender, 1),
                make_tx(1, vec![Value::U64(2), Value::U64(3)], sender, 2),
            ],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
        assert_eq!(result.tx_outcomes.len(), batch.transactions.len());
    }

    #[test]
    fn test_param_count_mismatch_fails() {
        let snap = TestSnapshot(BTreeMap::new());
        let sender = [1u8; 32];
        // write_tx_def expects 2 params, we send 1
        let batch = Batch {
            transactions: vec![make_tx(1, vec![Value::U64(0)], sender, 0)],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
        assert!(
            matches!(&result.tx_outcomes[0], TxOutcome::Failed { reason, .. } if reason.contains("expected 2 params"))
        );
    }

    #[test]
    fn test_param_type_mismatch_fails() {
        let snap = TestSnapshot(BTreeMap::new());
        let sender = [1u8; 32];
        // write_tx_def expects [U64, U64], we send [U64, Bool]
        let batch = Batch {
            transactions: vec![make_tx(
                1,
                vec![Value::U64(0), Value::Bool(true)],
                sender,
                0,
            )],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
        assert!(
            matches!(&result.tx_outcomes[0], TxOutcome::Failed { reason, .. } if reason.contains("param 1"))
        );
    }

    #[test]
    fn test_events_carry_correct_tx_index() {
        let snap = TestSnapshot(BTreeMap::new());
        let sender = [1u8; 32];
        let batch = Batch {
            transactions: vec![
                make_tx(1, vec![Value::U64(0), Value::U64(10)], sender, 0),
                make_tx(1, vec![Value::U64(1), Value::U64(20)], sender, 1),
                make_tx(1, vec![Value::U64(2), Value::U64(30)], sender, 2),
            ],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();

        // Each tx produces 1 Write event; verify tx_index
        let indices: Vec<u32> = result.events.iter().map(|e| e.tx_index).collect();
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_failed_tx_partial_events() {
        // Transfer tx: reads 2 cells, Cmp, asserts balance >= amount, then writes 2 cells.
        // If assert fails (instruction 3), we should get 2 partial read events.
        let mut data = BTreeMap::new();
        data.insert(cell(1, 0, 0), Value::U64(10)); // balance = 10
        data.insert(cell(1, 1, 0), Value::U64(50));
        let snap = TestSnapshot(data);
        let sender = [1u8; 32];

        let batch = Batch {
            transactions: vec![
                // Transfer 100 from row 0 to row 1 — fails at Assert (instruction 3)
                make_tx(2, vec![Value::U64(100)], sender, 0),
            ],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(transfer_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();

        match &result.tx_outcomes[0] {
            TxOutcome::Failed {
                partial_events,
                failed_instruction,
                ..
            } => {
                // 2 reads before the Cmp + assert
                assert_eq!(partial_events.len(), 2);
                assert_eq!(*failed_instruction, Some(3));
            }
            TxOutcome::Success => panic!("expected failure"),
        }
    }

    #[test]
    fn test_precheck_failure_empty_partial() {
        let snap = TestSnapshot(BTreeMap::new());
        let sender = [1u8; 32];
        // Wrong param count → pre-execution failure
        let batch = Batch {
            transactions: vec![make_tx(1, vec![Value::U64(0)], sender, 0)],
        };
        let mut prog = Program::new();
        prog.add_schema(test_schema());
        prog.register(write_tx_def()).unwrap();

        let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();

        match &result.tx_outcomes[0] {
            TxOutcome::Failed {
                partial_events,
                failed_instruction,
                ..
            } => {
                assert!(partial_events.is_empty());
                assert_eq!(*failed_instruction, None);
            }
            TxOutcome::Success => panic!("expected failure"),
        }
    }
}
