//! Lowering pass: AST → Tabula IR.
//!
//! Performs name resolution, type checking, slot allocation, and IR emission
//! in a single forward pass.

mod expr;
mod resolve;
mod stmt;
use std::collections::HashMap;

use tabula_core::{ColId, SchemeId, TableId, TableSchema, TxTypeId, Value, ValueType};
use tabula_ir::{Instruction, ParamDef, Slot, TxTypeDef, ValueExpr};

use crate::ast::{self, ColumnSchemeDecl, TypeName};
use crate::error::{CompileError, ErrorKind};
use crate::span::Span;

/// Lowering output: table schemas + tx type definitions.
#[derive(Debug, Clone)]
pub struct LoweredProgram {
    /// Table schemas (ordered by declaration order).
    pub schemas: Vec<TableSchema>,
    /// Transaction type definitions (ordered by declaration order).
    pub tx_types: Vec<TxTypeDef>,
    /// Non-default column commitment scheme selections from source.
    pub column_schemes: Vec<ColumnSchemeSelection>,
}

/// Backward-compatible alias for older call sites.
pub type CompiledProgram = LoweredProgram;

/// Lower an AST program to IR.
pub fn lower(program: &ast::Program) -> Result<LoweredProgram, Vec<CompileError>> {
    let mut ctx = LowerCtx::new();
    ctx.collect_schemas(program);
    if !ctx.errors.is_empty() {
        return Err(ctx.errors);
    }
    ctx.lower_transactions(program);
    if ctx.errors.is_empty() {
        Ok(LoweredProgram {
            schemas: ctx.schemas,
            tx_types: ctx.tx_types,
            column_schemes: ctx.column_schemes,
        })
    } else {
        Err(ctx.errors)
    }
}

// ---------------------------------------------------------------------------
// Table & column info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(super) struct TableInfo {
    pub(super) id: TableId,
    pub(super) columns: HashMap<String, ColumnInfo>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ColumnInfo {
    pub(super) id: ColId,
    pub(super) ty: ValueType,
}

// ---------------------------------------------------------------------------
// Lowering context
// ---------------------------------------------------------------------------

struct LowerCtx {
    /// table name → info
    tables: HashMap<String, TableInfo>,
    /// Compiled schemas (output).
    schemas: Vec<TableSchema>,
    /// Compiled tx type defs (output).
    tx_types: Vec<TxTypeDef>,
    /// Non-default column scheme selections (output).
    column_schemes: Vec<ColumnSchemeSelection>,
    errors: Vec<CompileError>,
}

impl LowerCtx {
    fn new() -> Self {
        Self {
            tables: HashMap::new(),
            schemas: Vec::new(),
            tx_types: Vec::new(),
            column_schemes: Vec::new(),
            errors: Vec::new(),
        }
    }

    // --- Phase 1: collect table schemas ---

    fn collect_schemas(&mut self, program: &ast::Program) {
        for (i, table) in program.tables.iter().enumerate() {
            if self.tables.contains_key(&table.name) {
                self.errors.push(CompileError::new(
                    ErrorKind::DuplicateTable,
                    table.span,
                    format!("duplicate table declaration '{}'", table.name),
                ));
                continue;
            }

            let table_id = TableId(i as u32);
            let mut columns = HashMap::new();
            let mut col_defs = Vec::new();
            let mut seen_cols: HashMap<String, Span> = HashMap::new();

            for (j, col) in table.columns.iter().enumerate() {
                if let Some(prev_span) = seen_cols.get(&col.name) {
                    self.errors.push(CompileError::new(
                        ErrorKind::DuplicateColumn,
                        col.span,
                        format!(
                            "duplicate column '{}' in table '{}' (first at {:?})",
                            col.name, table.name, prev_span
                        ),
                    ));
                    continue;
                }
                seen_cols.insert(col.name.clone(), col.span);

                let col_id = ColId(j as u16);
                let vt = ast_type_to_value_type(col.ty);
                columns.insert(col.name.clone(), ColumnInfo { id: col_id, ty: vt });
                col_defs.push(tabula_core::ColumnDef {
                    id: col_id,
                    name: col.name.clone(),
                    value_type: vt,
                });
                if let Some(scheme) = col.scheme {
                    let scheme_id = ast_scheme_to_scheme_id(scheme);
                    if scheme_id != SchemeId::SSMC {
                        self.column_schemes.push(ColumnSchemeSelection {
                            table_id,
                            col_id,
                            scheme_id,
                        });
                    }
                }
            }

            self.tables.insert(
                table.name.clone(),
                TableInfo {
                    id: table_id,
                    columns,
                },
            );
            self.schemas.push(TableSchema {
                id: table_id,
                name: table.name.clone(),
                columns: col_defs,
            });
        }
    }

    // --- Phase 2: lower transactions ---

    fn lower_transactions(&mut self, program: &ast::Program) {
        let mut seen_tx: HashMap<String, Span> = HashMap::new();
        for (i, tx) in program.transactions.iter().enumerate() {
            if let Some(prev_span) = seen_tx.get(&tx.name) {
                self.errors.push(CompileError::new(
                    ErrorKind::DuplicateTx,
                    tx.span,
                    format!(
                        "duplicate tx declaration '{}' (first at {:?})",
                        tx.name, prev_span
                    ),
                ));
                continue;
            }
            seen_tx.insert(tx.name.clone(), tx.span);

            let tx_id = TxTypeId(i as u32);
            match TxLower::new(&self.tables, tx).lower(tx_id) {
                Ok(def) => self.tx_types.push(def),
                Err(errs) => self.errors.extend(errs),
            }
        }
    }
}

/// Source-selected non-default commitment scheme for one column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSchemeSelection {
    /// Table identifier.
    pub table_id: TableId,
    /// Column identifier.
    pub col_id: ColId,
    /// Portable scheme identifier.
    pub scheme_id: SchemeId,
}

fn ast_scheme_to_scheme_id(scheme: ColumnSchemeDecl) -> SchemeId {
    match scheme {
        ColumnSchemeDecl::Ssmc => SchemeId::SSMC,
        ColumnSchemeDecl::Smt => SchemeId::SMT,
        ColumnSchemeDecl::Numeric(id) => SchemeId(id),
    }
}

// ---------------------------------------------------------------------------
// Per-transaction lowering
// ---------------------------------------------------------------------------

/// A local binding: either a real slot, an alias, or a 2-slot read result.
#[derive(Debug, Clone)]
pub(super) enum Binding {
    /// Occupies a physical IR slot.
    Slot(Slot, ValueType),
    /// Alias — resolved to a ValueExpr without emitting instructions.
    Alias(ValueExpr, ValueType),
    /// 2-slot cell read result: (val_slot, is_null_slot, column type).
    ReadSlot {
        val: Slot,
        is_null: Slot,
        ty: ValueType,
    },
}

impl Binding {
    pub(super) fn ty(&self) -> ValueType {
        match self {
            Binding::Slot(_, ty) | Binding::Alias(_, ty) | Binding::ReadSlot { ty, .. } => *ty,
        }
    }

    pub(super) fn to_value_expr(&self) -> ValueExpr {
        match self {
            Binding::Slot(s, _) => ValueExpr::Slot(*s),
            Binding::Alias(ve, _) => ve.clone(),
            Binding::ReadSlot { val, .. } => ValueExpr::Slot(*val),
        }
    }

    pub(super) fn is_null_slot(&self) -> Option<Slot> {
        match self {
            Binding::ReadSlot { is_null, .. } => Some(*is_null),
            _ => None,
        }
    }
}

pub(super) struct TxLower<'a> {
    pub(super) tables: &'a HashMap<String, TableInfo>,
    pub(super) tx: &'a ast::TxDecl,
    /// param name → (index, type)
    pub(super) params: HashMap<String, (u16, ValueType)>,
    /// local variable name → binding
    pub(super) locals: HashMap<String, Binding>,
    /// Next available slot.
    pub(super) next_slot: Slot,
    /// Emitted IR instructions.
    pub(super) instructions: Vec<Instruction>,
    pub(super) errors: Vec<CompileError>,
}

impl<'a> TxLower<'a> {
    fn new(tables: &'a HashMap<String, TableInfo>, tx: &'a ast::TxDecl) -> Self {
        Self {
            tables,
            tx,
            params: HashMap::new(),
            locals: HashMap::new(),
            next_slot: 0,
            instructions: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(super) fn alloc_slot(&mut self) -> Slot {
        let s = self.next_slot;
        self.next_slot += 1;
        s
    }

    fn lower(mut self, tx_id: TxTypeId) -> Result<TxTypeDef, Vec<CompileError>> {
        // Build param map.
        let mut param_schema = Vec::new();
        let mut seen_params: HashMap<String, Span> = HashMap::new();
        for (i, p) in self.tx.params.iter().enumerate() {
            if let Some(prev_span) = seen_params.get(&p.name) {
                self.errors.push(CompileError::new(
                    ErrorKind::DuplicateParam,
                    p.span,
                    format!(
                        "duplicate parameter '{}' (first at {:?})",
                        p.name, prev_span
                    ),
                ));
                continue;
            }
            seen_params.insert(p.name.clone(), p.span);

            let vt = ast_type_to_value_type(p.ty);
            self.params.insert(p.name.clone(), (i as u16, vt));
            param_schema.push(ParamDef {
                name: p.name.clone(),
                value_type: vt,
            });
        }

        // Lower each statement.
        for stmt in &self.tx.body {
            self.lower_stmt(stmt);
        }

        if self.errors.is_empty() {
            Ok(TxTypeDef {
                id: tx_id,
                name: self.tx.name.clone(),
                param_schema,
                body: self.instructions,
            })
        } else {
            Err(self.errors)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(super) enum LoweredExpr {
    Slot(Slot),
    ValueExpr(ValueExpr, ValueType),
}

pub(super) fn ast_type_to_value_type(ty: TypeName) -> ValueType {
    match ty {
        TypeName::U64 => ValueType::U64,
        TypeName::I64 => ValueType::I64,
        TypeName::Bool => ValueType::Bool,
        TypeName::Bytes32 => ValueType::Bytes32,
    }
}

pub(super) fn is_arithmetic(op: crate::ast::BinOp) -> bool {
    use crate::ast::BinOp;
    matches!(
        op,
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
    )
}
