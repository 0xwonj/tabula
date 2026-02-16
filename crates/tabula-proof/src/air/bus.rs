//! Interaction bus types for cross-chip LogUp arguments.
//!
//! Each chip declares which buses it sends/receives on. The actual LogUp
//! wiring is deferred to M9 (prover integration); M6 only defines the types.

/// Named interaction channels for cross-chip LogUp.
///
/// Each variant identifies a logical bus that connects two (or more) chips.
/// The bus name determines the LogUp fingerprint namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InteractionKind {
    /// Execution trace <-> GlobalSortedMem (memory consistency).
    Memory,
    /// GlobalSortedMem init rows <-> GlobalSSMC (membership proof).
    SsmcMembership,
    /// GlobalMerge <-> GlobalSSMC + WriteSet (completeness).
    MergeCompleteness,
    /// Any chip <-> ColumnMeta (metadata join).
    ColumnMetaJoin,
    /// Execution trace -> Range check table.
    RangeCheck,
    /// Execution <-> VC opening for read-only keys (no state update).
    ReadOnlyOpening,
    /// Any chip <-> PoseidonChip (shared permutation, M8).
    PoseidonPermutation,
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
