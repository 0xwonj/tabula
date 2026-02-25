//! Factory functions and builders for memory-layer test data.
//!
//! Covers `InterTxOrderRow`, `StateColumnRow`, `ColumnMeta`, and Poseidon helpers.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use tabula_commitment::{ColumnMeta, CommitmentStrategy, NativeDigest};
use tabula_core::{ColId, TableId};
use tabula_proof::chips::inter_tx_order::InterTxOrderRow;
use tabula_proof::chips::state_column::StateColumnRow;
use tabula_proof::chips::state_column::trace::EntrySource;

// ── Shared value helper ──

/// Convert a `[u32; 3]` array to `Vec<BabyBear>`.
fn fe_vals(v: [u32; 3]) -> Vec<BabyBear> {
    v.iter().map(|x| BabyBear::new(*x)).collect()
}

fn fe_zeros() -> Vec<BabyBear> {
    vec![BabyBear::ZERO; 3]
}

// ── InterTxOrder builder ──

/// Fluent builder for `InterTxOrderRow`.
pub struct InterTxOrderRowBuilder {
    inner: InterTxOrderRow,
}

impl InterTxOrderRowBuilder {
    /// Start with defaults: table 0, col 0, key 0, tx_index 0, not init, no read/write, zero values.
    pub fn new(t: u32, c: u16, key: u64) -> Self {
        Self {
            inner: InterTxOrderRow {
                table_id: t,
                col_id: c,
                key,
                tx_index: 0,
                is_init: false,
                has_read: false,
                has_write: false,
                input_val: fe_zeros(),
                input_is_null: false,
                output_val: fe_zeros(),
                output_is_null: false,
            },
        }
    }

    pub fn tx_index(mut self, idx: u32) -> Self {
        self.inner.tx_index = idx;
        self
    }

    pub fn init(mut self) -> Self {
        self.inner.is_init = true;
        self
    }

    pub fn has_read(mut self) -> Self {
        self.inner.has_read = true;
        self
    }

    pub fn has_write(mut self) -> Self {
        self.inner.has_write = true;
        self
    }

    pub fn input(mut self, val: [u32; 3], is_null: bool) -> Self {
        self.inner.input_val = fe_vals(val);
        self.inner.input_is_null = is_null;
        self
    }

    pub fn output(mut self, val: [u32; 3], is_null: bool) -> Self {
        self.inner.output_val = fe_vals(val);
        self.inner.output_is_null = is_null;
        self
    }

    pub fn build(self) -> InterTxOrderRow {
        self.inner
    }
}

// ── InterTxOrder factory functions ──

/// Init row: base state seed for a key.
pub fn ito_init(t: u32, c: u16, key: u64, val: [u32; 3], is_null: bool) -> InterTxOrderRow {
    InterTxOrderRowBuilder::new(t, c, key)
        .init()
        .input(val, is_null)
        .output(val, is_null)
        .build()
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
    InterTxOrderRowBuilder::new(t, c, key)
        .tx_index(tx_index)
        .has_read()
        .input(input, input_is_null)
        .output(input, input_is_null)
        .build()
}

/// Write-only access row: tx writes without reading.
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
    InterTxOrderRowBuilder::new(t, c, key)
        .tx_index(tx_index)
        .has_write()
        .input(input, input_is_null)
        .output(output, output_is_null)
        .build()
}

/// Read+write access row: tx reads and writes.
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
    InterTxOrderRowBuilder::new(t, c, key)
        .tx_index(tx_index)
        .has_read()
        .has_write()
        .input(input, input_is_null)
        .output(output, output_is_null)
        .build()
}

// ── StateColumn builder ──

/// Fluent builder for `StateColumnRow`.
pub struct StateColumnRowBuilder {
    inner: StateColumnRow,
}

impl StateColumnRowBuilder {
    /// Start with defaults: not gap, OldOnly source, zero values, not touched, no multiplicities.
    pub fn new(t: u32, c: u16, key: u64) -> Self {
        Self {
            inner: StateColumnRow {
                table_id: t,
                col_id: c,
                key,
                is_gap: false,
                source: EntrySource::OldOnly,
                old_val: fe_zeros(),
                new_val: fe_zeros(),
                segment_is_touched: false,
                old_hash_acc: [BabyBear::ZERO; 8],
                new_hash_acc: [BabyBear::ZERO; 8],
                read_mult: false,
                write_mult: false,
            },
        }
    }

    pub fn source(mut self, source: EntrySource) -> Self {
        self.inner.source = source;
        self
    }

    pub fn gap(mut self) -> Self {
        self.inner.is_gap = true;
        self
    }

    pub fn old_val(mut self, val: [u32; 3]) -> Self {
        self.inner.old_val = fe_vals(val);
        self
    }

    pub fn new_val(mut self, val: [u32; 3]) -> Self {
        self.inner.new_val = fe_vals(val);
        self
    }

    pub fn touched(mut self) -> Self {
        self.inner.segment_is_touched = true;
        self
    }

    pub fn build(self) -> StateColumnRow {
        self.inner
    }
}

// ── StateColumn factory functions ──

/// Entry: old_only -- key in old, not written. old_val=new_val. Both chains.
pub fn sc_old_only(t: u32, c: u16, key: u64, val: [u32; 3]) -> StateColumnRow {
    StateColumnRowBuilder::new(t, c, key)
        .source(EntrySource::OldOnly)
        .old_val(val)
        .new_val(val)
        .build()
}

/// Entry: write_only -- key not in old, newly written. New chain only.
pub fn sc_write_only(t: u32, c: u16, key: u64, val: [u32; 3]) -> StateColumnRow {
    StateColumnRowBuilder::new(t, c, key)
        .source(EntrySource::WriteOnly)
        .new_val(val)
        .touched()
        .build()
}

/// Entry: both -- key in old AND written. Both chains with different values.
pub fn sc_both(t: u32, c: u16, key: u64, old: [u32; 3], new: [u32; 3]) -> StateColumnRow {
    StateColumnRowBuilder::new(t, c, key)
        .source(EntrySource::Both)
        .old_val(old)
        .new_val(new)
        .touched()
        .build()
}

/// Entry: delete -- key in old, written as null. Old chain only.
pub fn sc_delete(t: u32, c: u16, key: u64, old: [u32; 3]) -> StateColumnRow {
    StateColumnRowBuilder::new(t, c, key)
        .source(EntrySource::Delete)
        .old_val(old)
        .touched()
        .build()
}

/// Gap row -- non-membership proof. No hash chains.
pub fn sc_gap(t: u32, c: u16, key: u64) -> StateColumnRow {
    StateColumnRowBuilder::new(t, c, key).gap().build()
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
