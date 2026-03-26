use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_ir as ir;

use crate::error::SdkError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterHandle {
    symbol: String,
    ty: ir::TypeRef,
}

impl ParameterHandle {
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn ty(&self) -> ir::TypeRef {
        self.ty
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldHandle {
    table_id: ir::TableId,
    field_id: ir::FieldId,
    symbol: String,
    ty: ir::TypeRef,
}

impl FieldHandle {
    pub fn table_id(&self) -> ir::TableId {
        self.table_id
    }

    pub fn id(&self) -> ir::FieldId {
        self.field_id
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn ty(&self) -> ir::TypeRef {
        self.ty
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableHandle {
    id: ir::TableId,
    symbol: String,
    key_arity: usize,
    fields: Vec<FieldHandle>,
}

impl TableHandle {
    pub fn id(&self) -> ir::TableId {
        self.id
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn key_arity(&self) -> usize {
        self.key_arity
    }

    pub fn fields(&self) -> &[FieldHandle] {
        &self.fields
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxHandle {
    id: ir::EntryId,
    symbol: String,
    params: Vec<ParameterHandle>,
}

impl TxHandle {
    pub fn id(&self) -> ir::EntryId {
        self.id
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn params(&self) -> &[ParameterHandle] {
        &self.params
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHandle {
    id: ir::EntryId,
    symbol: String,
    params: Vec<ParameterHandle>,
    returns: Vec<ir::TypeRef>,
}

impl QueryHandle {
    pub fn id(&self) -> ir::EntryId {
        self.id
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn params(&self) -> &[ParameterHandle] {
        &self.params
    }

    pub fn returns(&self) -> &[ir::TypeRef] {
        &self.returns
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFieldHandle {
    id: ir::ContextFieldId,
    symbol: String,
    ty: ir::TypeRef,
}

impl ContextFieldHandle {
    pub fn id(&self) -> ir::ContextFieldId {
        self.id
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn ty(&self) -> ir::TypeRef {
        self.ty
    }
}

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
    pub(crate) fn from_program(program: &ir::Program) -> Self {
        let tables = program
            .state
            .tables
            .iter()
            .map(|table| TableHandle {
                id: table.id,
                symbol: table.symbol.clone(),
                key_arity: table.key_tys.len(),
                fields: table
                    .fields
                    .iter()
                    .map(|field| FieldHandle {
                        table_id: table.id,
                        field_id: field.id,
                        symbol: field.symbol.clone(),
                        ty: field.ty,
                    })
                    .collect(),
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

        Self {
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
        }
    }

    pub fn table_count(&self) -> usize {
        self.inner.tables.len()
    }

    pub fn tx_count(&self) -> usize {
        self.inner.txs.len()
    }

    pub fn tables(&self) -> &[TableHandle] {
        &self.inner.tables
    }

    pub fn txs(&self) -> &[TxHandle] {
        &self.inner.txs
    }

    pub fn queries(&self) -> &[QueryHandle] {
        &self.inner.queries
    }

    pub fn context_fields(&self) -> &[ContextFieldHandle] {
        &self.inner.context_fields
    }

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
