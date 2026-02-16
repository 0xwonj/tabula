//! Trace generation for the ExecutionChip.
//!
//! Converts instruction records into a `RowMajorMatrix<BabyBear>` trace.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;
use crate::air::gadgets::integer::MASK_30;

use super::columns::{ExecutionCols, MAX_SLOTS, execution_width};

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
    /// Comparison.
    Cmp,
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
    let mut slot_vals = [[BabyBear::ZERO; 3]; MAX_SLOTS];
    let mut slot_nulls = [BabyBear::ZERO; MAX_SLOTS];
    let mut clk: u32 = 0;

    // Slot values need W-sized arrays but we use fixed 3 for Standard.
    // This is safe because W=3 for all current uses.
    const { assert!(W <= 3, "trace gen only supports W <= 3") };

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

        if is_access {
            cols.tau = BabyBear::new(clk + 1);
            clk += 1;

            // Access log
            if let Some(t) = rec.access_t {
                cols.access_t = BabyBear::new(t);
            }
            if let Some(c) = rec.access_c {
                cols.access_c = BabyBear::new(c as u32);
            }
            if let Some(r) = rec.access_r {
                cols.access_r.populate(r);
            }
            cols.access_is_write = bool_fe(matches!(rec.opcode, Opcode::Write));
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

        // Arithmetic carry (for Add/Sub)
        if matches!(rec.opcode, Opcode::Add | Opcode::Sub) && W >= 3 {
            populate_arith_carry(cols, rec);
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
        Opcode::Cmp => cols.op_cmp = T::ONE,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::debug::debug_check;

    const W: usize = 3;

    /// Create a minimal InstructionRecord for testing.
    fn make_add(dst_slot: usize, src1: u64, src2: u64) -> InstructionRecord {
        let result = src1.wrapping_add(src2);
        InstructionRecord {
            opcode: Opcode::Add,
            tx_index: 0,
            written_slots: vec![dst_slot],
            src1_val: u64_to_limbs(src1).to_vec(),
            src2_val: u64_to_limbs(src2).to_vec(),
            cond_val: false,
            access_t: None,
            access_c: None,
            access_r: None,
            access_val: None,
            access_is_null: None,
            dst_val: u64_to_limbs(result).to_vec(),
            dst_is_null: false,
        }
    }

    fn make_sub(dst_slot: usize, src1: u64, src2: u64) -> InstructionRecord {
        let result = src1.wrapping_sub(src2);
        InstructionRecord {
            opcode: Opcode::Sub,
            tx_index: 0,
            written_slots: vec![dst_slot],
            src1_val: u64_to_limbs(src1).to_vec(),
            src2_val: u64_to_limbs(src2).to_vec(),
            cond_val: false,
            access_t: None,
            access_c: None,
            access_r: None,
            access_val: None,
            access_is_null: None,
            dst_val: u64_to_limbs(result).to_vec(),
            dst_is_null: false,
        }
    }

    fn make_assert(src_val: bool) -> InstructionRecord {
        InstructionRecord {
            opcode: Opcode::Assert,
            tx_index: 0,
            written_slots: vec![],
            src1_val: vec![bool_fe(src_val), BabyBear::ZERO, BabyBear::ZERO],
            src2_val: vec![BabyBear::ZERO; 3],
            cond_val: false,
            access_t: None,
            access_c: None,
            access_r: None,
            access_val: None,
            access_is_null: None,
            dst_val: vec![],
            dst_is_null: false,
        }
    }

    fn make_select(dst_slot: usize, cond: bool, if_true: u64, if_false: u64) -> InstructionRecord {
        let result = if cond { if_true } else { if_false };
        InstructionRecord {
            opcode: Opcode::Select,
            tx_index: 0,
            written_slots: vec![dst_slot],
            src1_val: u64_to_limbs(if_true).to_vec(),
            src2_val: u64_to_limbs(if_false).to_vec(),
            cond_val: cond,
            access_t: None,
            access_c: None,
            access_r: None,
            access_val: None,
            access_is_null: None,
            dst_val: u64_to_limbs(result).to_vec(),
            dst_is_null: false,
        }
    }

    fn make_read(
        dst_slot: usize,
        table: u32,
        col: u16,
        row_key: u64,
        val: u64,
        is_null: bool,
    ) -> InstructionRecord {
        InstructionRecord {
            opcode: Opcode::Read,
            tx_index: 0,
            written_slots: vec![dst_slot],
            src1_val: vec![BabyBear::ZERO; 3],
            src2_val: vec![BabyBear::ZERO; 3],
            cond_val: false,
            access_t: Some(table),
            access_c: Some(col),
            access_r: Some(row_key),
            access_val: Some(u64_to_limbs(val).to_vec()),
            access_is_null: Some(is_null),
            dst_val: u64_to_limbs(val).to_vec(),
            dst_is_null: is_null,
        }
    }

    fn make_write(
        table: u32,
        col: u16,
        row_key: u64,
        val: u64,
        is_null: bool,
    ) -> InstructionRecord {
        InstructionRecord {
            opcode: Opcode::Write,
            tx_index: 0,
            written_slots: vec![],
            src1_val: vec![BabyBear::ZERO; 3],
            src2_val: vec![BabyBear::ZERO; 3],
            cond_val: false,
            access_t: Some(table),
            access_c: Some(col),
            access_r: Some(row_key),
            access_val: Some(u64_to_limbs(val).to_vec()),
            access_is_null: Some(is_null),
            dst_val: vec![],
            dst_is_null: false,
        }
    }

    // ── Valid trace tests ──

    #[test]
    fn single_add() {
        let records = vec![make_add(0, 100, 200)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("single add");
    }

    #[test]
    fn add_with_carry() {
        // Values that cause carry across limb boundaries
        let a = (1u64 << 30) - 1; // max limb0
        let b = 1u64;
        let records = vec![make_add(0, a, b)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("add with carry");
    }

    #[test]
    fn add_large_values() {
        let a = 1_000_000_000u64;
        let b = 2_000_000_000u64;
        let records = vec![make_add(0, a, b)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("add large");
    }

    #[test]
    fn single_sub() {
        let records = vec![make_sub(0, 300, 100)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("single sub");
    }

    #[test]
    fn sub_with_borrow() {
        // Force a borrow: limb0 of b > limb0 of a
        let a = 1u64 << 30; // limb0=0, limb1=1
        let b = 1u64; // limb0=1
        let records = vec![make_sub(0, a, b)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("sub with borrow");
    }

    #[test]
    fn assert_true() {
        let records = vec![make_assert(true)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("assert true");
    }

    #[test]
    fn select_true_branch() {
        let records = vec![make_select(0, true, 42, 99)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("select true");
    }

    #[test]
    fn select_false_branch() {
        let records = vec![make_select(0, false, 42, 99)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("select false");
    }

    #[test]
    fn read_then_write() {
        let records = vec![
            make_read(0, 1, 0, 100, 42, false),
            make_write(1, 0, 200, 42, false),
        ];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("read+write");
    }

    #[test]
    fn multi_instruction_sequence() {
        // Read value, add to it, assert result, write back
        let records = vec![
            make_read(0, 1, 0, 100, 10, false),
            make_add(1, 10, 20),
            make_assert(true),
            make_write(1, 0, 100, 30, false),
        ];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("multi-instr");
    }

    #[test]
    fn ssa_carry_across_rows() {
        // Write to slot 0, then slot 1 — slot 0 should carry forward
        let records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("SSA carry");
    }

    #[test]
    fn all_padding() {
        let records: Vec<InstructionRecord> = vec![];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect("all padding");
    }

    // ── Invalid trace tests ──

    #[test]
    fn invalid_wrong_add_result() {
        let mut records = vec![make_add(0, 100, 200)];
        // Corrupt the result
        records[0].dst_val = u64_to_limbs(999).to_vec();
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect_err("wrong add result");
    }

    #[test]
    fn invalid_assert_false() {
        let records = vec![make_assert(false)];
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace)
            .expect_err("assert false should fail");
    }

    #[test]
    fn invalid_wrong_select_result() {
        let mut records = vec![make_select(0, true, 42, 99)];
        // Should be 42 (true branch), corrupt to 99
        records[0].dst_val = u64_to_limbs(99).to_vec();
        let trace = generate_execution_trace::<W>(&records);
        debug_check(&super::super::air::ExecutionChip::<W>, &trace)
            .expect_err("wrong select result");
    }

    #[test]
    fn invalid_broken_slot_carry() {
        // Manually corrupt the trace to break slot carry
        let records = vec![make_add(0, 10, 20), make_add(1, 30, 40)];
        let mut trace = generate_execution_trace::<W>(&records);

        // Corrupt slot 0's value in row 1 (should carry 30 from row 0)
        let width = execution_width::<W>();
        let row1_offset = width;
        let cols: &mut ExecutionCols<BabyBear, W> =
            borrow_cols_mut(&mut trace.values[row1_offset..row1_offset + width]);
        cols.slots[0][0] = BabyBear::new(999); // break carry

        debug_check(&super::super::air::ExecutionChip::<W>, &trace).expect_err("broken slot carry");
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
}
