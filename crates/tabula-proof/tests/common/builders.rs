use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use tabula_commitment::{ColumnMeta, CommitmentStrategy, NativeDigest};
use tabula_core::{ColId, TableId};
use tabula_proof::air::{CmpOp, InstructionRecord, Opcode, bool_fe, u64_to_limbs};

// ── Execution ──

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

// ── InterTxOrder ──

use tabula_proof::air::InterTxOrderRow;

fn ito_val(v: [u32; 3]) -> Vec<BabyBear> {
    v.iter().map(|x| BabyBear::new(*x)).collect()
}

/// Init row: base state seed for a key.
pub fn ito_init(t: u32, c: u16, key: u64, val: [u32; 3], is_null: bool) -> InterTxOrderRow {
    InterTxOrderRow {
        table_id: t,
        col_id: c,
        key,
        tx_index: 0,
        is_init: true,
        has_read: false,
        has_write: false,
        input_val: ito_val(val),
        input_is_null: is_null,
        output_val: ito_val(val),
        output_is_null: is_null,
    }
}

/// Read-only access row: tx reads input value, output = input.
pub fn ito_read(
    t: u32,
    c: u16,
    key: u64,
    tx_index: u32,
    input: [u32; 3],
    input_is_null: bool,
) -> InterTxOrderRow {
    InterTxOrderRow {
        table_id: t,
        col_id: c,
        key,
        tx_index,
        is_init: false,
        has_read: true,
        has_write: false,
        input_val: ito_val(input),
        input_is_null,
        output_val: ito_val(input), // read-only: output = input
        output_is_null: input_is_null,
    }
}

/// Write-only access row: tx writes without reading.
#[allow(clippy::too_many_arguments)]
pub fn ito_write(
    t: u32,
    c: u16,
    key: u64,
    tx_index: u32,
    input: [u32; 3],
    input_is_null: bool,
    output: [u32; 3],
    output_is_null: bool,
) -> InterTxOrderRow {
    InterTxOrderRow {
        table_id: t,
        col_id: c,
        key,
        tx_index,
        is_init: false,
        has_read: false,
        has_write: true,
        input_val: ito_val(input),
        input_is_null,
        output_val: ito_val(output),
        output_is_null,
    }
}

/// Read+write access row: tx reads and writes.
#[allow(clippy::too_many_arguments)]
pub fn ito_read_write(
    t: u32,
    c: u16,
    key: u64,
    tx_index: u32,
    input: [u32; 3],
    input_is_null: bool,
    output: [u32; 3],
    output_is_null: bool,
) -> InterTxOrderRow {
    InterTxOrderRow {
        table_id: t,
        col_id: c,
        key,
        tx_index,
        is_init: false,
        has_read: true,
        has_write: true,
        input_val: ito_val(input),
        input_is_null,
        output_val: ito_val(output),
        output_is_null,
    }
}

// ── StateColumn ──

use tabula_proof::air::StateColumnRow;
use tabula_proof::air::chips::state_column::trace::EntrySource;

fn sc_val(v: [u32; 3]) -> Vec<BabyBear> {
    v.iter().map(|x| BabyBear::new(*x)).collect()
}

fn sc_zeros() -> Vec<BabyBear> {
    vec![BabyBear::ZERO; 3]
}

/// Entry: old_only — key in old, not written. old_val=new_val. Both chains.
pub fn sc_old_only(t: u32, c: u16, key: u64, val: [u32; 3]) -> StateColumnRow {
    StateColumnRow {
        table_id: t,
        col_id: c,
        key,
        is_gap: false,
        source: EntrySource::OldOnly,
        old_val: sc_val(val),
        new_val: sc_val(val),
        segment_is_touched: false,
        old_hash_acc: [BabyBear::ZERO; 8],
        new_hash_acc: [BabyBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
    }
}

/// Entry: write_only — key not in old, newly written. New chain only.
pub fn sc_write_only(t: u32, c: u16, key: u64, val: [u32; 3]) -> StateColumnRow {
    StateColumnRow {
        table_id: t,
        col_id: c,
        key,
        is_gap: false,
        source: EntrySource::WriteOnly,
        old_val: sc_zeros(),
        new_val: sc_val(val),
        segment_is_touched: true,
        old_hash_acc: [BabyBear::ZERO; 8],
        new_hash_acc: [BabyBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
    }
}

/// Entry: both — key in old AND written. Both chains with different values.
pub fn sc_both(t: u32, c: u16, key: u64, old: [u32; 3], new: [u32; 3]) -> StateColumnRow {
    StateColumnRow {
        table_id: t,
        col_id: c,
        key,
        is_gap: false,
        source: EntrySource::Both,
        old_val: sc_val(old),
        new_val: sc_val(new),
        segment_is_touched: true,
        old_hash_acc: [BabyBear::ZERO; 8],
        new_hash_acc: [BabyBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
    }
}

/// Entry: delete — key in old, written as null. Old chain only.
pub fn sc_delete(t: u32, c: u16, key: u64, old: [u32; 3]) -> StateColumnRow {
    StateColumnRow {
        table_id: t,
        col_id: c,
        key,
        is_gap: false,
        source: EntrySource::Delete,
        old_val: sc_val(old),
        new_val: sc_zeros(),
        segment_is_touched: true,
        old_hash_acc: [BabyBear::ZERO; 8],
        new_hash_acc: [BabyBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
    }
}

/// Gap row — non-membership proof. No hash chains.
pub fn sc_gap(t: u32, c: u16, key: u64) -> StateColumnRow {
    StateColumnRow {
        table_id: t,
        col_id: c,
        key,
        is_gap: true,
        source: EntrySource::OldOnly, // ignored for gap
        old_val: sc_zeros(),
        new_val: sc_zeros(),
        segment_is_touched: false,
        old_hash_acc: [BabyBear::ZERO; 8],
        new_hash_acc: [BabyBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
    }
}

// ── Column Meta ──

pub fn meta_entry(
    table: u32,
    col: u16,
    touched: bool,
    com_old: NativeDigest,
    com_new: NativeDigest,
) -> ColumnMeta {
    ColumnMeta {
        table: TableId(table),
        col: ColId(col),
        tag: CommitmentStrategy::Ssmc,
        com_old,
        com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: touched,
    }
}

// ── Poseidon ──

pub fn poseidon_test_input(seed: u32) -> [BabyBear; 16] {
    core::array::from_fn(|i| BabyBear::new(seed + i as u32))
}

// ── Hash ──

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

// ── Mul ──

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

// ── DivMod ──

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

// ── Lookup ──

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
