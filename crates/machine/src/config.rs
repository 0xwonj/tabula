//! Concrete type assembly for the Tabula STARK proof system.
//!
//! Wires together Plonky3 primitives into a single `TabulaStarkConfig` type
//! suitable for proving and verifying Tabula batch proofs.
//!
//! - **Field**: BabyBear (p = 2^31 - 2^27 + 1)
//! - **Extension**: BabyBear^4 (~124-bit security)
//! - **Merkle hash**: BLAKE3 (non-algebraic, ~10x faster than Poseidon2)
//! - **Fiat-Shamir**: Poseidon2 width-16 duplex sponge
//! - **PCS**: FRI over Merkle-committed polynomials

use p3_baby_bear::{
    BabyBear, Poseidon2ExternalLayerBabyBear, Poseidon2InternalLayerBabyBear,
    default_babybear_poseidon2_16,
};
use p3_challenger::DuplexChallenger;
use p3_commit::ExtensionMmcs;
use p3_dft::Radix2DitParallel;
use p3_fri::{FriParameters, TwoAdicFriPcs};
use p3_merkle_tree::MerkleTreeMmcs;
use p3_poseidon2::Poseidon2;
use p3_uni_stark::StarkConfig;

use crate::blake3_pcs::{Blake3FieldCompressor, Blake3FieldHasher};

/// Quartic extension of BabyBear for ~124-bit security.
///
/// Re-exported from `tabula-stark` where it is canonically defined.
pub use tabula_stark::EF4;

/// Concrete Poseidon2 permutation type (width=16, s-box degree=7).
///
/// Used for the Fiat-Shamir challenger (not for Merkle hashing).
pub(crate) type Perm = Poseidon2<
    BabyBear,
    Poseidon2ExternalLayerBabyBear<16>,
    Poseidon2InternalLayerBabyBear<16>,
    16,
    7,
>;

/// Merkle leaf hasher: BLAKE3 producing `[BabyBear; 8]` digests.
type FieldHash = Blake3FieldHasher;

/// Merkle inner node compression: BLAKE3 2-to-1.
type FieldCompress = Blake3FieldCompressor;

/// MMCS over base field values using BLAKE3 Merkle trees.
///
/// Digest format: `[BabyBear; 8]` (32 bytes mapped to 8 field elements),
/// compatible with the Poseidon2 `DuplexChallenger` for commitment observation.
type ValMmcs = MerkleTreeMmcs<BabyBear, BabyBear, FieldHash, FieldCompress, 8>;

/// MMCS lifted to the extension field.
type ChallengeMmcs = ExtensionMmcs<BabyBear, EF4, ValMmcs>;

/// Challenger: duplex sponge over BabyBear with Poseidon2.
///
/// Poseidon2 is used for Fiat-Shamir (algebraic challenger) even though
/// Merkle hashing uses BLAKE3. The two are independent: the challenger
/// observes Merkle roots as field elements, not as hash computations.
pub(crate) type Challenger = DuplexChallenger<BabyBear, Perm, 16, 8>;

/// Polynomial commitment scheme: FRI over two-adic cosets with BLAKE3 Merkle.
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
    let val_mmcs = MerkleTreeMmcs::new(Blake3FieldHasher, Blake3FieldCompressor);
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

    let perm = default_babybear_poseidon2_16();
    let challenger = DuplexChallenger::new(perm);

    StarkConfig::new(pcs, challenger)
}
