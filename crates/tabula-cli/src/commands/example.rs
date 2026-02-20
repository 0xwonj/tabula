//! Handler for the `example` subcommand.

use tabula_core::{ColId, ColumnDef, TableId, TableSchema, TxTypeId, Value, ValueType};
use tabula_driver::register_program;
use tabula_ir::{ArithOp, CmpOp, Instruction, ParamDef, RowExpr, TxTypeDef, ValueExpr};

use crate::io::{BatchFile, ProgramFile, StateCell, StateFile, TxInput, write_json};

/// The .tab source for the example program.
const EXAMPLE_TAB_SOURCE: &str = "\
table balances {
    balance: u64,
}

tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    balances[from].balance = sender_bal - amount
    balances[to].balance = recv_bal + amount
    emit \"transfer\" (from, to, amount)
}
";

pub fn cmd_example(dir: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;

    // Program: token transfer (IR form)
    let mut program = ProgramFile {
        table_schemas: vec![TableSchema {
            id: TableId(0),
            name: "balances".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "balance".into(),
                value_type: ValueType::U64,
            }],
        }],
        tx_types: vec![TxTypeDef {
            id: TxTypeId(0),
            name: "transfer".into(),
            param_schema: vec![
                ParamDef {
                    name: "from".into(),
                    value_type: ValueType::U64,
                },
                ParamDef {
                    name: "to".into(),
                    value_type: ValueType::U64,
                },
                ParamDef {
                    name: "amount".into(),
                    value_type: ValueType::U64,
                },
            ],
            body: vec![
                Instruction::Read {
                    dst_val: 0,
                    dst_is_null: 1,
                    table: TableId(0),
                    row: RowExpr::Param(0),
                    col: ColId(0),
                },
                Instruction::Read {
                    dst_val: 2,
                    dst_is_null: 3,
                    table: TableId(0),
                    row: RowExpr::Param(1),
                    col: ColId(0),
                },
                Instruction::Cmp {
                    dst: 4,
                    op: CmpOp::Gte,
                    lhs: ValueExpr::Slot(0),
                    rhs: ValueExpr::Param(2),
                },
                Instruction::Assert {
                    cond: ValueExpr::Slot(4),
                },
                Instruction::Arith {
                    dst: 5,
                    op: ArithOp::Sub,
                    lhs: ValueExpr::Slot(0),
                    rhs: ValueExpr::Param(2),
                },
                Instruction::Arith {
                    dst: 6,
                    op: ArithOp::Add,
                    lhs: ValueExpr::Slot(2),
                    rhs: ValueExpr::Param(2),
                },
                Instruction::Write {
                    table: TableId(0),
                    row: RowExpr::Param(0),
                    col: ColId(0),
                    src_val: ValueExpr::Slot(5),
                    src_is_null: ValueExpr::Literal(Value::Bool(false)),
                },
                Instruction::Write {
                    table: TableId(0),
                    row: RowExpr::Param(1),
                    col: ColId(0),
                    src_val: ValueExpr::Slot(6),
                    src_is_null: ValueExpr::Literal(Value::Bool(false)),
                },
                Instruction::Emit {
                    topic: b"transfer".to_vec(),
                    data: vec![
                        ValueExpr::Param(0),
                        ValueExpr::Param(1),
                        ValueExpr::Param(2),
                    ],
                },
            ],
        }],
        contract_metadata: None,
    };

    // Emit a fully registered JSON artifact with canonical metadata.
    let artifact = register_program(&program.table_schemas, &program.tx_types)?;
    program.contract_metadata = Some(artifact.metadata_envelope);

    // State: 3 accounts
    let state = StateFile {
        cells: vec![
            StateCell {
                table: 0,
                row: 0,
                col: 0,
                value: Some(Value::U64(1000)),
            },
            StateCell {
                table: 0,
                row: 1,
                col: 0,
                value: Some(Value::U64(500)),
            },
            StateCell {
                table: 0,
                row: 2,
                col: 0,
                value: Some(Value::U64(200)),
            },
        ],
    };

    // Batch: 3 transfers
    let batch = BatchFile {
        transactions: vec![
            TxInput {
                tx_type: 0,
                params: vec![Value::U64(0), Value::U64(1), Value::U64(300)],
                sender: "01".repeat(32),
                nonce: 0,
            },
            TxInput {
                tx_type: 0,
                params: vec![Value::U64(1), Value::U64(2), Value::U64(200)],
                sender: "01".repeat(32),
                nonce: 1,
            },
            TxInput {
                tx_type: 0,
                params: vec![Value::U64(2), Value::U64(0), Value::U64(50)],
                sender: "01".repeat(32),
                nonce: 2,
            },
        ],
    };

    // Write .tab source
    std::fs::write(dir.join("program.tab"), EXAMPLE_TAB_SOURCE)
        .map_err(|e| anyhow::anyhow!("failed to write program.tab: {e}"))?;

    write_json(&dir.join("program.json"), &program)?;
    write_json(&dir.join("state.json"), &state)?;
    write_json(&dir.join("batch.json"), &batch)?;

    println!("Generated example files in {}:", dir.display());
    println!("  program.tab   - DSL source");
    println!("  program.json  - compiled IR");
    println!("  state.json    - 3 accounts (1000, 500, 200)");
    println!("  batch.json    - 3 transfers");
    println!();
    println!("Run with:");
    println!(
        "  tabula execute -p {dir}/program.tab -s {dir}/state.json -b {dir}/batch.json",
        dir = dir.display()
    );
    println!(
        "  tabula execute -p {dir}/program.json -s {dir}/state.json -b {dir}/batch.json",
        dir = dir.display()
    );

    Ok(())
}
