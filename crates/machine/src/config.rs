//! Concrete type assembly for the Tabula STARK proof system.
//!
//! Wires together Plonky3 primitives into a single `TabulaStarkConfig` type
//! suitable for proving and verifying Tabula batch proofs.
//!
//! - **Field**: KoalaBear (p = 2^31 - 2^24 + 1)
//! - **Extension**: KoalaBear^4 (~124-bit security)
//! - **Merkle hash**: BLAKE3 (non-algebraic, ~10x faster than Poseidon2)
//! - **Fiat-Shamir**: Poseidon2 width-16 duplex sponge
//! - **PCS**: FRI over Merkle-committed polynomials

use p3_challenger::DuplexChallenger;
use p3_commit::ExtensionMmcs;
use p3_dft::Radix2DitParallel;
use p3_fri::{FriParameters, TwoAdicFriPcs};
use p3_koala_bear::{
    KoalaBear, Poseidon2ExternalLayerKoalaBear, Poseidon2InternalLayerKoalaBear,
    default_koalabear_poseidon2_16,
};
use p3_merkle_tree::MerkleTreeMmcs;
use p3_poseidon2::Poseidon2;
use p3_uni_stark::StarkConfig;

use crate::backend::pcs::{Blake3FieldCompressor, Blake3FieldHasher};

/// Quartic extension of KoalaBear for ~124-bit security.
///
/// Re-exported from `tabula-stark` where it is canonically defined.
pub use tabula_stark::EF4;

/// Concrete Poseidon2 permutation type (width=16, s-box degree=3).
///
/// Used for the Fiat-Shamir challenger (not for Merkle hashing).
pub(crate) type Perm = Poseidon2<
    KoalaBear,
    Poseidon2ExternalLayerKoalaBear<16>,
    Poseidon2InternalLayerKoalaBear<16>,
    16,
    3,
>;

/// Merkle leaf hasher: BLAKE3 producing `[KoalaBear; 8]` digests.
type FieldHash = Blake3FieldHasher;

/// Merkle inner node compression: BLAKE3 2-to-1.
type FieldCompress = Blake3FieldCompressor;

/// MMCS over base field values using BLAKE3 Merkle trees.
///
/// Digest format: `[KoalaBear; 8]` (32 bytes mapped to 8 field elements),
/// compatible with the Poseidon2 `DuplexChallenger` for commitment observation.
type ValMmcs = MerkleTreeMmcs<KoalaBear, KoalaBear, FieldHash, FieldCompress, 2, 8>;

/// MMCS lifted to the extension field.
type ChallengeMmcs = ExtensionMmcs<KoalaBear, EF4, ValMmcs>;

/// Challenger: duplex sponge over KoalaBear with Poseidon2.
///
/// Poseidon2 is used for Fiat-Shamir (algebraic challenger) even though
/// Merkle hashing uses BLAKE3. The two are independent: the challenger
/// observes Merkle roots as field elements, not as hash computations.
pub(crate) type Challenger = DuplexChallenger<KoalaBear, Perm, 16, 8>;

/// Polynomial commitment scheme: FRI over two-adic cosets with BLAKE3 Merkle.
type Pcs = TwoAdicFriPcs<KoalaBear, Radix2DitParallel<KoalaBear>, ValMmcs, ChallengeMmcs>;

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

/// Build a `TabulaStarkConfig` with the given FRI parameters.
///
/// `log_blowup` hard floor: `>= 3` (forced by degree 6-9 constraints in
/// Execution/MemoryShard/StateShard: `log_blowup >= ceil(log2(max_constraint_degree - 1))`).
///
/// Conjectured soundness: `log_blowup × num_queries + query_proof_of_work_bits` bits.
pub fn make_config(
    log_blowup: usize,
    num_queries: usize,
    commit_pow_bits: usize,
    query_pow_bits: usize,
) -> TabulaStarkConfig {
    let val_mmcs = MerkleTreeMmcs::new(Blake3FieldHasher, Blake3FieldCompressor, 0);
    let challenge_mmcs = ExtensionMmcs::new(val_mmcs.clone());

    let fri_params = FriParameters {
        log_blowup,
        log_final_poly_len: 0,
        max_log_arity: 2,
        num_queries,
        commit_proof_of_work_bits: commit_pow_bits,
        query_proof_of_work_bits: query_pow_bits,
        mmcs: challenge_mmcs,
    };

    let dft = Radix2DitParallel::default();
    let pcs = TwoAdicFriPcs::new(dft, val_mmcs, fri_params);

    let perm = default_koalabear_poseidon2_16();
    let challenger = DuplexChallenger::new(perm);

    StarkConfig::new(pcs, challenger)
}

/// Build the default `TabulaStarkConfig` with 128-bit conjectured security.
///
/// - `log_blowup = 3`: forced by degree 6-9 constraints in Execution/MemoryShard/StateShard.
/// - `max_log_arity = 2`: 4-way FRI folding.
/// - `num_queries = 38`, `query_pow = 14`: `3 × 38 + 14 = 128` bits (ethSTARK conjecture).
/// - `commit_pow = 8`: defense-in-depth against folding challenge grinding.
pub fn default_config() -> TabulaStarkConfig {
    make_config(3, 38, 8, 14)
}
