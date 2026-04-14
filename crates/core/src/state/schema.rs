//! Sealed state and execution contract definitions.

use std::collections::BTreeSet;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    ColId, ColumnProfileId, EncodingProfileId, ProgramMachineShape, PropertyQueryKind, TableId,
    TypeId,
};

/// One logical key component in declaration order.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct KeyComponentSchema {
    /// Source-level key component symbol.
    pub symbol: String,
    /// Logical semantic type.
    pub ty: TypeId,
}

/// Canonical ordering family used by one committed key contract.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(deny_unknown_fields)]
pub enum KeyOrderingFamily {
    /// Compare committed keys lexicographically by encoded component order.
    LexicographicByComponent,
    /// Reserved escape hatch for future custom ordering families.
    Opaque {
        /// Stable family label.
        family: String,
    },
}

/// Fixed committed-key layout for one table.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CommittedKeyLayout {
    /// Canonical committed-key width in bytes.
    pub byte_width: u16,
    /// Canonical committed-key width in field elements.
    pub fe_width: u16,
}

/// Compiler-sealed committed-key contract for one table.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct TableKeyContract {
    /// Logical key components in declaration order.
    pub components: Vec<KeyComponentSchema>,
    /// Encoding profile selected for each key component.
    pub component_encoding_profile_ids: Vec<EncodingProfileId>,
    /// Committed-key ordering contract.
    pub ordering_family: KeyOrderingFamily,
    /// Canonical committed-key layout.
    pub committed_layout: CommittedKeyLayout,
}

/// Compiler-sealed state column contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StateColumnContract {
    /// Column identifier.
    pub id: ColId,
    /// Human-readable source column name.
    pub name: String,
    /// Logical semantic type of the column value.
    pub ty: TypeId,
    /// Sealed per-column profile selected during compiler registration.
    pub column_profile_id: ColumnProfileId,
    /// Exact property-query kinds required by the program for this column.
    pub required_property_queries: BTreeSet<PropertyQueryKind>,
}

/// Compiler-sealed state table contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StateTableContract {
    /// Table identifier.
    pub id: TableId,
    /// Human-readable source table name.
    pub name: String,
    /// Logical + committed key contract.
    pub key: TableKeyContract,
    /// Ordered column contracts.
    pub columns: Vec<StateColumnContract>,
}

/// Compiler-sealed state contract for the whole program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StateContract {
    /// All user-state tables in deterministic order.
    pub tables: Vec<StateTableContract>,
}

/// Compiler-sealed execution contract for runtime, proving, and public schema projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProgramExecutionContract {
    /// Sealed user-state contract.
    pub state: StateContract,
    /// Sealed machine geometry contract.
    pub machine_shape: ProgramMachineShape,
}
