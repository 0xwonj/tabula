//! Witness population functions for ExecutionChip trace generation.
//!
//! These functions fill specific column groups in `ExecutionCols` from
//! `InstructionRecord` data. They are called from the main trace generation
//! loop in `trace.rs`.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_gadgets::bool_fe;
use tabula_gadgets::integer::{MASK_30, SHIFT_30_U32};

use super::columns::ExecutionCols;
use super::trace::{CmpOp, InstructionRecord, Opcode};
use super::trace_utils::{babybear_to_u32, reconstruct_u64_from_limbs, u64_to_limbs};

/// Set the opcode one-hot selector for the given opcode.
pub(super) fn set_opcode_selectors<T: PrimeCharacteristicRing, const W: usize>(
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
        Opcode::Precompile => cols.op_precompile = T::ONE,
        Opcode::PropertyRead => cols.op_property_read = T::ONE,
    }
}

/// Populate carry columns for Add/Sub.
pub(super) fn populate_arith_carry<const W: usize>(
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
pub(super) fn populate_cmp_witness<const W: usize>(
    cols: &mut ExecutionCols<BabyBear, W>,
    rec: &InstructionRecord,
    cmp_op: CmpOp,
) {
    // Set cmp sub-selector
    match cmp_op {
        CmpOp::Eq => cols.cmp.is_eq = BabyBear::ONE,
        CmpOp::Ne => cols.cmp.is_ne = BabyBear::ONE,
        CmpOp::Lt => cols.cmp.is_lt = BabyBear::ONE,
        CmpOp::Lte => cols.cmp.is_lte = BabyBear::ONE,
        CmpOp::Gt => cols.cmp.is_gt = BabyBear::ONE,
        CmpOp::Gte => cols.cmp.is_gte = BabyBear::ONE,
    }

    // Reconstruct u64 operands from limbs
    let s1 = reconstruct_u64_from_limbs(&rec.src1_val);
    let s2 = reconstruct_u64_from_limbs(&rec.src2_val);

    let is_eq = s1 == s2;
    let is_lt = s1 < s2;

    cols.cmp.lt_witness = bool_fe(is_lt);
    cols.cmp.eq_witness = bool_fe(is_eq);

    // Per-limb IsZero for equality detection (avoids field reconstruction collision).
    let limb0_diff = rec.src1_val[0] - rec.src2_val[0];
    let limb1_diff = rec.src1_val.get(1).copied().unwrap_or(BabyBear::ZERO)
        - rec.src2_val.get(1).copied().unwrap_or(BabyBear::ZERO);
    let limb2_diff = rec.src1_val.get(2).copied().unwrap_or(BabyBear::ZERO)
        - rec.src2_val.get(2).copied().unwrap_or(BabyBear::ZERO);
    cols.cmp.eq_limb0_iz.populate(limb0_diff);
    cols.cmp.eq_limb1_iz.populate(limb1_diff);
    cols.cmp.eq_limb2_iz.populate(limb2_diff);

    // StrictIneq + halves + diff2 bits: only when not equal
    if !is_eq {
        let (a, b) = if is_lt { (s1, s2) } else { (s2, s1) };
        cols.cmp.ineq.populate(a, b);
        let gap = b - a - 1;
        let d0 = (gap & MASK_30) as u32;
        let d1 = ((gap >> 30) & MASK_30) as u32;
        let d2 = (gap >> 60) as u32;
        cols.cmp.ineq_diff0_halves.populate(d0);
        cols.cmp.ineq_diff1_halves.populate(d1);
        cols.cmp.ineq_diff2_bits.populate(d2);
    }
}

/// Populate Mul carry columns: carry chain for u64 multiplication.
pub(super) fn populate_mul_carry<const W: usize>(
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

    cols.mul.c0 = BabyBear::new(c0 as u32);
    cols.mul.c0_halves.populate(c0 as u32);
    cols.mul.c1_lo = BabyBear::new((c1 & 0xFFFF) as u32);
    cols.mul.c1_hi = BabyBear::new((c1 >> 16) as u32);
}

/// Populate DivMod columns: carry chain for q*rhs + remainder bound.
pub(super) fn populate_divmod<const W: usize>(
    cols: &mut ExecutionCols<BabyBear, W>,
    rec: &InstructionRecord,
) {
    if rec.src1_val.len() < 3 || rec.src2_val.len() < 3 || rec.dst_val.len() < 3 {
        return;
    }

    // lhs (src1) / rhs (src2) = q (first written slot) remainder r (second written slot)
    let lhs = reconstruct_u64_from_limbs(&rec.src1_val);
    let rhs = reconstruct_u64_from_limbs(&rec.src2_val);

    if rhs == 0 {
        // Non-zero divisor check will fail -- just populate IsZero witness
        cols.divmod.rhs_iz.populate(BabyBear::ZERO);
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

    cols.divmod.c0 = BabyBear::new(c0 as u32);
    cols.divmod.c0_halves.populate(c0 as u32);
    cols.divmod.c1_lo = BabyBear::new((c1 & 0xFFFF) as u32);
    cols.divmod.c1_hi = BabyBear::new((c1 >> 16) as u32);

    // Remainder bound: rem < rhs
    cols.divmod.rem_ineq.populate(rem, rhs);
    let gap = rhs - rem - 1;
    let d0_gap = (gap & MASK_30) as u32;
    let d1_gap = ((gap >> 30) & MASK_30) as u32;
    let d2_gap = (gap >> 60) as u32;
    cols.divmod.rem_diff0_halves.populate(d0_gap);
    cols.divmod.rem_diff1_halves.populate(d1_gap);
    cols.divmod.rem_diff2_bits.populate(d2_gap);

    // Non-zero divisor: IsZero on combined rhs
    let shift_30 = BabyBear::new(SHIFT_30_U32);
    let shift_60 = shift_30 * shift_30;
    let rhs_combined = rec.src2_val[0] + rec.src2_val[1] * shift_30 + rec.src2_val[2] * shift_60;
    cols.divmod.rhs_iz.populate(rhs_combined);
}
