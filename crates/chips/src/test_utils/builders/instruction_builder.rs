//! Fluent builder for `InstructionRecord`.
//!
//! Provides a chainable API so test factory functions can construct
//! records in 3-8 lines instead of 20-30 lines of struct-literal boilerplate.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use crate::execution::{InstructionRecord, Opcode, u64_to_limbs};
use tabula_gadgets::bool_fe;

/// Fluent builder for constructing `InstructionRecord` instances.
pub struct InstructionBuilder {
    inner: InstructionRecord,
}

impl InstructionBuilder {
    /// Start building a record with the given opcode; all other fields default.
    pub fn new(opcode: Opcode) -> Self {
        Self {
            inner: InstructionRecord {
                opcode,
                ..Default::default()
            },
        }
    }

    /// Set the transaction index.
    pub fn tx_index(mut self, idx: u32) -> Self {
        self.inner.tx_index = idx;
        self
    }

    /// Set the effect ordinal within the transaction.
    pub fn effect_ordinal(mut self, ordinal: u32) -> Self {
        self.inner.effect_ordinal_in_tx = ordinal;
        self
    }

    /// Set the list of slot indices written by this instruction.
    pub fn written_slots(mut self, slots: Vec<usize>) -> Self {
        self.inner.written_slots = slots;
        self
    }

    /// Set src1 operand: slot index + u64 value (converted to limbs).
    pub fn src1(mut self, slot: usize, val: u64) -> Self {
        self.inner.src1_slot_idx = Some(slot);
        self.inner.src1_val = u64_to_limbs(val).to_vec();
        self
    }

    /// Set src1 operand with raw BabyBear field elements.
    pub fn src1_fe(mut self, slot: usize, val: Vec<BabyBear>) -> Self {
        self.inner.src1_slot_idx = Some(slot);
        self.inner.src1_val = val;
        self
    }

    /// Set src2 operand: slot index + u64 value (converted to limbs).
    pub fn src2(mut self, slot: usize, val: u64) -> Self {
        self.inner.src2_slot_idx = Some(slot);
        self.inner.src2_val = u64_to_limbs(val).to_vec();
        self
    }

    /// Set src2 operand with raw BabyBear field elements.
    pub fn src2_fe(mut self, slot: usize, val: Vec<BabyBear>) -> Self {
        self.inner.src2_slot_idx = Some(slot);
        self.inner.src2_val = val;
        self
    }

    /// Set the condition operand for Select.
    pub fn cond(mut self, slot: usize, val: bool) -> Self {
        self.inner.cond_slot_idx = Some(slot);
        self.inner.cond_val = val;
        self
    }

    /// Set access columns (table, col, row key).
    pub fn access(mut self, t: u32, c: u16, r: u64) -> Self {
        self.inner.access_t = Some(t);
        self.inner.access_c = Some(c);
        self.inner.access_r = Some(r);
        self
    }

    /// Set access value as u64 (converted to limbs).
    pub fn access_val(mut self, val: u64, is_null: bool) -> Self {
        self.inner.access_val = Some(u64_to_limbs(val).to_vec());
        self.inner.access_is_null = Some(is_null);
        self
    }

    /// Add a slot write: u64 value (converted to limbs), not null.
    pub fn write(mut self, slot: usize, val: u64) -> Self {
        self.inner.writes.push((slot, u64_to_limbs(val).to_vec(), false));
        self
    }

    /// Add a slot write with raw BabyBear field elements.
    pub fn write_fe(mut self, slot: usize, val: Vec<BabyBear>, is_null: bool) -> Self {
        self.inner.writes.push((slot, val, is_null));
        self
    }

    /// Add a null slot write: u64 value with is_null = true.
    pub fn write_null(mut self, slot: usize, val: u64) -> Self {
        self.inner.writes.push((slot, u64_to_limbs(val).to_vec(), true));
        self
    }

    /// Set hash permutation input/output.
    pub fn hash_perm(mut self, input: [BabyBear; 16], output: [BabyBear; 8]) -> Self {
        self.inner.hash_perm_input = Some(input);
        self.inner.hash_perm_output = Some(output);
        self
    }

    /// Mark the column being read as empty.
    pub fn mark_empty_col(mut self) -> Self {
        self.inner.is_empty_col = true;
        self
    }

    /// Set the precompile identifier.
    pub fn precompile_id(mut self, id: u16) -> Self {
        self.inner.precompile_id = Some(id);
        self
    }

    /// Set PropertyRead columns.
    pub fn property_read(
        mut self,
        query_type: u8,
        result_val: Vec<BabyBear>,
        result_key: Vec<BabyBear>,
        is_null: bool,
    ) -> Self {
        self.inner.property_query_type = Some(query_type);
        self.inner.property_result_val = result_val;
        self.inner.property_result_key = result_key;
        self.inner.property_result_is_null = is_null;
        self
    }

    /// Consume the builder and produce the `InstructionRecord`.
    pub fn build(self) -> InstructionRecord {
        self.inner
    }
}

/// Helper: build a boolean BabyBear triple `[bool_fe(v), 0, 0]`.
pub fn bool_val(v: bool) -> Vec<BabyBear> {
    vec![bool_fe(v), BabyBear::ZERO, BabyBear::ZERO]
}
