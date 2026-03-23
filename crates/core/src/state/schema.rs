//! Table and column schema definitions.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{ColId, ColumnProfileId, TableId};

/// A column definition within a table schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ColumnDef {
    /// Column identifier.
    pub id: ColId,
    /// Human-readable name.
    pub name: String,
    /// Sealed per-column profile selected during compiler registration.
    pub column_profile_id: ColumnProfileId,
}

/// Schema definition for a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TableSchema {
    /// Table identifier.
    pub id: TableId,
    /// Human-readable name.
    pub name: String,
    /// Ordered column definitions.
    pub columns: Vec<ColumnDef>,
}
