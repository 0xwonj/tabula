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
mod codec;
#[cfg(feature = "stark")]
mod field;
#[cfg(feature = "stark")]
mod hasher;
#[cfg(feature = "stark")]
mod hybrid;
#[cfg(feature = "stark")]
mod poseidon;
#[cfg(feature = "stark")]
mod smt;
#[cfg(feature = "stark")]
mod ssmc;

#[cfg(feature = "stark")]
pub use codec::{BabyBearCodec, decode_trace, encode_trace, trace_width};
#[cfg(feature = "stark")]
pub use field::{
    COL_DATA_SMT_DEPTH, COL_STATE_SMT_DEPTH, DOMAIN_COL, DOMAIN_HASH_IR, DOMAIN_LEAF, DOMAIN_SMT,
    DOMAIN_SSMC, DOMAIN_TABLE, NativeDigest, TABLE_STATE_SMT_DEPTH, decode_u64_limbs,
    encode_u64_limbs,
};
#[cfg(feature = "stark")]
pub use hasher::{FieldHasher, MockFieldHasher};
#[cfg(feature = "stark")]
pub use hybrid::{ColumnMeta, ColumnState, CommitmentStrategy, HybridVC};
#[cfg(feature = "stark")]
pub use poseidon::PoseidonHasher;
#[cfg(feature = "stark")]
pub use smt::{MerkleProof, SparseMerkleTree};
#[cfg(feature = "stark")]
pub use ssmc::{MergeSource, MergeStep, MergeTrace, SsmcCommitment, SsmcEntry, SsmcList};
