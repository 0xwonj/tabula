//! Value encoding and SSMC hash-chain utilities.
//!
//! Pure encoding and commitment helpers for witness preparation:
//! - Value → field element encoding with null handling
//! - Poseidon2 hash-chain input construction for SSMC commitments
//! - Proof-compatible column commitment computation

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_commitment::{ColumnState, DOMAIN_SSMC, FieldHasher, NativeDigest, encode_u64_limbs};
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{ColId, TableId, Value, ValueType};

use tabula_chips::poseidon::constants::poseidon2_permutation;

/// Maximum number of value field elements that fit in an SSMC continuation row.
///
/// Poseidon width is 16. Continuation layout: `[prev_hash(8), key(3), value(..)]`.
/// So at most `16 - 8 - 3 = 5` FEs per value. This limits SSMC to types with
/// encoding width ≤ 5 (Bool=1, U64=3, I64=3 all fit; Bytes32=8 requires SMT).
pub(crate) const SSMC_MAX_VALUE_FES: usize = 5;

/// Encode a value as Tier 1 ComEnc field elements, with null handling.
///
/// When `is_null` is true, produces literal zeros (`[0; w]`) matching
/// the `encode_trace` convention — the null flag gates the value, so
/// the actual FE content is irrelevant.
pub(crate) fn encode_value_with_null_flag(
    codec: &impl ValueCodec<FieldRepr = KoalaBear>,
    value: &Value,
    is_null: bool,
    value_type: ValueType,
) -> Result<(Vec<KoalaBear>, bool), TabulaError> {
    if is_null {
        let w = codec.field_elements_per(value_type);
        Ok((vec![KoalaBear::ZERO; w], true))
    } else {
        let fes = codec.encode(value)?;
        Ok((fes, false))
    }
}

/// Encode an `Option<Value>` as Tier 1 ComEnc field elements.
///
/// `None` maps to null (literal zeros). Delegates to `encode_value_with_null_flag`.
pub(crate) fn encode_value(
    codec: &impl ValueCodec<FieldRepr = KoalaBear>,
    value: &Option<Value>,
    value_type: ValueType,
) -> Result<(Vec<KoalaBear>, bool), TabulaError> {
    match value {
        Some(v) => encode_value_with_null_flag(codec, v, false, value_type),
        None => {
            let w = codec.field_elements_per(value_type);
            Ok((vec![KoalaBear::ZERO; w], true))
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
    key_limbs: &[KoalaBear],
    value: &[KoalaBear],
    prev: Option<&NativeDigest>,
) -> [KoalaBear; 16] {
    let mut input = [KoalaBear::ZERO; 16];
    match prev {
        None => {
            input[0] = KoalaBear::new(DOMAIN_SSMC);
            input[1] = KoalaBear::new(table.0);
            input[2] = KoalaBear::new(col.0 as u32);
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
#[doc(hidden)]
pub fn proof_column_commitment<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>>(
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
                if entry.value.len() > SSMC_MAX_VALUE_FES {
                    return Err(TabulaError::ProofError {
                        phase: "witness",
                        detail: format!(
                            "value width {} exceeds SSMC continuation limit (max {SSMC_MAX_VALUE_FES})",
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
