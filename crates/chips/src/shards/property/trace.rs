//! Trace generation for the SSMC property chip.

use std::collections::BTreeMap;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};
use tabula_gadgets::bool_fe;
use tabula_stark::air::columns::borrow_cols_mut;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::generator::TraceGenerator;
use tabula_stark::trace::trace_map::TraceMap;

use crate::ChipSpec;
use crate::execution::limbs_to_u64;
use crate::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcColumnWitness, SsmcWitness};

use super::air::SsmcPropertyChip;
use super::columns::{LessOrEqChecked, SsmcPropertyCols, ssmc_property_width};

/// Per-column label for execution-side property claims.
pub const PROPERTY_READ_WITNESS_LABEL: &str = "property_read_records";

/// Raw property claim emitted by execution lowering.
#[derive(Debug, Clone)]
pub struct PropertyReadRecord {
    /// Canonical query kind ordinal.
    pub query_type: u8,
    /// First canonical query operand as `U64` limbs.
    pub query_arg0: Vec<KoalaBear>,
    /// Second canonical query operand as `U64` limbs.
    pub query_arg1: Vec<KoalaBear>,
    /// Claimed execution result value.
    pub result_val: Vec<KoalaBear>,
    /// Claimed execution result key as `U64` limbs.
    pub result_key: Vec<KoalaBear>,
    /// 1 iff the claimed result is null.
    pub is_null: bool,
}

#[derive(Debug, Clone)]
/// Internal SSMC property witness row.
#[doc(hidden)]
pub struct SsmcPropertyRecord {
    query_type: u8,
    query_arg0: u64,
    query_arg1: u64,
    result_val: Vec<KoalaBear>,
    result_key: u64,
    result_is_null: bool,
    uses_empty_old: bool,
    anchor_key: u64,
    anchor_val: Vec<KoalaBear>,
    has_prev_old: bool,
    prev_old_key: u64,
    is_last_old: bool,
    next_old_key: u64,
}

#[derive(Clone)]
struct OldEntryAnchor {
    key: u64,
    value: Vec<KoalaBear>,
    has_prev_old: bool,
    prev_old_key: u64,
    is_last_old: bool,
    next_old_key: u64,
}

fn zero_ordering() -> tabula_gadgets::OrderingRangeChecked<KoalaBear> {
    use tabula_gadgets::{Limb2Bits, LimbHalves, StrictIneq};

    tabula_gadgets::OrderingRangeChecked {
        ineq: StrictIneq {
            diff0: KoalaBear::ZERO,
            diff1: KoalaBear::ZERO,
            diff2: KoalaBear::ZERO,
            borrow0: KoalaBear::ZERO,
            borrow1: KoalaBear::ZERO,
        },
        diff0_halves: LimbHalves {
            lo: KoalaBear::ZERO,
            hi: KoalaBear::ZERO,
        },
        diff1_halves: LimbHalves {
            lo: KoalaBear::ZERO,
            hi: KoalaBear::ZERO,
        },
        diff2_bits: Limb2Bits {
            b0: KoalaBear::ZERO,
            b1: KoalaBear::ZERO,
            b2: KoalaBear::ZERO,
            b3: KoalaBear::ZERO,
        },
    }
}

fn populate_leq(leq: &mut LessOrEqChecked<KoalaBear>, lhs: u64, rhs: u64) {
    if lhs == rhs {
        leq.is_eq = KoalaBear::ONE;
        leq.lt = zero_ordering();
    } else {
        leq.is_eq = KoalaBear::ZERO;
        leq.lt.populate(lhs, rhs);
    }
}

fn claim_arg_u64(limbs: &[KoalaBear]) -> u64 {
    let arr = [
        *limbs.first().unwrap_or(&KoalaBear::ZERO),
        *limbs.get(1).unwrap_or(&KoalaBear::ZERO),
        *limbs.get(2).unwrap_or(&KoalaBear::ZERO),
    ];
    limbs_to_u64(&arr)
}

fn zero_fes<const W: usize>() -> Vec<KoalaBear> {
    vec![KoalaBear::ZERO; W]
}

fn is_zero_fes(fes: &[KoalaBear]) -> bool {
    fes.iter().all(p3_field::Field::is_zero)
}

fn build_old_entry_anchors(col_data: &SsmcColumnWitness) -> Vec<OldEntryAnchor> {
    col_data
        .state_rows
        .iter()
        .filter(|row| !row.is_gap && row.source.in_old())
        .enumerate()
        .map(|(idx, row)| OldEntryAnchor {
            key: row.key,
            value: row.old_val.clone(),
            has_prev_old: row.prev_old_key != 0 || idx > 0,
            prev_old_key: row.prev_old_key,
            is_last_old: row.is_last_old_entry_hint(),
            next_old_key: row.next_old_key,
        })
        .collect()
}

trait StateRowExt {
    fn is_last_old_entry_hint(&self) -> bool;
}

impl StateRowExt for crate::shards::state::trace::StateShardRow {
    fn is_last_old_entry_hint(&self) -> bool {
        self.source.in_old() && self.next_old_key == 0
    }
}

fn build_ssmc_property_records<const W: usize>(
    claims: &[PropertyReadRecord],
    col_data: &SsmcColumnWitness,
) -> Result<Vec<SsmcPropertyRecord>, TabulaError> {
    let old_entries = build_old_entry_anchors(col_data);
    let is_empty_old = old_entries.is_empty();
    let first_entry = old_entries.first().cloned();
    let last_entry = old_entries.last().cloned();

    claims
        .iter()
        .map(|claim| {
            let query_arg0 = claim_arg_u64(&claim.query_arg0);
            let query_arg1 = claim_arg_u64(&claim.query_arg1);
            let result_key = claim_arg_u64(&claim.result_key);
            let zero_val = zero_fes::<W>();

            let selected_anchor = match claim.query_type {
                2 => {
                    if let Some(entry) = old_entries.iter().find(|entry| entry.key > query_arg0) {
                        Some(entry.clone())
                    } else if is_empty_old {
                        None
                    } else {
                        last_entry.clone()
                    }
                }
                3 => {
                    if let Some(entry) = old_entries
                        .iter()
                        .rev()
                        .find(|entry| entry.key < query_arg0)
                    {
                        Some(entry.clone())
                    } else if is_empty_old {
                        None
                    } else {
                        first_entry.clone()
                    }
                }
                other => {
                    return Err(TabulaError::ProofError {
                        phase: "ssmc_property_witness",
                        detail: format!(
                            "SSMC property witness only supports successor/predecessor, got ordinal {other}"
                        ),
                    });
                }
            };

            if claim.is_null
                && (!is_zero_fes(&claim.result_val) || result_key != 0) {
                    return Err(TabulaError::ProofError {
                        phase: "ssmc_property_witness",
                        detail: "null PropertyRead claims must carry canonical zero value/key"
                            .to_string(),
                    });
                }

            if !claim.is_null
                && let Some(anchor) = &selected_anchor
                && (result_key != anchor.key || claim.result_val != anchor.value) {
                    return Err(TabulaError::ProofError {
                        phase: "ssmc_property_witness",
                        detail: "PropertyRead claim does not match SSMC anchor row".to_string(),
                    });
                }

            if let Some(anchor) = selected_anchor {
                Ok(SsmcPropertyRecord {
                    query_type: claim.query_type,
                    query_arg0,
                    query_arg1,
                    result_val: claim.result_val.clone(),
                    result_key,
                    result_is_null: claim.is_null,
                    uses_empty_old: false,
                    anchor_key: anchor.key,
                    anchor_val: anchor.value,
                    has_prev_old: anchor.has_prev_old,
                    prev_old_key: anchor.prev_old_key,
                    is_last_old: anchor.is_last_old,
                    next_old_key: anchor.next_old_key,
                })
            } else {
                Ok(SsmcPropertyRecord {
                    query_type: claim.query_type,
                    query_arg0,
                    query_arg1,
                    result_val: zero_val,
                    result_key: 0,
                    result_is_null: true,
                    uses_empty_old: true,
                    anchor_key: 0,
                    anchor_val: zero_fes::<W>(),
                    has_prev_old: false,
                    prev_old_key: 0,
                    is_last_old: false,
                    next_old_key: 0,
                })
            }
        })
        .collect()
}

/// Count how many property queries anchor on each SSMC old-state row.
///
/// This uses the same anchor-selection logic as property-trace generation so
/// that StateShard `SSMC_OLD_ENTRY` sends stay aligned with property-chip
/// receives.
pub fn ssmc_property_anchor_multiplicities<const W: usize>(
    claims: &[PropertyReadRecord],
    col_data: &SsmcColumnWitness,
) -> Result<BTreeMap<u64, u32>, TabulaError> {
    let mut mults = BTreeMap::new();
    for record in build_ssmc_property_records::<W>(claims, col_data)? {
        if record.uses_empty_old {
            continue;
        }
        *mults.entry(record.anchor_key).or_insert(0) += 1;
    }
    Ok(mults)
}

/// Generate the SSMC property trace.
fn generate_ssmc_property_trace<const W: usize>(
    table_id: u32,
    col_id: u16,
    records: &[SsmcPropertyRecord],
) -> RowMajorMatrix<KoalaBear> {
    let width = ssmc_property_width::<W>();
    let n_real = records.len();
    let n_rows = (n_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; n_rows * width];

    for (row_idx, record) in records.iter().enumerate() {
        let row = &mut values[row_idx * width..(row_idx + 1) * width];
        let cols: &mut SsmcPropertyCols<KoalaBear, W> = borrow_cols_mut(row);

        cols.is_real = KoalaBear::ONE;
        cols.table_id = KoalaBear::new(table_id);
        cols.col_id = KoalaBear::new(col_id as u32);
        cols.query_type = KoalaBear::new(record.query_type as u32);
        cols.query_is_successor = bool_fe(record.query_type == 2);
        cols.query_is_predecessor = bool_fe(record.query_type == 3);
        cols.query_arg0.populate(record.query_arg0);
        cols.query_arg1.populate(record.query_arg1);

        for i in 0..W {
            cols.result_val[i] = *record.result_val.get(i).unwrap_or(&KoalaBear::ZERO);
            cols.anchor_val[i] = *record.anchor_val.get(i).unwrap_or(&KoalaBear::ZERO);
        }
        cols.result_key.populate(record.result_key);
        cols.result_is_null = bool_fe(record.result_is_null);
        cols.uses_empty_old = bool_fe(record.uses_empty_old);
        cols.uses_anchor = bool_fe(!record.uses_empty_old);
        cols.anchor_key.populate(record.anchor_key);
        cols.has_prev_old = bool_fe(record.has_prev_old);
        cols.prev_old_key.populate(record.prev_old_key);
        cols.is_last_old = bool_fe(record.is_last_old);
        cols.next_old_key.populate(record.next_old_key);

        cols.query_lt_anchor = zero_ordering();
        cols.anchor_lt_query = zero_ordering();
        cols.prev_le_query.is_eq = KoalaBear::ZERO;
        cols.prev_le_query.lt = zero_ordering();
        cols.anchor_le_query.is_eq = KoalaBear::ZERO;
        cols.anchor_le_query.lt = zero_ordering();
        cols.query_le_next.is_eq = KoalaBear::ZERO;
        cols.query_le_next.lt = zero_ordering();
        cols.query_le_anchor.is_eq = KoalaBear::ZERO;
        cols.query_le_anchor.lt = zero_ordering();

        match record.query_type {
            2 => {
                if !record.result_is_null {
                    cols.query_lt_anchor
                        .populate(record.query_arg0, record.anchor_key);
                    if record.has_prev_old {
                        populate_leq(
                            &mut cols.prev_le_query,
                            record.prev_old_key,
                            record.query_arg0,
                        );
                    }
                } else if !record.uses_empty_old {
                    populate_leq(
                        &mut cols.anchor_le_query,
                        record.anchor_key,
                        record.query_arg0,
                    );
                }
            }
            3 => {
                if !record.result_is_null {
                    cols.anchor_lt_query
                        .populate(record.anchor_key, record.query_arg0);
                    if !record.is_last_old {
                        populate_leq(
                            &mut cols.query_le_next,
                            record.query_arg0,
                            record.next_old_key,
                        );
                    }
                } else if !record.uses_empty_old {
                    populate_leq(
                        &mut cols.query_le_anchor,
                        record.query_arg0,
                        record.anchor_key,
                    );
                }
            }
            _ => {}
        }
    }

    RowMajorMatrix::new(values, width)
}

impl<const W: usize> TraceGenerator for SsmcPropertyChip<W> {
    type Input = Vec<SsmcPropertyRecord>;

    fn generate_trace(&self, input: &Vec<SsmcPropertyRecord>) -> RowMajorMatrix<KoalaBear> {
        generate_ssmc_property_trace::<W>(self.table_id(), self.col_id(), input)
    }
}

impl<const W: usize> TraceContributor for SsmcPropertyChip<W> {
    fn phase(&self) -> TracePhase {
        TracePhase::MEMORY
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let witness = store.get::<SsmcWitness>(SSMC_WITNESS_LABEL)?;
        let claims = store
            .get::<Vec<PropertyReadRecord>>(PROPERTY_READ_WITNESS_LABEL)
            .cloned()
            .unwrap_or_default();
        let Some(col_data) = witness.get(TableId(self.table_id()), ColId(self.col_id())) else {
            return Err(TabulaError::ProofError {
                phase: "ssmc_property_trace",
                detail: format!(
                    "no SSMC witness data for ({}, {})",
                    self.table_id(),
                    self.col_id()
                ),
            });
        };

        let records = build_ssmc_property_records::<W>(&claims, col_data)?;
        let trace = generate_ssmc_property_trace::<W>(self.table_id(), self.col_id(), &records);
        map.insert(self.chip_id(), trace);
        Ok(())
    }
}
