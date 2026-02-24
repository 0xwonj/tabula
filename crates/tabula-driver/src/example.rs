//! Built-in transfer example program and bundle.

use tabula_artifact::{BatchFile, StateCell, StateFile, TxInput};
use tabula_core::Value;

use crate::ProgramSourceFile;
use crate::compile::compile_program_source;
use crate::register::{MetadataPolicy, register_program_sources};

/// Built-in transfer example source used by adapter commands.
pub const TRANSFER_EXAMPLE_TAB_SOURCE: &str = "\
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

/// Program/state/batch bundle for sample scenarios.
#[derive(Debug, Clone)]
pub struct ExampleBundle {
    /// `.tab` source text.
    pub program_tab_source: String,
    /// Program artifact JSON payload.
    pub program: ProgramSourceFile,
    /// Initial state payload.
    pub state: StateFile,
    /// Batch payload.
    pub batch: BatchFile,
}

/// Build the canonical transfer example bundle.
pub fn transfer_example_bundle() -> anyhow::Result<ExampleBundle> {
    let mut program =
        compile_program_source(TRANSFER_EXAMPLE_TAB_SOURCE).map_err(anyhow::Error::new)?;
    let artifact =
        register_program_sources(&program, MetadataPolicy::Optional).map_err(anyhow::Error::new)?;
    program.contract_metadata = Some(artifact.metadata_envelope);

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

    Ok(ExampleBundle {
        program_tab_source: TRANSFER_EXAMPLE_TAB_SOURCE.to_string(),
        program,
        state,
        batch,
    })
}
