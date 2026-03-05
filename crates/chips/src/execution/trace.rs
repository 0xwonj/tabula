//! Trace generation for the ExecutionChip.
//!
//! Converts instruction records into a `RowMajorMatrix<BabyBear>` trace.
//!
//! Utility functions live in `trace_utils.rs`; witness population helpers
//! live in `trace_witness.rs`.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_gadgets::bool_fe;
use tabula_stark::air::columns::borrow_cols_mut;

use super::columns::{ExecutionCols, MAX_SLOTS, execution_width};
use super::trace_witness::{
    populate_arith_carry, populate_cmp_witness, populate_divmod, populate_mul_carry,
    set_opcode_selectors,
};

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
    /// Effect ordinal within the transaction (increments on Read/Write only).
    pub effect_ordinal_in_tx: u32,
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
    /// For Read: whether the column being read is empty.
    pub is_empty_col: bool,
}

impl Default for InstructionRecord {
    fn default() -> Self {
        Self {
            opcode: Opcode::Add,
            tx_index: 0,
            effect_ordinal_in_tx: 0,
            written_slots: vec![],
            src1_val: vec![BabyBear::ZERO; 3],
            src2_val: vec![BabyBear::ZERO; 3],
            cond_val: false,
            src1_slot_idx: None,
            src2_slot_idx: None,
            cond_slot_idx: None,
            access_t: None,
            access_c: None,
            access_r: None,
            access_val: None,
            access_is_null: None,
            dst_val: vec![],
            dst_is_null: false,
            dst2_val: vec![],
            dst2_is_null: false,
            hash_perm_input: None,
            hash_perm_output: None,
            is_empty_col: false,
        }
    }
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
        cols.effect_ordinal_in_tx = BabyBear::new(rec.effect_ordinal_in_tx);

        // Set opcode one-hot
        set_opcode_selectors(cols, rec.opcode);

        let is_access = matches!(rec.opcode, Opcode::Read | Opcode::Write);
        cols.is_access = bool_fe(is_access);
        cols.clk = BabyBear::new(clk);

        // Populate access columns for Read, Write, and Lookup.
        // Only Read/Write set is_access and advance the clock.
        let uses_access_cols = matches!(rec.opcode, Opcode::Read | Opcode::Write | Opcode::Lookup);

        if is_access {
            clk += 1;
            cols.access_is_write = bool_fe(matches!(rec.opcode, Opcode::Write));
        }

        cols.is_empty_col = bool_fe(rec.is_empty_col);

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
                cols.divmod.q_sel[q_slot] = BabyBear::ONE;
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

// ── TraceGenerator impl ─────────────────────────────────────────────────────

use tabula_stark::trace::TraceGenerator;

impl<const W: usize> TraceGenerator for super::air::ExecutionChip<W> {
    type Input = [InstructionRecord];

    fn generate_trace(&self, input: &[InstructionRecord]) -> RowMajorMatrix<BabyBear> {
        generate_execution_trace::<W>(input)
    }
}

// ── TraceContributor impl ──────────────────────────────────────────────────

use crate::ChipSpec;
use tabula_core::error::TabulaError;
use tabula_stark::trace::contributor::{
    TraceContributor, TracePhase, WitnessStore, witness_labels,
};
use tabula_stark::trace::trace_map::TraceMap;

impl<const W: usize> TraceContributor for super::air::ExecutionChip<W> {
    fn phase(&self) -> TracePhase {
        TracePhase::Independent
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let records = store.get::<Vec<InstructionRecord>>(witness_labels::EXECUTION_RECORDS)?;
        let entry = self.build_entry(records);
        map.insert_entry(self.chip_id(), entry);
        Ok(())
    }
}
