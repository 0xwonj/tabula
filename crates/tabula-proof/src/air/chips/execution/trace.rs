//! Trace generation for the ExecutionChip.
//!
//! Converts instruction records into a `RowMajorMatrix<BabyBear>` trace.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;
use crate::air::gadgets::integer::{MASK_30, SHIFT_30_U32};

use super::columns::{ExecutionCols, MAX_SLOTS, execution_width};

/// Comparison sub-operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
}

/// Opcode discriminant for trace generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// Read from state.
    Read,
    /// Write to state.
    Write,
    /// Integer addition.
    Add,
    /// Integer subtraction.
    Sub,
    /// Integer multiplication.
    Mul,
    /// Integer division + modulo.
    DivMod,
    /// Comparison (with sub-operation).
    Cmp(CmpOp),
    /// Boolean NOT.
    Not,
    /// Boolean AND.
    And,
    /// Boolean OR.
    Or,
    /// Assert condition.
    Assert,
    /// Conditional select.
    Select,
    /// Poseidon hash.
    Hash,
    /// Static table lookup.
    Lookup,
}

/// Per-instruction witness record for trace generation.
///
/// The prover fills this from execution results. For M8 testing,
/// records are constructed manually.
#[derive(Debug, Clone)]
pub struct InstructionRecord {
    /// Opcode for this instruction.
    pub opcode: Opcode,
    /// Transaction index.
    pub tx_index: u32,
    /// Which slots this instruction writes to.
    pub written_slots: Vec<usize>,
    /// Source operand 1 values (W field elements).
    pub src1_val: Vec<BabyBear>,
    /// Source operand 2 values (W field elements).
    pub src2_val: Vec<BabyBear>,
    /// Condition value for Select (boolean).
    pub cond_val: bool,
    /// Which slot src1 reads from (for operand-to-slot linkage).
    pub src1_slot_idx: Option<usize>,
    /// Which slot src2 reads from (for operand-to-slot linkage).
    pub src2_slot_idx: Option<usize>,
    /// Which slot cond reads from (for operand-to-slot linkage).
    pub cond_slot_idx: Option<usize>,
    /// For access instructions: table id.
    pub access_t: Option<u32>,
    /// For access instructions: column id.
    pub access_c: Option<u16>,
    /// For access instructions: row key.
    pub access_r: Option<u64>,
    /// For access instructions: value (W field elements).
    pub access_val: Option<Vec<BabyBear>>,
    /// For access instructions: null flag.
    pub access_is_null: Option<bool>,
    /// Destination value for the written slot (W field elements).
    /// For Read: comes from access_val. For Arith: computed result.
    pub dst_val: Vec<BabyBear>,
    /// Destination null flag for the written slot.
    pub dst_is_null: bool,
    /// For DivMod: second destination value (remainder), written to second slot.
    pub dst2_val: Vec<BabyBear>,
    /// For DivMod: second destination null flag.
    pub dst2_is_null: bool,
    /// For Hash: precomputed Poseidon permutation input (16 FE).
    pub hash_perm_input: Option<[BabyBear; 16]>,
    /// For Hash: precomputed Poseidon permutation output (8 FE).
    pub hash_perm_output: Option<[BabyBear; 8]>,
}

/// Generate an ExecutionChip trace from instruction records.
///
/// Returns a power-of-2 padded `RowMajorMatrix`.
pub fn generate_execution_trace<const W: usize>(
    records: &[InstructionRecord],
) -> RowMajorMatrix<BabyBear> {
    let width = execution_width::<W>();
    let num_real = records.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    // Running state
    let mut slot_vals = [[BabyBear::ZERO; W]; MAX_SLOTS];
    let mut slot_nulls = [BabyBear::ZERO; MAX_SLOTS];
    let mut clk: u32 = 0;

    for (i, rec) in records.iter().enumerate() {
        let offset = i * width;
        let cols: &mut ExecutionCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        cols.is_real = BabyBear::ONE;
        cols.tx_index = BabyBear::new(rec.tx_index);

        // Set opcode one-hot
        set_opcode_selectors(cols, rec.opcode);

        let is_access = matches!(rec.opcode, Opcode::Read | Opcode::Write);
        cols.is_access = bool_fe(is_access);
        cols.clk = BabyBear::new(clk);

        // Populate access columns for Read, Write, and Lookup.
        // Only Read/Write set is_access and advance the clock.
        let uses_access_cols = matches!(rec.opcode, Opcode::Read | Opcode::Write | Opcode::Lookup);

        if is_access {
            let tau_val = clk as u64 + 1;
            cols.tau = BabyBear::new(clk + 1);
            cols.tau_rc.populate(tau_val);
            clk += 1;

            cols.access_is_write = bool_fe(matches!(rec.opcode, Opcode::Write));
        }

        if uses_access_cols {
            if let Some(t) = rec.access_t {
                cols.access_t = BabyBear::new(t);
            }
            if let Some(c) = rec.access_c {
                cols.access_c = BabyBear::new(c as u32);
            }
            if let Some(r) = rec.access_r {
                cols.access_r.populate(r);
            }
            if let Some(ref val) = rec.access_val {
                for (j, v) in val.iter().enumerate().take(W) {
                    cols.access_val[j] = *v;
                }
            }
            if let Some(is_null) = rec.access_is_null {
                cols.access_is_null = bool_fe(is_null);
            }
        }

        // Operand witness
        for (j, v) in rec.src1_val.iter().enumerate().take(W) {
            cols.src1_val[j] = *v;
        }
        for (j, v) in rec.src2_val.iter().enumerate().take(W) {
            cols.src2_val[j] = *v;
        }
        cols.cond_val = bool_fe(rec.cond_val);

        // Arith sub-selectors
        if rec.opcode == Opcode::Sub {
            cols.arith_is_sub = BabyBear::ONE;
        }
        if rec.opcode == Opcode::Mul {
            cols.arith_is_mul = BabyBear::ONE;
        }

        // Operand-to-slot selectors
        if let Some(s) = rec.src1_slot_idx {
            assert!(s < MAX_SLOTS, "src1_slot_idx {s} >= MAX_SLOTS");
            cols.src1_sel[s] = BabyBear::ONE;
            cols.src1_is_null = slot_nulls[s];
        }
        if let Some(s) = rec.src2_slot_idx {
            assert!(s < MAX_SLOTS, "src2_slot_idx {s} >= MAX_SLOTS");
            cols.src2_sel[s] = BabyBear::ONE;
        }
        if let Some(s) = rec.cond_slot_idx {
            assert!(s < MAX_SLOTS, "cond_slot_idx {s} >= MAX_SLOTS");
            cols.cond_sel[s] = BabyBear::ONE;
        }

        // Arithmetic carry (for Add/Sub)
        if matches!(rec.opcode, Opcode::Add | Opcode::Sub) && W >= 3 {
            populate_arith_carry(cols, rec);
        }

        // Mul carry (M10-C1)
        if rec.opcode == Opcode::Mul && W >= 3 {
            populate_mul_carry(cols, rec);
        }

        // DivMod carry + remainder bound (M10-C2)
        if rec.opcode == Opcode::DivMod && W >= 3 {
            populate_divmod(cols, rec);
            // Populate divmod_q_sel: first written slot is quotient
            if let Some(&q_slot) = rec.written_slots.first() {
                assert!(q_slot < MAX_SLOTS, "divmod q_slot {q_slot} >= MAX_SLOTS");
                cols.divmod_q_sel[q_slot] = BabyBear::ONE;
            }
        }

        // Cmp witness columns
        if let Opcode::Cmp(cmp_op) = rec.opcode {
            populate_cmp_witness(cols, rec, cmp_op);
        }

        // Hash permutation columns
        if rec.opcode == Opcode::Hash {
            if let Some(ref input) = rec.hash_perm_input {
                cols.hash_perm_input = *input;
            }
            if let Some(ref output) = rec.hash_perm_output {
                cols.hash_perm_output = *output;
            }
        }

        // Slot written flags
        for &s in &rec.written_slots {
            assert!(s < MAX_SLOTS, "slot index {s} >= MAX_SLOTS ({MAX_SLOTS})");
            cols.slot_written[s] = BabyBear::ONE;
        }

        // Update slot values for written slots
        if let Some(&first_slot) = rec.written_slots.first() {
            for (j, v) in rec.dst_val.iter().enumerate().take(W) {
                slot_vals[first_slot][j] = *v;
            }
            slot_nulls[first_slot] = bool_fe(rec.dst_is_null);
        }
        // DivMod second slot (remainder)
        if rec.written_slots.len() >= 2 && !rec.dst2_val.is_empty() {
            let second_slot = rec.written_slots[1];
            for (j, v) in rec.dst2_val.iter().enumerate().take(W) {
                slot_vals[second_slot][j] = *v;
            }
            slot_nulls[second_slot] = bool_fe(rec.dst2_is_null);
        }

        // Write all slot values to trace (carry + new writes)
        for s in 0..MAX_SLOTS {
            cols.slots[s][..W].copy_from_slice(&slot_vals[s][..W]);
            cols.slot_is_null[s] = slot_nulls[s];
        }
    }

    RowMajorMatrix::new(values, width)
}

/// Set the opcode one-hot selector for the given opcode.
fn set_opcode_selectors<T: PrimeCharacteristicRing, const W: usize>(
    cols: &mut ExecutionCols<T, W>,
    opcode: Opcode,
) {
    match opcode {
        Opcode::Read => cols.op_read = T::ONE,
        Opcode::Write => cols.op_write = T::ONE,
        Opcode::Add | Opcode::Sub | Opcode::Mul => cols.op_arith = T::ONE,
        Opcode::DivMod => cols.op_divmod = T::ONE,
        Opcode::Cmp(_) => cols.op_cmp = T::ONE,
        Opcode::Not => cols.op_not = T::ONE,
        Opcode::And => cols.op_and = T::ONE,
        Opcode::Or => cols.op_or = T::ONE,
        Opcode::Assert => cols.op_assert = T::ONE,
        Opcode::Select => cols.op_select = T::ONE,
        Opcode::Hash => cols.op_hash = T::ONE,
        Opcode::Lookup => cols.op_lookup = T::ONE,
    }
}

/// Populate carry columns for Add/Sub.
fn populate_arith_carry<const W: usize>(
    cols: &mut ExecutionCols<BabyBear, W>,
    rec: &InstructionRecord,
) {
    if rec.src1_val.len() < 3 || rec.src2_val.len() < 3 || rec.dst_val.len() < 3 {
        return;
    }

    // Extract raw u32 values from BabyBear
    let s1 = [
        babybear_to_u32(rec.src1_val[0]),
        babybear_to_u32(rec.src1_val[1]),
        babybear_to_u32(rec.src1_val[2]),
    ];
    let s2 = [
        babybear_to_u32(rec.src2_val[0]),
        babybear_to_u32(rec.src2_val[1]),
        babybear_to_u32(rec.src2_val[2]),
    ];

    match rec.opcode {
        Opcode::Add => {
            // Carry from limb additions
            let sum0 = s1[0] as u64 + s2[0] as u64;
            let c0 = if sum0 >= (1u64 << 30) { 1u32 } else { 0 };
            let sum1 = s1[1] as u64 + s2[1] as u64 + c0 as u64;
            let c1 = if sum1 >= (1u64 << 30) { 1u32 } else { 0 };
            cols.carry0 = BabyBear::new(c0);
            cols.carry1 = BabyBear::new(c1);
        }
        Opcode::Sub => {
            // Borrow from limb subtractions
            let b0 = if s1[0] < s2[0] { 1u32 } else { 0 };
            let eff1_src1 = s1[1] as i64 - b0 as i64;
            let b1 = if eff1_src1 < s2[1] as i64 { 1u32 } else { 0 };
            cols.carry0 = BabyBear::new(b0);
            cols.carry1 = BabyBear::new(b1);
        }
        _ => {}
    }
}

/// Populate Cmp witness columns: sub-selectors, lt/eq witnesses, inequality proof.
fn populate_cmp_witness<const W: usize>(
    cols: &mut ExecutionCols<BabyBear, W>,
    rec: &InstructionRecord,
    cmp_op: CmpOp,
) {
    // Set cmp sub-selector
    match cmp_op {
        CmpOp::Eq => cols.cmp_is_eq = BabyBear::ONE,
        CmpOp::Ne => cols.cmp_is_ne = BabyBear::ONE,
        CmpOp::Lt => cols.cmp_is_lt = BabyBear::ONE,
        CmpOp::Lte => cols.cmp_is_lte = BabyBear::ONE,
        CmpOp::Gt => cols.cmp_is_gt = BabyBear::ONE,
        CmpOp::Gte => cols.cmp_is_gte = BabyBear::ONE,
    }

    // Reconstruct u64 operands from limbs
    let s1 = reconstruct_u64_from_limbs(&rec.src1_val);
    let s2 = reconstruct_u64_from_limbs(&rec.src2_val);

    let is_eq = s1 == s2;
    let is_lt = s1 < s2;

    cols.cmp_lt_witness = bool_fe(is_lt);
    cols.cmp_eq_witness = bool_fe(is_eq);

    // Per-limb IsZero for equality detection (avoids field reconstruction collision).
    let limb0_diff = rec.src1_val[0] - rec.src2_val[0];
    let limb1_diff = rec.src1_val.get(1).copied().unwrap_or(BabyBear::ZERO)
        - rec.src2_val.get(1).copied().unwrap_or(BabyBear::ZERO);
    let limb2_diff = rec.src1_val.get(2).copied().unwrap_or(BabyBear::ZERO)
        - rec.src2_val.get(2).copied().unwrap_or(BabyBear::ZERO);
    cols.cmp_eq_limb0_iz.populate(limb0_diff);
    cols.cmp_eq_limb1_iz.populate(limb1_diff);
    cols.cmp_eq_limb2_iz.populate(limb2_diff);

    // StrictIneq + halves + diff2 bits: only when not equal
    if !is_eq {
        let (a, b) = if is_lt { (s1, s2) } else { (s2, s1) };
        cols.cmp_ineq.populate(a, b);
        let gap = b - a - 1;
        let d0 = (gap & MASK_30) as u32;
        let d1 = ((gap >> 30) & MASK_30) as u32;
        let d2 = (gap >> 60) as u32;
        cols.cmp_ineq_diff0_halves.populate(d0);
        cols.cmp_ineq_diff1_halves.populate(d1);
        cols.cmp_ineq_diff2_bits.populate(d2);
    }
}

/// Populate Mul carry columns: carry chain for u64 multiplication.
fn populate_mul_carry<const W: usize>(
    cols: &mut ExecutionCols<BabyBear, W>,
    rec: &InstructionRecord,
) {
    if rec.src1_val.len() < 3 || rec.src2_val.len() < 3 {
        return;
    }

    let a0 = babybear_to_u32(rec.src1_val[0]) as u64;
    let a1 = babybear_to_u32(rec.src1_val[1]) as u64;
    let b0 = babybear_to_u32(rec.src2_val[0]) as u64;
    let b1 = babybear_to_u32(rec.src2_val[1]) as u64;

    // T0 = a0*b0, carry0 = T0 >> 30
    let t0 = a0 * b0;
    let c0 = t0 >> 30;

    // T1 + c0, carry1 = (T1 + c0) >> 30
    let t1_plus_c0 = a0 * b1 + a1 * b0 + c0;
    let c1 = t1_plus_c0 >> 30;

    cols.mul_c0 = BabyBear::new(c0 as u32);
    cols.mul_c0_halves.populate(c0 as u32);
    cols.mul_c1_lo = BabyBear::new((c1 & 0xFFFF) as u32);
    cols.mul_c1_hi = BabyBear::new((c1 >> 16) as u32);
}

/// Populate DivMod columns: carry chain for q*rhs + remainder bound.
fn populate_divmod<const W: usize>(cols: &mut ExecutionCols<BabyBear, W>, rec: &InstructionRecord) {
    if rec.src1_val.len() < 3 || rec.src2_val.len() < 3 || rec.dst_val.len() < 3 {
        return;
    }

    // lhs (src1) / rhs (src2) = q (first written slot) remainder r (second written slot)
    let lhs = reconstruct_u64_from_limbs(&rec.src1_val);
    let rhs = reconstruct_u64_from_limbs(&rec.src2_val);

    if rhs == 0 {
        // Non-zero divisor check will fail — just populate IsZero witness
        cols.divmod_rhs_iz.populate(BabyBear::ZERO);
        return;
    }

    let q = lhs / rhs;
    let rem = lhs % rhs;

    // Carry chain for q * rhs + rem (matches AIR identity: q*d + rem = lhs)
    let q_limbs = u64_to_limbs(q);
    let q0 = babybear_to_u32(q_limbs[0]) as u64;
    let q1 = babybear_to_u32(q_limbs[1]) as u64;
    let d0 = babybear_to_u32(rec.src2_val[0]) as u64;
    let d1 = babybear_to_u32(rec.src2_val[1]) as u64;

    let rem_limbs = u64_to_limbs(rem);
    let rem0 = babybear_to_u32(rem_limbs[0]) as u64;
    let rem1 = babybear_to_u32(rem_limbs[1]) as u64;

    // AIR: q0*d0 + rem0 = l0 + c0 * 2^30
    let t0 = q0 * d0 + rem0;
    let c0 = t0 >> 30;

    // AIR: q0*d1 + q1*d0 + rem1 + c0 = l1 + c1 * 2^30
    let t1_plus_c0 = q0 * d1 + q1 * d0 + rem1 + c0;
    let c1 = t1_plus_c0 >> 30;

    cols.divmod_c0 = BabyBear::new(c0 as u32);
    cols.divmod_c0_halves.populate(c0 as u32);
    cols.divmod_c1_lo = BabyBear::new((c1 & 0xFFFF) as u32);
    cols.divmod_c1_hi = BabyBear::new((c1 >> 16) as u32);

    // Remainder bound: rem < rhs
    cols.divmod_rem_ineq.populate(rem, rhs);
    let gap = rhs - rem - 1;
    let d0_gap = (gap & MASK_30) as u32;
    let d1_gap = ((gap >> 30) & MASK_30) as u32;
    let d2_gap = (gap >> 60) as u32;
    cols.divmod_rem_diff0_halves.populate(d0_gap);
    cols.divmod_rem_diff1_halves.populate(d1_gap);
    cols.divmod_rem_diff2_bits.populate(d2_gap);

    // Non-zero divisor: IsZero on combined rhs
    let shift_30 = BabyBear::new(SHIFT_30_U32);
    let shift_60 = shift_30 * shift_30;
    let rhs_combined = rec.src2_val[0] + rec.src2_val[1] * shift_30 + rec.src2_val[2] * shift_60;
    cols.divmod_rhs_iz.populate(rhs_combined);
}

/// Reconstruct a u64 from limb-encoded BabyBear values.
fn reconstruct_u64_from_limbs(limbs: &[BabyBear]) -> u64 {
    use p3_field::PrimeField32;
    let l0 = limbs.first().map_or(0, |v| v.as_canonical_u32() as u64);
    let l1 = limbs.get(1).map_or(0, |v| v.as_canonical_u32() as u64);
    let l2 = limbs.get(2).map_or(0, |v| v.as_canonical_u32() as u64);
    l0 | (l1 << 30) | (l2 << 60)
}

/// Extract the canonical u32 value from a BabyBear element.
fn babybear_to_u32(x: BabyBear) -> u32 {
    // BabyBear stores values in Montgomery form internally.
    // Use the as_canonical_u32 method to get the actual value.
    use p3_field::PrimeField32;
    x.as_canonical_u32()
}

/// Helper: create limb-encoded BabyBear values from a u64.
pub fn u64_to_limbs(val: u64) -> [BabyBear; 3] {
    [
        BabyBear::new((val & MASK_30) as u32),
        BabyBear::new(((val >> 30) & MASK_30) as u32),
        BabyBear::new((val >> 60) as u32),
    ]
}

/// Helper: reconstruct u64 from limb BabyBear values.
pub fn limbs_to_u64(limbs: &[BabyBear; 3]) -> u64 {
    use p3_field::PrimeField32;
    let l0 = limbs[0].as_canonical_u32() as u64;
    let l1 = limbs[1].as_canonical_u32() as u64;
    let l2 = limbs[2].as_canonical_u32() as u64;
    l0 | (l1 << 30) | (l2 << 60)
}

/// Helper: add two u64 values and return result as limbs.
pub fn u64_add_limbs(a: u64, b: u64) -> [BabyBear; 3] {
    u64_to_limbs(a.wrapping_add(b))
}

/// Helper: subtract two u64 values and return result as limbs.
pub fn u64_sub_limbs(a: u64, b: u64) -> [BabyBear; 3] {
    u64_to_limbs(a.wrapping_sub(b))
}
