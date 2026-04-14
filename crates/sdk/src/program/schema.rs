//! Schema handle types: symbol-indexed views over the program's static schema.

use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_compiler::RegisteredProgram;
use tabula_core::KeyComponentSchema;
use tabula_ir as ir;

use crate::error::SdkError;

/// Handle to a single entry parameter, carrying its source name and type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterHandle {
    symbol: String,
    ty: ir::TypeRef,
}

impl ParameterHandle {
    /// Source-level parameter name.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Parameter value type.
    pub fn ty(&self) -> ir::TypeRef {
        self.ty
    }
}

/// Handle to a single state table column, carrying its ID, name, and type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldHandle {
    table_id: ir::TableId,
    field_id: ir::FieldId,
    symbol: String,
    ty: ir::TypeRef,
}

impl FieldHandle {
    /// ID of the table that owns this field.
    pub fn table_id(&self) -> ir::TableId {
        self.table_id
    }

    /// Unique field identifier within the program.
    pub fn id(&self) -> ir::FieldId {
        self.field_id
    }

    /// Source-level field name.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Field value type.
    pub fn ty(&self) -> ir::TypeRef {
        self.ty
    }
}

/// Handle to a state table, carrying its ID, name, key arity, and fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyComponentHandle {
    symbol: String,
    ty: ir::TypeRef,
}

impl KeyComponentHandle {
    /// Source-level key component name.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Key component value type.
    pub fn ty(&self) -> ir::TypeRef {
        self.ty
    }

    pub(crate) fn as_parameter_handle(&self) -> ParameterHandle {
        ParameterHandle {
            symbol: self.symbol.clone(),
            ty: self.ty,
        }
    }
}

/// Handle to a state table, carrying its ID, name, key components, and fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableHandle {
    id: ir::TableId,
    symbol: String,
    key_components: Vec<KeyComponentHandle>,
    fields: Vec<FieldHandle>,
}

impl TableHandle {
    /// Unique table identifier within the program.
    pub fn id(&self) -> ir::TableId {
        self.id
    }

    /// Source-level table name.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Number of key columns in the table's primary key.
    pub fn key_arity(&self) -> usize {
        self.key_components.len()
    }

    /// Ordered logical key components for the table.
    pub fn key_components(&self) -> &[KeyComponentHandle] {
        &self.key_components
    }

    /// All non-key fields declared on the table.
    pub fn fields(&self) -> &[FieldHandle] {
        &self.fields
    }

    /// Look up a field by source symbol; returns an error if not found.
    pub fn field(&self, symbol: &str) -> Result<FieldHandle, SdkError> {
        self.fields
            .iter()
            .find(|field| field.symbol == symbol)
            .cloned()
            .ok_or_else(|| SdkError::SchemaLookup {
                detail: format!("unknown field `{}` on table `{}`", symbol, self.symbol),
            })
    }
}

/// Handle to a transaction entry, carrying its ID, name, and parameter list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxHandle {
    id: ir::EntryId,
    symbol: String,
    params: Vec<ParameterHandle>,
}

impl TxHandle {
    /// Unique entry identifier within the program.
    pub fn id(&self) -> ir::EntryId {
        self.id
    }

    /// Source-level transaction name.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Ordered parameter handles for this transaction.
    pub fn params(&self) -> &[ParameterHandle] {
        &self.params
    }
}

/// Handle to a query entry, carrying its ID, name, parameter list, and return types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHandle {
    id: ir::EntryId,
    symbol: String,
    params: Vec<ParameterHandle>,
    returns: Vec<ir::TypeRef>,
}

impl QueryHandle {
    /// Unique entry identifier within the program.
    pub fn id(&self) -> ir::EntryId {
        self.id
    }

    /// Source-level query name.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Ordered parameter handles for this query.
    pub fn params(&self) -> &[ParameterHandle] {
        &self.params
    }

    /// Return value types in declaration order.
    pub fn returns(&self) -> &[ir::TypeRef] {
        &self.returns
    }
}

/// Handle to a public context field, carrying its ID, name, and type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFieldHandle {
    id: ir::ContextFieldId,
    symbol: String,
    ty: ir::TypeRef,
}

impl ContextFieldHandle {
    /// Unique context field identifier within the program.
    pub fn id(&self) -> ir::ContextFieldId {
        self.id
    }

    /// Source-level context field name.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Context field value type.
    pub fn ty(&self) -> ir::TypeRef {
        self.ty
    }
}

/// Symbol-indexed view over a program's complete static schema.
///
/// `Schema` is cheap to clone (it wraps an `Arc`).
#[derive(Debug, Clone)]
pub struct Schema {
    inner: Arc<SchemaInner>,
}

#[derive(Debug)]
struct SchemaInner {
    tables: Vec<TableHandle>,
    txs: Vec<TxHandle>,
    queries: Vec<QueryHandle>,
    context_fields: Vec<ContextFieldHandle>,
    tables_by_symbol: BTreeMap<String, usize>,
    txs_by_symbol: BTreeMap<String, usize>,
    queries_by_symbol: BTreeMap<String, usize>,
    context_fields_by_symbol: BTreeMap<String, usize>,
}

impl Schema {
    pub(crate) fn from_registered(registered: &RegisteredProgram) -> Result<Self, SdkError> {
        let program = registered.program();
        let tables = registered
            .execution_contract()
            .state
            .tables
            .iter()
            .map(|table| {
                let fields = table
                    .columns
                    .iter()
                    .map(|column| FieldHandle {
                        table_id: ir::TableId(table.id.0),
                        field_id: ir::FieldId(column.id.0),
                        symbol: column.name.clone(),
                        ty: column.ty,
                    })
                    .collect::<Vec<_>>();
                TableHandle {
                    id: ir::TableId(table.id.0),
                    symbol: table.name.clone(),
                    key_components: table
                        .key
                        .components
                        .iter()
                        .map(key_component_handle)
                        .collect(),
                    fields,
                }
            })
            .collect::<Vec<_>>();
        let tables_by_symbol = tables
            .iter()
            .enumerate()
            .map(|(index, table)| (table.symbol.clone(), index))
            .collect();

        let mut txs = Vec::new();
        let mut queries = Vec::new();
        for entry in &program.entries {
            let params = entry
                .params
                .iter()
                .map(|param| ParameterHandle {
                    symbol: param.symbol.clone(),
                    ty: param.ty,
                })
                .collect::<Vec<_>>();
            match entry.kind {
                ir::EntryKind::Tx => txs.push(TxHandle {
                    id: entry.id,
                    symbol: entry.symbol.clone(),
                    params,
                }),
                ir::EntryKind::Query => queries.push(QueryHandle {
                    id: entry.id,
                    symbol: entry.symbol.clone(),
                    params,
                    returns: entry.returns.clone(),
                }),
            }
        }
        let txs_by_symbol = txs
            .iter()
            .enumerate()
            .map(|(index, tx)| (tx.symbol.clone(), index))
            .collect();
        let queries_by_symbol = queries
            .iter()
            .enumerate()
            .map(|(index, query)| (query.symbol.clone(), index))
            .collect();
        let context_fields = program
            .context
            .fields
            .iter()
            .map(|field| ContextFieldHandle {
                id: field.id,
                symbol: field.symbol.clone(),
                ty: field.ty,
            })
            .collect::<Vec<_>>();
        let context_fields_by_symbol = context_fields
            .iter()
            .enumerate()
            .map(|(index, field)| (field.symbol.clone(), index))
            .collect();

        Ok(Self {
            inner: Arc::new(SchemaInner {
                tables,
                txs,
                queries,
                context_fields,
                tables_by_symbol,
                txs_by_symbol,
                queries_by_symbol,
                context_fields_by_symbol,
            }),
        })
    }

    /// Total number of state tables declared by the program.
    pub fn table_count(&self) -> usize {
        self.inner.tables.len()
    }

    /// Total number of transaction entries declared by the program.
    pub fn tx_count(&self) -> usize {
        self.inner.txs.len()
    }

    /// All state table handles in declaration order.
    pub fn tables(&self) -> &[TableHandle] {
        &self.inner.tables
    }

    /// All transaction entry handles in declaration order.
    pub fn txs(&self) -> &[TxHandle] {
        &self.inner.txs
    }

    /// All query entry handles in declaration order.
    pub fn queries(&self) -> &[QueryHandle] {
        &self.inner.queries
    }

    /// All public context field handles in declaration order.
    pub fn context_fields(&self) -> &[ContextFieldHandle] {
        &self.inner.context_fields
    }

    /// Look up a state table by source symbol; returns an error if not found.
    pub fn table(&self, symbol: &str) -> Result<TableHandle, SdkError> {
        self.inner
            .tables_by_symbol
            .get(symbol)
            .copied()
            .map(|index| self.inner.tables[index].clone())
            .ok_or_else(|| SdkError::SchemaLookup {
                detail: format!("unknown table `{symbol}`"),
            })
    }

    /// Look up a transaction entry by source symbol; returns an error if not found.
    pub fn tx(&self, symbol: &str) -> Result<TxHandle, SdkError> {
        self.inner
            .txs_by_symbol
            .get(symbol)
            .copied()
            .map(|index| self.inner.txs[index].clone())
            .ok_or_else(|| SdkError::SchemaLookup {
                detail: format!("unknown tx `{symbol}`"),
            })
    }

    /// Look up a query entry by source symbol; returns an error if not found.
    pub fn query(&self, symbol: &str) -> Result<QueryHandle, SdkError> {
        self.inner
            .queries_by_symbol
            .get(symbol)
            .copied()
            .map(|index| self.inner.queries[index].clone())
            .ok_or_else(|| SdkError::SchemaLookup {
                detail: format!("unknown query `{symbol}`"),
            })
    }

    /// Look up a public context field by source symbol; returns an error if not found.
    pub fn context_field(&self, symbol: &str) -> Result<ContextFieldHandle, SdkError> {
        self.inner
            .context_fields_by_symbol
            .get(symbol)
            .copied()
            .map(|index| self.inner.context_fields[index].clone())
            .ok_or_else(|| SdkError::SchemaLookup {
                detail: format!("unknown context field `{symbol}`"),
            })
    }
}

fn key_component_handle(component: &KeyComponentSchema) -> KeyComponentHandle {
    KeyComponentHandle {
        symbol: component.symbol.clone(),
        ty: component.ty,
    }
}
