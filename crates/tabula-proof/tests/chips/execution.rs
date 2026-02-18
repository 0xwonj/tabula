use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::execution::air::ExecutionChip;
use tabula_proof::air::chips::execution::columns::{
    EXECUTION_STANDARD_WIDTH, ExecutionCols, execution_width,
};
use tabula_proof::air::chips::execution::trace::{
    InstructionRecord, generate_execution_trace, limbs_to_u64, u64_to_limbs,
};
use tabula_proof::air::{borrow_cols_mut, debug_check};

use crate::common::builders::{
    make_add, make_and, make_assert, make_not, make_or, make_read, make_select, make_sub,
    make_write,
};

const W: usize = 3;

// ── Valid trace tests ──

#[test]
fn single_add() {
    let records = vec![make_add(0, 100, 200)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("single add");
}

#[test]
fn add_with_carry() {
    let a = (1u64 << 30) - 1;
    let b = 1u64;
    let records = vec![make_add(0, a, b)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("add with carry");
}

#[test]
fn add_large_values() {
    let a = 1_000_000_000u64;
    let b = 2_000_000_000u64;
    let records = vec![make_add(0, a, b)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("add large");
}

#[test]
fn single_sub() {
    let records = vec![make_sub(0, 300, 100)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("single sub");
}

#[test]
fn sub_with_borrow() {
    let a = 1u64 << 30;
    let b = 1u64;
    let records = vec![make_sub(0, a, b)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("sub with borrow");
}

#[test]
fn assert_true() {
    let records = vec![make_assert(true)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("assert true");
}

#[test]
fn select_true_branch() {
    let records = vec![make_select(0, true, 42, 99)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("select true");
}

#[test]
fn select_false_branch() {
    let records = vec![make_select(0, false, 42, 99)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("select false");
}

#[test]
fn read_then_write() {
    let records = vec![
        make_read(0, 1, 0, 100, 42, false),
        make_write(1, 0, 200, 42, false),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("read+write");
}

#[test]
fn multi_instruction_sequence() {
    let records = vec![
        make_read(0, 1, 0, 100, 10, false),
        make_add(1, 10, 20),
        make_assert(true),
        make_write(1, 0, 100, 30, false),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("multi-instr");
}

#[test]
fn ssa_carry_across_rows() {
    let records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("SSA carry");
}

#[test]
fn all_padding() {
    let records: Vec<InstructionRecord> = vec![];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("all padding");
}

#[test]
fn not_true() {
    let records = vec![make_not(0, true)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("not true");
}

#[test]
fn not_false() {
    let records = vec![make_not(0, false)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("not false");
}

#[test]
fn and_all_combinations() {
    for (a, b) in [(false, false), (false, true), (true, false), (true, true)] {
        let records = vec![make_and(0, a, b)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&ExecutionChip::<W>, &trace)
            .unwrap_or_else(|e| panic!("and({a},{b}) failed: {e}"));
    }
}

#[test]
fn or_all_combinations() {
    for (a, b) in [(false, false), (false, true), (true, false), (true, true)] {
        let records = vec![make_or(0, a, b)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&ExecutionChip::<W>, &trace)
            .unwrap_or_else(|e| panic!("or({a},{b}) failed: {e}"));
    }
}

#[test]
fn multi_tx_monotone_index() {
    let mut records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
    records[1].tx_index = 1;
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("monotone tx_index");
}

// ── Invalid trace tests ──

#[test]
fn invalid_wrong_add_result() {
    let mut records = vec![make_add(0, 100, 200)];
    records[0].dst_val = u64_to_limbs(999).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong add result");
}

#[test]
fn invalid_assert_false() {
    let records = vec![make_assert(false)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("assert false should fail");
}

#[test]
fn invalid_wrong_select_result() {
    let mut records = vec![make_select(0, true, 42, 99)];
    records[0].dst_val = u64_to_limbs(99).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong select result");
}

#[test]
fn invalid_wrong_not_result() {
    let mut records = vec![make_not(0, true)];
    records[0].dst_val = vec![BabyBear::ONE, BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong not result should fail");
}

#[test]
fn invalid_wrong_and_result() {
    let mut records = vec![make_and(0, true, false)];
    records[0].dst_val = vec![BabyBear::ONE, BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong and result should fail");
}

#[test]
fn invalid_wrong_or_result() {
    let mut records = vec![make_or(0, false, false)];
    records[0].dst_val = vec![BabyBear::ONE, BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong or result should fail");
}

#[test]
fn invalid_non_monotone_tx_index() {
    let mut records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
    records[0].tx_index = 1;
    records[1].tx_index = 0;
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("non-monotone tx_index should fail");
}

#[test]
fn invalid_tx_index_jump_by_two() {
    let mut records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
    records[1].tx_index = 2;
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("tx_index jump by 2 should fail");
}

#[test]
fn invalid_broken_slot_carry() {
    let records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
    let mut trace = generate_execution_trace::<W>(&records);

    let width = execution_width::<W>();
    let row1_offset = width;
    let cols: &mut ExecutionCols<BabyBear, W> =
        borrow_cols_mut(&mut trace.values[row1_offset..row1_offset + width]);
    cols.slots[0][0] = BabyBear::new(999);

    debug_check(&ExecutionChip::<W>, &trace).expect_err("broken slot carry");
}

// ── Helper tests ──

#[test]
fn u64_limb_roundtrip() {
    for val in [0u64, 1, 42, 1_000_000_000, u64::MAX, (1 << 30) - 1, 1 << 30] {
        let limbs = u64_to_limbs(val);
        let recovered = limbs_to_u64(&limbs);
        assert_eq!(val, recovered, "roundtrip failed for {val}");
    }
}

// ── Soundness tests ──

#[test]
fn soundness_wrong_sub_result() {
    let mut records = vec![make_sub(0, 300, 100)];
    records[0].dst_val = u64_to_limbs(999).to_vec(); // should be 200
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong sub result should fail");
}

#[test]
fn soundness_double_opcode() {
    // Generate valid add, then set op_and=1 too → opcode sum = 2 ≠ 1
    let records = vec![make_add(0, 10, 20)];
    let mut trace = generate_execution_trace::<W>(&records);
    let width = execution_width::<W>();
    let cols: &mut ExecutionCols<BabyBear, W> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.op_and = BabyBear::ONE;
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("two opcodes set should fail one-hot constraint");
}

#[test]
fn soundness_is_real_prefix_gap() {
    let records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
    let mut trace = generate_execution_trace::<W>(&records);
    let width = execution_width::<W>();
    // Set row 0 is_real=0, keep row 1 is_real=1 → 0→1 violates prefix
    let cols: &mut ExecutionCols<BabyBear, W> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_real = BabyBear::ZERO;
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("is_real 0→1 should fail prefix constraint");
}

#[test]
fn soundness_clock_mismatch() {
    // Two non-access instructions (adds): clk should stay 0 for both.
    let records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
    let mut trace = generate_execution_trace::<W>(&records);
    let width = execution_width::<W>();
    // Forge row 1 clk = 1 (should be 0)
    let cols: &mut ExecutionCols<BabyBear, W> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.clk = BabyBear::ONE;
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("clock mismatch should fail recurrence constraint");
}

// ── Column width test ──

#[test]
fn standard_width() {
    assert_eq!(EXECUTION_STANDARD_WIDTH, 118);
}
