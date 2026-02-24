//! Error types for debug constraint checking.

/// Error from a failed constraint check.
#[derive(Clone, Debug, thiserror::Error)]
#[error("constraint {constraint_index} failed on row {row}: value = {value}")]
pub struct ConstraintError {
    /// Row index where the violation occurred.
    pub row: usize,
    /// Index of the failing constraint (0-based within that row's eval).
    pub constraint_index: usize,
    /// The nonzero value of the failing constraint.
    pub value: String,
}

/// Error from a failed multi-chip LogUp check.
#[derive(Clone, Debug, thiserror::Error)]
pub enum MultiChipError {
    /// A local/transition constraint failed.
    #[error("[{chip}] {error}")]
    Constraint {
        /// Which chip (by name).
        chip: String,
        /// The constraint error.
        #[source]
        error: ConstraintError,
    },
    /// LogUp balance failed: global sum is nonzero.
    #[error("LogUp imbalance: {description}")]
    LogUpImbalance {
        /// Human-readable description of the imbalance.
        description: String,
    },
}
