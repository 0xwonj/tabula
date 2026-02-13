#![warn(missing_docs)]
#![deny(unused)]

//! Protocol-level cryptographic primitives for the Tabula kernel (out-of-circuit).
//!
//! This crate computes cryptographic commitments to state: Poseidon hashing,
//! Sparse Merkle Trees (SMT), Small Sparse Map Commitments (SSMC), and
//! hybrid state commitment dispatch.
//!
//! All Plonky3 dependencies are behind the `stark` feature flag.
//! Without the feature, this crate compiles as an empty shell.

#[cfg(feature = "stark")]
mod field;
#[cfg(feature = "stark")]
mod hasher;
#[cfg(feature = "stark")]
mod codec;
#[cfg(feature = "stark")]
mod poseidon;
#[cfg(feature = "stark")]
mod smt;
#[cfg(feature = "stark")]
mod ssmc;
#[cfg(feature = "stark")]
mod hybrid;

#[cfg(feature = "stark")]
pub use field::{
    NativeDigest, encode_u64_limbs, decode_u64_limbs,
    DOMAIN_SSMC, DOMAIN_SMT, DOMAIN_LEAF, DOMAIN_TABLE, DOMAIN_COL,
};
#[cfg(feature = "stark")]
pub use hasher::{FieldHasher, MockFieldHasher};
#[cfg(feature = "stark")]
pub use codec::BabyBearCodec;
#[cfg(feature = "stark")]
pub use poseidon::PoseidonHasher;
#[cfg(feature = "stark")]
pub use smt::{SparseMerkleTree, MerkleProof};
#[cfg(feature = "stark")]
pub use ssmc::{SsmcList, SsmcEntry, SsmcCommitment, MergeSource, MergeStep, MergeTrace};
#[cfg(feature = "stark")]
pub use hybrid::{HybridVC, ColumnState, ColumnMeta, CommitmentStrategy};
