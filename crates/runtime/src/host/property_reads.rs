use std::collections::BTreeMap;

use tabula_core::RowKey;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_profile::is_u64_type;
use tabula_types::{
    TypeRuntimeRegistry, TypedColumnEntry, TypedValue, bool_typed, typed_row_key, u64_typed,
};

use tabula_executor::{PropertyReadExecutor, PropertyReadQuery, PropertyReadRequest};

#[derive(Debug, Clone, Copy)]
enum RowSubsetKind {
    Minimum,
    Maximum,
    Successor,
    Predecessor,
}

#[derive(Default)]
pub(crate) struct V1PropertyReads {
    columns: BTreeMap<(ir::TableId, ir::FieldId), Vec<TypedColumnEntry>>,
}

impl V1PropertyReads {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_column(
        mut self,
        table: ir::TableId,
        field: ir::FieldId,
        rows: Vec<TypedColumnEntry>,
    ) -> Self {
        self.columns.insert((table, field), rows);
        self
    }

    fn get_column(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<&[TypedColumnEntry], TabulaError> {
        self.columns
            .get(&(table, field))
            .map(Vec::as_slice)
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!("missing committed column {}.{}", table.0, field.0))
            })
    }

    fn execute_row_subset_property_query(
        &self,
        request: &PropertyReadRequest,
        pivot: Option<RowKey>,
        kind: RowSubsetKind,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        if request.output_arity != 3 {
            return Err(TabulaError::InvalidIr(
                "row-oriented property reads require exactly 3 outputs".into(),
            ));
        }
        if request.key_arity != 1 || !is_u64_type(request.key_type) {
            return Err(TabulaError::InvalidIr(format!(
                "V1 canonical executor only supports [u64] key schema, table {} declared arity {} with key type {}",
                request.table.0, request.key_arity, request.key_type.0
            )));
        }

        let entries = self.get_column(request.table, request.field)?;
        for entry in entries {
            if entry.value.type_id() != request.field_type {
                return Err(TabulaError::InvalidIr(format!(
                    "committed column {}.{} yielded value type {} but field type is {}",
                    request.table.0,
                    request.field.0,
                    entry.value.type_id().0,
                    request.field_type.0
                )));
            }
        }

        let selected = match kind {
            RowSubsetKind::Minimum => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .min_by_key(|entry| entry.row_key.0),
            RowSubsetKind::Maximum => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .max_by_key(|entry| entry.row_key.0),
            RowSubsetKind::Successor => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .filter(|entry| Some(entry.row_key) > pivot)
                .min_by_key(|entry| entry.row_key.0),
            RowSubsetKind::Predecessor => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .filter(|entry| Some(entry.row_key) < pivot)
                .max_by_key(|entry| entry.row_key.0),
        };

        if let Some(entry) = selected {
            Ok(vec![
                entry.value.clone(),
                row_key_typed(entry.row_key, request.key_type)?,
                bool_typed(false),
            ])
        } else {
            Ok(vec![
                type_runtimes.zero_of(request.field_type)?,
                type_runtimes.zero_of(request.key_type)?,
                bool_typed(true),
            ])
        }
    }
}

impl PropertyReadExecutor for V1PropertyReads {
    fn execute(
        &self,
        request: &PropertyReadRequest,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        match &request.query {
            PropertyReadQuery::Minimum => self.execute_row_subset_property_query(
                request,
                None,
                RowSubsetKind::Minimum,
                type_runtimes,
            ),
            PropertyReadQuery::Maximum => self.execute_row_subset_property_query(
                request,
                None,
                RowSubsetKind::Maximum,
                type_runtimes,
            ),
            PropertyReadQuery::Successor { key } => self.execute_row_subset_property_query(
                request,
                Some(decode_single_row_key(key, type_runtimes)?),
                RowSubsetKind::Successor,
                type_runtimes,
            ),
            PropertyReadQuery::Predecessor { key } => self.execute_row_subset_property_query(
                request,
                Some(decode_single_row_key(key, type_runtimes)?),
                RowSubsetKind::Predecessor,
                type_runtimes,
            ),
            PropertyReadQuery::Aggregate { .. } => Err(TabulaError::InvalidIr(
                "ReadStateProperty Aggregate is not yet supported in V1 adapter".into(),
            )),
            PropertyReadQuery::NonExistenceRange { .. } => Err(TabulaError::InvalidIr(
                "ReadStateProperty NonExistenceRange is not yet supported in V1 adapter".into(),
            )),
        }
    }
}

fn row_key_typed(row: RowKey, key_ty: tabula_core::TypeId) -> Result<TypedValue, TabulaError> {
    if !is_u64_type(key_ty) {
        return Err(TabulaError::InvalidIr(format!(
            "V1 canonical executor only supports u64 key outputs, got {}",
            key_ty.0
        )));
    }
    Ok(u64_typed(row.0))
}

fn decode_single_row_key(
    values: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<RowKey, TabulaError> {
    if values.len() != 1 {
        return Err(TabulaError::InvalidIr(
            "V1 canonical executor only supports single-component state keys".into(),
        ));
    }
    typed_row_key(&values[0], type_runtimes).map_err(|_| {
        TabulaError::InvalidIr(format!(
            "V1 canonical executor expects state keys to be u64, got {}",
            values[0].type_id().0
        ))
    })
}
