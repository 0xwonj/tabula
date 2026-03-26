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

/// Fixed logical value width used by the generic execution lane.
pub const EXECUTION_STANDARD_VALUE_WIDTH: usize = 3;

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
    /// Effect ordinal within the transaction (E-Trace identity anchor).
    pub effect_ordinal_in_tx: T,

    // ── Opcode one-hot selectors (14) ──
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
    /// Hash (digest relay to the dedicated IR-hash lane).
    pub op_hash: T,
    /// Lookup (static table query).
    pub op_lookup: T,
    /// Capability call (custom instruction).
    pub op_capability_call: T,
    /// PropertyRead (structural query on committed state).
    pub op_property_read: T,
    /// Relation lookup / static functional relation evaluation.
    pub op_relation_table: T,

    /// Capability transcript ID witness (populated when op_capability_call=1).
    pub capability_transcript_id: T,
    /// Instruction index within the tx body (populated when op_capability_call=1).
    pub instruction_index: T,
    /// Number of capability input values.
    pub capability_input_count: T,
    /// Number of capability output values / written slots.
    pub capability_output_count: T,
    /// Canonical transcript digest for the capability call.
    pub capability_event_digest: [T; 8],

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

    // ── Hash opcode ──
    /// Canonical IR-hash digest (first 8 KoalaBear elements of the final sponge state).
    pub hash_digest: [T; 8],

    // ── Mul opcode (M10-C1) ──
    /// Mul carry chain witnesses (5 cols).
    pub mul: MulCarry<T>,

    // ── DivMod opcode (M10-C2) ──
    /// DivMod witness: quotient selector + carry chain + remainder bound (36 cols).
    pub divmod: DivModWitness<T, MAX_SLOTS>,

    // ── PropertyRead opcode ──
    /// Query type discriminant (PropertyQueryKind ordinal, 0–5).
    pub property_query_type: T,
    /// First canonical query operand (encoded as canonical portable `u64`).
    pub property_query_arg0: [T; W],
    /// Second canonical query operand (encoded as canonical portable `u64`).
    pub property_query_arg1: [T; W],
    /// Result value field elements.
    pub property_result_val: [T; W],
    /// Result key as u64 limbs (W field elements).
    pub property_result_key: [T; W],
    /// Result null flag (boolean: 1 if no matching key found).
    pub property_result_is_null: T,
    /// One-hot: which slot receives the value result.
    pub property_val_sel: [T; MAX_SLOTS],
    /// One-hot: which slot receives the key result.
    pub property_key_sel: [T; MAX_SLOTS],

    // ── RelationProof opcode ──
    /// Whether this relation row is an `eval` (1) or `assert` (0).
    pub relation_is_eval: T,
    /// Relation identifier.
    pub relation_id: T,
    /// Canonical transcript digest for the input tuple.
    pub relation_input_digest: [T; 8],
    /// Canonical transcript digest for the output tuple.
    pub relation_output_digest: [T; 8],
    /// Prefix-boolean occupancy mask for input tuple positions.
    pub relation_input_used: [T; MAX_SLOTS],
    /// Type ids for input tuple positions.
    pub relation_input_type_ids: [T; MAX_SLOTS],
    /// Prefix-boolean occupancy mask for output tuple positions.
    pub relation_output_used: [T; MAX_SLOTS],
    /// Type ids for output tuple positions.
    pub relation_output_type_ids: [T; MAX_SLOTS],
    /// Input tuple values in canonical position order.
    pub relation_input_vals: [[T; W]; MAX_SLOTS],
    /// Output tuple values in canonical position order.
    pub relation_output_vals: [[T; W]; MAX_SLOTS],
    /// One-hot input slot selectors per tuple position.
    pub relation_input_sel: [[T; MAX_SLOTS]; MAX_SLOTS],
    /// One-hot output slot selectors per tuple position.
    pub relation_output_sel: [[T; MAX_SLOTS]; MAX_SLOTS],
}

/// Compute the width of ExecutionCols for a given value width.
pub const fn execution_width<const W: usize>() -> usize {
    num_cols::<ExecutionCols<u8, W>, u8>()
}

/// Total trace-column width for the standard generic execution lane (W=3).
pub const EXECUTION_STANDARD_WIDTH: usize = execution_width::<EXECUTION_STANDARD_VALUE_WIDTH>();
