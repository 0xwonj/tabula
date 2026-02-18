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
            let tau_val = clk as u64 + 1;
            cols.tau = BabyBear::new(clk + 1);
            cols.tau_limbs.populate(tau_val);
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
