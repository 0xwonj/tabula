//! Factory functions and builders for memory-layer test data.
//!
//! Covers `MemoryShardRow`, `StateShardRow`, `ColumnMeta`, and Poseidon helpers.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_commitment::schemes::tags;
use tabula_commitment::{ColumnMeta, NativeDigest};
use tabula_core::{ColId, TableId};

use crate::shards::state::trace::EntrySource;

// ── Shared value helper ──

/// Convert a `[u32; 3]` array to `Vec<KoalaBear>`.
fn fe_vals(v: [u32; 3]) -> Vec<KoalaBear> {
    v.iter().map(|x| KoalaBear::new(*x)).collect()
}

fn fe_zeros() -> Vec<KoalaBear> {
    vec![KoalaBear::ZERO; 3]
}

// ── MemoryShard builder ──

use crate::shards::memory::MemoryShardRow;

/// Fluent builder for `MemoryShardRow`.
pub struct MemoryShardRowBuilder {
    inner: MemoryShardRow,
}

impl MemoryShardRowBuilder {
    /// Start with defaults: key 0, tx_index 0, not init, no read/write, zero values.
    pub fn new(key: u64) -> Self {
        Self {
            inner: MemoryShardRow {
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

    /// Set the transaction index.
    pub fn tx_index(mut self, idx: u32) -> Self {
        self.inner.tx_index = idx;
        self
    }

    /// Mark as init row.
    pub fn init(mut self) -> Self {
        self.inner.is_init = true;
        self
    }

    /// Mark as having a read.
    pub fn has_read(mut self) -> Self {
        self.inner.has_read = true;
        self
    }

    /// Mark as having a write.
    pub fn has_write(mut self) -> Self {
        self.inner.has_write = true;
        self
    }

    /// Set input value and null flag.
    pub fn input(mut self, val: [u32; 3], is_null: bool) -> Self {
        self.inner.input_val = fe_vals(val);
        self.inner.input_is_null = is_null;
        self
    }

    /// Set output value and null flag.
    pub fn output(mut self, val: [u32; 3], is_null: bool) -> Self {
        self.inner.output_val = fe_vals(val);
        self.inner.output_is_null = is_null;
        self
    }

    /// Consume the builder and produce the `MemoryShardRow`.
    pub fn build(self) -> MemoryShardRow {
        self.inner
    }
}

// ── MemoryShard factory functions ──

/// Init row for a memory shard.
pub fn ms_init(key: u64, val: [u32; 3], is_null: bool) -> MemoryShardRow {
    MemoryShardRowBuilder::new(key)
        .init()
        .input(val, is_null)
        .output(val, is_null)
        .build()
}

/// Read-only access row for a memory shard.
pub fn ms_read(key: u64, tx_index: u32, input: [u32; 3], input_is_null: bool) -> MemoryShardRow {
    MemoryShardRowBuilder::new(key)
        .tx_index(tx_index)
        .has_read()
        .input(input, input_is_null)
        .output(input, input_is_null)
        .build()
}

/// Write-only access row for a memory shard.
pub fn ms_write(
    key: u64,
    tx_index: u32,
    input: [u32; 3],
    input_is_null: bool,
    output: [u32; 3],
    output_is_null: bool,
) -> MemoryShardRow {
    MemoryShardRowBuilder::new(key)
        .tx_index(tx_index)
        .has_write()
        .input(input, input_is_null)
        .output(output, output_is_null)
        .build()
}

/// Read+write access row for a memory shard.
pub fn ms_read_write(
    key: u64,
    tx_index: u32,
    input: [u32; 3],
    input_is_null: bool,
    output: [u32; 3],
    output_is_null: bool,
) -> MemoryShardRow {
    MemoryShardRowBuilder::new(key)
        .tx_index(tx_index)
        .has_read()
        .has_write()
        .input(input, input_is_null)
        .output(output, output_is_null)
        .build()
}

// ── StateShard builder ──

use crate::shards::state::trace::StateShardRow;

/// Entry: old_only for state shard. old_val=new_val.
pub fn ss_old_only(key: u64, val: [u32; 3]) -> StateShardRow {
    StateShardRow {
        key,
        is_gap: false,
        source: EntrySource::OldOnly,
        old_val: fe_vals(val),
        new_val: fe_vals(val),
        segment_is_touched: false,
        old_hash_acc: [KoalaBear::ZERO; 8],
        new_hash_acc: [KoalaBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
        prev_old_key: 0,
        next_old_key: 0,
    }
}

/// Entry: write_only for state shard. New chain only.
pub fn ss_write_only(key: u64, val: [u32; 3]) -> StateShardRow {
    StateShardRow {
        key,
        is_gap: false,
        source: EntrySource::WriteOnly,
        old_val: fe_zeros(),
        new_val: fe_vals(val),
        segment_is_touched: true,
        old_hash_acc: [KoalaBear::ZERO; 8],
        new_hash_acc: [KoalaBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
        prev_old_key: 0,
        next_old_key: 0,
    }
}

/// Entry: both for state shard. Both chains with different values.
pub fn ss_both(key: u64, old: [u32; 3], new: [u32; 3]) -> StateShardRow {
    StateShardRow {
        key,
        is_gap: false,
        source: EntrySource::Both,
        old_val: fe_vals(old),
        new_val: fe_vals(new),
        segment_is_touched: true,
        old_hash_acc: [KoalaBear::ZERO; 8],
        new_hash_acc: [KoalaBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
        prev_old_key: 0,
        next_old_key: 0,
    }
}

/// Entry: delete for state shard. Old chain only.
pub fn ss_delete(key: u64, old: [u32; 3]) -> StateShardRow {
    StateShardRow {
        key,
        is_gap: false,
        source: EntrySource::Delete,
        old_val: fe_vals(old),
        new_val: fe_zeros(),
        segment_is_touched: true,
        old_hash_acc: [KoalaBear::ZERO; 8],
        new_hash_acc: [KoalaBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
        prev_old_key: 0,
        next_old_key: 0,
    }
}

/// Gap row for state shard.
pub fn ss_gap(key: u64) -> StateShardRow {
    StateShardRow {
        key,
        is_gap: true,
        source: EntrySource::OldOnly,
        old_val: fe_zeros(),
        new_val: fe_zeros(),
        segment_is_touched: false,
        old_hash_acc: [KoalaBear::ZERO; 8],
        new_hash_acc: [KoalaBear::ZERO; 8],
        read_mult: false,
        write_mult: false,
        prev_old_key: 0,
        next_old_key: 0,
    }
}

// ── Column Meta ──

/// Build a `ColumnMeta` entry for testing.
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
        tag: tags::SSMC,
        com_old,
        com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: touched,
    }
}

// ── Poseidon ──

/// Build a deterministic Poseidon test input from a seed.
pub fn poseidon_test_input(seed: u32) -> [KoalaBear; 16] {
    core::array::from_fn(|i| KoalaBear::new(seed + i as u32))
}
