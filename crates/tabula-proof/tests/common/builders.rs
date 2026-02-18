use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use tabula_commitment::{ColumnMeta, CommitmentStrategy, NativeDigest};
use tabula_core::{ColId, TableId};
use tabula_proof::air::{
    InstructionRecord, MergeRow, MergeSource, Opcode, SortedMemRow, SsmcEntry, bool_fe,
    u64_to_limbs,
};

// ── Sorted Memory ──

pub fn init_row(t: u32, c: u16, r: u64, val: [u32; 3], is_null: bool) -> SortedMemRow {
    SortedMemRow {
        table_id: t,
        col_id: c,
        row_key: r,
        timestamp: 0,
        is_init: true,
        is_write: false,
        val: val.iter().map(|v| BabyBear::new(*v)).collect(),
        val_is_null: is_null,
        meta_is_empty_old: false,
    }
}

pub fn read_row(t: u32, c: u16, r: u64, tau: u64, val: [u32; 3], is_null: bool) -> SortedMemRow {
    SortedMemRow {
        table_id: t,
        col_id: c,
        row_key: r,
        timestamp: tau,
        is_init: false,
        is_write: false,
        val: val.iter().map(|v| BabyBear::new(*v)).collect(),
        val_is_null: is_null,
        meta_is_empty_old: false,
    }
}

pub fn write_row(t: u32, c: u16, r: u64, tau: u64, val: [u32; 3], is_null: bool) -> SortedMemRow {
    SortedMemRow {
        table_id: t,
        col_id: c,
        row_key: r,
        timestamp: tau,
        is_init: false,
        is_write: true,
        val: val.iter().map(|v| BabyBear::new(*v)).collect(),
        val_is_null: is_null,
        meta_is_empty_old: false,
    }
}

// ── SSMC ──

pub fn ssmc_entry(t: u32, c: u16, key: u64, val: [u32; 3]) -> SsmcEntry {
    SsmcEntry {
        table_id: t,
        col_id: c,
        key,
        value: val.iter().map(|v| BabyBear::new(*v)).collect(),
        hash_acc: [BabyBear::ZERO; 8],
        mult_witness: false,
        segment_is_touched: false,
    }
}

// ── Merge ──

pub fn merge_val(v: [u32; 3]) -> Vec<BabyBear> {
    v.iter().map(|x| BabyBear::new(*x)).collect()
}

pub fn merge_zeros() -> Vec<BabyBear> {
    vec![BabyBear::ZERO; 3]
}

pub fn old_only_row(t: u32, c: u16, key: u64, old: [u32; 3]) -> MergeRow {
    MergeRow {
        table_id: t,
        col_id: c,
        key,
        source: MergeSource::OldOnly,
        old_val: merge_val(old),
        write_val: merge_zeros(),
        new_val: merge_val(old),
        in_new: true,
        hash_acc: [BabyBear::ZERO; 8],
    }
}

pub fn write_only_row(t: u32, c: u16, key: u64, write: [u32; 3]) -> MergeRow {
    MergeRow {
        table_id: t,
        col_id: c,
        key,
        source: MergeSource::WriteOnly,
        old_val: merge_zeros(),
        write_val: merge_val(write),
        new_val: merge_val(write),
        in_new: true,
        hash_acc: [BabyBear::ZERO; 8],
    }
}

pub fn both_row(t: u32, c: u16, key: u64, old: [u32; 3], write: [u32; 3]) -> MergeRow {
    MergeRow {
        table_id: t,
        col_id: c,
        key,
        source: MergeSource::Both,
        old_val: merge_val(old),
        write_val: merge_val(write),
        new_val: merge_val(write),
        in_new: true,
        hash_acc: [BabyBear::ZERO; 8],
    }
}

pub fn delete_row(t: u32, c: u16, key: u64, old: [u32; 3]) -> MergeRow {
    MergeRow {
        table_id: t,
        col_id: c,
        key,
        source: MergeSource::Delete,
        old_val: merge_val(old),
        write_val: merge_zeros(),
        new_val: merge_zeros(),
        in_new: false,
        hash_acc: [BabyBear::ZERO; 8],
    }
}

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
