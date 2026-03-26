//! Protocol identifiers and commitment wrappers shared across the stack.

pub mod address;
pub mod commitments;
pub mod profiles;
pub mod tx;

pub use address::{CellKey, ColId, RowKey, TableId};
pub use commitments::{ColumnCommitmentId, Digest, StateRoot, TableCommitmentId};
pub use profiles::{
    ColumnLayoutKind, ColumnProfileId, EncodingProfileId, RootProfileId, RootProofFamilyId,
    SchemeId, SchemeProfileId, TypeId,
};
pub use tx::TxTypeId;
