use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use tabula_commitment::{ColumnMeta, CommitmentStrategy, NativeDigest};
use tabula_core::{ColId, TableId};
use tabula_proof::air::chips::state_column::trace::EntrySource;
use tabula_proof::air::{InterTxOrderRow, StateColumnRow};

// ── InterTxOrder ──

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
