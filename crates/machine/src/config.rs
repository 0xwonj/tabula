//! Concrete type assembly for the Tabula STARK proof system.
//!
//! Wires together Plonky3 primitives into a single `TabulaStarkConfig` type
//! suitable for proving and verifying Tabula batch proofs.
//!
//! - **Field**: BabyBear (p = 2^31 - 2^27 + 1)
//! - **Extension**: BabyBear^4 (~124-bit security)
//! - **Hash**: Poseidon2 width-16 sponge
//! - **PCS**: FRI over Merkle-committed polynomials

use p3_baby_bear::{
    BabyBear, Poseidon2ExternalLayerBabyBear, Poseidon2InternalLayerBabyBear,
    default_babybear_poseidon2_16,
};
use p3_challenger::DuplexChallenger;
use p3_commit::ExtensionMmcs;
use p3_dft::Radix2DitParallel;
use p3_field::extension::BinomialExtensionField;
use p3_fri::{FriParameters, TwoAdicFriPcs};
use p3_merkle_tree::MerkleTreeMmcs;
use p3_poseidon2::Poseidon2;
use p3_symmetric::{PaddingFreeSponge, TruncatedPermutation};
use p3_uni_stark::StarkConfig;

/// Quartic extension of BabyBear for ~124-bit security.
pub type EF4 = BinomialExtensionField<BabyBear, 4>;

/// Concrete Poseidon2 permutation type (width=16, s-box degree=7).
pub(crate) type Perm = Poseidon2<
    BabyBear,
    Poseidon2ExternalLayerBabyBear<16>,
    Poseidon2InternalLayerBabyBear<16>,
    16,
    7,
>;

/// Sponge hash: absorbs rate=8 field elements, squeezes 8 elements.
type FieldHash = PaddingFreeSponge<Perm, 16, 8, 8>;

/// Merkle inner node compression: 2-to-1 via truncated permutation.
type FieldCompress = TruncatedPermutation<Perm, 2, 8, 16>;

/// MMCS over base field values.
type ValMmcs = MerkleTreeMmcs<
    <BabyBear as p3_field::Field>::Packing,
    <BabyBear as p3_field::Field>::Packing,
    FieldHash,
    FieldCompress,
    8,
>;

/// MMCS lifted to the extension field.
type ChallengeMmcs = ExtensionMmcs<BabyBear, EF4, ValMmcs>;

/// Challenger: duplex sponge over BabyBear with Poseidon2.
pub(crate) type Challenger = DuplexChallenger<BabyBear, Perm, 16, 8>;

/// Polynomial commitment scheme: FRI over two-adic cosets.
type Pcs = TwoAdicFriPcs<BabyBear, Radix2DitParallel<BabyBear>, ValMmcs, ChallengeMmcs>;

/// Concrete STARK configuration for Tabula proofs.
pub type TabulaStarkConfig = StarkConfig<Pcs, EF4, Challenger>;

/// PCS type alias for UFCS disambiguation of `Pcs` trait methods.
pub(crate) type TabulaPcs = <TabulaStarkConfig as p3_uni_stark::StarkGenericConfig>::Pcs;

/// PCS domain type (two-adic coset).
pub(crate) type PcsDomain = <TabulaPcs as p3_commit::Pcs<EF4, Challenger>>::Domain;

/// PCS commitment type (Merkle root).
pub(crate) type PcsCommitment = <TabulaPcs as p3_commit::Pcs<EF4, Challenger>>::Commitment;

/// PCS opening proof type (FRI proof).
pub(crate) type PcsOpeningProof = <TabulaPcs as p3_commit::Pcs<EF4, Challenger>>::Proof;

/// Build a default `TabulaStarkConfig` with test-friendly FRI parameters.
///
/// Uses `log_blowup = 3, num_queries = 2, proof_of_work_bits = 1`
/// for fast iteration. `log_blowup = 3` supports AIR constraint degrees
/// up to 9 (required by our chips which have degree-4+ constraints).
/// Production parameters would increase `num_queries` and `proof_of_work_bits`.
pub fn default_config() -> TabulaStarkConfig {
    let perm = default_babybear_poseidon2_16();
    let hash = PaddingFreeSponge::new(perm.clone());
    let compress = TruncatedPermutation::new(perm.clone());
    let val_mmcs = MerkleTreeMmcs::new(hash, compress);
    let challenge_mmcs = ExtensionMmcs::new(val_mmcs.clone());

    let fri_params = FriParameters {
        log_blowup: 3,
        log_final_poly_len: 0,
        num_queries: 2,
        commit_proof_of_work_bits: 1,
        query_proof_of_work_bits: 1,
        mmcs: challenge_mmcs,
    };

    let dft = Radix2DitParallel::default();
    let pcs = TwoAdicFriPcs::new(dft, val_mmcs, fri_params);
    let challenger = DuplexChallenger::new(perm);

    StarkConfig::new(pcs, challenger)
}
