use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;

use crate::air::chips::smt_path::air::{
    SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET, SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
    SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET,
};
use crate::air::chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use crate::witness::BatchWitness;

pub(super) fn validate_smt_path_shapes(
    smt_col_paths: &[SmtPathWitness],
    smt_table_paths: &[SmtTablePathWitness],
) -> Result<(), TabulaError> {
    for (idx, w) in smt_col_paths.iter().enumerate() {
        if w.path_bits.len() != w.siblings.len() {
            return Err(TabulaError::ConsistencyError(format!(
                "smt_col_paths[{idx}] shape mismatch: path_bits={}, siblings={}",
                w.path_bits.len(),
                w.siblings.len()
            )));
        }
    }
    for (idx, w) in smt_table_paths.iter().enumerate() {
        if w.path.path_bits.len() != w.path.siblings.len() {
            return Err(TabulaError::ConsistencyError(format!(
                "smt_table_paths[{idx}] shape mismatch: path_bits={}, siblings={}",
                w.path.path_bits.len(),
                w.path.siblings.len()
            )));
        }
    }
    Ok(())
}

pub(super) fn smt_table_public_values<H>(witness: &BatchWitness<H>) -> Vec<BabyBear>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    smt_table_public_values_from_roots(&witness.old_state_root, &witness.new_state_root)
}

pub(super) fn smt_table_public_values_from_roots(
    old_state_root: &NativeDigest,
    new_state_root: &NativeDigest,
) -> Vec<BabyBear> {
    let mut pvs = vec![BabyBear::ZERO; SMT_TABLE_PATH_NUM_PUBLIC_VALUES];
    pvs[SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET..SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET + 8]
        .copy_from_slice(&old_state_root.0);
    pvs[SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET..SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET + 8]
        .copy_from_slice(&new_state_root.0);
    pvs
}
