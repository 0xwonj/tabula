use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_field::PrimeField32;

use tabula_core::error::TabulaError;

use tabula_chips::range_check::RANGE_CHECK_SIZE;
use tabula_stark::air::interaction::{InteractionDirection, core_buses};
use tabula_stark::debug::ChipRecord;

pub(super) fn collect_poseidon_inputs(
    records: &[&ChipRecord<BabyBear>],
) -> Result<Vec<[BabyBear; 16]>, TabulaError> {
    let mut inputs = Vec::new();
    for record in records {
        for interaction in &record.interactions {
            if interaction.bus != core_buses::POSEIDON_PERM
                || interaction.direction != InteractionDirection::Send
            {
                continue;
            }
            if interaction.values.len() != 24 {
                return Err(TabulaError::ProofError {
                    phase: "collectors",
                    detail: format!(
                        "poseidon interaction width mismatch in {}: expected 24, got {}",
                        record.name,
                        interaction.values.len()
                    ),
                });
            }
            let mult = interaction.multiplicity.as_canonical_u32();
            if mult == 0 {
                continue;
            }
            let mut input = [BabyBear::ZERO; 16];
            input.copy_from_slice(&interaction.values[..16]);
            for _ in 0..mult {
                inputs.push(input);
            }
        }
    }
    Ok(inputs)
}

pub(super) fn collect_range_check_multiplicities(
    records: &[&ChipRecord<BabyBear>],
) -> Result<[u32; RANGE_CHECK_SIZE], TabulaError> {
    let mut mults = [0u32; RANGE_CHECK_SIZE];
    for record in records {
        for interaction in &record.interactions {
            if interaction.bus != core_buses::RANGE_CHECK
                || interaction.direction != InteractionDirection::Send
            {
                continue;
            }
            if interaction.values.len() != 1 {
                return Err(TabulaError::ProofError {
                    phase: "collectors",
                    detail: format!(
                        "range_check interaction width mismatch in {}: expected 1, got {}",
                        record.name,
                        interaction.values.len()
                    ),
                });
            }
            let mult = interaction.multiplicity.as_canonical_u32();
            if mult == 0 {
                continue;
            }
            let value = interaction.values[0].as_canonical_u32() as usize;
            if value >= RANGE_CHECK_SIZE {
                return Err(TabulaError::ProofError {
                    phase: "collectors",
                    detail: format!(
                        "range_check value out of domain in {}: {}",
                        record.name, value
                    ),
                });
            }
            mults[value] =
                mults[value]
                    .checked_add(mult)
                    .ok_or_else(|| TabulaError::ProofError {
                        phase: "collectors",
                        detail: format!("range_check multiplicity overflow at value {}", value),
                    })?;
        }
    }
    Ok(mults)
}
