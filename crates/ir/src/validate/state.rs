use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;

use super::{FieldId, StateSchema, TableId, TypeRef};

pub(super) struct TableValidationInfo {
    pub(super) key_tys: Vec<TypeRef>,
    pub(super) fields: BTreeMap<FieldId, TypeRef>,
}

pub(super) fn validate_state(
    state: &StateSchema,
) -> Result<BTreeMap<TableId, TableValidationInfo>, TabulaError> {
    let mut tables = BTreeMap::new();
    let mut seen_tables = BTreeSet::new();
    for table in &state.tables {
        if !seen_tables.insert(table.id) {
            return Err(TabulaError::InvalidIr(format!(
                "duplicate table ID {}",
                table.id.0
            )));
        }
        if table.key_tys.is_empty() {
            return Err(TabulaError::InvalidIr(format!(
                "table {} must declare at least one key type",
                table.id.0
            )));
        }
        let mut fields = BTreeMap::new();
        let mut seen_fields = BTreeSet::new();
        for field in &table.fields {
            if !seen_fields.insert(field.id) {
                return Err(TabulaError::InvalidIr(format!(
                    "duplicate field ID {} in table {}",
                    field.id.0, table.id.0
                )));
            }
            fields.insert(field.id, field.ty);
        }
        tables.insert(
            table.id,
            TableValidationInfo {
                key_tys: table.key_tys.clone(),
                fields,
            },
        );
    }
    Ok(tables)
}
