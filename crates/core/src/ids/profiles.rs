//! Semantic/profile and layout identifiers.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Identifies a column commitment scheme in portable artifacts.
///
/// This is the protocol-facing identifier that links compiler-selected
/// column proof plans to runtime-installed scheme implementations.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct SchemeId(pub u16);

impl SchemeId {
    /// Built-in sorted-state Merkle commitment scheme.
    pub const SSMC: Self = Self(0);
    /// Built-in sparse Merkle tree scheme.
    pub const SMT: Self = Self(1);

    /// Return the raw protocol identifier.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Identifies one registered semantic type definition inside a profile catalog.
///
/// This is a catalog-scoped lookup handle, not a cross-program semantic
/// identity. Semantic identity is carried by descriptor hashes.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct TypeId(pub u32);

/// Identifies one registered proof/transcript encoding profile inside a profile
/// catalog.
///
/// This is a catalog-scoped lookup handle, not a cross-program semantic
/// identity. Semantic identity is carried by descriptor hashes.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct EncodingProfileId(pub u32);

/// Identifies one registered commitment/opening scheme profile inside a profile
/// catalog.
///
/// This is distinct from [`SchemeId`]: `SchemeId` names the portable scheme
/// family, while `SchemeProfileId` names one concrete registered profile.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct SchemeProfileId(pub u32);

/// Identifies one registered per-column sealed profile inside a profile
/// catalog.
///
/// This is a catalog-scoped lookup handle, not a cross-program semantic
/// identity. Semantic identity is carried by the column profile hash.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ColumnProfileId(pub u32);

/// Identifies the verifier-relevant commitment layout/backend for a column.
///
/// Unlike [`SchemeId`], this is not the public SDK/profile identity. It seals
/// the actual column-state representation expected by witness generation and
/// proof chips.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ColumnLayoutKind(pub u16);

impl ColumnLayoutKind {
    /// Built-in sorted-state Merkle commitment layout.
    pub const SSMC_V1: Self = Self(0);
    /// Built-in sparse Merkle tree layout.
    pub const SMT_V1: Self = Self(1);

    /// Return the raw layout identifier.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Identifies a root-proof compatibility profile in portable artifacts.
///
/// Column commitment schemes bind to one root profile so runtime and verifier
/// can fail closed when a artifact and installed root proof disagree.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct RootProfileId(pub u16);

impl RootProfileId {
    /// Two-level SMT root proof profile used by Tabula v1.
    pub const SMT_V1: Self = Self(0);

    /// Return the raw protocol identifier.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Identifies the root-proof backend family selected for machine setup.
///
/// This is distinct from [`RootProfileId`], which remains the column-side root
/// binding family sealed into scheme profiles. A root proof backend may accept
/// one or more root binding families.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct RootProofFamilyId(pub u16);

impl RootProofFamilyId {
    /// Built-in SMT root proof backend family used by Tabula v1.
    pub const SMT_V1: Self = Self(0);

    /// Return the raw protocol identifier.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl std::fmt::Display for SchemeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scheme:{}", self.0)
    }
}

impl std::fmt::Display for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "type:{}", self.0)
    }
}

impl std::fmt::Display for EncodingProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "encoding_profile:{}", self.0)
    }
}

impl std::fmt::Display for SchemeProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scheme_profile:{}", self.0)
    }
}

impl std::fmt::Display for ColumnProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "column_profile:{}", self.0)
    }
}

impl std::fmt::Display for ColumnLayoutKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "column_layout:{}", self.0)
    }
}

impl std::fmt::Display for RootProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "root_profile:{}", self.0)
    }
}

impl From<u16> for SchemeId {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl From<SchemeId> for u16 {
    fn from(id: SchemeId) -> Self {
        id.0
    }
}

impl From<u32> for TypeId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<TypeId> for u32 {
    fn from(id: TypeId) -> Self {
        id.0
    }
}

impl From<u32> for EncodingProfileId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<EncodingProfileId> for u32 {
    fn from(id: EncodingProfileId) -> Self {
        id.0
    }
}

impl From<u32> for SchemeProfileId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<SchemeProfileId> for u32 {
    fn from(id: SchemeProfileId) -> Self {
        id.0
    }
}

impl From<u32> for ColumnProfileId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<ColumnProfileId> for u32 {
    fn from(id: ColumnProfileId) -> Self {
        id.0
    }
}

impl From<u16> for RootProfileId {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl From<RootProfileId> for u16 {
    fn from(id: RootProfileId) -> Self {
        id.0
    }
}
