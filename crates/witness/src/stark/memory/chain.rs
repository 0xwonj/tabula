use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_types::NativeKeyPayload;

use super::state::StateColumnRow;
use tabula_chips::execution::native_key_payload_prefix3;
use tabula_chips::poseidon::constants::poseidon2_permutation;

pub(super) fn populate_state_chain_accumulators<const W: usize>(rows: &mut [StateColumnRow]) {
    let mut i = 0;
    while i < rows.len() {
        let (t, c) = (rows[i].table_id, rows[i].col_id);
        let mut j = i;
        while j < rows.len() && rows[j].table_id == t && rows[j].col_id == c {
            j += 1;
        }

        let mut prev_old: Option<[KoalaBear; 8]> = None;
        let mut prev_new: Option<[KoalaBear; 8]> = None;

        for row in rows[i..j].iter_mut() {
            if !row.is_gap && row.source.in_old() {
                let acc = match prev_old {
                    Some(prev) => hash_chain_step_cont::<W>(prev, row.key, &row.old_val),
                    None => hash_chain_step_first::<W>(t, c, row.key, &row.old_val),
                };
                row.old_hash_acc = acc;
                prev_old = Some(acc);
            } else if let Some(prev) = prev_old {
                row.old_hash_acc = prev;
            }

            if !row.is_gap && row.source.in_new() {
                let acc = match prev_new {
                    Some(prev) => hash_chain_step_cont::<W>(prev, row.key, &row.new_val),
                    None => hash_chain_step_first::<W>(t, c, row.key, &row.new_val),
                };
                row.new_hash_acc = acc;
                prev_new = Some(acc);
            } else if let Some(prev) = prev_new {
                row.new_hash_acc = prev;
            }
        }

        i = j;
    }
}

fn hash_chain_step_first<const W: usize>(
    table_id: u32,
    col_id: u16,
    key: NativeKeyPayload,
    value: &[KoalaBear],
) -> [KoalaBear; 8] {
    let mut input = [KoalaBear::ZERO; 16];
    let key_limbs = native_key_payload_prefix3(&key);
    input[1] = KoalaBear::new(table_id);
    input[2] = KoalaBear::new(col_id as u32);
    input[3] = key_limbs[0];
    input[4] = key_limbs[1];
    input[5] = key_limbs[2];
    for (idx, v) in value.iter().enumerate().take(W) {
        input[6 + idx] = *v;
    }
    let (_, out) = poseidon2_permutation(input);
    core::array::from_fn(|i| out[i])
}

fn hash_chain_step_cont<const W: usize>(
    prev: [KoalaBear; 8],
    key: NativeKeyPayload,
    value: &[KoalaBear],
) -> [KoalaBear; 8] {
    let mut input = [KoalaBear::ZERO; 16];
    let key_limbs = native_key_payload_prefix3(&key);
    input[..8].copy_from_slice(&prev);
    input[8] = key_limbs[0];
    input[9] = key_limbs[1];
    input[10] = key_limbs[2];
    for (idx, v) in value.iter().enumerate().take(W) {
        input[11 + idx] = *v;
    }
    let (_, out) = poseidon2_permutation(input);
    core::array::from_fn(|i| out[i])
}
