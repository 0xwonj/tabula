//! Column layout for the ExecutionChip AIR.
//!
//! One row per instruction. Columns encode:
//! - Opcode (one-hot selectors)
//! - Clock/timestamp (access counter)
//! - Access log (table, column, row key, value, null flag)
//! - Operand witness values (source values for opcode semantics)
//! - Operand-to-slot selectors (one-hot, constrained in M9-A1)
//! - SSA slots (Layout A: full carry)

use crate::air::columns::num_cols;
use crate::air::gadgets::{IsZero, KeyRangeChecked, Limb2Bits, LimbHalves, StrictIneq};

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

    // ── Clock & Timestamp ──
    /// 1 if this instruction accesses state (= op_read + op_write).
    pub is_access: T,
    /// Access counter (running count of access instructions so far).
    pub clk: T,
    /// Timestamp for memory bus: tau = clk + 1 when is_access=1.
    pub tau: T,
    /// Timestamp as u64 limbs + half-decomposition for range checks.
    /// Constrained: `is_access ⟹ tau = reconstruct(tau_rc.limbs)`.
    pub tau_rc: KeyRangeChecked<T>,

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
    /// Cmp sub-selector: Eq.
    pub cmp_is_eq: T,
    /// Cmp sub-selector: Ne.
    pub cmp_is_ne: T,
    /// Cmp sub-selector: Lt.
    pub cmp_is_lt: T,
    /// Cmp sub-selector: Lte.
    pub cmp_is_lte: T,
    /// Cmp sub-selector: Gt.
    pub cmp_is_gt: T,
    /// Cmp sub-selector: Gte.
    pub cmp_is_gte: T,
    /// 1 if src1 < src2 (ordering witness).
    pub cmp_lt_witness: T,
    /// 1 if src1 == src2 (equality witness).
    pub cmp_eq_witness: T,
    /// StrictIneq gap for ordering proof.
    pub cmp_ineq: StrictIneq<T>,
    /// Range check halves for cmp_ineq.diff0.
    pub cmp_ineq_diff0_halves: LimbHalves<T>,
    /// Range check halves for cmp_ineq.diff1.
    pub cmp_ineq_diff1_halves: LimbHalves<T>,
    /// 4-bit boolean decomposition of cmp_ineq.diff2 (proves diff2 ∈ [0, 16)).
    pub cmp_ineq_diff2_bits: Limb2Bits<T>,
    /// Per-limb IsZero for equality detection (avoids field reconstruction collision).
    /// Equality holds iff all three limb diffs are zero.
    pub cmp_eq_limb0_iz: IsZero<T>,
    /// IsZero for limb1 diff.
    pub cmp_eq_limb1_iz: IsZero<T>,
    /// IsZero for limb2 diff.
    pub cmp_eq_limb2_iz: IsZero<T>,

    // ── Hash opcode (M10-B2) ──
    /// Poseidon permutation input (16 field elements).
    pub hash_perm_input: [T; 16],
    /// Poseidon permutation output / digest (8 field elements).
    pub hash_perm_output: [T; 8],

    // ── Mul opcode (M10-C1) ──
    /// Carry from limb0 to limb1 in Mul (c0 ∈ [0, 2^30)).
    pub mul_c0: T,
    /// Half-decomposition of mul_c0 for range check.
    pub mul_c0_halves: LimbHalves<T>,
    /// Low part of carry from limb1 to limb2 (c1_lo ∈ [0, 2^16)).
    pub mul_c1_lo: T,
    /// High part of carry from limb1 to limb2 (c1_hi ∈ [0, 2^15)).
    pub mul_c1_hi: T,

    // ── DivMod opcode (M10-C2) ──
    /// One-hot selector: which slot holds the quotient.
    /// Derived: `r_sel[s] = slot_written[s] - divmod_q_sel[s]` gives remainder slot.
    pub divmod_q_sel: [T; MAX_SLOTS],
    /// Carry from product limb0 in DivMod (c0 ∈ [0, 2^30)).
    pub divmod_c0: T,
    /// Half-decomposition of divmod_c0 for range check.
    pub divmod_c0_halves: LimbHalves<T>,
    /// Low part of carry from product limb1 (c1_lo ∈ [0, 2^16)).
    pub divmod_c1_lo: T,
    /// High part of carry from product limb1 (c1_hi ∈ [0, 2^15)).
    pub divmod_c1_hi: T,
    /// StrictIneq gap for rem < rhs.
    pub divmod_rem_ineq: StrictIneq<T>,
    /// Range check halves for divmod_rem_ineq.diff0.
    pub divmod_rem_diff0_halves: LimbHalves<T>,
    /// Range check halves for divmod_rem_ineq.diff1.
    pub divmod_rem_diff1_halves: LimbHalves<T>,
    /// 4-bit boolean decomposition of divmod_rem_ineq.diff2 (proves diff2 ∈ [0, 16)).
    pub divmod_rem_diff2_bits: Limb2Bits<T>,
    /// IsZero for rhs ≠ 0 check.
    pub divmod_rhs_iz: IsZero<T>,
}

/// Compute the width of ExecutionCols for a given value width.
pub const fn execution_width<const W: usize>() -> usize {
    num_cols::<ExecutionCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const EXECUTION_STANDARD_WIDTH: usize = execution_width::<3>();
