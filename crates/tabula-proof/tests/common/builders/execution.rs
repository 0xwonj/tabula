use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use tabula_proof::air::{CmpOp, InstructionRecord, Opcode, bool_fe, u64_to_limbs};

pub fn make_add(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    src1: u64,
    src2: u64,
) -> InstructionRecord {
    let result = src1.wrapping_add(src2);
    InstructionRecord {
        opcode: Opcode::Add,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: u64_to_limbs(src1).to_vec(),
        src2_val: u64_to_limbs(src2).to_vec(),
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: u64_to_limbs(result).to_vec(),
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}

pub fn make_sub(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    src1: u64,
    src2: u64,
) -> InstructionRecord {
    let result = src1.wrapping_sub(src2);
    InstructionRecord {
        opcode: Opcode::Sub,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: u64_to_limbs(src1).to_vec(),
        src2_val: u64_to_limbs(src2).to_vec(),
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: u64_to_limbs(result).to_vec(),
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}

pub fn make_assert(src1_slot: usize, src_val: bool) -> InstructionRecord {
    InstructionRecord {
        opcode: Opcode::Assert,
        tx_index: 0,
        written_slots: vec![],
        src1_val: vec![bool_fe(src_val), BabyBear::ZERO, BabyBear::ZERO],
        src2_val: vec![BabyBear::ZERO; 3],
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
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
    InstructionRecord {
        opcode: Opcode::Select,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: u64_to_limbs(if_true).to_vec(),
        src2_val: u64_to_limbs(if_false).to_vec(),
        cond_val: cond,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: Some(cond_slot),
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: u64_to_limbs(result).to_vec(),
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}

pub fn make_read(
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
        src1_slot_idx: None,
        src2_slot_idx: None,
        cond_slot_idx: None,
        access_t: Some(table),
        access_c: Some(col),
        access_r: Some(row_key),
        access_val: Some(u64_to_limbs(val).to_vec()),
        access_is_null: Some(is_null),
        dst_val: u64_to_limbs(val).to_vec(),
        dst_is_null: is_null,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}

pub fn make_write(
    src1_slot: usize,
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
        src1_val: u64_to_limbs(val).to_vec(),
        src2_val: vec![BabyBear::ZERO; 3],
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: None,
        cond_slot_idx: None,
        access_t: Some(table),
        access_c: Some(col),
        access_r: Some(row_key),
        access_val: Some(u64_to_limbs(val).to_vec()),
        access_is_null: Some(is_null),
        dst_val: vec![],
        dst_is_null: is_null,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}

pub fn make_not(dst_slot: usize, src1_slot: usize, src: bool) -> InstructionRecord {
    InstructionRecord {
        opcode: Opcode::Not,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: vec![bool_fe(src), BabyBear::ZERO, BabyBear::ZERO],
        src2_val: vec![BabyBear::ZERO; 3],
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: None,
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: vec![bool_fe(!src), BabyBear::ZERO, BabyBear::ZERO],
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}

pub fn make_and(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    a: bool,
    b: bool,
) -> InstructionRecord {
    InstructionRecord {
        opcode: Opcode::And,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: vec![bool_fe(a), BabyBear::ZERO, BabyBear::ZERO],
        src2_val: vec![bool_fe(b), BabyBear::ZERO, BabyBear::ZERO],
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: vec![bool_fe(a && b), BabyBear::ZERO, BabyBear::ZERO],
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}

pub fn make_or(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    a: bool,
    b: bool,
) -> InstructionRecord {
    InstructionRecord {
        opcode: Opcode::Or,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: vec![bool_fe(a), BabyBear::ZERO, BabyBear::ZERO],
        src2_val: vec![bool_fe(b), BabyBear::ZERO, BabyBear::ZERO],
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: vec![bool_fe(a || b), BabyBear::ZERO, BabyBear::ZERO],
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
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
    InstructionRecord {
        opcode: Opcode::Cmp(cmp_op),
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: u64_to_limbs(src1).to_vec(),
        src2_val: u64_to_limbs(src2).to_vec(),
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: vec![bool_fe(result), BabyBear::ZERO, BabyBear::ZERO],
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
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
    // perm_input[8..16] = 0 (capacity)

    let (_rounds, perm_output_full) = poseidon2_permutation(perm_input);
    let perm_output: [BabyBear; 8] = core::array::from_fn(|i| perm_output_full[i]);

    // dst_val = first W=3 elements of digest
    let dst_val = vec![perm_output[0], perm_output[1], perm_output[2]];

    InstructionRecord {
        opcode: Opcode::Hash,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: src1_fe,
        src2_val: src2_fe,
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val,
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: Some(perm_input),
        hash_perm_output: Some(perm_output),
        is_empty_col: false,
    }
}

pub fn make_mul(
    dst_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    src1: u64,
    src2: u64,
) -> InstructionRecord {
    let result = src1.wrapping_mul(src2);
    InstructionRecord {
        opcode: Opcode::Mul,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: u64_to_limbs(src1).to_vec(),
        src2_val: u64_to_limbs(src2).to_vec(),
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: u64_to_limbs(result).to_vec(),
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}

pub fn make_divmod(
    q_slot: usize,
    r_slot: usize,
    src1_slot: usize,
    src2_slot: usize,
    lhs: u64,
    rhs: u64,
) -> InstructionRecord {
    let q = lhs / rhs;
    let rem = lhs % rhs;
    InstructionRecord {
        opcode: Opcode::DivMod,
        tx_index: 0,
        written_slots: vec![q_slot, r_slot],
        src1_val: u64_to_limbs(lhs).to_vec(),
        src2_val: u64_to_limbs(rhs).to_vec(),
        cond_val: false,
        src1_slot_idx: Some(src1_slot),
        src2_slot_idx: Some(src2_slot),
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: u64_to_limbs(q).to_vec(),
        dst_is_null: false,
        dst2_val: u64_to_limbs(rem).to_vec(),
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
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
    InstructionRecord {
        opcode: Opcode::Lookup,
        tx_index: 0,
        written_slots: vec![dst_slot],
        src1_val: vec![BabyBear::ZERO; 3],
        src2_val: vec![BabyBear::ZERO; 3],
        cond_val: false,
        src1_slot_idx: None,
        src2_slot_idx: None,
        cond_slot_idx: None,
        access_t: Some(table),
        access_c: Some(col),
        access_r: Some(row_key),
        access_val: Some(u64_to_limbs(val).to_vec()),
        access_is_null: Some(false),
        dst_val: u64_to_limbs(val).to_vec(),
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
}
