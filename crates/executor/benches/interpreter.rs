//! Interpreter benchmarks.

use std::collections::BTreeMap;

use criterion::{Criterion, criterion_group, criterion_main};

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, StateSnapshot, StaticTableProvider};
use tabula_core::{CellKey, ColId, RowKey, TableId, TableSchema, Value};
use tabula_ir::{ArithOp, CmpOp, Instruction, RowExpr, ValueExpr};
use tabula_testing::fixtures::schema::single_u64_column_schema;
use tabula_testing::fixtures::state::cell_key;

use tabula_executor::interpreter::{ExecContext, execute};
use tabula_executor::overlay::Overlay;
use tabula_executor::property::PropertyQueryRegistry;

// ── Test doubles ─────────────────────────────────────────────────────

struct BenchSnapshot(BTreeMap<CellKey, Value>);

impl StateSnapshot for BenchSnapshot {
    fn read(&self, key: &CellKey) -> Result<Option<Value>, TabulaError> {
        Ok(self.0.get(key).copied())
    }
    fn table_exists(&self, _: TableId) -> bool {
        true
    }
}

struct XorHasher;

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

struct TestStaticTables;

impl StaticTableProvider for TestStaticTables {
    fn lookup(&self, _: TableId, key: RowKey, _: ColId) -> Result<Value, TabulaError> {
        Ok(Value::U64(key.0))
    }
    fn contains(&self, _: TableId, _: RowKey) -> Result<bool, TabulaError> {
        Ok(true)
    }
}

fn test_schemas() -> BTreeMap<TableId, TableSchema> {
    let mut m = BTreeMap::new();
    m.insert(
        TableId(1),
        single_u64_column_schema(TableId(1), ColId(0), "test", "val"),
    );
    m
}

// ── Benchmarks ───────────────────────────────────────────────────────

fn bench_arith_chain(c: &mut Criterion) {
    // 100 chained arithmetic operations.
    let mut instrs = Vec::new();
    instrs.push(Instruction::Arith {
        dst: 0,
        op: ArithOp::Add,
        lhs: ValueExpr::Literal(Value::U64(1)),
        rhs: ValueExpr::Literal(Value::U64(2)),
    });
    for i in 1..100u32 {
        instrs.push(Instruction::Arith {
            dst: i as u16,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot((i - 1) as u16),
            rhs: ValueExpr::Literal(Value::U64(1)),
        });
    }
    instrs.push(Instruction::Write {
        table: TableId(1),
        row: RowExpr::Literal(RowKey(0)),
        col: ColId(0),
        src_val: ValueExpr::Slot(99),
        src_is_null: ValueExpr::Literal(Value::Bool(false)),
    });

    let snap = BenchSnapshot(BTreeMap::new());
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

    c.bench_function("arith_chain_100", |b| {
        b.iter(|| {
            let mut ov = Overlay::new(&snap);
            execute(&instrs, &[], &mut ov, &ctx).unwrap();
        });
    });
}

fn bench_read_write_mix(c: &mut Criterion) {
    // 50 reads from state + 50 writes.
    let mut data = BTreeMap::new();
    for i in 0..50u64 {
        data.insert(cell_key(1, i, 0), Value::U64(i * 10));
    }

    let mut instrs = Vec::new();
    for i in 0..50u32 {
        instrs.push(Instruction::Read {
            dst_val: i as u16 * 2,
            dst_is_null: i as u16 * 2 + 1,
            table: TableId(1),
            row: RowExpr::Literal(RowKey(i as u64)),
            col: ColId(0),
        });
    }
    for i in 0..50u32 {
        instrs.push(Instruction::Write {
            table: TableId(1),
            row: RowExpr::Literal(RowKey(i as u64)),
            col: ColId(0),
            src_val: ValueExpr::Slot(i as u16 * 2),
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
        });
    }

    let snap = BenchSnapshot(data);
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

    c.bench_function("read_write_50_50", |b| {
        b.iter(|| {
            let mut ov = Overlay::new(&snap);
            execute(&instrs, &[], &mut ov, &ctx).unwrap();
        });
    });
}

fn bench_cmp_assert(c: &mut Criterion) {
    // 100 compare + assert pairs.
    let mut instrs = Vec::new();
    for i in 0..100u32 {
        instrs.push(Instruction::Cmp {
            dst: i as u16,
            op: CmpOp::Lt,
            lhs: ValueExpr::Literal(Value::U64(i as u64)),
            rhs: ValueExpr::Literal(Value::U64(i as u64 + 1)),
        });
        instrs.push(Instruction::Assert {
            cond: ValueExpr::Slot(i as u16),
        });
    }

    let snap = BenchSnapshot(BTreeMap::new());
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

    c.bench_function("cmp_assert_100", |b| {
        b.iter(|| {
            let mut ov = Overlay::new(&snap);
            execute(&instrs, &[], &mut ov, &ctx).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_arith_chain,
    bench_read_write_mix,
    bench_cmp_assert
);
criterion_main!(benches);
