//! Keygen phase and metadata types for the multi-chip STARK prover/verifier.
//!
//! [`TierProvingMetadata`] caches per-chip interaction descriptors and metadata,
//! avoiding redundant keygen during repeated `prove()` calls.
//!
//! [`TierVerificationMetadata`] stores the minimal per-chip metadata sufficient for
//! standalone verification without a machine instance.

use std::collections::{BTreeMap, BTreeSet};

use tabula_stark::air::interaction::BusId;
use tabula_stark::air::keygen::{ChipKeygenInfo, keygen_chip};
use tabula_stark::chips::ChipId;

use super::registry::ChipRegistry;

/// Proving metadata: per-chip keygen info computed once at machine build time.
pub struct TierProvingMetadata {
    /// Per-chip keygen metadata, indexed by [`ChipId`].
    pub(crate) chip_info: BTreeMap<ChipId, ChipKeygenInfo>,
}

impl TierProvingMetadata {
    /// Build proving metadata by running keygen on all chips in the registry.
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

    /// All chip IDs in this proving metadata.
    #[cfg(test)]
    pub fn chip_ids(&self) -> Vec<ChipId> {
        self.chip_info.keys().copied().collect()
    }

    /// Buses that are unbalanced in this tier (only sends or only receives).
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
#[derive(Clone, Debug)]
pub struct ChipVerificationMetadata {
    /// Width of the main trace.
    pub main_width: usize,
    /// Width of the preprocessed trace (0 if none).
    pub preprocessed_width: usize,
    /// Indices of preprocessed columns that require next-row access.
    pub preprocessed_next_row_columns: Vec<usize>,
    /// Number of public values consumed by this chip.
    pub num_public_values: usize,
    /// Number of LogUp interactions per row (sends + receives).
    pub interactions_per_row: usize,
}

/// Verification metadata: minimal metadata for standalone proof verification.
pub struct TierVerificationMetadata {
    /// Per-chip verification metadata, indexed by [`ChipId`].
    pub(crate) chip_info: BTreeMap<ChipId, ChipVerificationMetadata>,
}

impl TierVerificationMetadata {
    /// Derive verification metadata from proving metadata.
    pub fn from_proving_metadata(pk: &TierProvingMetadata) -> Self {
        let chip_info = pk
            .chip_info
            .iter()
            .map(|(&id, info)| {
                let interactions_per_row =
                    info.interactions.num_sends_per_row + info.interactions.num_receives_per_row;
                (
                    id,
                    ChipVerificationMetadata {
                        main_width: info.main_width,
                        preprocessed_width: info.preprocessed_width,
                        preprocessed_next_row_columns: info.preprocessed_next_row_columns.clone(),
                        num_public_values: info.num_public_values,
                        interactions_per_row,
                    },
                )
            })
            .collect();
        Self { chip_info }
    }

    /// Look up verification metadata for a chip.
    pub fn get(&self, id: ChipId) -> Option<&ChipVerificationMetadata> {
        self.chip_info.get(&id)
    }

    /// All chip IDs in this verification metadata.
    pub fn chip_ids(&self) -> Vec<ChipId> {
        self.chip_info.keys().copied().collect()
    }
}

/// Compute the set of external buses across multiple proof tiers.
pub(crate) fn compute_external_buses<'a>(
    tiers: impl IntoIterator<Item = &'a TierProvingMetadata>,
) -> BTreeSet<BusId> {
    tiers
        .into_iter()
        .flat_map(TierProvingMetadata::unbalanced_buses)
        .collect()
}

/// Number of RAP constraints for a given interaction count.
pub(crate) fn rap_constraint_count(interactions_per_row: usize) -> usize {
    4 * interactions_per_row + 12
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_air::BaseAir;
    use p3_field::PrimeCharacteristicRing;
    use p3_koala_bear::KoalaBear;
    use tabula_chips::execution::ExecutionChip;
    use tabula_chips::ir_hash::IrHashChip;
    use tabula_chips::poseidon::PoseidonChip;
    use tabula_chips::range_check::RangeCheckChip;
    use tabula_chips::smt_path::{SmtColPathChip, SmtTablePathChip};
    use tabula_chips::static_table::StaticTableChip;
    use tabula_stark::chips::core_chips;

    fn perm_trace_width(info: &ChipKeygenInfo) -> usize {
        let n = info.interactions.num_sends_per_row + info.interactions.num_receives_per_row;
        if n == 0 { 0 } else { 4 * (n + 1) }
    }

    #[test]
    fn proving_metadata_from_execution_tier() {
        let topology = crate::setup::recipes::execution_tier_topology().unwrap();
        let proving = &topology.proving_metadata;
        assert!(proving.get(core_chips::EXECUTION).is_some());
        assert!(proving.get(core_chips::RANGE_CHECK).is_some());
        assert!(proving.get(core_chips::POSEIDON).is_none());
        assert_eq!(proving.chip_ids().len(), 3);
    }

    #[test]
    fn verification_metadata_from_proving_metadata() {
        let topology = crate::setup::recipes::execution_tier_topology().unwrap();
        let verification = &topology.verification_metadata;

        let exec = verification.get(core_chips::EXECUTION).unwrap();
        assert!(exec.interactions_per_row > 0);

        let rc = verification.get(core_chips::RANGE_CHECK).unwrap();
        assert_eq!(rc.interactions_per_row, 1);
    }

    #[test]
    fn verification_metadata_matches_keygen() {
        let topology = crate::setup::recipes::execution_tier_topology().unwrap();
        let proving = &topology.proving_metadata;
        let verification = &topology.verification_metadata;

        for (&id, keygen) in &proving.chip_info {
            let verify_info = verification.get(id).expect("chip should be in metadata");
            assert_eq!(verify_info.main_width, keygen.main_width);
            assert_eq!(verify_info.preprocessed_width, keygen.preprocessed_width);
            assert_eq!(
                verify_info.preprocessed_next_row_columns,
                keygen.preprocessed_next_row_columns
            );
            assert_eq!(verify_info.num_public_values, keygen.num_public_values);
            let expected_interactions =
                keygen.interactions.num_sends_per_row + keygen.interactions.num_receives_per_row;
            assert_eq!(verify_info.interactions_per_row, expected_interactions);
        }
    }

    #[test]
    fn core_chip_capabilities_have_one_authority_path() {
        let chips: Vec<Box<dyn crate::backend::AnyRap>> = vec![
            Box::new(ExecutionChip::<3>),
            Box::new(StaticTableChip::<3>),
            Box::new(PoseidonChip),
            Box::new(RangeCheckChip),
            Box::new(SmtColPathChip),
            Box::new(SmtTablePathChip),
            Box::new(IrHashChip),
        ];
        let proving = TierProvingMetadata {
            chip_info: chips
                .iter()
                .map(|chip| {
                    let info = keygen_chip(chip.as_ref());
                    (info.chip_id, info)
                })
                .collect(),
        };
        let verification = TierVerificationMetadata::from_proving_metadata(&proving);

        for chip in &chips {
            let chip_id = chip.chip_id();
            let keygen = proving.get(chip_id).expect("keygen metadata");
            let verify = verification.get(chip_id).expect("verification metadata");
            let chip_ref = crate::proof::chip_ref::ChipRef::new(chip.as_ref());

            assert_eq!(
                keygen.num_public_values,
                BaseAir::<KoalaBear>::num_public_values(&chip_ref)
            );
            assert_eq!(
                keygen.preprocessed_next_row_columns,
                BaseAir::<KoalaBear>::preprocessed_next_row_columns(&chip_ref)
            );
            assert_eq!(verify.num_public_values, keygen.num_public_values);
            assert_eq!(
                verify.preprocessed_next_row_columns,
                keygen.preprocessed_next_row_columns
            );
        }
    }

    #[test]
    fn perm_width_zero_for_no_interactions() {
        let info = ChipKeygenInfo {
            chip_id: core_chips::EXECUTION,
            main_width: 10,
            preprocessed_width: 0,
            preprocessed_next_row_columns: vec![],
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
    fn perm_width_nonzero_for_interactions() {
        let info = ChipKeygenInfo {
            chip_id: core_chips::EXECUTION,
            main_width: 10,
            preprocessed_width: 0,
            preprocessed_next_row_columns: vec![],
            num_public_values: 0,
            interactions: tabula_stark::air::descriptor::InteractionDescriptor {
                sends: vec![tabula_stark::air::interaction::Interaction {
                    bus: BusId(1),
                    values: vec![],
                    multiplicity: tabula_stark::air::interaction::VirtualPairCol::constant(
                        p3_koala_bear::KoalaBear::ONE,
                    ),
                    direction: tabula_stark::air::interaction::InteractionDirection::Send,
                }],
                receives: vec![],
                num_sends_per_row: 1,
                num_receives_per_row: 0,
            },
        };
        assert_eq!(perm_trace_width(&info), 8);
    }
}
