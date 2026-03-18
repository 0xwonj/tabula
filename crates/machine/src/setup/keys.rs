//! Keygen phase and key types for the multi-chip STARK prover/verifier.
//!
//! [`TabulaProvingKey`] caches per-chip interaction descriptors and metadata,
//! avoiding redundant keygen during repeated `prove()` calls.
//!
//! [`TabulaVerifyingKey`] stores the minimal per-chip metadata sufficient for
//! standalone verification without a machine instance.

use std::collections::{BTreeMap, BTreeSet};

use tabula_stark::air::interaction::BusId;
use tabula_stark::air::keygen::{ChipKeygenInfo, keygen_chip};
use tabula_stark::chips::ChipId;

use super::registry::ChipRegistry;

/// Proving key: per-chip keygen info computed once at machine build time.
///
/// Caches [`ChipKeygenInfo`] (interaction descriptors, widths, public value counts)
/// so that [`crate::Prover::prove()`] does not re-run the keygen
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

    /// Buses that are unbalanced in this tier (only sends or only receives).
    ///
    /// A bus is unbalanced if the chips in this proving key collectively
    /// only send on it (no receivers) or only receive on it (no senders).
    /// Unbalanced buses cannot balance within a single proof and must be
    /// balanced across proof instances (i.e., they are "external" buses).
    pub fn unbalanced_buses(&self) -> BTreeSet<BusId> {
        let mut send_buses = BTreeSet::new();
        let mut recv_buses = BTreeSet::new();

        for info in self.chip_info.values() {
            for interaction in &info.interactions.sends {
                send_buses.insert(interaction.bus);
            }
            for interaction in &info.interactions.receives {
                recv_buses.insert(interaction.bus);
            }
        }

        let send_only = send_buses.difference(&recv_buses).copied();
        let recv_only = recv_buses.difference(&send_buses).copied();
        send_only.chain(recv_only).collect()
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

/// Compute the set of external buses across multiple proof tiers.
///
/// A bus is external if it is unbalanced (only sends or only receives)
/// in any tier. External buses require cross-proof balance verification.
pub fn compute_external_buses<'a>(
    tiers: impl IntoIterator<Item = &'a TabulaProvingKey>,
) -> BTreeSet<BusId> {
    tiers
        .into_iter()
        .flat_map(TabulaProvingKey::unbalanced_buses)
        .collect()
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
    use super::*;
    use tabula_stark::chips::core_chips;

    fn perm_trace_width(info: &ChipKeygenInfo) -> usize {
        let n = info.interactions.num_sends_per_row + info.interactions.num_receives_per_row;
        if n == 0 { 0 } else { 4 * (n + 1) }
    }

    #[test]
    fn proving_key_from_execution_tier() {
        let setup = crate::setup::build::execution_tier_setup().unwrap();
        let pk = &setup.proving_key;
        assert!(pk.get(core_chips::EXECUTION).is_some());
        assert!(pk.get(core_chips::RANGE_CHECK).is_some());
        assert!(pk.get(core_chips::POSEIDON).is_some());
        assert_eq!(pk.chip_ids().len(), 4);
    }

    #[test]
    fn verifying_key_from_proving_key() {
        let setup = crate::setup::build::execution_tier_setup().unwrap();
        let vk = &setup.verifying_key;

        let exec = vk.get(core_chips::EXECUTION).unwrap();
        assert!(exec.interactions_per_row > 0);

        let rc = vk.get(core_chips::RANGE_CHECK).unwrap();
        assert_eq!(rc.interactions_per_row, 1);
    }

    #[test]
    fn verify_info_widths_match_keygen() {
        let setup = crate::setup::build::execution_tier_setup().unwrap();
        let pk = &setup.proving_key;
        let vk = &setup.verifying_key;

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
        assert_eq!(perm_trace_width(&info), 24);
    }

    #[test]
    fn execution_chip_has_nonzero_perm_width() {
        let setup = crate::setup::build::execution_tier_setup().unwrap();
        let exec_info = setup.proving_key.get(core_chips::EXECUTION).unwrap();
        let width = perm_trace_width(exec_info);
        assert!(width > 0, "ExecutionChip must have interactions");
        assert_eq!(width % 4, 0, "perm width must be multiple of 4");
    }

    #[test]
    fn external_buses_derived_from_tier_metadata() {
        use crate::setup::build::{column_tier_setup, execution_tier_setup, root_tier_setup};
        use crate::testing::TestSsmcProofColumn;
        use tabula_core::{ColId, TableId};
        use tabula_stark::air::interaction::core_buses;

        let exec = execution_tier_setup().unwrap();
        let column = TestSsmcProofColumn {
            table_id: TableId(1),
            col_id: ColId(1),
            receives_commitment: true,
        };
        let col = column_tier_setup(&column).unwrap();
        let root = root_tier_setup(&crate::setup::root::SmtRootProof).unwrap();

        let external =
            compute_external_buses([&exec.proving_key, &col.proving_key, &root.proving_key]);

        assert!(external.contains(&core_buses::READ_ACCESS));
        assert!(external.contains(&core_buses::WRITE_ACCESS));
        assert!(external.contains(&core_buses::EMPTY_COL_READ));
        assert!(external.contains(&core_buses::SMT_LEAF_DIGEST));
        assert!(external.contains(&core_buses::RANGE_CHECK));

        assert!(!external.contains(&core_buses::POSEIDON_PERM));
        assert!(!external.contains(&core_buses::STATIC_TABLE_LOOKUP));
        assert!(!external.contains(&core_buses::SMT_TABLE_ROOT));
    }
}
