//! Interaction bus types for cross-chip LogUp arguments.
//!
//! Eight LogUp buses, each identified by an `InteractionKind` tag with an
//! integer discriminant used in the RLC fingerprint (m9-design §3.2).

/// Named interaction channels for cross-chip LogUp.
///
/// Each variant identifies a logical bus connecting two or more chips.
/// Integer discriminants are used as `kind_tag` in the RLC fingerprint
/// formula: `f = α + β^0 · kind_tag + β^1 · values[0] + ...`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InteractionKind {
    /// Execution trace <-> GlobalSortedMem (memory consistency).
    Memory = 1,
    /// GlobalSortedMem init rows <-> GlobalSSMC (membership proof).
    SsmcMembership = 2,
    /// GlobalSSMC -> GlobalMerge (OldList completeness).
    MergeOldList = 3,
    /// GlobalSortedMem write-set -> GlobalMerge (WriteSet completeness).
    MergeWriteSet = 4,
    /// SSMC/Merge hash chains <-> PoseidonChip (permutation verification).
    PoseidonPermutation = 5,
    /// SSMC/Merge segment hashes <-> ColumnMeta (commitment verification).
    CommitmentVerification = 6,
    /// GlobalSortedMem first-of-segment <-> ColumnMeta (metadata).
    SortedMemMeta = 7,
    /// All chips -> RangeCheckChip (u16 range proofs).
    RangeCheck = 8,
}

/// Direction of an interaction: send or receive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionDirection {
    /// This chip sends tuples into the bus.
    Send,
    /// This chip receives tuples from the bus.
    Receive,
}

/// Declaration of a chip's participation in a LogUp bus.
///
/// Pure data — no Plonky3 types. Column indices are relative to the
/// chip's own trace width. Wired in M9.
#[derive(Clone, Debug)]
pub struct InteractionDecl {
    /// Which bus this interaction belongs to.
    pub kind: InteractionKind,
    /// Send or receive.
    pub direction: InteractionDirection,
    /// Column indices that form the interaction tuple.
    pub column_indices: Vec<usize>,
    /// Column index for the multiplicity selector.
    pub multiplicity_index: usize,
}
