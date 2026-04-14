use super::*;

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

/// Slot written count: extra slot_written flag should fail.
#[test]
fn slot_written_count_extra_write_fails() {
    let mut records = vec![make_read(0, 0, 0, 100, 42, false)];
    // Read should write exactly 1 slot. Claim 2 written.
    records[0].written_slots = vec![0, 1];
    records[0].writes.push((1, u64_to_limbs(0).to_vec(), false));
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
    let cols: &mut ExecutionCols<KoalaBear, W> =
        borrow_cols_mut(&mut trace.values[row2_offset..row2_offset + width]);
    // Clear current q_sel[2] = 1, set q_sel[3] = 1 (swap q and rem selectors)
    cols.divmod.q_sel[2] = KoalaBear::ZERO;
    cols.divmod.q_sel[3] = KoalaBear::ONE;
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
    use tabula_chips::execution::columns::ExecutionCols;

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
    let cols: &mut ExecutionCols<KoalaBear, W> =
        borrow_cols_mut(&mut trace.values[row2_offset..row2_offset + width]);

    // Zero out src2 limbs (pretend rhs=0).
    cols.src2_val[0] = KoalaBear::ZERO;
    cols.src2_val[1] = KoalaBear::ZERO;
    cols.src2_val[2] = KoalaBear::ZERO;

    // Set is_zero=1 (rhs "is" zero) — this makes the IsZero witness consistent
    // but triggers the final `assert_zero(gate * is_zero)` constraint.
    cols.divmod.rhs_iz.is_zero = KoalaBear::ONE;
    // Set inv=0 (consistent with val=0 → no inverse).
    cols.divmod.rhs_iz.inv = KoalaBear::ZERO;

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
    records[2].writes[1].1 = u64_to_limbs(3).to_vec(); // rem=3 instead of 1
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("rem=rhs must fail StrictIneq (rem < rhs) constraint");
}

// ── T4: Select null propagation ──

/// T4: Select where the write's is_null flag correctly propagates from selected branch.
///
/// When cond=1 (true branch), dst gets if_true's value. If if_true's slot
/// is not null, is_null must be false. We forge is_null=true to verify
/// it fails.
#[test]
fn select_null_propagation_forged_fails() {
    // Read non-null 42 into slot 0 (if_true), non-null 99 into slot 1 (if_false),
    // cond=1 into slot 2. Select → dst=42, is_null=false.
    let mut records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_read(1, 0, 0, 200, 99, false),
        make_read(2, 0, 0, 300, 1, false),
        make_select(3, 0, 1, 2, true, 42, 99),
    ];
    // Forge writes[0].is_null=true (should be false since if_true slot is not null).
    records[3].writes[0].2 = true;
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("forged is_null=true on select should fail operand linkage");
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
    debug_check(&ExecutionChip::<W>, &trace).expect("valid select with false cond should pass");
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
    records[2].writes[0].1 = u64_to_limbs(a.wrapping_mul(b)).to_vec();
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("u64::MAX * 2 overflow must fail no-overflow constraint");
}

// ── T14: Lookup forged null ──

/// T14: Lookup with is_null=true must fail because Lookup is a total function.
///
/// The constraint `assert_zero(slot_gate * slot_is_null[s])` in constrain_lookup
/// enforces that Lookup's destination slot is never null.
#[test]
fn lookup_forged_null_dst_fails() {
    let mut records = vec![make_lookup(0, 5, 0, 100, 42)];
    // Forge writes[0].is_null=true. Trace gen will set slot_is_null[0]=1 for this slot,
    // violating constrain_lookup's slot_is_null assertion.
    records[0].writes[0].2 = true;
    let trace = generate_execution_trace::<W>(&records);
    debug_check(&ExecutionChip::<W>, &trace)
        .expect_err("lookup with is_null=true must fail (total function, no null output)");
}

// ── Column width test ──

#[test]
fn standard_width() {
    assert_eq!(EXECUTION_STANDARD_WIDTH, 1012);
}
