#![allow(dead_code)]
//! Shared test doubles and helpers for executor integration tests.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, NoncePolicy, SigVerifier, StateSnapshot, StaticTableProvider};
use tabula_core::{
    CellKey, ColId, ColumnDef, RowKey, TableId, TableSchema, Transaction, Value, ValueType,
};
use tabula_ir::{Instruction, RowExpr, ValueExpr};

use tabula_executor::interpreter::{ExecContext, InterpreterError, TxExecutionOutput};
use tabula_executor::overlay::{Overlay, OverlayResult};
use tabula_executor::property::PropertyQueryRegistry;

// ── StateSnapshot impls ─────────────────────────────────────────────────

/// Simple BTreeMap-backed snapshot for tests.
pub struct TestSnapshot(pub BTreeMap<CellKey, Value>);

impl StateSnapshot for TestSnapshot {
    fn read(&self, key: &CellKey) -> Result<Option<Value>, TabulaError> {
        Ok(self.0.get(key).copied())
    }

    fn table_exists(&self, _: TableId) -> bool {
        true
    }
}

/// Snapshot that tracks how many times `read()` is called.
pub struct CountingSnapshot {
    pub data: BTreeMap<CellKey, Value>,
    pub call_count: AtomicU32,
}

impl CountingSnapshot {
    pub fn new(data: BTreeMap<CellKey, Value>) -> Self {
        Self {
            data,
            call_count: AtomicU32::new(0),
        }
    }

    pub fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl StateSnapshot for CountingSnapshot {
    fn read(&self, key: &CellKey) -> Result<Option<Value>, TabulaError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(self.data.get(key).copied())
    }

    fn table_exists(&self, _: TableId) -> bool {
        true
    }
}

// ── Hasher ──────────────────────────────────────────────────────────────

/// XOR-based hasher for deterministic testing.
pub struct XorHasher;

impl Hasher for XorHasher {
    fn hash(&self, data: &[u8]) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, b) in data.iter().enumerate() {
            out[i % 32] ^= b;
        }
        out
    }

    fn hash_pair(&self, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        let mut combined = Vec::new();
        combined.extend_from_slice(left);
        combined.extend_from_slice(right);
        self.hash(&combined)
    }
}

// ── SigVerifier impls ───────────────────────────────────────────────────

/// Always-valid signature verifier.
pub struct AlwaysValidSig;

impl SigVerifier for AlwaysValidSig {
    fn verify(&self, _: &[u8; 32], _: &[u8], _: &[u8]) -> Result<(), TabulaError> {
        Ok(())
    }
}

/// Always-invalid signature verifier.
pub struct AlwaysInvalidSig;

impl SigVerifier for AlwaysInvalidSig {
    fn verify(&self, _: &[u8; 32], _: &[u8], _: &[u8]) -> Result<(), TabulaError> {
        Err(TabulaError::SignatureInvalid)
    }
}

// ── NoncePolicy ─────────────────────────────────────────────────────────

/// Sequential nonce: `tx_nonce == current`, next = `current + 1`.
pub struct SeqNonce;

impl NoncePolicy for SeqNonce {
    fn validate(&self, sender: &[u8; 32], tx_nonce: u64, current: u64) -> Result<(), TabulaError> {
        if tx_nonce == current {
            Ok(())
        } else {
            Err(TabulaError::InvalidNonce {
                sender: *sender,
                expected: current,
                actual: tx_nonce,
            })
        }
    }

    fn next_nonce(&self, _: &[u8; 32], current: u64) -> u64 {
        current + 1
    }
}

// ── StaticTableProvider impls ───────────────────────────────────────────

/// Static table that returns `Value::U64(row_key)` for any lookup.
pub struct TestStaticTables;

impl StaticTableProvider for TestStaticTables {
    fn lookup(&self, _: TableId, key: RowKey, _: ColId) -> Result<Value, TabulaError> {
        Ok(Value::U64(key.0))
    }

    fn contains(&self, _: TableId, _: RowKey) -> Result<bool, TabulaError> {
        Ok(true)
    }
}

/// Static table that always returns `TableNotFound`.
pub struct EmptyStaticTables;

impl StaticTableProvider for EmptyStaticTables {
    fn lookup(&self, t: TableId, _: RowKey, _: ColId) -> Result<Value, TabulaError> {
        Err(TabulaError::TableNotFound(t))
    }

    fn contains(&self, _: TableId, _: RowKey) -> Result<bool, TabulaError> {
        Ok(false)
    }
}

// ── Helper constructors ─────────────────────────────────────────────────

/// Shorthand for building a `CellKey`.
pub fn cell(t: u32, r: u64, c: u16) -> CellKey {
    CellKey {
        table: TableId(t),
        col: ColId(c),
        row: RowKey(r),
    }
}

/// Build a `Transaction` with an empty signature.
pub fn make_tx(tx_type: u32, params: Vec<Value>, sender: [u8; 32], nonce: u64) -> Transaction {
    Transaction {
        tx_type: tabula_core::TxTypeId(tx_type),
        params,
        sender,
        nonce,
        signature: vec![],
    }
}

/// Build a `BatchEnv` using the standard test doubles.
pub fn test_env() -> tabula_executor::batch::BatchEnv<'static> {
    let property_queries = Box::leak(Box::new(PropertyQueryRegistry::new()));
    tabula_executor::batch::BatchEnv {
        hasher: &XorHasher,
        sig_verifier: &AlwaysValidSig,
        nonce_policy: &SeqNonce,
        static_tables: &TestStaticTables,
        precompiles: None,
        committed_state: None,
        property_queries,
    }
}

// ── Interpreter helpers ─────────────────────────────────────────────────

/// Standard single-table schema for interpreter tests.
pub fn test_schemas() -> BTreeMap<TableId, TableSchema> {
    let mut m = BTreeMap::new();
    m.insert(
        TableId(1),
        TableSchema {
            id: TableId(1),
            name: "test".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "val".into(),
                value_type: ValueType::U64,
            }],
        },
    );
    m
}

fn make_snapshot(entries: Vec<(CellKey, Value)>) -> TestSnapshot {
    TestSnapshot(entries.into_iter().collect())
}

/// Execute instructions with empty params and empty snapshot.
pub fn run(instrs: Vec<Instruction>) -> (TxExecutionOutput, OverlayResult) {
    run_with(instrs, &[], vec![])
}

/// Execute instructions with custom params and initial state.
#[allow(clippy::needless_pass_by_value)]
pub fn run_with(
    instrs: Vec<Instruction>,
    params: &[Value],
    entries: Vec<(CellKey, Value)>,
) -> (TxExecutionOutput, OverlayResult) {
    let snap = make_snapshot(entries);
    let mut ov = Overlay::new(&snap);
    let schemas = test_schemas();
    let property_queries = PropertyQueryRegistry::new();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        schemas: &schemas,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    let out = tabula_executor::interpreter::execute(&instrs, params, &mut ov, &ctx).unwrap();
    (out, ov.into_result())
}

/// Execute instructions expecting failure. Returns the error.
pub fn run_err(instrs: Vec<Instruction>) -> InterpreterError {
    run_err_with(instrs, &[], vec![])
}

/// Execute instructions with custom params/state, expecting failure.
#[allow(clippy::needless_pass_by_value)]
pub fn run_err_with(
    instrs: Vec<Instruction>,
    params: &[Value],
    entries: Vec<(CellKey, Value)>,
) -> InterpreterError {
    let snap = make_snapshot(entries);
    let mut ov = Overlay::new(&snap);
    let schemas = test_schemas();
    let property_queries = PropertyQueryRegistry::new();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        schemas: &schemas,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    tabula_executor::interpreter::execute(&instrs, params, &mut ov, &ctx).unwrap_err()
}

/// Write slot 0 to cell(1,0,0) — append to capture a computed value.
pub fn write_slot0() -> Instruction {
    Instruction::Write {
        table: TableId(1),
        row: RowExpr::Literal(RowKey(0)),
        col: ColId(0),
        src_val: ValueExpr::Slot(0),
        src_is_null: ValueExpr::Literal(Value::Bool(false)),
    }
}
