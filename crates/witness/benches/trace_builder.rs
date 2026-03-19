//! Trace-builder benchmarks.

use criterion::{Criterion, criterion_group, criterion_main};
use tabula_testing::fixtures::cases::{arith_add_sub_trace_case, touch_trace_case};
use tabula_testing::witness::{build_trace_map, compile_execute_case};

fn bench_trace_read_write(c: &mut Criterion) {
    let case = touch_trace_case();
    let setup = compile_execute_case(&case);

    c.bench_function("trace_read_write", |b| {
        b.iter(|| {
            build_trace_map::<3>(&setup).unwrap();
        });
    });
}

fn bench_trace_arith(c: &mut Criterion) {
    let case = arith_add_sub_trace_case();
    let setup = compile_execute_case(&case);

    c.bench_function("trace_arith", |b| {
        b.iter(|| {
            build_trace_map::<3>(&setup).unwrap();
        });
    });
}

criterion_group!(benches, bench_trace_read_write, bench_trace_arith);
criterion_main!(benches);
