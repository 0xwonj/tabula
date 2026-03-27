use super::*;

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
    records[2].writes[0].1 = vec![KoalaBear::ZERO, KoalaBear::ZERO, KoalaBear::ZERO];
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
    // Corrupt the hash output (writes[0] value)
    records[2].writes[0].1 = vec![KoalaBear::new(999), KoalaBear::ZERO, KoalaBear::ZERO];
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("wrong hash output should fail result binding");
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
    // Corrupt writes[0] value
    records[0].writes[0].1 = u64_to_limbs(999).to_vec();
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
    records[2].writes[0].1 = u64_to_limbs(999).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong mul result should fail");
}

#[test]
fn mul_overflow_detected() {
    // Product > 2^64: limb2 * limb2 != 0 → T4 != 0
    // Use a = 2^60 + 1 → limbs (1, 0, 1), b = 2^30 → limbs (0, 1, 0)
    // T3 = a1*b2 + a2*b1 = 0*0 + 1*1 = 1 ≠ 0 → fails
    let a = (1u64 << 60) + 1;
    let b = 1u64 << 30;
    // Product overflows: (2^60+1) * 2^30 = 2^90 + 2^30 > 2^64
    let result = a.wrapping_mul(b); // wrapping for the writes value

    let mut records = vec![
        make_read(0, 0, 0, 100, a, false),
        make_read(1, 0, 0, 200, b, false),
        make_mul(2, 0, 1, a, b),
    ];
    // Force the "correct" wrapping result to check that the overflow constraint catches it
    records[2].writes[0].1 = u64_to_limbs(result).to_vec();
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
    records[2].writes[0].1 = u64_to_limbs(999).to_vec(); // wrong quotient
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
    records[2].writes[1].1 = u64_to_limbs(2).to_vec(); // rem=2 but should be 1
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace).expect_err("wrong remainder should fail");
}
