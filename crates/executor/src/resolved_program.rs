//! Canonical hot-path execution contract.
//!
//! This module owns the resolved execution view consumed by the executor.
//! Runtime constructs this contract, and the executor runs it without needing
//! to rediscover schema or profile metadata on the instruction hot path.

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId, TxTypeId, TypeId};
use tabula_ir::{Instruction, ParamDef, Program};

/// Resolved execution metadata for one committed column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedColumnLayout {
    /// Canonical semantic type id for this column.
    pub type_id: TypeId,
}

/// Resolved execution definition for one transaction type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTxDefinition {
    /// Transaction type identifier.
    pub tx_type: TxTypeId,
    /// Ordered parameter schema for this transaction type.
    pub param_schema: Vec<ParamDef>,
    /// Canonical IR body.
    pub body: Vec<Instruction>,
}

/// Canonical executor-owned resolved execution contract.
#[derive(Debug, Clone)]
pub struct ResolvedExecutionProgram {
    tx_definitions: BTreeMap<TxTypeId, ResolvedTxDefinition>,
    columns: BTreeMap<(TableId, ColId), ResolvedColumnLayout>,
    tables: BTreeSet<TableId>,
}

impl ResolvedExecutionProgram {
    /// Construct the canonical execution contract directly from resolved parts.
    pub fn new(
        tx_definitions: BTreeMap<TxTypeId, ResolvedTxDefinition>,
        columns: BTreeMap<(TableId, ColId), ResolvedColumnLayout>,
    ) -> Self {
        let tables = columns.keys().map(|(table, _)| *table).collect();
        Self {
            tx_definitions,
            columns,
            tables,
        }
    }

    /// Resolve a raw IR program into the canonical execution contract.
    pub fn from_program(program: &Program) -> Result<Self, TabulaError> {
        let tx_definitions = program
            .all_types()
            .into_iter()
            .map(|def| {
                (
                    def.id,
                    ResolvedTxDefinition {
                        tx_type: def.id,
                        param_schema: def.param_schema.clone(),
                        body: def.body.clone(),
                    },
                )
            })
            .collect();

        let mut columns = BTreeMap::new();
        for (table_id, schema) in program.schemas() {
            for column in &schema.columns {
                let resolved = program
                    .profile_catalog()
                    .resolve_column_profile(column.column_profile_id)
                    .map_err(|err| {
                        TabulaError::InvalidIr(format!(
                            "column profile {} for table {:?} col {:?} is invalid: {err}",
                            column.column_profile_id.0, table_id, column.id
                        ))
                    })?;
                columns.insert(
                    (*table_id, column.id),
                    ResolvedColumnLayout {
                        type_id: resolved.type_descriptor.type_id,
                    },
                );
            }
        }
        Ok(Self::new(tx_definitions, columns))
    }

    /// Resolve one transaction type definition by id.
    pub fn tx_definition(&self, tx_type: TxTypeId) -> Result<&ResolvedTxDefinition, TabulaError> {
        self.tx_definitions
            .get(&tx_type)
            .ok_or(TabulaError::TxTypeNotFound(tx_type))
    }

    /// Resolve one committed column layout by `(table, col)`.
    pub fn column_layout(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<&ResolvedColumnLayout, TabulaError> {
        if !self.tables.contains(&table) {
            return Err(TabulaError::TableNotFound(table));
        }
        self.columns.get(&(table, col)).ok_or_else(|| {
            TabulaError::InvalidIr(format!("column {col:?} not found in table {table:?}"))
        })
    }

    /// Return whether `(table, col)` is part of the declared execution state surface.
    pub fn has_column(&self, table: TableId, col: ColId) -> bool {
        self.columns.contains_key(&(table, col))
    }
}
