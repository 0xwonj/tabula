use super::*;

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
