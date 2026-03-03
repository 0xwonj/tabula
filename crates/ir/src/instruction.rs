//! Tabula IR (TIR) instruction set: the language of Tabula transaction bodies.

use std::cmp::Ordering;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId, Value};

/// Slot index for local variables within a tx execution.
pub type Slot = u16;

/// Where a row key comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum RowExpr {
    /// Hardcoded row key.
    Literal(RowKey),
    /// Cast slot value to row key.
    Slot(Slot),
    /// Transaction parameter index, cast to row key.
    Param(u16),
}

/// Where a value comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ValueExpr {
    /// Hardcoded value.
    Literal(Value),
    /// Reference to a local variable slot.
    Slot(Slot),
    /// Transaction parameter index.
    Param(u16),
}

/// Arithmetic operation kind.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ArithOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
}

impl ArithOp {
    /// Apply this arithmetic operation to two values.
    pub fn apply(&self, lhs: &Value, rhs: &Value) -> Result<Value, TabulaError> {
        match self {
            Self::Add => lhs.checked_add(rhs),
            Self::Sub => lhs.checked_sub(rhs),
            Self::Mul => lhs.checked_mul(rhs),
        }
    }
}

/// Comparison operation kind.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CmpOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
}

impl RowExpr {
    /// Apply a function to the inner `Slot`, if this is a `Slot` variant.
    pub fn map_slot(self, f: &impl Fn(Slot) -> Slot) -> Self {
        match self {
            Self::Slot(s) => Self::Slot(f(s)),
            other => other,
        }
    }
}

impl ValueExpr {
    /// Apply a function to the inner `Slot`, if this is a `Slot` variant.
    pub fn map_slot(self, f: &impl Fn(Slot) -> Slot) -> Self {
        match self {
            Self::Slot(s) => Self::Slot(f(s)),
            other => other,
        }
    }
}

impl CmpOp {
    /// Apply this comparison to two values, producing `Value::Bool`.
    pub fn apply(&self, lhs: &Value, rhs: &Value) -> Result<Value, TabulaError> {
        let b = match self {
            Self::Eq => Ok(lhs == rhs),
            Self::Ne => Ok(lhs != rhs),
            Self::Lt => lhs.compare(rhs).map(|o| o == Ordering::Less),
            Self::Lte => lhs.compare(rhs).map(|o| o != Ordering::Greater),
            Self::Gt => lhs.compare(rhs).map(|o| o == Ordering::Greater),
            Self::Gte => lhs.compare(rhs).map(|o| o != Ordering::Less),
        }?;
        Ok(Value::Bool(b))
    }
}

/// A single Tabula IR instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Read a cell from state, store value in `dst_val` and null flag in `dst_is_null`.
    Read {
        /// Destination slot for the value (canonical zero if absent).
        dst_val: Slot,
        /// Destination slot for the null flag (Bool: true = absent).
        dst_is_null: Slot,
        /// Table to read from.
        table: TableId,
        /// Column to read.
        col: ColId,
        /// Row expression.
        row: RowExpr,
    },

    /// Write a value to a cell in state.
    Write {
        /// Table to write to.
        table: TableId,
        /// Column to write.
        col: ColId,
        /// Row expression.
        row: RowExpr,
        /// Source value expression.
        src_val: ValueExpr,
        /// Source null flag expression (Bool: true = delete).
        src_is_null: ValueExpr,
    },

    /// Lookup in a static (fixed) table, store result in `dst`.
    Lookup {
        /// Destination slot.
        dst: Slot,
        /// Static table to lookup from.
        static_table: TableId,
        /// Column to read.
        col: ColId,
        /// Row expression.
        row: RowExpr,
    },

    /// `dst = lhs <op> rhs` where op is Add, Sub, or Mul.
    Arith {
        /// Destination slot.
        dst: Slot,
        /// Arithmetic operation.
        op: ArithOp,
        /// Left operand.
        lhs: ValueExpr,
        /// Right operand.
        rhs: ValueExpr,
    },

    /// `dst_q = lhs / rhs`, `dst_r = lhs % rhs`
    DivMod {
        /// Destination slot for quotient.
        dst_q: Slot,
        /// Destination slot for remainder.
        dst_r: Slot,
        /// Left operand (dividend).
        lhs: ValueExpr,
        /// Right operand (divisor).
        rhs: ValueExpr,
    },

    /// Compare two values, producing a Bool in `dst`.
    Cmp {
        /// Destination slot.
        dst: Slot,
        /// Comparison operation.
        op: CmpOp,
        /// Left operand.
        lhs: ValueExpr,
        /// Right operand.
        rhs: ValueExpr,
    },

    /// Logical NOT: `dst = !src` (Bool → Bool).
    Not {
        /// Destination slot.
        dst: Slot,
        /// Source expression (must be Bool).
        src: ValueExpr,
    },

    /// Logical AND: `dst = lhs && rhs` (Bool × Bool → Bool).
    And {
        /// Destination slot.
        dst: Slot,
        /// Left operand (must be Bool).
        lhs: ValueExpr,
        /// Right operand (must be Bool).
        rhs: ValueExpr,
    },

    /// Logical OR: `dst = lhs || rhs` (Bool × Bool → Bool).
    Or {
        /// Destination slot.
        dst: Slot,
        /// Left operand (must be Bool).
        lhs: ValueExpr,
        /// Right operand (must be Bool).
        rhs: ValueExpr,
    },

    /// Assert a boolean condition. Execution fails if `cond` is not `Bool(true)`.
    Assert {
        /// The condition to check (must be Bool).
        cond: ValueExpr,
    },

    /// Hash inputs, store result in `dst` as `Value::Bytes32`.
    Hash {
        /// Destination slot.
        dst: Slot,
        /// Values to hash.
        inputs: Vec<ValueExpr>,
    },

    /// Conditional select: `dst = if cond then if_true else if_false`.
    ///
    /// `cond` must evaluate to `Bool`. Constraint: `dst = cond·if_true + (1-cond)·if_false`.
    Select {
        /// Destination slot.
        dst: Slot,
        /// Condition (must be Bool).
        cond: ValueExpr,
        /// Value when condition is true.
        if_true: ValueExpr,
        /// Value when condition is false.
        if_false: ValueExpr,
    },

    /// Emit an application event.
    Emit {
        /// Topic identifier.
        topic: Vec<u8>,
        /// Data values to include.
        data: Vec<ValueExpr>,
    },
}

impl Instruction {
    /// Apply a function to every `Slot` reference (destinations and sources).
    pub fn map_slots(self, f: &impl Fn(Slot) -> Slot) -> Self {
        match self {
            Self::Read {
                dst_val,
                dst_is_null,
                table,
                col,
                row,
            } => Self::Read {
                dst_val: f(dst_val),
                dst_is_null: f(dst_is_null),
                table,
                col,
                row: row.map_slot(f),
            },
            Self::Write {
                table,
                col,
                row,
                src_val,
                src_is_null,
            } => Self::Write {
                table,
                col,
                row: row.map_slot(f),
                src_val: src_val.map_slot(f),
                src_is_null: src_is_null.map_slot(f),
            },
            Self::Lookup {
                dst,
                static_table,
                col,
                row,
            } => Self::Lookup {
                dst: f(dst),
                static_table,
                col,
                row: row.map_slot(f),
            },
            Self::Arith { dst, op, lhs, rhs } => Self::Arith {
                dst: f(dst),
                op,
                lhs: lhs.map_slot(f),
                rhs: rhs.map_slot(f),
            },
            Self::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => Self::DivMod {
                dst_q: f(dst_q),
                dst_r: f(dst_r),
                lhs: lhs.map_slot(f),
                rhs: rhs.map_slot(f),
            },
            Self::Cmp { dst, op, lhs, rhs } => Self::Cmp {
                dst: f(dst),
                op,
                lhs: lhs.map_slot(f),
                rhs: rhs.map_slot(f),
            },
            Self::Not { dst, src } => Self::Not {
                dst: f(dst),
                src: src.map_slot(f),
            },
            Self::And { dst, lhs, rhs } => Self::And {
                dst: f(dst),
                lhs: lhs.map_slot(f),
                rhs: rhs.map_slot(f),
            },
            Self::Or { dst, lhs, rhs } => Self::Or {
                dst: f(dst),
                lhs: lhs.map_slot(f),
                rhs: rhs.map_slot(f),
            },
            Self::Assert { cond } => Self::Assert {
                cond: cond.map_slot(f),
            },
            Self::Hash { dst, inputs } => Self::Hash {
                dst: f(dst),
                inputs: inputs.into_iter().map(|e| e.map_slot(f)).collect(),
            },
            Self::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => Self::Select {
                dst: f(dst),
                cond: cond.map_slot(f),
                if_true: if_true.map_slot(f),
                if_false: if_false.map_slot(f),
            },
            Self::Emit { topic, data } => Self::Emit {
                topic,
                data: data.into_iter().map(|e| e.map_slot(f)).collect(),
            },
        }
    }

    /// Return all destination slots defined by this instruction.
    pub fn dst_slots(&self) -> Vec<Slot> {
        match self {
            Self::Read {
                dst_val,
                dst_is_null,
                ..
            } => vec![*dst_val, *dst_is_null],
            Self::DivMod { dst_q, dst_r, .. } => vec![*dst_q, *dst_r],
            Self::Lookup { dst, .. }
            | Self::Arith { dst, .. }
            | Self::Cmp { dst, .. }
            | Self::Not { dst, .. }
            | Self::And { dst, .. }
            | Self::Or { dst, .. }
            | Self::Hash { dst, .. }
            | Self::Select { dst, .. } => vec![*dst],
            Self::Write { .. } | Self::Assert { .. } | Self::Emit { .. } => vec![],
        }
    }
}
