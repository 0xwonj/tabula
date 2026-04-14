//! Runtime transaction types: concrete transactions and batches.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{PortableValue, TxTypeId};

/// Current native machine capability ceiling shared by registration and proof/runtime.
pub const NATIVE_MAX_SLOTS: u16 = 16;
/// Current native committed-key component ceiling.
pub const NATIVE_MAX_KEY_COMPONENTS: u16 = 1;
/// Current native committed-key field-element width ceiling.
pub const NATIVE_MAX_KEY_FES: u16 = 3;

/// A concrete transaction in a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Transaction {
    /// Which transaction type to execute.
    pub tx_type: TxTypeId,
    /// Concrete parameter values.
    pub params: Vec<PortableValue>,
}

/// Program-level resource budgets for DoS prevention.
///
/// **Status: data structure only** — enforcement is not yet implemented
/// in the executor, prover, verifier, or IR validation pipeline. See
/// semantics-spec §1.8.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ProgramBudgets {
    /// Maximum number of IR instructions per transaction.
    pub max_ops: u32,
    /// Maximum number of SSA slots per transaction.
    pub max_slots: u16,
    /// Maximum number of state accesses (reads + writes) per transaction.
    pub max_accesses: u32,
}

/// Program-level machine shape sealed into the registered artifact.
///
/// Unlike [`ProgramBudgets`], this describes proof geometry requirements rather
/// than execution resource ceilings.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ProgramMachineShape {
    /// Maximum number of SSA slots required by any entry body.
    pub max_slots: u16,
    /// Maximum number of logical key components required by any state table.
    pub max_key_components: u16,
    /// Maximum committed-key width in field elements required by any state table.
    pub max_key_fes: u16,
}

/// Capability ceiling of the current native runtime/proof stack.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct MachineCapabilities {
    /// Maximum number of SSA slots supported by the current machine.
    pub max_slots: u16,
    /// Maximum number of logical key components supported natively.
    pub max_key_components: u16,
    /// Maximum committed-key width supported by the proof/runtime stack.
    pub max_key_fes: u16,
}

impl MachineCapabilities {
    /// Seed the current native capability ceiling.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            max_slots: NATIVE_MAX_SLOTS,
            max_key_components: NATIVE_MAX_KEY_COMPONENTS,
            max_key_fes: NATIVE_MAX_KEY_FES,
        }
    }

    /// Whether this machine can natively satisfy the requested shape.
    #[must_use]
    pub const fn supports(&self, shape: ProgramMachineShape) -> bool {
        shape.max_slots <= self.max_slots
            && shape.max_key_components <= self.max_key_components
            && shape.max_key_fes <= self.max_key_fes
    }
}

/// An ordered batch of transactions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Batch {
    /// The transactions in execution order.
    pub transactions: Vec<Transaction>,
}
