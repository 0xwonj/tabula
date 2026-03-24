#![allow(dead_code)]
//! Shared test doubles and helpers for executor integration tests.

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, StateView, StaticTableProvider};
use tabula_core::{
    CellKey, ColId, ColumnDef, ColumnProfileId, PortableValue, RowKey, TableId, TableSchema,
    Transaction,
};
use tabula_ir::{Instruction, RowExpr, ValueExpr};
use tabula_profile::{
    ColumnProfile, CommitmentRole, ENCODING_U64_ID, ProfileCatalog, SCHEME_PROFILE_SSMC_ID,
    TYPE_U64_ID, builtin_catalog,
};
use tabula_testing::fixtures::batch::core_tx;
use tabula_testing::fixtures::state::cell_key;
use tabula_types::{TypeRuntimeRegistry, TypedValue};
#[allow(unused_imports)]
pub use tabula_types::{
    bool_portable, bool_typed, bytes32_portable, bytes32_typed, i64_portable, i64_typed,
    u64_portable, u64_typed,
};

use tabula_executor::interpreter::{ExecContext, InterpreterError};
use tabula_executor::overlay::{Overlay, OverlayResult};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_executor::{
    ResolvedColumnLayout, ResolvedExecutionProgram, SuccessfulTxExecution, execute_tx,
};

// ── StateView impls ─────────────────────────────────────────────────

/// Simple BTreeMap-backed snapshot for tests.
pub struct TestSnapshot(pub BTreeMap<CellKey, PortableValue>);

impl StateView for TestSnapshot {
    fn read(&self, key: &CellKey) -> Result<Option<PortableValue>, TabulaError> {
        Ok(self.0.get(key).cloned())
    }

    fn table_exists(&self, _: TableId) -> bool {
        true
    }
}

/// Snapshot that tracks how many times `read()` is called.
pub struct CountingSnapshot {
    pub data: BTreeMap<CellKey, PortableValue>,
    pub call_count: AtomicU32,
}

impl CountingSnapshot {
    pub fn new(data: BTreeMap<CellKey, PortableValue>) -> Self {
        Self {
            data,
            call_count: AtomicU32::new(0),
        }
    }

    pub fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl StateView for CountingSnapshot {
    fn read(&self, key: &CellKey) -> Result<Option<PortableValue>, TabulaError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(self.data.get(key).cloned())
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

// ── StaticTableProvider impls ───────────────────────────────────────────

/// Static table that returns `u64_portable(row_key)` for any lookup.
pub struct TestStaticTables;

impl StaticTableProvider for TestStaticTables {
    fn lookup(&self, _: TableId, key: RowKey, _: ColId) -> Result<PortableValue, TabulaError> {
        Ok(u64_portable(key.0))
    }

    fn contains(&self, _: TableId, _: RowKey) -> Result<bool, TabulaError> {
        Ok(true)
    }
}

/// Static table that always returns `TableNotFound`.
pub struct EmptyStaticTables;

impl StaticTableProvider for EmptyStaticTables {
    fn lookup(&self, t: TableId, _: RowKey, _: ColId) -> Result<PortableValue, TabulaError> {
        Err(TabulaError::TableNotFound(t))
    }

    fn contains(&self, _: TableId, _: RowKey) -> Result<bool, TabulaError> {
        Ok(false)
    }
}

// ── Helper constructors ─────────────────────────────────────────────────

/// Shorthand for building a `CellKey`.
pub fn cell(t: u32, r: u64, c: u16) -> CellKey {
    cell_key(t, r, c)
}

pub fn portable(value: PortableValue) -> PortableValue {
    value
}

pub fn typed(value: PortableValue) -> TypedValue {
    let type_id = value.type_id();
    let payload = value.into_payload();
    type_runtimes()
        .resolve(type_id)
        .expect("typed runtime")
        .decode_portable(&PortableValue::new(type_id, payload))
        .expect("typed test value")
}

pub fn lit(value: PortableValue) -> ValueExpr {
    ValueExpr::Literal(portable(value))
}

pub fn opt(value: PortableValue) -> Option<PortableValue> {
    Some(portable(value))
}

pub fn portable_read_set(result: &OverlayResult) -> Vec<(CellKey, Option<PortableValue>)> {
    result
        .read_set_old
        .iter()
        .map(|entry| {
            (
                entry.key,
                entry.value.as_ref().map(|value| {
                    type_runtimes()
                        .encode_typed(value)
                        .expect("encode read set value")
                }),
            )
        })
        .collect()
}

pub fn portable_write_set(result: &OverlayResult) -> Vec<(CellKey, Option<PortableValue>)> {
    result
        .write_set_final
        .iter()
        .map(|entry| {
            (
                entry.key,
                entry.value.as_ref().map(|value| {
                    type_runtimes()
                        .encode_typed(value)
                        .expect("encode write set value")
                }),
            )
        })
        .collect()
}

pub fn type_runtimes() -> &'static TypeRuntimeRegistry {
    static TYPE_RUNTIMES: OnceLock<TypeRuntimeRegistry> = OnceLock::new();
    TYPE_RUNTIMES.get_or_init(|| TypeRuntimeRegistry::seeded().expect("seeded type runtimes"))
}

/// Build a `Transaction` using the simplified auth-free batch shape.
pub fn make_tx(tx_type: u32, params: Vec<PortableValue>) -> Transaction {
    core_tx(tx_type, params)
}

/// Build a `BatchEnv` using the standard test doubles.
pub fn test_env() -> tabula_executor::batch::BatchEnv<'static> {
    let property_queries = Box::leak(Box::new(PropertyQueryRegistry::new()));
    tabula_executor::batch::BatchEnv {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        precompiles: None,
        committed_state: None,
        property_queries,
        type_runtimes: type_runtimes(),
    }
}

// ── Interpreter helpers ─────────────────────────────────────────────────

/// Standard single-table schema bundle for interpreter tests.
pub fn test_schema_bundle() -> (BTreeMap<TableId, TableSchema>, ProfileCatalog) {
    let mut catalog = builtin_catalog().expect("built-in catalog");
    let type_descriptor = catalog
        .type_descriptor(TYPE_U64_ID)
        .cloned()
        .expect("u64 type descriptor");
    let encoding_profile = catalog
        .encoding_profile(ENCODING_U64_ID)
        .cloned()
        .expect("u64 encoding");
    let scheme_profile = catalog
        .scheme_profile(SCHEME_PROFILE_SSMC_ID)
        .cloned()
        .expect("ssmc scheme");
    let column_profile = ColumnProfile::new(
        ColumnProfileId(0),
        "test.val",
        None,
        &type_descriptor,
        &encoding_profile,
        &scheme_profile,
        CommitmentRole::IncludedInRoot,
    )
    .expect("column profile");
    let column_profile_id = column_profile.column_profile_id;
    catalog
        .register_column(column_profile)
        .expect("register column");
    let mut m = BTreeMap::new();
    m.insert(
        TableId(1),
        TableSchema {
            id: TableId(1),
            name: "test".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "val".into(),
                column_profile_id,
            }],
        },
    );
    (m, catalog)
}

pub fn test_execution_program() -> ResolvedExecutionProgram {
    let mut columns = BTreeMap::new();
    columns.insert(
        (TableId(1), ColId(0)),
        ResolvedColumnLayout {
            type_id: TYPE_U64_ID,
        },
    );
    ResolvedExecutionProgram::new(BTreeMap::new(), columns)
}

fn make_snapshot(entries: Vec<(CellKey, PortableValue)>) -> TestSnapshot {
    TestSnapshot(entries.into_iter().collect())
}

pub fn snapshot(data: BTreeMap<CellKey, PortableValue>) -> TestSnapshot {
    TestSnapshot(data)
}

/// Execute instructions with empty params and empty snapshot.
pub fn run(instrs: Vec<Instruction>) -> (SuccessfulTxExecution, OverlayResult) {
    run_with(instrs, &[], vec![])
}

/// Execute instructions with custom params and initial state.
#[allow(clippy::needless_pass_by_value)]
pub fn run_with(
    instrs: Vec<Instruction>,
    params: &[PortableValue],
    entries: Vec<(CellKey, PortableValue)>,
) -> (SuccessfulTxExecution, OverlayResult) {
    let snap = make_snapshot(entries);
    let mut ov = Overlay::new(&snap, type_runtimes());
    let execution_program = test_execution_program();
    let property_queries = PropertyQueryRegistry::new();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        type_runtimes: type_runtimes(),
        execution_program: &execution_program,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    let typed_params: Vec<_> = params.iter().cloned().map(typed).collect();
    let out = execute_tx(0, &instrs, &typed_params, &mut ov, &ctx).unwrap();
    (out, ov.into_result().unwrap())
}

/// Execute instructions expecting failure. Returns the error.
pub fn run_err(instrs: Vec<Instruction>) -> InterpreterError {
    run_err_with(instrs, &[], vec![])
}

/// Execute instructions with custom params/state, expecting failure.
#[allow(clippy::needless_pass_by_value)]
pub fn run_err_with(
    instrs: Vec<Instruction>,
    params: &[PortableValue],
    entries: Vec<(CellKey, PortableValue)>,
) -> InterpreterError {
    let snap = make_snapshot(entries);
    let mut ov = Overlay::new(&snap, type_runtimes());
    let execution_program = test_execution_program();
    let property_queries = PropertyQueryRegistry::new();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        type_runtimes: type_runtimes(),
        execution_program: &execution_program,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    let typed_params: Vec<_> = params.iter().cloned().map(typed).collect();
    tabula_executor::interpreter::execute(0, &instrs, &typed_params, &mut ov, &ctx).unwrap_err()
}

/// Write slot 0 to cell(1,0,0) — append to capture a computed value.
pub fn write_slot0() -> Instruction {
    Instruction::Write {
        table: TableId(1),
        row: RowExpr::Literal(RowKey(0)),
        col: ColId(0),
        src_val: ValueExpr::Slot(0),
        src_is_null: ValueExpr::Literal(bool_portable(false)),
    }
}
