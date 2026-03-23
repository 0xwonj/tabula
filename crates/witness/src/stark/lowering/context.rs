use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::traits::StaticTableProvider;
use tabula_core::{ColId, PortableValue, RowKey, TableId};
use tabula_ir::{PrecompileId, PrecompileSignature, RowExpr, ValueExpr};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedValue, typed_row_key};

use tabula_chips::execution::MAX_SLOTS;
use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::ir_hash::IrHashCall;
use tabula_chips::static_table::trace::StaticTableRow;

use crate::{AccessEvent, ColumnValueProfile};

use super::{LoweringPrecompileCall, LoweringPropertyRead};

/// Mutable lowering context threaded through per-opcode lowering functions.
pub(super) struct LoweringContext<'a, const W: usize> {
    pub(super) slots: Vec<Option<TypedValue>>,
    pub(super) slot_fes: Vec<Vec<KoalaBear>>,
    pub(super) slot_nulls: Vec<bool>,
    pub(super) slot_initialized: Vec<bool>,
    pub(super) max_slot: usize,

    pub(super) effect_ordinal: u32,
    pub(super) tx_index: u32,

    pub(super) records: Vec<InstructionRecord>,
    pub(super) static_rows: Vec<StaticTableRow>,
    pub(super) ir_hash_calls: Vec<IrHashCall>,

    pub(super) tx_events: &'a [&'a AccessEvent],
    pub(super) profile_map: &'a BTreeMap<(TableId, ColId), ColumnValueProfile>,
    pub(super) type_runtimes: &'a TypeRuntimeRegistry,
    pub(super) encoding_runtimes: &'a EncodingRuntimeRegistry,
    pub(super) static_tables: &'a dyn StaticTableProvider,
    pub(super) empty_columns: &'a BTreeSet<(TableId, ColId)>,
    pub(super) params: &'a [PortableValue],
    pub(super) precompile_signatures: &'a BTreeMap<PrecompileId, PrecompileSignature>,
    pub(super) precompile_events_by_instruction: BTreeMap<usize, &'a LoweringPrecompileCall>,
    pub(super) matched_precompile_instructions: BTreeSet<usize>,
    pub(super) property_reads_stored: &'a [LoweringPropertyRead],
    pub(super) property_read_idx: usize,
}

impl<'a, const W: usize> LoweringContext<'a, W> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        tx_index: u32,
        tx_events: &'a [&'a AccessEvent],
        profile_map: &'a BTreeMap<(TableId, ColId), ColumnValueProfile>,
        type_runtimes: &'a TypeRuntimeRegistry,
        encoding_runtimes: &'a EncodingRuntimeRegistry,
        static_tables: &'a dyn StaticTableProvider,
        empty_columns: &'a BTreeSet<(TableId, ColId)>,
        params: &'a [PortableValue],
        precompile_signatures: &'a BTreeMap<PrecompileId, PrecompileSignature>,
        num_instructions: usize,
        precompile_events: &'a [LoweringPrecompileCall],
        property_reads_stored: &'a [LoweringPropertyRead],
    ) -> Result<Self, TabulaError> {
        Ok(Self {
            slots: vec![None; MAX_SLOTS],
            slot_fes: vec![vec![KoalaBear::ZERO; W]; MAX_SLOTS],
            slot_nulls: vec![false; MAX_SLOTS],
            slot_initialized: vec![false; MAX_SLOTS],
            max_slot: 0,
            effect_ordinal: 0,
            tx_index,
            records: Vec::with_capacity(num_instructions),
            static_rows: Vec::new(),
            ir_hash_calls: Vec::new(),
            tx_events,
            profile_map,
            type_runtimes,
            encoding_runtimes,
            static_tables,
            empty_columns,
            params,
            precompile_signatures,
            precompile_events_by_instruction: build_precompile_event_map(
                tx_index,
                precompile_events,
            )?,
            matched_precompile_instructions: BTreeSet::new(),
            property_reads_stored,
            property_read_idx: 0,
        })
    }

    pub(super) fn column_profile(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<&ColumnValueProfile, TabulaError> {
        self.profile_map
            .get(&(table, col))
            .ok_or_else(|| TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!("missing sealed column profile for ({table:?}, {col:?})"),
            })
    }

    pub(super) fn resolve_row(&self, expr: &RowExpr) -> Result<RowKey, TabulaError> {
        match expr {
            RowExpr::Literal(rk) => Ok(*rk),
            RowExpr::Slot(s) => {
                let value = self
                    .slots
                    .get(*s as usize)
                    .and_then(|o| o.as_ref())
                    .cloned()
                    .ok_or_else(|| TabulaError::SlotOutOfBounds {
                        index: *s,
                        max: self.slots.len().saturating_sub(1) as u16,
                    })?;
                typed_row_key(&value, self.type_runtimes)
            }
            RowExpr::Param(p) => {
                let value = self
                    .params
                    .get(*p as usize)
                    .ok_or(TabulaError::ParamOutOfBounds {
                        index: *p,
                        max: self.params.len().saturating_sub(1) as u16,
                    })?;
                typed_row_key(
                    &self.type_runtimes.decode_portable(value)?,
                    self.type_runtimes,
                )
            }
        }
    }

    pub(super) fn resolve_val(&self, expr: &ValueExpr) -> Result<TypedValue, TabulaError> {
        match expr {
            ValueExpr::Literal(v) => self.type_runtimes.decode_portable(v),
            ValueExpr::Slot(s) => self
                .slots
                .get(*s as usize)
                .and_then(|o| o.as_ref())
                .cloned()
                .ok_or(TabulaError::SlotOutOfBounds {
                    index: *s,
                    max: self.slots.len().saturating_sub(1) as u16,
                }),
            ValueExpr::Param(p) => self
                .params
                .get(*p as usize)
                .map(|value| self.type_runtimes.decode_portable(value))
                .transpose()?
                .ok_or(TabulaError::ParamOutOfBounds {
                    index: *p,
                    max: self.params.len().saturating_sub(1) as u16,
                }),
        }
    }

    pub(super) fn resolve_slot_idx(
        &self,
        expr: &ValueExpr,
        encoded: &[KoalaBear],
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
                    detail: "no slot contains the required operand value (param/literal); the current AIR requires all operands to come from slots".to_string(),
                })
            }
        }
    }

    pub(super) fn update_slot(
        &mut self,
        slot: usize,
        value: TypedValue,
        encoded: Vec<KoalaBear>,
        is_null: bool,
    ) -> Result<(), TabulaError> {
        if slot >= MAX_SLOTS {
            return Err(TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!("slot {slot} >= MAX_SLOTS ({MAX_SLOTS})"),
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

    pub(super) fn encode_padded(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        let encoding = self.encoding_runtimes.resolve_for_type(value.type_id())?;
        Self::encode_with_runtime_padded(encoding.as_ref(), value)
    }

    pub(super) fn encode_with_runtime_padded(
        encoding: &dyn tabula_types::EncodingRuntime,
        value: &TypedValue,
    ) -> Result<Vec<KoalaBear>, TabulaError> {
        let mut fes = encoding.encode_field_elements(value)?;
        if fes.len() > W {
            return Err(TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "value encoded width {} exceeds trace width {} for type {}",
                    fes.len(),
                    W,
                    value.type_id().0
                ),
            });
        }
        fes.resize(W, KoalaBear::ZERO);
        Ok(fes)
    }

    pub(super) fn encode_u64_padded(&self, value: u64) -> Result<Vec<KoalaBear>, TabulaError> {
        self.encode_padded(&tabula_types::u64_typed(value))
    }

    pub(super) fn empty_record(&self, opcode: Opcode) -> InstructionRecord {
        InstructionRecord {
            opcode,
            tx_index: self.tx_index,
            effect_ordinal_in_tx: self.effect_ordinal,
            written_slots: vec![],
            src1_val: vec![KoalaBear::ZERO; W],
            src2_val: vec![KoalaBear::ZERO; W],
            cond_val: false,
            src1_slot_idx: None,
            src2_slot_idx: None,
            cond_slot_idx: None,
            access_t: None,
            access_c: None,
            access_r: None,
            access_val: None,
            access_is_null: None,
            writes: vec![],
            hash_digest: None,
            is_empty_col: false,
            precompile_id: None,
            instruction_index: None,
            precompile_input_count: None,
            precompile_output_count: None,
            precompile_event_digest: None,
            property_query_type: None,
            property_query_arg0: vec![],
            property_query_arg1: vec![],
            property_result_val: vec![],
            property_result_key: vec![],
            property_result_is_null: false,
        }
    }

    pub(super) fn find_event(&self, instr_idx: usize) -> Result<&'a AccessEvent, TabulaError> {
        let tx_index = self.tx_index;
        let effect_ordinal = self.effect_ordinal;
        self.tx_events
            .iter()
            .find(|e| e.effect_ordinal_in_tx == effect_ordinal)
            .copied()
            .ok_or_else(|| TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "no event found for tx={tx_index} effect_ordinal={effect_ordinal} at instruction {instr_idx}"
                ),
            })
    }

    pub(super) fn push_record(&mut self, rec: InstructionRecord) {
        self.records.push(rec);
    }

    pub(super) fn precompile_event(
        &mut self,
        instruction_index: usize,
    ) -> Result<&'a LoweringPrecompileCall, TabulaError> {
        let event = self
            .precompile_events_by_instruction
            .get(&instruction_index)
            .copied()
            .ok_or_else(|| TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "missing precompile event for tx={} instruction {}",
                    self.tx_index, instruction_index
                ),
            })?;
        self.matched_precompile_instructions
            .insert(instruction_index);
        Ok(event)
    }

    pub(super) fn precompile_signature(
        &self,
        precompile_id: PrecompileId,
    ) -> Result<&PrecompileSignature, TabulaError> {
        self.precompile_signatures
            .get(&precompile_id)
            .ok_or_else(|| TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "missing sealed precompile signature for 0x{:04x}",
                    precompile_id.0,
                ),
            })
    }

    pub(super) fn validate_precompile_events_consumed(&self) -> Result<(), TabulaError> {
        if self.matched_precompile_instructions.len() != self.precompile_events_by_instruction.len()
        {
            let unmatched: Vec<_> = self
                .precompile_events_by_instruction
                .keys()
                .filter(|instruction_index| {
                    !self
                        .matched_precompile_instructions
                        .contains(instruction_index)
                })
                .copied()
                .collect();
            return Err(TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "unmatched precompile events remain for tx={} at instructions {:?}",
                    self.tx_index, unmatched
                ),
            });
        }
        Ok(())
    }

    pub(super) fn push_static_row(&mut self, row: StaticTableRow) {
        self.static_rows.push(row);
    }

    pub(super) fn push_ir_hash_call(&mut self, call: IrHashCall) {
        self.ir_hash_calls.push(call);
    }

    pub(super) fn into_output(
        self,
    ) -> (Vec<InstructionRecord>, Vec<StaticTableRow>, Vec<IrHashCall>) {
        (self.records, self.static_rows, self.ir_hash_calls)
    }
}

fn build_precompile_event_map(
    tx_index: u32,
    events: &[LoweringPrecompileCall],
) -> Result<BTreeMap<usize, &LoweringPrecompileCall>, TabulaError> {
    let mut map = BTreeMap::new();
    for event in events {
        if map.insert(event.instruction_index, event).is_some() {
            return Err(TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "duplicate precompile event for tx={} instruction {}",
                    tx_index, event.instruction_index
                ),
            });
        }
    }
    Ok(map)
}
