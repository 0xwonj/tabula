//! Program: holds tx type definitions with type info, resolves `TxTypeId`.
//!
//! Registration pipeline: `canonicalize` → `typecheck` → `validate`.

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_core::{TableId, TableSchema, TxTypeId};

use crate::instruction::{PrecompileId, PropertyQueryKind, PropertyRequirement};
use crate::pass::{BodyTypeInfo, canonicalize, typecheck, validate};
use crate::{Instruction, TxTypeDef};

/// Holds registered transaction type definitions.
#[derive(Debug, Clone)]
pub struct Program {
    types: BTreeMap<TxTypeId, TxTypeDef>,
    type_info: BTreeMap<TxTypeId, BodyTypeInfo>,
    schemas: BTreeMap<TableId, TableSchema>,
}

impl Program {
    /// Create an empty program.
    pub fn new() -> Self {
        Self {
            types: BTreeMap::new(),
            type_info: BTreeMap::new(),
            schemas: BTreeMap::new(),
        }
    }

    /// Register a table schema. Must be called before `register()` so
    /// that type inference can use column type information.
    pub fn add_schema(&mut self, schema: TableSchema) {
        self.schemas.insert(schema.id, schema);
    }

    /// Register a transaction type definition.
    ///
    /// Pipeline: canonicalize → typecheck → validate NF.
    /// Returns an error if the body contains type mismatches, SSA violations,
    /// or remaining NF violations that cannot be auto-fixed.
    pub fn register(&mut self, mut def: TxTypeDef) -> Result<(), TabulaError> {
        def.body = canonicalize::canonicalize(def.body);
        let info = typecheck::check(&def, &self.schemas)?;
        validate::check_normal_form(&def.body)?;
        self.type_info.insert(def.id, info);
        self.types.insert(def.id, def);
        Ok(())
    }

    /// Resolve a `TxTypeId` to its definition.
    pub fn resolve(&self, id: TxTypeId) -> Result<&TxTypeDef, TabulaError> {
        self.types.get(&id).ok_or(TabulaError::TxTypeNotFound(id))
    }

    /// Get the inferred type info for a registered tx type.
    pub fn type_info(&self, id: TxTypeId) -> Option<&BodyTypeInfo> {
        self.type_info.get(&id)
    }

    /// Return all registered type definitions.
    pub fn all_types(&self) -> Vec<&TxTypeDef> {
        self.types.values().collect()
    }

    /// Return the table schemas.
    pub fn schemas(&self) -> &BTreeMap<TableId, TableSchema> {
        &self.schemas
    }

    /// Collect all `TableId` values referenced by state-accessing instructions.
    ///
    /// Scans all tx bodies for `Read`, `Write`, and `PropertyRead` instructions.
    /// `Lookup` (static table) references are excluded — those are validated
    /// separately via `StaticTableProvider`.
    pub fn referenced_table_ids(&self) -> BTreeSet<TableId> {
        let mut ids = BTreeSet::new();
        for def in self.types.values() {
            for instr in &def.body {
                match instr {
                    Instruction::Read { table, .. }
                    | Instruction::Write { table, .. }
                    | Instruction::PropertyRead { table, .. } => {
                        ids.insert(*table);
                    }
                    _ => {}
                }
            }
        }
        ids
    }

    /// Collect all `PrecompileId` values referenced by `Precompile` instructions.
    pub fn referenced_precompile_ids(&self) -> BTreeSet<PrecompileId> {
        let mut ids = BTreeSet::new();
        for def in self.types.values() {
            for instr in &def.body {
                if let Instruction::Precompile { id, .. } = instr {
                    ids.insert(*id);
                }
            }
        }
        ids
    }

    /// Collect all structural property query kinds referenced by `PropertyRead`.
    pub fn referenced_property_query_kinds(&self) -> BTreeSet<PropertyQueryKind> {
        let mut kinds = BTreeSet::new();
        for def in self.types.values() {
            for instr in &def.body {
                if let Instruction::PropertyRead { query, .. } = instr {
                    kinds.insert(query.kind());
                }
            }
        }
        kinds
    }

    /// Collect exact structural property requirements referenced by `PropertyRead`.
    pub fn referenced_property_requirements(&self) -> BTreeSet<PropertyRequirement> {
        let mut requirements = BTreeSet::new();
        for def in self.types.values() {
            for instr in &def.body {
                if let Instruction::PropertyRead {
                    table, col, query, ..
                } = instr
                {
                    requirements.insert(PropertyRequirement {
                        table_id: *table,
                        col_id: *col,
                        query_kind: query.kind(),
                    });
                }
            }
        }
        requirements
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}
