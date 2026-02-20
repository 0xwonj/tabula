//! Program: holds tx type definitions with type info, resolves `TxTypeId`.
//!
//! Registration pipeline: `canonicalize` → `typecheck` → `validate`.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{TableId, TableSchema, TxTypeId};

use crate::TxTypeDef;
use crate::pass::{BodyTypeInfo, canonicalize, typecheck, validate};

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

    /// Register without NF validation (canonicalize + typecheck only).
    ///
    /// Use for compile/check where NF-4 (ambiguous alias) is too strict
    /// for common patterns like transfers with `Param(a)` vs `Param(b)`.
    pub fn register_lenient(&mut self, mut def: TxTypeDef) -> Result<(), TabulaError> {
        def.body = canonicalize::canonicalize(def.body);
        let info = typecheck::check(&def, &self.schemas)?;
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
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}
