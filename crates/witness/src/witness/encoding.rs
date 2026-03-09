//! Value encoding and SSMC hash-chain utilities.
//!
//! Pure encoding functions extracted from `WitnessGenerator`:
//! - Value → field element encoding with null handling
//! - Poseidon2 hash-chain input construction for SSMC commitments
//! - Proof-compatible column commitment computation

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{
    ColumnState, DOMAIN_SSMC, FieldHasher, HybridVC, NativeDigest, encode_u64_limbs,
};
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{ColId, TableId, Value, ValueType, zero_value};

use tabula_chips::poseidon::constants::poseidon2_permutation;

/// Encode a value as Tier 1 ComEnc field elements, using canonical zero when null.
///
/// Unified null-encoding logic: when `is_null` is true, encodes the canonical
/// zero for the given `value_type` regardless of the `value` argument.
pub(crate) fn encode_value_with_null_flag(
    codec: &impl ValueCodec<FieldRepr = BabyBear>,
    value: &Value,
    is_null: bool,
    value_type: ValueType,
) -> Result<(Vec<BabyBear>, bool), TabulaError> {
    if is_null {
        let zero = zero_value(value_type);
        let fes = codec.encode(&zero)?;
        Ok((fes, true))
    } else {
        let fes = codec.encode(value)?;
        Ok((fes, false))
    }
}

/// Encode an `Option<Value>` as Tier 1 ComEnc field elements.
///
/// `None` maps to canonical zero (null). Delegates to `encode_value_with_null_flag`.
pub(crate) fn encode_value(
    codec: &impl ValueCodec<FieldRepr = BabyBear>,
    value: &Option<Value>,
    value_type: ValueType,
) -> Result<(Vec<BabyBear>, bool), TabulaError> {
    match value {
        Some(v) => encode_value_with_null_flag(codec, v, false, value_type),
        None => {
            // Value content is irrelevant when null — zero_value is used inside.
            let placeholder = zero_value(value_type);
            encode_value_with_null_flag(codec, &placeholder, true, value_type)
        }
    }
}

/// Build a Poseidon2 width-16 hash input for SSMC hash-chain computation.
///
/// Layout depends on whether this is the first row or a continuation:
/// - First row / empty: `[domain, t, c, key[0..3], value[0..], 0...]`
/// - Continuation: `[prev_hash[0..8], key[0..3], value[0..], 0...]`
pub(crate) fn build_ssmc_hash_input(
    table: TableId,
    col: ColId,
    key_limbs: &[BabyBear],
    value: &[BabyBear],
    prev: Option<&NativeDigest>,
) -> [BabyBear; 16] {
    let mut input = [BabyBear::ZERO; 16];
    match prev {
        None => {
            input[0] = BabyBear::new(DOMAIN_SSMC);
            input[1] = BabyBear::new(table.0);
            input[2] = BabyBear::new(col.0 as u32);
            for (i, &limb) in key_limbs.iter().enumerate() {
                input[3 + i] = limb;
            }
            for (i, &v) in value.iter().enumerate() {
                input[6 + i] = v;
            }
        }
        Some(prev_digest) => {
            input[..8].copy_from_slice(&prev_digest.0);
            for (i, &limb) in key_limbs.iter().enumerate() {
                input[8 + i] = limb;
            }
            for (i, &v) in value.iter().enumerate() {
                input[11 + i] = v;
            }
        }
    }
    input
}

/// Compute a proof-compatible column commitment.
///
/// For SSMC columns this follows the AIR hash-chain layout used by
/// `StateColumnChip`:
/// - empty: `Poseidon(0x00, t, c, 0, ..., 0)`
/// - first row: `Poseidon(0x00, t, c, key[3], value, 0, ..., 0)`
/// - continuation: `Poseidon(prev_hash[8], key[3], value, 0, ..., 0)`
///
/// For SMT columns, the tree root is already the native commitment.
pub(crate) fn proof_column_commitment<H: FieldHasher<F = BabyBear, Digest = NativeDigest>>(
    table: TableId,
    col: ColId,
    state: &ColumnState<H>,
) -> Result<NativeDigest, TabulaError> {
    match state {
        ColumnState::Ssmc(list) => {
            if list.table != table || list.col != col {
                return Err(TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "SSMC list identity mismatch: expected ({:?},{:?}), got ({:?},{:?})",
                        table, col, list.table, list.col
                    ),
                });
            }

            if list.entries().is_empty() {
                let input = build_ssmc_hash_input(table, col, &[], &[], None);
                let (_, out) = poseidon2_permutation(input);
                return Ok(NativeDigest(core::array::from_fn(|i| out[i])));
            }

            let mut prev: Option<NativeDigest> = None;
            for entry in list.entries() {
                if entry.value.len() > 5 {
                    return Err(TabulaError::ProofError {
                        phase: "witness",
                        detail: format!(
                            "value width {} is unsupported by proof hash-chain layout (max 5)",
                            entry.value.len()
                        ),
                    });
                }

                let key_limbs = encode_u64_limbs(entry.key.0);
                let input =
                    build_ssmc_hash_input(table, col, &key_limbs, &entry.value, prev.as_ref());
                let (_, out) = poseidon2_permutation(input);
                prev = Some(NativeDigest(core::array::from_fn(|i| out[i])));
            }

            Ok(prev.expect("non-empty entries must produce a hash"))
        }
        ColumnState::Smt(tree) => Ok(tree.root()),
    }
}

/// Compute the two-level state root from column states.
///
/// Groups columns by table, computes per-column leaf hashes and per-table
/// roots, then combines into a single state root via `HybridVC`.
pub(crate) fn compute_state_root<H: FieldHasher<F = BabyBear, Digest = NativeDigest>>(
    vc: &HybridVC<H>,
    column_states: &BTreeMap<(TableId, ColId), ColumnState<H>>,
) -> Result<NativeDigest, TabulaError> {
    // Group by table → columns.
    let mut tables: BTreeMap<TableId, BTreeMap<ColId, NativeDigest>> = BTreeMap::new();
    for (&(table, col), state) in column_states {
        let com = proof_column_commitment(table, col, state)?;
        let leaf = vc.compute_leaf(table, col, state.scheme_tag(), &com);
        tables.entry(table).or_default().insert(col, leaf);
    }

    // table roots → state root.
    let mut table_roots = BTreeMap::new();
    for (table, col_leaves) in &tables {
        table_roots.insert(*table, vc.compute_table_root(col_leaves));
    }

    Ok(vc.compute_state_root(&table_roots))
}
