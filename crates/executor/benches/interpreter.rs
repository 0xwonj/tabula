//! Interpreter benchmarks.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use criterion::{Criterion, criterion_group, criterion_main};

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, StateView, StaticTableProvider};
use tabula_core::{CellKey, ColId, PortableValue, RowKey, TableId, TxTypeId};
use tabula_ir::{ArithOp, CmpOp, Instruction, ParamDef, RowExpr, ValueExpr};
use tabula_profile::TYPE_U64_ID;
use tabula_testing::fixtures::state::cell_key;
use tabula_types::{TypeRuntimeRegistry, bool_portable, u64_portable};

use tabula_executor::interpreter::{ExecContext, execute};
use tabula_executor::overlay::Overlay;
use tabula_executor::property::PropertyQueryRegistry;
use tabula_executor::{ResolvedColumnLayout, ResolvedExecutionProgram, ResolvedTxDefinition};

// ── Test doubles ─────────────────────────────────────────────────────

struct BenchSnapshot(BTreeMap<CellKey, PortableValue>);

impl StateView for BenchSnapshot {
    fn read(&self, key: &CellKey) -> Result<Option<PortableValue>, TabulaError> {
        Ok(self.0.get(key).cloned())
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
    fn lookup(&self, _: TableId, key: RowKey, _: ColId) -> Result<PortableValue, TabulaError> {
        Ok(u64_portable(key.0))
    }
    fn contains(&self, _: TableId, _: RowKey) -> Result<bool, TabulaError> {
        Ok(true)
    }
}

fn portable(value: PortableValue) -> PortableValue {
    value
}

fn lit(value: PortableValue) -> ValueExpr {
    ValueExpr::Literal(portable(value))
}

fn type_runtimes() -> &'static TypeRuntimeRegistry {
    static TYPE_RUNTIMES: OnceLock<TypeRuntimeRegistry> = OnceLock::new();
    TYPE_RUNTIMES.get_or_init(|| TypeRuntimeRegistry::seeded().expect("seeded type runtimes"))
}

fn execution_program(
    tx_type: TxTypeId,
    param_schema: Vec<ParamDef>,
    body: Vec<Instruction>,
) -> ResolvedExecutionProgram {
    let mut tx_definitions = BTreeMap::new();
    tx_definitions.insert(
        tx_type,
        ResolvedTxDefinition {
            tx_type,
            param_schema,
            body,
        },
    );

    let mut columns = BTreeMap::new();
    columns.insert(
        (TableId(1), ColId(0)),
        ResolvedColumnLayout {
            type_id: TYPE_U64_ID,
        },
    );

    ResolvedExecutionProgram::new(tx_definitions, columns)
}

// ── Benchmarks ───────────────────────────────────────────────────────

fn bench_arith_chain(c: &mut Criterion) {
    // 100 chained arithmetic operations.
    let mut instrs = Vec::new();
    instrs.push(Instruction::Arith {
        dst: 0,
        op: ArithOp::Add,
        lhs: lit(u64_portable(1)),
        rhs: lit(u64_portable(2)),
    });
    for i in 1..100u32 {
        instrs.push(Instruction::Arith {
            dst: i as u16,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot((i - 1) as u16),
            rhs: lit(u64_portable(1)),
        });
    }
    instrs.push(Instruction::Write {
        table: TableId(1),
        row: RowExpr::Literal(RowKey(0)),
        col: ColId(0),
        src_val: ValueExpr::Slot(99),
        src_is_null: lit(bool_portable(false)),
    });

    let execution_program = execution_program(TxTypeId(1), vec![], instrs.clone());
    let snap = BenchSnapshot(BTreeMap::new());
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

    c.bench_function("arith_chain_100", |b| {
        b.iter(|| {
            let mut ov = Overlay::new(&snap, type_runtimes());
            execute(0, &instrs, &[], &mut ov, &ctx).unwrap();
        });
    });
}

fn bench_read_write_mix(c: &mut Criterion) {
    // 50 reads from state + 50 writes.
    let mut data = BTreeMap::new();
    for i in 0..50u64 {
        data.insert(cell_key(1, i, 0), portable(u64_portable(i * 10)));
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
            src_is_null: lit(bool_portable(false)),
        });
    }

    let execution_program = execution_program(TxTypeId(2), vec![], instrs.clone());
    let snap = BenchSnapshot(data);
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

    c.bench_function("read_write_50_50", |b| {
        b.iter(|| {
            let mut ov = Overlay::new(&snap, type_runtimes());
            execute(0, &instrs, &[], &mut ov, &ctx).unwrap();
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
            lhs: lit(u64_portable(i as u64)),
            rhs: lit(u64_portable(i as u64 + 1)),
        });
        instrs.push(Instruction::Assert {
            cond: ValueExpr::Slot(i as u16),
        });
    }

    let execution_program = execution_program(TxTypeId(3), vec![], instrs.clone());
    let snap = BenchSnapshot(BTreeMap::new());
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

    c.bench_function("cmp_assert_100", |b| {
        b.iter(|| {
            let mut ov = Overlay::new(&snap, type_runtimes());
            execute(0, &instrs, &[], &mut ov, &ctx).unwrap();
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
