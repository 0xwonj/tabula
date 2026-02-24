//! Factory functions for `InstructionRecord` test data.
//!
//! Each function delegates to `InstructionBuilder` for a concise, readable body.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use tabula_proof::air::{CmpOp, InstructionRecord, Opcode};

use super::instruction_builder::{InstructionBuilder, bool_val};

pub fn make_add(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    src1: u64,
    src2: u64,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Add)
        .written_slots(vec![dst_slot])
        .src1(src1_slot, src1)
        .src2(src2_slot, src2)
        .dst_u64(src1.wrapping_add(src2))
        .build()
}

pub fn make_sub(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    src1: u64,
    src2: u64,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Sub)
        .written_slots(vec![dst_slot])
        .src1(src1_slot, src1)
        .src2(src2_slot, src2)
        .dst_u64(src1.wrapping_sub(src2))
        .build()
}

pub fn make_assert(src1_slot: usize, src_val: bool) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Assert)
        .src1_fe(src1_slot, bool_val(src_val))
        .build()
}

pub fn make_select(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    cond_slot: usize,
    cond: bool,
    if_true: u64,
    if_false: u64,
) -> InstructionRecord {
    let result = if cond { if_true } else { if_false };
    InstructionBuilder::new(Opcode::Select)
        .written_slots(vec![dst_slot])
        .src1(src1_slot, if_true)
        .src2(src2_slot, if_false)
        .cond(cond_slot, cond)
        .dst_u64(result)
        .build()
}

pub fn make_read(
    dst_slot: usize,
    table: u32,
    col: u16,
    row_key: u64,
    val: u64,
    is_null: bool,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Read)
        .written_slots(vec![dst_slot])
        .access(table, col, row_key)
        .access_val(val, is_null)
        .dst_u64(val)
        .dst_null(is_null)
        .build()
}

pub fn make_write(
    src1_slot: usize,
    table: u32,
    col: u16,
    row_key: u64,
    val: u64,
    is_null: bool,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Write)
        .src1(src1_slot, val)
        .access(table, col, row_key)
        .access_val(val, is_null)
        .dst_null(is_null)
        .build()
}

pub fn make_not(dst_slot: usize, src1_slot: usize, src: bool) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Not)
        .written_slots(vec![dst_slot])
        .src1_fe(src1_slot, bool_val(src))
        .dst_fe(bool_val(!src))
        .build()
}

pub fn make_and(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    a: bool,
    b: bool,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::And)
        .written_slots(vec![dst_slot])
        .src1_fe(src1_slot, bool_val(a))
        .src2_fe(src2_slot, bool_val(b))
        .dst_fe(bool_val(a && b))
        .build()
}

pub fn make_or(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    a: bool,
    b: bool,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Or)
        .written_slots(vec![dst_slot])
        .src1_fe(src1_slot, bool_val(a))
        .src2_fe(src2_slot, bool_val(b))
        .dst_fe(bool_val(a || b))
        .build()
}

pub fn make_cmp(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    cmp_op: CmpOp,
    src1: u64,
    src2: u64,
) -> InstructionRecord {
    let result = match cmp_op {
        CmpOp::Eq => src1 == src2,
        CmpOp::Ne => src1 != src2,
        CmpOp::Lt => src1 < src2,
        CmpOp::Lte => src1 <= src2,
        CmpOp::Gt => src1 > src2,
        CmpOp::Gte => src1 >= src2,
    };
    InstructionBuilder::new(Opcode::Cmp(cmp_op))
        .written_slots(vec![dst_slot])
        .src1(src1_slot, src1)
        .src2(src2_slot, src2)
        .dst_fe(bool_val(result))
        .build()
}

/// Composite helper: Read two values into slots, then Add.
///
/// Returns a 3-instruction sequence: Read(slot0), Read(slot1), Add(dst_slot).
pub fn make_read_then_add(
    slot0: usize,
    slot1: usize,
    dst_slot: usize,
    val1: u64,
    val2: u64,
) -> Vec<InstructionRecord> {
    vec![
        make_read(slot0, 0, 0, 100, val1, false),
        make_read(slot1, 0, 0, 200, val2, false),
        make_add(dst_slot, slot0, slot1, val1, val2),
    ]
}

/// Build a Hash instruction record.
///
/// Composes the Poseidon permutation input from domain_tag, n, src1, src2,
/// runs the actual permutation, and constructs the record.
pub fn make_hash(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    domain_tag: u32,
    n: u32,
    src1: [u32; 3],
    src2: [u32; 3],
) -> InstructionRecord {
    use tabula_proof::air::chips::poseidon::constants::poseidon2_permutation;

    let src1_fe: Vec<BabyBear> = src1.iter().map(|v| BabyBear::new(*v)).collect();
    let src2_fe: Vec<BabyBear> = src2.iter().map(|v| BabyBear::new(*v)).collect();

    // Compose permutation input
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[0] = BabyBear::new(domain_tag);
    perm_input[1] = BabyBear::new(n);
    perm_input[2] = src1_fe[0];
    perm_input[3] = src1_fe[1];
    perm_input[4] = src1_fe[2];
    perm_input[5] = src2_fe[0];
    perm_input[6] = src2_fe[1];
    perm_input[7] = src2_fe[2];

    let (_rounds, perm_output_full) = poseidon2_permutation(perm_input);
    let perm_output: [BabyBear; 8] = core::array::from_fn(|i| perm_output_full[i]);
    let dst_val = vec![perm_output[0], perm_output[1], perm_output[2]];

    InstructionBuilder::new(Opcode::Hash)
        .written_slots(vec![dst_slot])
        .src1_fe(src1_slot, src1_fe)
        .src2_fe(src2_slot, src2_fe)
        .dst_fe(dst_val)
        .hash_perm(perm_input, perm_output)
        .build()
}

pub fn make_mul(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    src1: u64,
    src2: u64,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Mul)
        .written_slots(vec![dst_slot])
        .src1(src1_slot, src1)
        .src2(src2_slot, src2)
        .dst_u64(src1.wrapping_mul(src2))
        .build()
}

pub fn make_divmod(
    q_slot: usize,
    r_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    lhs: u64,
    rhs: u64,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::DivMod)
        .written_slots(vec![q_slot, r_slot])
        .src1(src1_slot, lhs)
        .src2(src2_slot, rhs)
        .dst_u64(lhs / rhs)
        .dst2_u64(lhs % rhs)
        .build()
}

/// Build a Lookup instruction record.
///
/// Populates access columns for static table lookup.
pub fn make_lookup(
    dst_slot: usize,
    table: u32,
    col: u16,
    row_key: u64,
    val: u64,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Lookup)
        .written_slots(vec![dst_slot])
        .access(table, col, row_key)
        .access_val(val, false)
        .dst_u64(val)
        .build()
}
