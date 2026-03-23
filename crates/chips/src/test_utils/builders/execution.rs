//! Factory functions for `InstructionRecord` test data.
//!
//! Each function delegates to `InstructionBuilder` for a concise, readable body.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use crate::execution::{CmpOp, InstructionRecord, Opcode, u64_to_limbs};

use super::instruction_builder::{InstructionBuilder, bool_val};

/// Build an Add instruction record.
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
        .write(dst_slot, src1.wrapping_add(src2))
        .build()
}

/// Build a Sub instruction record.
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
        .write(dst_slot, src1.wrapping_sub(src2))
        .build()
}

/// Build an Assert instruction record.
pub fn make_assert(src1_slot: usize, src_val: bool) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Assert)
        .src1_fe(src1_slot, bool_val(src_val))
        .build()
}

/// Build a Select instruction record.
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
        .write(dst_slot, result)
        .build()
}

/// Build a Read instruction record.
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
        .write_fe(dst_slot, u64_to_limbs(val).to_vec(), is_null)
        .build()
}

/// Build a Write instruction record.
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
        .build()
}

/// Build a Not instruction record.
pub fn make_not(dst_slot: usize, src1_slot: usize, src: bool) -> InstructionRecord {
    InstructionBuilder::new(Opcode::Not)
        .written_slots(vec![dst_slot])
        .src1_fe(src1_slot, bool_val(src))
        .write_fe(dst_slot, bool_val(!src), false)
        .build()
}

/// Build an And instruction record.
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
        .write_fe(dst_slot, bool_val(a && b), false)
        .build()
}

/// Build an Or instruction record.
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
        .write_fe(dst_slot, bool_val(a || b), false)
        .build()
}

/// Build a Cmp instruction record.
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
        .write_fe(dst_slot, bool_val(result), false)
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
/// Computes a digest relay for a synthetic two-input hash opcode record.
pub fn make_hash(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    _domain_tag: u32,
    _n: u32,
    src1: [u32; 3],
    src2: [u32; 3],
) -> InstructionRecord {
    let src1_fe: Vec<KoalaBear> = src1.iter().map(|v| KoalaBear::new(*v)).collect();
    let src2_fe: Vec<KoalaBear> = src2.iter().map(|v| KoalaBear::new(*v)).collect();
    let digest = core::array::from_fn(|idx| {
        if idx < 3 {
            src1_fe[idx] + src2_fe[idx]
        } else {
            KoalaBear::ZERO
        }
    });
    let dst_val = vec![digest[0], digest[1], digest[2]];

    InstructionBuilder::new(Opcode::Hash)
        .written_slots(vec![dst_slot])
        .src1_fe(src1_slot, src1_fe)
        .src2_fe(src2_slot, src2_fe)
        .write_fe(dst_slot, dst_val, false)
        .hash_digest(digest)
        .build()
}

/// Build a Mul instruction record.
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
        .write(dst_slot, src1.wrapping_mul(src2))
        .build()
}

/// Build a DivMod instruction record.
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
        .write(q_slot, lhs / rhs)
        .write(r_slot, lhs % rhs)
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
        .write(dst_slot, val)
        .build()
}

/// Build a PropertyRead instruction record.
///
/// Populates access and property columns for structural queries.
#[allow(clippy::too_many_arguments)]
pub fn make_property_read(
    val_slot: usize,
    key_slot: usize,
    null_slot: usize,
    table: u32,
    col: u16,
    query_type: u8,
    query_arg0: u64,
    query_arg1: u64,
    result_val: Vec<KoalaBear>,
    result_key: Vec<KoalaBear>,
    is_null: bool,
) -> InstructionRecord {
    InstructionBuilder::new(Opcode::PropertyRead)
        .written_slots(vec![val_slot, key_slot, null_slot])
        .access(table, col, 0)
        .property_read(
            query_type,
            query_arg0,
            query_arg1,
            result_val.clone(),
            result_key.clone(),
            is_null,
        )
        .write_fe(val_slot, result_val, false)
        .write_fe(key_slot, result_key, false)
        .write_fe(null_slot, bool_val(is_null), false)
        .build()
}
