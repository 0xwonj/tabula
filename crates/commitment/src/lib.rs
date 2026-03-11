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
mod column_meta;
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
mod ssmc_merge;
#[cfg(feature = "stark")]
mod state_root;

#[cfg(feature = "stark")]
pub use codec::{BabyBearCodec, decode_trace, encode_trace, trace_width};
#[cfg(feature = "stark")]
pub use column_meta::{ColumnMeta, ColumnState, scheme_tags};
#[cfg(feature = "stark")]
pub use field::{
    COL_DATA_SMT_DEPTH, COL_STATE_SMT_DEPTH, DOMAIN_COL, DOMAIN_HASH_IR, DOMAIN_LEAF, DOMAIN_SMT,
    DOMAIN_SSMC, DOMAIN_TABLE, NativeDigest, TABLE_STATE_SMT_DEPTH, decode_u64_limbs,
    encode_u64_limbs,
};
#[cfg(feature = "stark")]
pub use hasher::{FieldHasher, MockFieldHasher};
#[cfg(feature = "stark")]
pub use hybrid::HybridVC;
#[cfg(feature = "stark")]
pub use poseidon::PoseidonHasher;
#[cfg(feature = "stark")]
pub use smt::{MerkleProof, SparseMerkleTree};
#[cfg(feature = "stark")]
pub use ssmc::{SsmcCommitment, SsmcEntry, SsmcList};
#[cfg(feature = "stark")]
pub use ssmc_merge::{MergeSource, MergeStep, MergeTrace};
#[cfg(feature = "stark")]
pub use state_root::{compute_leaf, compute_state_root, compute_table_root};
