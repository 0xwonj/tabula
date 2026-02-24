//! Error types for debug constraint checking.

use std::fmt;

/// Error from a failed constraint check.
#[derive(Clone, Debug)]
pub struct ConstraintError {
    /// Row index where the violation occurred.
    pub row: usize,
    /// Index of the failing constraint (0-based within that row's eval).
    pub constraint_index: usize,
    /// The nonzero value of the failing constraint.
    pub value: String,
}

impl fmt::Display for ConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "constraint {} failed on row {}: value = {}",
            self.constraint_index, self.row, self.value
        )
    }
}

impl std::error::Error for ConstraintError {}

/// Error from a failed multi-chip LogUp check.
#[derive(Clone, Debug)]
pub enum MultiChipError {
    /// A local/transition constraint failed.
    Constraint {
        /// Which chip (by name).
        chip: String,
        /// The constraint error.
        error: ConstraintError,
    },
    /// LogUp balance failed: global sum is nonzero.
    LogUpImbalance {
        /// Human-readable description of the imbalance.
        description: String,
    },
}

impl fmt::Display for MultiChipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constraint { chip, error } => {
                write!(f, "[{chip}] {error}")
            }
            Self::LogUpImbalance { description } => {
                write!(f, "LogUp imbalance: {description}")
            }
        }
    }
}

impl std::error::Error for MultiChipError {}
