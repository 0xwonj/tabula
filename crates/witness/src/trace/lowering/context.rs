use std::collections::BTreeMap;
use std::collections::BTreeSet;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::BabyBearCodec;
use tabula_core::error::TabulaError;
use tabula_core::traits::{StaticTableProvider, ValueCodec};
use tabula_core::{ColId, ExecutionEvent, RowKey, TableId, Value};
use tabula_ir::{RowExpr, ValueExpr};

use tabula_chips::execution::MAX_SLOTS;
use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::static_table::trace::StaticTableRow;

/// Mutable lowering context threaded through per-opcode lowering functions.
///
/// Holds slot state, accumulated records, and shared immutable references
/// needed by all opcode lowering handlers.
pub(super) struct LoweringContext<'a, const W: usize> {
    // ── Slot state ──────────────────────────────────────────────────────
    /// Value-level slot contents (for resolution).
    pub(super) slots: Vec<Option<Value>>,
    /// Encoded slot contents (BabyBear FEs for trace).
    pub(super) slot_fes: Vec<Vec<BabyBear>>,
    /// Slot null flags.
    pub(super) slot_nulls: Vec<bool>,
    /// Tracks which slots have been explicitly written within this tx.
    pub(super) slot_initialized: Vec<bool>,
    /// Next available slot (how many are in use).
    pub(super) max_slot: usize,

    // ── Per-instruction tracking ────────────────────────────────────────
    /// Effect ordinal counter (increments on Read/Write).
    pub(super) effect_ordinal: u32,
    /// Transaction index.
    pub(super) tx_index: u32,

    // ── Accumulated output ──────────────────────────────────────────────
    /// Instruction records.
    pub(super) records: Vec<InstructionRecord>,
    /// Static table rows from Lookup instructions.
    pub(super) static_rows: Vec<StaticTableRow>,

    // ── Shared immutable references ─────────────────────────────────────
    /// Execution events for this tx.
    pub(super) tx_events: &'a [&'a ExecutionEvent],
    /// Schema type map: (table, col) -> ValueType.
    pub(super) type_map: &'a BTreeMap<(TableId, ColId), tabula_core::ValueType>,
    /// Static table provider.
    pub(super) static_tables: &'a dyn StaticTableProvider,
    /// Empty columns set.
    pub(super) empty_columns: &'a BTreeSet<(TableId, ColId)>,
    /// Transaction parameters.
    pub(super) params: &'a [Value],
    /// Value codec.
    pub(super) codec: &'a BabyBearCodec,
}

impl<'a, const W: usize> LoweringContext<'a, W> {
    /// Create a new context with zeroed slot state.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        tx_index: u32,
        tx_events: &'a [&'a ExecutionEvent],
        type_map: &'a BTreeMap<(TableId, ColId), tabula_core::ValueType>,
        static_tables: &'a dyn StaticTableProvider,
        empty_columns: &'a BTreeSet<(TableId, ColId)>,
        params: &'a [Value],
        codec: &'a BabyBearCodec,
        num_instructions: usize,
    ) -> Self {
        Self {
            slots: vec![None; MAX_SLOTS],
            slot_fes: vec![vec![BabyBear::ZERO; W]; MAX_SLOTS],
            slot_nulls: vec![false; MAX_SLOTS],
            slot_initialized: vec![false; MAX_SLOTS],
            max_slot: 0,
            effect_ordinal: 0,
            tx_index,
            records: Vec::with_capacity(num_instructions),
            static_rows: Vec::new(),
            tx_events,
            type_map,
            static_tables,
            empty_columns,
            params,
            codec,
        }
    }

    /// Resolve a `RowExpr` to a concrete `RowKey`.
    pub(super) fn resolve_row(&self, expr: &RowExpr) -> Result<RowKey, TabulaError> {
        match expr {
            RowExpr::Literal(rk) => Ok(*rk),
            RowExpr::Slot(s) => {
                let v = self
                    .slots
                    .get(*s as usize)
                    .and_then(|o| o.as_ref())
                    .ok_or_else(|| TabulaError::SlotOutOfBounds {
                        index: *s,
                        max: self.slots.len().saturating_sub(1) as u16,
                    })?;
                match v {
                    Value::U64(n) => Ok(RowKey(*n)),
                    _ => Err(TabulaError::TypeMismatch {
                        expected: "U64",
                        actual: v.type_name(),
                    }),
                }
            }
            RowExpr::Param(p) => {
                let v = self
                    .params
                    .get(*p as usize)
                    .ok_or(TabulaError::ParamOutOfBounds {
                        index: *p,
                        max: self.params.len().saturating_sub(1) as u16,
                    })?;
                match v {
                    Value::U64(n) => Ok(RowKey(*n)),
                    _ => Err(TabulaError::TypeMismatch {
                        expected: "U64",
                        actual: v.type_name(),
                    }),
                }
            }
        }
    }

    /// Resolve a `ValueExpr` to a concrete `Value`.
    pub(super) fn resolve_val(&self, expr: &ValueExpr) -> Result<Value, TabulaError> {
        match expr {
            ValueExpr::Literal(v) => Ok(*v),
            ValueExpr::Slot(s) => {
                self.slots
                    .get(*s as usize)
                    .and_then(|o| *o)
                    .ok_or(TabulaError::SlotOutOfBounds {
                        index: *s,
                        max: self.slots.len().saturating_sub(1) as u16,
                    })
            }
            ValueExpr::Param(p) => {
                self.params
                    .get(*p as usize)
                    .copied()
                    .ok_or(TabulaError::ParamOutOfBounds {
                        index: *p,
                        max: self.params.len().saturating_sub(1) as u16,
                    })
            }
        }
    }

    /// Get slot index from a `ValueExpr`, searching existing slots for a matching value.
    ///
    /// `exclude_slots` prevents matching a slot that the current instruction writes to.
    pub(super) fn resolve_slot_idx(
        &self,
        expr: &ValueExpr,
        encoded: &[BabyBear],
        is_null: bool,
        exclude_slots: &[usize],
    ) -> Result<Option<usize>, TabulaError> {
        match expr {
            ValueExpr::Slot(s) => Ok(Some(*s as usize)),
            ValueExpr::Param(_) | ValueExpr::Literal(_) => {
                let found = (0..self.max_slot)
                    .filter(|s| !exclude_slots.contains(s))
                    .filter(|s| self.slot_initialized[*s])
                    .find(|&s| self.slot_fes[s] == encoded && self.slot_nulls[s] == is_null);
                if let Some(idx) = found {
                    return Ok(Some(idx));
                }
                Err(TabulaError::ProofError {
                    phase: "trace_lowering",
                    detail: "no slot contains the required operand value (param/literal); \
                     the current AIR requires all operands to come from slots"
                        .to_string(),
                })
            }
        }
    }

    /// Update a slot with a new value.
    pub(super) fn update_slot(
        &mut self,
        slot: usize,
        value: Value,
        encoded: Vec<BabyBear>,
        is_null: bool,
    ) -> Result<(), TabulaError> {
        if slot >= MAX_SLOTS {
            return Err(TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!("slot {} >= MAX_SLOTS ({})", slot, MAX_SLOTS),
            });
        }
        self.slots[slot] = Some(value);
        self.slot_fes[slot] = encoded;
        self.slot_nulls[slot] = is_null;
        self.slot_initialized[slot] = true;
        if slot >= self.max_slot {
            self.max_slot = slot + 1;
        }
        Ok(())
    }

    /// Encode a `Value` and pad to exactly `W` field elements.
    pub(super) fn encode_padded(&self, value: &Value) -> Result<Vec<BabyBear>, TabulaError> {
        let mut fes = self.codec.encode(value)?;
        fes.resize(W, BabyBear::ZERO);
        Ok(fes)
    }

    /// Default InstructionRecord with zero/empty fields.
    pub(super) fn empty_record(&self, opcode: Opcode) -> InstructionRecord {
        InstructionRecord {
            opcode,
            tx_index: self.tx_index,
            effect_ordinal_in_tx: self.effect_ordinal,
            written_slots: vec![],
            src1_val: vec![BabyBear::ZERO; W],
            src2_val: vec![BabyBear::ZERO; W],
            cond_val: false,
            src1_slot_idx: None,
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

    /// Find the event matching the current tx_index and effect ordinal.
    pub(super) fn find_event(&self, instr_idx: usize) -> Result<&'a ExecutionEvent, TabulaError> {
        let tx_index = self.tx_index;
        let effect_ordinal = self.effect_ordinal;
        self.tx_events
            .iter()
            .find(|e| e.tx_index == tx_index && e.effect_ordinal_in_tx == effect_ordinal)
            .copied()
            .ok_or_else(|| TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "no event found for tx={} effect_ordinal={} at instruction {}",
                    tx_index, effect_ordinal, instr_idx
                ),
            })
    }

    /// Append a record.
    pub(super) fn push_record(&mut self, rec: InstructionRecord) {
        self.records.push(rec);
    }

    /// Append a static table row.
    pub(super) fn push_static_row(&mut self, row: StaticTableRow) {
        self.static_rows.push(row);
    }

    /// Consume this context and return accumulated output.
    pub(super) fn into_output(self) -> (Vec<InstructionRecord>, Vec<StaticTableRow>) {
        (self.records, self.static_rows)
    }
}
