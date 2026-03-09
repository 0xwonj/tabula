//! Keygen phase and key types for the multi-chip STARK prover/verifier.
//!
//! [`TabulaProvingKey`] caches per-chip interaction descriptors and metadata,
//! avoiding redundant keygen during repeated `prove()` calls.
//!
//! [`TabulaVerifyingKey`] stores the minimal per-chip metadata sufficient for
//! standalone verification without a machine instance.

use std::collections::BTreeMap;

use tabula_stark::air::keygen::{ChipKeygenInfo, keygen_chip};
use tabula_stark::chips::ChipId;

use super::registry::ChipRegistry;

/// Proving key: per-chip keygen info computed once at machine build time.
///
/// Caches [`ChipKeygenInfo`] (interaction descriptors, widths, public value counts)
/// so that [`prove()`](crate::TabulaMachine::prove) does not re-run the keygen
/// phase on every call.
pub struct TabulaProvingKey {
    /// Per-chip keygen metadata, indexed by [`ChipId`].
    pub(crate) chip_info: BTreeMap<ChipId, ChipKeygenInfo>,
}

impl TabulaProvingKey {
    /// Build a proving key by running keygen on all chips in the registry.
    pub fn from_registry(registry: &ChipRegistry) -> Self {
        let chip_info = registry
            .chips()
            .iter()
            .map(|chip| {
                let info = keygen_chip(chip.air());
                (info.chip_id, info)
            })
            .collect();
        Self { chip_info }
    }

    /// Look up keygen info for a chip.
    pub fn get(&self, id: ChipId) -> Option<&ChipKeygenInfo> {
        self.chip_info.get(&id)
    }

    /// All chip IDs in this proving key.
    pub fn chip_ids(&self) -> Vec<ChipId> {
        self.chip_info.keys().copied().collect()
    }
}

/// Per-chip verification metadata.
///
/// Contains the minimal info needed to verify a chip proof without
/// access to the chip's AIR implementation.
#[derive(Clone, Debug)]
pub struct ChipVerifyInfo {
    /// Type-safe chip identifier.
    pub chip_id: ChipId,
    /// Width of the main trace.
    pub main_width: usize,
    /// Width of the preprocessed trace (0 if none).
    pub preprocessed_width: usize,
    /// Number of public values consumed by this chip.
    pub num_public_values: usize,
    /// Number of LogUp interactions per row (sends + receives).
    pub interactions_per_row: usize,
}

/// Verifying key: minimal metadata for standalone proof verification.
///
/// Can be serialized, stored, and used to verify proofs without
/// reconstructing the machine or chip AIR implementations.
pub struct TabulaVerifyingKey {
    /// Per-chip verification metadata, indexed by [`ChipId`].
    pub(crate) chip_info: BTreeMap<ChipId, ChipVerifyInfo>,
}

impl TabulaVerifyingKey {
    /// Derive a verifying key from a proving key.
    pub fn from_proving_key(pk: &TabulaProvingKey) -> Self {
        let chip_info = pk
            .chip_info
            .iter()
            .map(|(&id, info)| {
                let interactions_per_row =
                    info.interactions.num_sends_per_row + info.interactions.num_receives_per_row;
                (
                    id,
                    ChipVerifyInfo {
                        chip_id: id,
                        main_width: info.main_width,
                        preprocessed_width: info.preprocessed_width,
                        num_public_values: info.num_public_values,
                        interactions_per_row,
                    },
                )
            })
            .collect();
        Self { chip_info }
    }

    /// Build a verifying key directly from a registry.
    pub fn from_registry(registry: &ChipRegistry) -> Self {
        Self::from_proving_key(&TabulaProvingKey::from_registry(registry))
    }

    /// Look up verification info for a chip.
    pub fn get(&self, id: ChipId) -> Option<&ChipVerifyInfo> {
        self.chip_info.get(&id)
    }

    /// All chip IDs in this verifying key.
    pub fn chip_ids(&self) -> Vec<ChipId> {
        self.chip_info.keys().copied().collect()
    }
}

/// Number of RAP constraints for a given interaction count.
///
/// Each interaction contributes 4 constraints (phi·f = m decomposed into EF4 components).
/// Plus 4 first-row + 4 transition + 4 last-row cumsum constraints = 12.
pub(crate) fn rap_constraint_count(interactions_per_row: usize) -> usize {
    4 * interactions_per_row + 12
}

#[cfg(test)]
mod tests {
    /// Compute the permutation trace width for a chip.
    ///
    /// Layout: `N` phi values (one EF4 per interaction) + 1 running cumsum.
    /// Each EF4 value is stored as 4 BabyBear field elements.
    ///
    /// Returns 0 if the chip has no interactions.
    fn perm_trace_width(info: &super::ChipKeygenInfo) -> usize {
        let n = info.interactions.num_sends_per_row + info.interactions.num_receives_per_row;
        if n == 0 { 0 } else { 4 * (n + 1) }
    }
    use super::*;
    use tabula_stark::chips::core_chips;

    #[test]
    fn proving_key_from_registry() {
        let machine = crate::TabulaMachine::builder()
            .with_core_chips()
            .with_default_commitments()
            .build()
            .expect("build");
        let pk = machine.proving_key();
        assert!(pk.get(core_chips::EXECUTION).is_some());
        assert!(pk.get(core_chips::RANGE_CHECK).is_some());
        assert!(pk.get(core_chips::POSEIDON).is_some());
        assert_eq!(pk.chip_ids().len(), 9);
    }

    #[test]
    fn verifying_key_from_proving_key() {
        let machine = crate::TabulaMachine::builder()
            .with_core_chips()
            .with_default_commitments()
            .build()
            .expect("build");
        let vk = machine.verifying_key();
        assert_eq!(vk.chip_ids().len(), 9);

        let exec = vk.get(core_chips::EXECUTION).unwrap();
        assert!(exec.interactions_per_row > 0);

        let rc = vk.get(core_chips::RANGE_CHECK).unwrap();
        // RangeCheck has 1 receive interaction per row (bus receive).
        assert_eq!(rc.interactions_per_row, 1);
    }

    #[test]
    fn verify_info_widths_match_keygen() {
        let machine = crate::TabulaMachine::builder()
            .with_core_chips()
            .with_default_commitments()
            .build()
            .expect("build");
        let pk = machine.proving_key();
        let vk = machine.verifying_key();

        for (&id, keygen) in &pk.chip_info {
            let verify_info = vk.get(id).expect("chip should be in vk");
            assert_eq!(verify_info.main_width, keygen.main_width);
            assert_eq!(verify_info.preprocessed_width, keygen.preprocessed_width);
            assert_eq!(verify_info.num_public_values, keygen.num_public_values);
            let expected_interactions =
                keygen.interactions.num_sends_per_row + keygen.interactions.num_receives_per_row;
            assert_eq!(verify_info.interactions_per_row, expected_interactions);
        }
    }

    #[test]
    fn perm_width_zero_for_no_interactions() {
        let info = ChipKeygenInfo {
            chip_id: core_chips::EXECUTION,
            main_width: 10,
            preprocessed_width: 0,
            num_public_values: 0,
            interactions: tabula_stark::air::descriptor::InteractionDescriptor {
                sends: vec![],
                receives: vec![],
                num_sends_per_row: 0,
                num_receives_per_row: 0,
            },
        };
        assert_eq!(perm_trace_width(&info), 0);
    }

    #[test]
    fn perm_width_correct_for_interactions() {
        let info = ChipKeygenInfo {
            chip_id: core_chips::EXECUTION,
            main_width: 10,
            preprocessed_width: 0,
            num_public_values: 0,
            interactions: tabula_stark::air::descriptor::InteractionDescriptor {
                sends: vec![],
                receives: vec![],
                num_sends_per_row: 3,
                num_receives_per_row: 2,
            },
        };
        // 5 interactions + 1 cumsum = 6, each 4 BabyBear = 24
        assert_eq!(perm_trace_width(&info), 24);
    }

    #[test]
    fn execution_chip_has_nonzero_perm_width() {
        let machine = crate::TabulaMachine::builder()
            .with_core_chips()
            .with_default_commitments()
            .build()
            .expect("build");
        let pk = machine.proving_key();
        let exec_info = pk.get(core_chips::EXECUTION).unwrap();
        let width = perm_trace_width(exec_info);
        assert!(width > 0, "ExecutionChip must have interactions");
        assert_eq!(width % 4, 0, "perm width must be multiple of 4");
    }
}
