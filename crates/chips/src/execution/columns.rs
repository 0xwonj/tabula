//! Column layout for the ExecutionChip AIR.
//!
//! One row per instruction. Columns encode:
//! - Opcode (one-hot selectors)
//! - Clock/timestamp (access counter)
//! - Access log (table, column, row key, value, null flag)
//! - Operand witness values (source values for opcode semantics)
//! - Operand-to-slot selectors (one-hot, constrained in M9-A1)
//! - SSA slots (Layout A: full carry)

use tabula_gadgets::KeyRangeChecked;
use tabula_stark::air::columns::num_cols;

use super::ops::cmp::CmpWitness;
use super::ops::divmod::DivModWitness;
use super::ops::mul::MulCarry;

/// Maximum number of SSA slots per program.
///
/// Must be ≥ `ProgramBudgets.max_slots` for any registered program.
/// 16 covers the expected upper bound for M8; future phases may make this dynamic.
pub const MAX_SLOTS: usize = 16;

/// Column layout for the ExecutionChip AIR.
///
/// Generic over `W` (value width in field elements).
/// Standard width: W=3 (U64/I64).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct ExecutionCols<T, const W: usize> {
    // ── Control ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Transaction index within the batch.
    pub tx_index: T,

    // ── Opcode one-hot selectors (12) ──
    // Note: Emit is intentionally omitted — it is out-of-protocol (semantics-spec §2.8)
    // and produces no AIR constraints.
    /// Read from state.
    pub op_read: T,
    /// Write to state.
    pub op_write: T,
    /// Arithmetic (Add/Sub/Mul, distinguished by sub-selectors).
    pub op_arith: T,
    /// Integer division + modulo.
    pub op_divmod: T,
    /// Comparison (Eq/Neq/Lt/Lte/Gt/Gte).
    pub op_cmp: T,
    /// Boolean NOT.
    pub op_not: T,
    /// Boolean AND.
    pub op_and: T,
    /// Boolean OR.
    pub op_or: T,
    /// Assert (tx fails if condition is false).
    pub op_assert: T,
    /// Select (conditional value).
    pub op_select: T,
    /// Hash (Poseidon permutation).
    pub op_hash: T,
    /// Lookup (static table query).
    pub op_lookup: T,

    // ── Arith sub-selectors (gated by op_arith) ──
    /// 1 if Sub, 0 otherwise.
    pub arith_is_sub: T,
    /// 1 if Mul, 0 otherwise.
    pub arith_is_mul: T,

    // ── Clock & Access flags ──
    /// 1 if this instruction accesses state (= op_read + op_write).
    pub is_access: T,
    /// Access counter (running count of access instructions so far).
    pub clk: T,
    /// 1 if reading from an empty column (implies op_read and access_is_null).
    pub is_empty_col: T,

    // ── Access log (populated when is_access=1, zeros otherwise) ──
    /// Table identifier for the access.
    pub access_t: T,
    /// Column identifier for the access.
    pub access_c: T,
    /// Row key for the access (u64 limbs + half-decomposition for range checks).
    pub access_r: KeyRangeChecked<T>,
    /// 1 if Write, 0 if Read (when is_access=1).
    pub access_is_write: T,
    /// Value field elements for the access.
    pub access_val: [T; W],
    /// Null flag for the access value.
    pub access_is_null: T,

    // ── Operand witness values ──
    // These carry source operand values for opcode semantics constraints.
    // Slot linkage (proving these match actual slot values) deferred to M9.
    /// First source operand value (Arith lhs, Select if_true, Assert check).
    pub src1_val: [T; W],
    /// Second source operand value (Arith rhs, Select if_false).
    pub src2_val: [T; W],
    /// Condition value for Select (single boolean field element).
    pub cond_val: T,

    // ── Operand-to-slot selectors (M9 A1) ──
    /// One-hot: which slot src1 reads from.
    pub src1_sel: [T; MAX_SLOTS],
    /// One-hot: which slot src2 reads from.
    pub src2_sel: [T; MAX_SLOTS],
    /// One-hot: which slot cond reads from.
    pub cond_sel: [T; MAX_SLOTS],
    /// Null flag for src1 operand (must match slot_is_null of selected slot).
    pub src1_is_null: T,

    // ── Arithmetic carry columns ──
    /// Carry from limb0 to limb1 in integer Add/Sub.
    pub carry0: T,
    /// Carry from limb1 to limb2 in integer Add/Sub.
    pub carry1: T,

    // ── SSA Slots (Layout A: full carry) ──
    /// Slot values: `slots[s][i]` = limb i of slot s.
    pub slots: [[T; W]; MAX_SLOTS],
    /// Null flag per slot.
    pub slot_is_null: [T; MAX_SLOTS],
    /// 1 if this instruction writes to slot s.
    pub slot_written: [T; MAX_SLOTS],

    // ── Cmp opcode (M10-B1) ──
    /// Cmp witness: sub-selectors + ordering/equality proof (27 cols).
    pub cmp: CmpWitness<T>,

    // ── Hash opcode (M10-B2) ──
    /// Poseidon permutation input (16 field elements).
    pub hash_perm_input: [T; 16],
    /// Poseidon permutation output / digest (8 field elements).
    pub hash_perm_output: [T; 8],

    // ── Mul opcode (M10-C1) ──
    /// Mul carry chain witnesses (5 cols).
    pub mul: MulCarry<T>,

    // ── DivMod opcode (M10-C2) ──
    /// DivMod witness: quotient selector + carry chain + remainder bound (36 cols).
    pub divmod: DivModWitness<T, MAX_SLOTS>,
}

/// Compute the width of ExecutionCols for a given value width.
pub const fn execution_width<const W: usize>() -> usize {
    num_cols::<ExecutionCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const EXECUTION_STANDARD_WIDTH: usize = execution_width::<3>();
