//! DB-IR instruction set: the language of Tabula transaction bodies.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::types::{ColId, RowKey, TableId, Value};

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

/// A boolean predicate used by ASSERT instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Predicate {
    /// Left equals right.
    Eq(ValueExpr, ValueExpr),
    /// Left is strictly less than right.
    Lt(ValueExpr, ValueExpr),
    /// Left is less than or equal to right.
    Lte(ValueExpr, ValueExpr),
    /// Left is strictly greater than right.
    Gt(ValueExpr, ValueExpr),
    /// Left is greater than or equal to right.
    Gte(ValueExpr, ValueExpr),
    /// Value is not null.
    NotNull(ValueExpr),
    /// Logical AND of two predicates.
    And(Box<Predicate>, Box<Predicate>),
    /// Logical OR of two predicates.
    Or(Box<Predicate>, Box<Predicate>),
    /// Logical NOT of a predicate.
    Not(Box<Predicate>),
}

/// A single DB-IR instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Read a cell from state, store result in `dst`.
    Read {
        /// Destination slot.
        dst: Slot,
        /// Table to read from.
        table: TableId,
        /// Row expression.
        row: RowExpr,
        /// Column to read.
        col: ColId,
    },

    /// Write a value to a cell in state.
    Write {
        /// Table to write to.
        table: TableId,
        /// Row expression.
        row: RowExpr,
        /// Column to write.
        col: ColId,
        /// Source value expression.
        src: ValueExpr,
    },

    /// Lookup in a static (fixed) table, store result in `dst`.
    Lookup {
        /// Destination slot.
        dst: Slot,
        /// Static table to lookup from.
        static_table: TableId,
        /// Key expression.
        key: RowExpr,
        /// Column to read.
        col: ColId,
    },

    /// `dst = lhs + rhs`
    Add {
        /// Destination slot.
        dst: Slot,
        /// Left operand.
        lhs: ValueExpr,
        /// Right operand.
        rhs: ValueExpr,
    },

    /// `dst = lhs - rhs`
    Sub {
        /// Destination slot.
        dst: Slot,
        /// Left operand.
        lhs: ValueExpr,
        /// Right operand.
        rhs: ValueExpr,
    },

    /// `dst = lhs * rhs`
    Mul {
        /// Destination slot.
        dst: Slot,
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

    /// Assert a predicate. Execution of this tx fails if false.
    Assert {
        /// The predicate to check.
        predicate: Predicate,
    },

    /// Hash inputs, store result in `dst` as `Value::Bytes32`.
    Hash {
        /// Destination slot.
        dst: Slot,
        /// Values to hash.
        inputs: Vec<ValueExpr>,
    },

    /// Emit an application event.
    Emit {
        /// Topic identifier.
        topic: Vec<u8>,
        /// Data values to include.
        data: Vec<ValueExpr>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_borsh_round_trip() {
        let instr = Instruction::Read {
            dst: 0,
            table: TableId(1),
            row: RowExpr::Param(0),
            col: ColId(0),
        };
        let bytes = borsh::to_vec(&instr).unwrap();
        let decoded: Instruction = borsh::from_slice(&bytes).unwrap();
        assert_eq!(instr, decoded);
    }

    #[test]
    fn test_predicate_borsh_round_trip() {
        let pred = Predicate::And(
            Box::new(Predicate::Gte(ValueExpr::Slot(0), ValueExpr::Param(2))),
            Box::new(Predicate::NotNull(ValueExpr::Slot(1))),
        );
        let bytes = borsh::to_vec(&pred).unwrap();
        let decoded: Predicate = borsh::from_slice(&bytes).unwrap();
        assert_eq!(pred, decoded);
    }
}
