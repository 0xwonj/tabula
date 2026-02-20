use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::execution::air::ExecutionChip;
use tabula_proof::air::chips::execution::columns::{
    EXECUTION_STANDARD_WIDTH, ExecutionCols, execution_width,
};
use tabula_proof::air::chips::execution::trace::{
    CmpOp, InstructionRecord, generate_execution_trace, limbs_to_u64, u64_to_limbs,
};
use tabula_proof::air::{borrow_cols_mut, debug_check};

use crate::common::builders::{
    make_add, make_and, make_assert, make_cmp, make_divmod, make_hash, make_lookup, make_mul,
    make_not, make_or, make_read, make_read_then_add, make_select, make_sub, make_write,
};

const W: usize = 3;

// ── Valid trace tests ──

#[test]
fn read_then_add() {
    let records = make_read_then_add(0, 1, 2, 100, 200);
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("read+read+add");
}

#[test]
fn add_with_carry() {
    let a = (1u64 << 30) - 1;
    let b = 1u64;
    let records = make_read_then_add(0, 1, 2, a, b);
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("add with carry");
}

#[test]
fn add_large_values() {
    let a = 1_000_000_000u64;
    let b = 2_000_000_000u64;
    let records = make_read_then_add(0, 1, 2, a, b);
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("add large");
}

#[test]
fn read_then_sub() {
    let records = vec![
        make_read(0, 0, 0, 100, 300, false),
        make_read(1, 0, 0, 200, 100, false),
        make_sub(2, 0, 1, 300, 100),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("read+read+sub");
}

#[test]
fn sub_with_borrow() {
    let a = 1u64 << 30;
    let b = 1u64;
    let records = vec![
        make_read(0, 0, 0, 100, a, false),
        make_read(1, 0, 0, 200, b, false),
        make_sub(2, 0, 1, a, b),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("sub with borrow");
}

#[test]
fn assert_true() {
    // Read a boolean 1 into slot 0, then assert it
    let records = vec![make_read(0, 0, 0, 100, 1, false), make_assert(0, true)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("assert true");
}

#[test]
fn select_true_branch() {
    // Read if_true=42 into slot 0, if_false=99 into slot 1, cond=1 into slot 2
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_read(2, 0, 0, 300, 1, false),
        make_select(3, 0, 1, 2, true, 42, 99),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("select true");
}

#[test]
fn select_false_branch() {
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_read(2, 0, 0, 300, 0, false),
        make_select(3, 0, 1, 2, false, 42, 99),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("select false");
}

#[test]
fn read_then_write() {
    let records = vec![
        make_read(0, 1, 0, 100, 42, false),
        make_write(0, 1, 0, 200, 42, false),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("read+write");
}

#[test]
fn multi_instruction_sequence() {
    let records = vec![
        make_read(0, 1, 0, 100, 10, false),
        make_read(1, 1, 0, 200, 20, false),
        make_add(2, 0, 1, 10, 20),
        make_read(3, 1, 0, 300, 1, false),
        make_assert(3, true),
        make_write(2, 1, 0, 100, 30, false),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("multi-instr");
}

#[test]
fn ssa_carry_across_rows() {
    let records = vec![
        make_read(0, 0, 0, 100, 10, false),
        make_read(1, 0, 0, 200, 20, false),
        make_add(2, 0, 1, 10, 20),
        make_read(3, 0, 0, 300, 30, false),
        make_read(4, 0, 0, 400, 40, false),
        make_add(5, 3, 4, 30, 40),
    ];
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
    // Read boolean 1 into slot 0, then Not
    let records = vec![make_read(0, 0, 0, 100, 1, false), make_not(1, 0, true)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("not true");
}

#[test]
fn not_false() {
    let records = vec![make_read(0, 0, 0, 100, 0, false), make_not(1, 0, false)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("not false");
}

#[test]
fn and_all_combinations() {
    for (a, b) in [(false, false), (false, true), (true, false), (true, true)] {
        let a_val = if a { 1u64 } else { 0 };
        let b_val = if b { 1u64 } else { 0 };
        let records = vec![
            make_read(0, 0, 0, 100, a_val, false),
            make_read(1, 0, 0, 200, b_val, false),
            make_and(2, 0, 1, a, b),
        ];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&ExecutionChip::<W>, &trace)
            .unwrap_or_else(|e| panic!("and({a},{b}) failed: {e}"));
    }
}

#[test]
fn or_all_combinations() {
    for (a, b) in [(false, false), (false, true), (true, false), (true, true)] {
        let a_val = if a { 1u64 } else { 0 };
        let b_val = if b { 1u64 } else { 0 };
        let records = vec![
            make_read(0, 0, 0, 100, a_val, false),
            make_read(1, 0, 0, 200, b_val, false),
            make_or(2, 0, 1, a, b),
        ];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&ExecutionChip::<W>, &trace)
            .unwrap_or_else(|e| panic!("or({a},{b}) failed: {e}"));
    }
}

#[test]
fn multi_tx_monotone_index() {
    let mut records = make_read_then_add(0, 1, 2, 10, 20);
    let mut records2 = make_read_then_add(3, 4, 5, 30, 40);
    for r in &mut records2 {
        r.tx_index = 1;
    }
    records.extend(records2);
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("monotone tx_index");
}

// ── Invalid trace tests ──

#[test]
fn invalid_wrong_add_result() {
    let mut records = make_read_then_add(0, 1, 2, 100, 200);
    records[2].dst_val = u64_to_limbs(999).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong add result");
}

#[test]
fn invalid_assert_false() {
    let records = vec![make_read(0, 0, 0, 100, 0, false), make_assert(0, false)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("assert false should fail");
}

#[test]
fn invalid_wrong_select_result() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_read(2, 0, 0, 300, 1, false),
        make_select(3, 0, 1, 2, true, 42, 99),
    ];
    records[3].dst_val = u64_to_limbs(99).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong select result");
}

#[test]
fn invalid_wrong_not_result() {
    let mut records = vec![make_read(0, 0, 0, 100, 1, false), make_not(1, 0, true)];
    records[1].dst_val = vec![BabyBear::ONE, BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong not result should fail");
}

#[test]
fn invalid_wrong_and_result() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 1, false),
        make_read(1, 0, 0, 200, 0, false),
        make_and(2, 0, 1, true, false),
    ];
    records[2].dst_val = vec![BabyBear::ONE, BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong and result should fail");
}

#[test]
fn invalid_wrong_or_result() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 0, false),
        make_read(1, 0, 0, 200, 0, false),
        make_or(2, 0, 1, false, false),
    ];
    records[2].dst_val = vec![BabyBear::ONE, BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong or result should fail");
}

#[test]
fn invalid_non_monotone_tx_index() {
    let mut records = make_read_then_add(0, 1, 2, 10, 20);
    let mut records2 = make_read_then_add(3, 4, 5, 30, 40);
    for r in &mut records {
        r.tx_index = 1;
    }
    for r in &mut records2 {
        r.tx_index = 0;
    }
    records.extend(records2);
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("non-monotone tx_index should fail");
}

#[test]
fn invalid_tx_index_jump_by_two() {
    let mut records = make_read_then_add(0, 1, 2, 10, 20);
    let mut records2 = make_read_then_add(3, 4, 5, 30, 40);
    for r in &mut records2 {
        r.tx_index = 2;
    }
    records.extend(records2);
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("tx_index jump by 2 should fail");
}

#[test]
fn invalid_broken_slot_carry() {
    let records = make_read_then_add(0, 1, 2, 10, 20);
    let mut trace = generate_execution_trace::<W>(&records);

    // Corrupt slot 0 on row 2 (the add instruction) to break carry
    let width = execution_width::<W>();
    let row2_offset = 2 * width;
    let cols: &mut ExecutionCols<BabyBear, W> =
        borrow_cols_mut(&mut trace.values[row2_offset..row2_offset + width]);
    cols.slots[0][0] = BabyBear::new(999);

    debug_check(&ExecutionChip::<W>, &trace).expect_err("broken slot carry");
}

// ── Operand-to-slot linkage tests (M9 A1) ──

#[test]
fn valid_operand_linkage_add() {
    // Read 10 into slot 0, Read 20 into slot 1, Add from slots 0+1 into slot 2
    let records = make_read_then_add(0, 1, 2, 10, 20);
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("operand linkage add");
}

#[test]
fn valid_write_src1_linkage() {
    // Read val=42 into slot 0, Write from slot 0
    let records = vec![
        make_read(0, 1, 0, 100, 42, false),
        make_write(0, 1, 0, 200, 42, false),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("write src1 linkage");
}

#[test]
fn valid_read_destination() {
    // Read writes to destination slot correctly
    let records = vec![make_read(0, 1, 0, 100, 42, false)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("read destination");
}

#[test]
fn invalid_wrong_src1_selector() {
    // Read 10 into slot 0, Read 20 into slot 1
    // Add with src1_sel pointing to slot 1 (which has 20, not 10)
    // but src1_val says 10 → mismatch
    let mut records = make_read_then_add(0, 1, 2, 10, 20);
    // Point src1 at slot 1 instead of slot 0
    records[2].src1_slot_idx = Some(1);
    // src1_val still says 10 but slot 1 has 20 → linkage fails
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong src1 selector should fail");
}

#[test]
fn invalid_missing_selector() {
    // Add with no src1_sel set → selector sum = 0 ≠ 1
    let mut records = make_read_then_add(0, 1, 2, 10, 20);
    records[2].src1_slot_idx = None;
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("missing selector should fail");
}

#[test]
fn invalid_write_val_mismatch() {
    // Read 42 into slot 0, Write claims val=99 from slot 0
    let records = vec![
        make_read(0, 1, 0, 100, 42, false),
        make_write(0, 1, 0, 200, 99, false), // access_val=99 ≠ slot 0 val=42
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("write val mismatch should fail");
}

#[test]
fn invalid_null_flag_mismatch() {
    // Read with is_null=false into slot 0, then write claiming null from that slot
    let records = vec![
        make_read(0, 1, 0, 100, 42, false),
        make_write(0, 1, 0, 200, 42, true), // is_null=true but slot 0 is not null
    ];
    // We need src1_is_null to disagree with slot_is_null
    // make_write sets access_is_null = true, and src1_is_null is taken from slot
    // But the write operand constraint checks access_is_null == src1_is_null
    // and src1_is_null is populated from slot_nulls[src1_slot_idx] in trace gen
    // So src1_is_null = false (from slot), but access_is_null = true → mismatch
    // This should fail the write operand constraint.
    let _ = records; // suppress unused warning
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("null flag mismatch should fail");
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
    let mut records = vec![
        make_read(0, 0, 0, 100, 300, false),
        make_read(1, 0, 0, 200, 100, false),
        make_sub(2, 0, 1, 300, 100),
    ];
    records[2].dst_val = u64_to_limbs(999).to_vec(); // should be 200
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong sub result should fail");
}

#[test]
fn soundness_double_opcode() {
    let records = make_read_then_add(0, 1, 2, 10, 20);
    let mut trace = generate_execution_trace::<W>(&records);
    let width = execution_width::<W>();
    // Corrupt the add row (row 2) to also have op_and=1
    let cols: &mut ExecutionCols<BabyBear, W> =
        borrow_cols_mut(&mut trace.values[2 * width..3 * width]);
    cols.op_and = BabyBear::ONE;
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("two opcodes set should fail one-hot constraint");
}

#[test]
fn soundness_is_real_prefix_gap() {
    let records = make_read_then_add(0, 1, 2, 10, 20);
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
    // Two reads: clk should go 0→1. Corrupt to 0→0.
    let records = vec![
        make_read(0, 0, 0, 100, 10, false),
        make_read(1, 0, 0, 200, 20, false),
    ];
    let mut trace = generate_execution_trace::<W>(&records);
    let width = execution_width::<W>();
    // Forge row 1 clk = 0 (should be 1)
    let cols: &mut ExecutionCols<BabyBear, W> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.clk = BabyBear::ZERO;
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("clock mismatch should fail recurrence constraint");
}

// ── Cmp opcode tests (M10-B1) ──

#[test]
fn cmp_eq_true() {
    // Read 42, Read 42, Cmp(Eq) → 1
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 42, false),
        make_cmp(2, 0, 1, CmpOp::Eq, 42, 42),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp eq true should pass");
}

#[test]
fn cmp_eq_false() {
    // Read 42, Read 99, Cmp(Eq) → 0
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_cmp(2, 0, 1, CmpOp::Eq, 42, 99),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp eq false should pass");
}

#[test]
fn cmp_lt_true() {
    // 10 < 20 → 1
    let records = vec![
        make_read(0, 0, 0, 100, 10, false),
        make_read(1, 0, 0, 200, 20, false),
        make_cmp(2, 0, 1, CmpOp::Lt, 10, 20),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp lt true should pass");
}

#[test]
fn cmp_lt_false() {
    // 20 < 10 → 0
    let records = vec![
        make_read(0, 0, 0, 100, 20, false),
        make_read(1, 0, 0, 200, 10, false),
        make_cmp(2, 0, 1, CmpOp::Lt, 20, 10),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp lt false should pass");
}

#[test]
fn cmp_lte_boundary() {
    // 42 <= 42 → 1
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 42, false),
        make_cmp(2, 0, 1, CmpOp::Lte, 42, 42),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp lte equal should pass");
}

#[test]
fn cmp_gt_gte() {
    // 99 > 42 → 1, 99 >= 42 → 1
    let records = vec![
        make_read(0, 0, 0, 100, 99, false),
        make_read(1, 0, 0, 200, 42, false),
        make_cmp(2, 0, 1, CmpOp::Gt, 99, 42),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp gt should pass");

    let records = vec![
        make_read(0, 0, 0, 100, 99, false),
        make_read(1, 0, 0, 200, 42, false),
        make_cmp(2, 0, 1, CmpOp::Gte, 99, 42),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp gte should pass");
}

#[test]
fn cmp_ne() {
    // 10 != 20 → 1
    let records = vec![
        make_read(0, 0, 0, 100, 10, false),
        make_read(1, 0, 0, 200, 20, false),
        make_cmp(2, 0, 1, CmpOp::Ne, 10, 20),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp ne true should pass");
}

#[test]
fn cmp_wrong_result_fails() {
    // 10 < 20 → correct result is 1, but we'll corrupt to 0
    let mut records = vec![
        make_read(0, 0, 0, 100, 10, false),
        make_read(1, 0, 0, 200, 20, false),
        make_cmp(2, 0, 1, CmpOp::Lt, 10, 20),
    ];
    // Corrupt: set dst to 0 (wrong: 10 < 20 should be 1)
    records[2].dst_val = vec![BabyBear::ZERO, BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect_err("wrong cmp result should fail constraint");
}

#[test]
fn cmp_large_values() {
    // Compare large u64 values that span multiple limbs
    let a = (1u64 << 50) + 42;
    let b = (1u64 << 50) + 99;
    let records = vec![
        make_read(0, 0, 0, 100, a, false),
        make_read(1, 0, 0, 200, b, false),
        make_cmp(2, 0, 1, CmpOp::Lt, a, b),
    ];
    let trace = generate_execution_trace::<3>(&records);
    debug_check(&ExecutionChip::<3>, &trace).expect("cmp large values should pass");
}

// ── Hash opcode tests (M10-B2) ──

#[test]
fn hash_two_inputs() {
    // Read two values, then Hash them
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_hash(2, 0, 1, 0x20, 2, [42, 0, 0], [99, 0, 0]),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("hash two inputs should pass");
}

#[test]
fn hash_single_input_zero_padded() {
    // Read one value into slot 0, read zero into slot 1, Hash with n=1
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 0, false),
        make_hash(2, 0, 1, 0x20, 2, [42, 0, 0], [0, 0, 0]),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("hash single input should pass");
}

#[test]
fn hash_wrong_output_fails() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_hash(2, 0, 1, 0x20, 2, [42, 0, 0], [99, 0, 0]),
    ];
    // Corrupt the hash output (dst_val)
    records[2].dst_val = vec![BabyBear::new(999), BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("wrong hash output should fail result binding");
}

#[test]
fn hash_wrong_input_composition_fails() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_hash(2, 0, 1, 0x20, 2, [42, 0, 0], [99, 0, 0]),
    ];
    // Corrupt perm_input[2] (should match src1_val[0])
    if let Some(ref mut input) = records[2].hash_perm_input {
        input[2] = BabyBear::new(999);
    }
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong input composition should fail");
}

#[test]
fn hash_nonzero_capacity_fails() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_hash(2, 0, 1, 0x20, 2, [42, 0, 0], [99, 0, 0]),
    ];
    // Corrupt capacity (perm_input[8] should be 0)
    if let Some(ref mut input) = records[2].hash_perm_input {
        input[8] = BabyBear::ONE;
    }
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("nonzero capacity should fail");
}

// ── Lookup opcode tests (M10-B3) ──

#[test]
fn lookup_result_binding() {
    // Lookup val=42 from static table
    let records = vec![make_lookup(0, 5, 0, 100, 42)];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("lookup result binding should pass");
}

#[test]
fn lookup_wrong_result_fails() {
    let mut records = vec![make_lookup(0, 5, 0, 100, 42)];
    // Corrupt dst_val
    records[0].dst_val = u64_to_limbs(999).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong lookup result should fail");
}

#[test]
fn lookup_does_not_set_is_access() {
    // Lookup followed by Read — clk should start at 0, Read bumps to 1
    // If Lookup wrongly set is_access, clk would be wrong
    let records = vec![
        make_lookup(0, 5, 0, 100, 42),
        make_read(1, 0, 0, 200, 99, false),
    ];
    let trace = generate_execution_trace::<W>(&records);
    // Row 0 (Lookup): clk=0, is_access=0, no tau
    // Row 1 (Read): clk=0, is_access=1, tau=1
    debug_check(&ExecutionChip::<W>, &trace).expect("lookup does not advance clock");
}

// ── Mul opcode tests (M10-C1) ──

#[test]
fn mul_small_values() {
    let records = vec![
        make_read(0, 0, 0, 100, 3, false),
        make_read(1, 0, 0, 200, 5, false),
        make_mul(2, 0, 1, 3, 5),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("3*5=15 should pass");
}

#[test]
fn mul_large_values() {
    let a = (1u64 << 30) - 1;
    let b = 2u64;
    let records = vec![
        make_read(0, 0, 0, 100, a, false),
        make_read(1, 0, 0, 200, b, false),
        make_mul(2, 0, 1, a, b),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("(2^30-1)*2 should pass");
}

#[test]
fn mul_carry_propagation() {
    let a = 1_000_000u64;
    let b = 1_000_000u64;
    let records = vec![
        make_read(0, 0, 0, 100, a, false),
        make_read(1, 0, 0, 200, b, false),
        make_mul(2, 0, 1, a, b),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("1M*1M with carry should pass");
}

#[test]
fn mul_zero() {
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 0, false),
        make_mul(2, 0, 1, 42, 0),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("42*0=0 should pass");
}

#[test]
fn mul_wrong_result_fails() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 3, false),
        make_read(1, 0, 0, 200, 5, false),
        make_mul(2, 0, 1, 3, 5),
    ];
    records[2].dst_val = u64_to_limbs(999).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong mul result should fail");
}

#[test]
fn mul_overflow_detected() {
    // Product > 2^64: limb2 * limb2 != 0 → T4 != 0
    // Use a = 2^32, b = 2^33 → product = 2^65 > 2^64
    // a limbs: (0, 4, 0), b limbs: (0, 8, 0) → a2*b2 = 0 but a1*b1 = 32
    // Actually need a1*b2 + a2*b1 != 0 for T3 constraint.
    // a = 2^60 + 1 → limbs (1, 0, 1), b = 2^30 → limbs (0, 1, 0)
    // T3 = a1*b2 + a2*b1 = 0*0 + 1*1 = 1 ≠ 0 → fails
    let a = (1u64 << 60) + 1;
    let b = 1u64 << 30;
    // Product overflows: (2^60+1) * 2^30 = 2^90 + 2^30 > 2^64
    let result = a.wrapping_mul(b); // wrapping for the dst_val

    let mut records = vec![
        make_read(0, 0, 0, 100, a, false),
        make_read(1, 0, 0, 200, b, false),
        make_mul(2, 0, 1, a, b),
    ];
    // Force the "correct" wrapping result to check that the overflow constraint catches it
    records[2].dst_val = u64_to_limbs(result).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("mul overflow should fail T3/T4");
}

// ── DivMod opcode tests (M10-C2) ──

#[test]
fn divmod_basic() {
    // 7 / 3 = (2, 1)
    let records = vec![
        make_read(0, 0, 0, 100, 7, false),
        make_read(1, 0, 0, 200, 3, false),
        make_divmod(2, 3, 0, 1, 7, 3),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("7/3 = (2,1) should pass");
}

#[test]
fn divmod_exact() {
    // 6 / 3 = (2, 0)
    let records = vec![
        make_read(0, 0, 0, 100, 6, false),
        make_read(1, 0, 0, 200, 3, false),
        make_divmod(2, 3, 0, 1, 6, 3),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("6/3 = (2,0) should pass");
}

#[test]
fn divmod_large_values() {
    let lhs = 1_000_000_000u64;
    let rhs = 7u64;
    let records = vec![
        make_read(0, 0, 0, 100, lhs, false),
        make_read(1, 0, 0, 200, rhs, false),
        make_divmod(2, 3, 0, 1, lhs, rhs),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("large divmod should pass");
}

#[test]
fn divmod_dividend_smaller_than_divisor() {
    // 3 / 7 = (0, 3)
    let records = vec![
        make_read(0, 0, 0, 100, 3, false),
        make_read(1, 0, 0, 200, 7, false),
        make_divmod(2, 3, 0, 1, 3, 7),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("3/7 = (0,3) should pass");
}

#[test]
fn divmod_wrong_quotient_fails() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 7, false),
        make_read(1, 0, 0, 200, 3, false),
        make_divmod(2, 3, 0, 1, 7, 3),
    ];
    records[2].dst_val = u64_to_limbs(999).to_vec(); // wrong quotient
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong quotient should fail");
}

#[test]
fn divmod_wrong_remainder_fails() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 7, false),
        make_read(1, 0, 0, 200, 3, false),
        make_divmod(2, 3, 0, 1, 7, 3),
    ];
    records[2].dst2_val = u64_to_limbs(2).to_vec(); // rem=2 but should be 1
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong remainder should fail");
}

// ── Regression tests ──

/// Regression: DivMod carry must include remainder (not just q*rhs).
/// lhs=2^30, rhs=3 triggers carry divergence: q0*d0 = 357913941*3 = 1073741823
/// which has c0=0 (no carry from product alone), but q0*d0 + rem0 = 1073741824
/// which has c0=1 (rem=1 pushes past 2^30 boundary).
#[test]
fn divmod_carry_with_remainder_overflow() {
    let lhs = 1u64 << 30; // 1073741824
    let rhs = 3u64;
    // q = 357913941, rem = 1
    let records = vec![
        make_read(0, 0, 0, 100, lhs, false),
        make_read(1, 0, 0, 200, rhs, false),
        make_divmod(2, 3, 0, 1, lhs, rhs),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect("divmod carry with remainder overflow should pass");
}

/// Regression: Hash domain tag must be 0x20 (not arbitrary).
#[test]
fn hash_wrong_domain_tag_fails() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_hash(2, 0, 1, 0x20, 2, [42, 0, 0], [99, 0, 0]),
    ];
    // Corrupt domain tag in perm_input
    if let Some(ref mut input) = records[2].hash_perm_input {
        input[0] = BabyBear::new(0x10); // wrong domain tag
    }
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong domain tag should fail");
}

/// Regression: Hash input count must be 2.
#[test]
fn hash_wrong_input_count_fails() {
    let mut records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_hash(2, 0, 1, 0x20, 2, [42, 0, 0], [99, 0, 0]),
    ];
    // Corrupt input count in perm_input
    if let Some(ref mut input) = records[2].hash_perm_input {
        input[1] = BabyBear::new(1); // wrong count
    }
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong input count should fail");
}

/// Slot written count: extra slot_written flag should fail.
#[test]
fn slot_written_count_extra_write_fails() {
    let mut records = vec![make_read(0, 0, 0, 100, 42, false)];
    // Read should write exactly 1 slot. Claim 2 written.
    records[0].written_slots = vec![0, 1];
    records[0].dst2_val = u64_to_limbs(0).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("extra slot_written should fail");
}

/// Slot written count: missing slot_written flag should fail.
#[test]
fn slot_written_count_missing_write_fails() {
    let mut records = vec![make_read(0, 0, 0, 100, 42, false)];
    // Read should write exactly 1 slot. Claim 0 written.
    records[0].written_slots = vec![];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("missing slot_written should fail");
}

/// DivMod: wrong divmod_q_sel pointing to remainder slot should fail.
#[test]
fn divmod_wrong_q_sel_fails() {
    // 7 / 3 = (q=2, rem=1). q_slot=2, r_slot=3.
    let records = vec![
        make_read(0, 0, 0, 100, 7, false),
        make_read(1, 0, 0, 200, 3, false),
        make_divmod(2, 3, 0, 1, 7, 3),
    ];
    let mut trace = generate_execution_trace::<W>(&records);
    // Swap divmod_q_sel: point to r_slot (3) instead of q_slot (2).
    // This makes the AIR treat rem=1 as q and q=2 as rem, which should fail identity.
    let width = EXECUTION_STANDARD_WIDTH;
    let row2_offset = 2 * width;
    let cols: &mut ExecutionCols<BabyBear, W> =
        borrow_cols_mut(&mut trace.values[row2_offset..row2_offset + width]);
    // Clear current q_sel[2] = 1, set q_sel[3] = 1 (swap q and rem selectors)
    cols.divmod_q_sel[2] = BabyBear::ZERO;
    cols.divmod_q_sel[3] = BabyBear::ONE;
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("wrong q_sel should fail division identity");
}

// ── T1: DivMod div-by-zero ──

/// T1: DivMod with rhs=0 must be rejected by the non-zero divisor constraint.
///
/// We cannot call `make_divmod` with rhs=0 because it would panic at lhs/rhs.
/// Instead, construct a valid 7/3 DivMod record, generate the trace, then tamper
/// the trace to force `divmod_rhs_iz.is_zero = 1` (signalling rhs=0) while also
/// zeroing src2 limbs so the IsZero witness is consistent. The constraint
/// `assert_zero(gate * rhs_iz.is_zero)` must then fire.
#[test]
fn divmod_div_by_zero_rejected() {
    use tabula_proof::air::chips::execution::columns::ExecutionCols;

    // Start with a valid 7/3 trace so trace generation doesn't panic.
    let records = vec![
        make_read(0, 0, 0, 100, 7, false),
        make_read(1, 0, 0, 200, 3, false),
        make_divmod(2, 3, 0, 1, 7, 3),
    ];
    let mut trace = generate_execution_trace::<W>(&records);

    // Row index 2 is the DivMod instruction.
    let width = EXECUTION_STANDARD_WIDTH;
    let row2_offset = 2 * width;
    let cols: &mut ExecutionCols<BabyBear, W> =
        borrow_cols_mut(&mut trace.values[row2_offset..row2_offset + width]);

    // Zero out src2 limbs (pretend rhs=0).
    cols.src2_val[0] = BabyBear::ZERO;
    cols.src2_val[1] = BabyBear::ZERO;
    cols.src2_val[2] = BabyBear::ZERO;

    // Set is_zero=1 (rhs "is" zero) — this makes the IsZero witness consistent
    // but triggers the final `assert_zero(gate * is_zero)` constraint.
    cols.divmod_rhs_iz.is_zero = BabyBear::ONE;
    // Set inv=0 (consistent with val=0 → no inverse).
    cols.divmod_rhs_iz.inv = BabyBear::ZERO;

    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("divmod with rhs=0 must fail non-zero divisor constraint");
}

// ── T2: DivMod rem>=rhs ──

/// T2: Forge rem=rhs (should be rem < rhs). The StrictIneq constraint enforces rem < rhs.
#[test]
fn divmod_rem_equals_rhs_rejected() {
    // 7 / 3 = (q=2, rem=1). Forge rem=3 (=rhs), which violates rem < rhs.
    let mut records = vec![
        make_read(0, 0, 0, 100, 7, false),
        make_read(1, 0, 0, 200, 3, false),
        make_divmod(2, 3, 0, 1, 7, 3),
    ];
    // Set rem=3 (same as rhs=3). The identity 7 = q*3 + rem also breaks,
    // but the StrictIneq constraint fires first: diff0 = rhs[0]-rem[0]-1 = 3-3-1 = -1 (wraps).
    records[2].dst2_val = u64_to_limbs(3).to_vec(); // rem=3 instead of 1
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("rem=rhs must fail StrictIneq (rem < rhs) constraint");
}

// ── T4: Select null propagation ──

/// T4: Select where dst_is_null correctly propagates from selected branch.
///
/// When cond=1 (true branch), dst gets if_true's value. If if_true's slot
/// is not null, dst_is_null must be false. We forge dst_is_null=true to verify
/// it fails.
#[test]
fn select_null_propagation_forged_fails() {
    // Read non-null 42 into slot 0 (if_true), non-null 99 into slot 1 (if_false),
    // cond=1 into slot 2. Select → dst=42, dst_is_null=false.
    let mut records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_read(2, 0, 0, 300, 1, false),
        make_select(3, 0, 1, 2, true, 42, 99),
    ];
    // Forge dst_is_null=true (should be false since if_true slot is not null).
    records[3].dst_is_null = true;
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("forged dst_is_null=true on select should fail operand linkage");
}

/// T4b: Valid Select with null input propagation — cond selects null branch correctly.
///
/// Both branches are non-null in our standard encoding; verify the valid path passes.
#[test]
fn select_valid_null_propagation_passes() {
    let records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_read(2, 0, 0, 300, 0, false),
        make_select(3, 0, 1, 2, false, 42, 99),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect("valid select with false cond should pass");
}

// ── T10: Mul u64::MAX boundary ──

/// T10: Multiply values near u64::MAX boundary — overflow constraint must fire.
///
/// u64::MAX * 2 overflows, so the no-overflow (T3=0, T4=0) constraint must fail.
/// u64::MAX = (2^30-1) + (2^30-1)*2^30 + 15*2^60 in our limb encoding.
/// T3 = a1*b2 + a2*b1 = nonzero → fails.
#[test]
fn mul_u64_max_boundary() {
    // u64::MAX * 1 = u64::MAX (no overflow, must pass).
    let a = u64::MAX;
    let b = 1u64;
    let records = vec![
        make_read(0, 0, 0, 100, a, false),
        make_read(1, 0, 0, 200, b, false),
        make_mul(2, 0, 1, a, b),
    ];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect("u64::MAX * 1 should pass (no overflow)");
}

/// T10b: u64::MAX * 2 overflows u64 → should fail the no-overflow constraint.
#[test]
fn mul_u64_max_times_two_overflow() {
    // u64::MAX * 2 = 2^65 - 2, which exceeds 64-bit range.
    // a = u64::MAX = limbs(2^30-1, 2^30-1, 15)
    // b = 2 = limbs(2, 0, 0)
    // T3 = a1*b2 + a2*b1 = (2^30-1)*0 + 15*0 = 0 for this specific b
    // But T2 carries overflow: a0*b0 = (2^30-1)*2, c0=1; a1*b0 = (2^30-1)*2, c1=...
    // The actual overflow: wrapping result is u64::MAX.wrapping_mul(2).
    let a = u64::MAX;
    let b = 2u64;
    // This overflows: 2^65 - 2 > 2^64.
    // Use limb analysis: a2 = 15, b0 = 2 → T3 includes a2*b0 cross-product term.
    // Actually T3 = a0*b2 + a1*b1 + a2*b0 + carry_from_T2.
    // a0=(2^30-1), b2=0; a1=(2^30-1), b1=0; a2=15, b0=2 → a2*b0 = 30.
    // T3 = 30 ≠ 0 → overflow constraint fails. Good.
    let mut records = vec![
        make_read(0, 0, 0, 100, a, false),
        make_read(1, 0, 0, 200, b, false),
        make_mul(2, 0, 1, a, b),
    ];
    // Force the wrapping result (trace gen uses wrapping_mul already, but it still
    // fails because the overflow check detects T3≠0).
    records[2].dst_val = u64_to_limbs(a.wrapping_mul(b)).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("u64::MAX * 2 overflow must fail no-overflow constraint");
}

// ── T14: Lookup forged null ──

/// T14: Lookup sets dst_is_null=true, which must fail because Lookup is a total function.
///
/// The constraint `assert_zero(slot_gate * slot_is_null[s])` in constrain_lookup
/// enforces that Lookup's destination slot is never null.
#[test]
fn lookup_forged_null_dst_fails() {
    let mut records = vec![make_lookup(0, 5, 0, 100, 42)];
    // Forge dst_is_null=true. Trace gen will set slot_is_null[0]=1 for this slot,
    // violating constrain_lookup's slot_is_null assertion.
    records[0].dst_is_null = true;
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("lookup with dst_is_null=true must fail (total function, no null output)");
}

// ── Column width test ──

#[test]
fn standard_width() {
    assert_eq!(EXECUTION_STANDARD_WIDTH, 278);
}
