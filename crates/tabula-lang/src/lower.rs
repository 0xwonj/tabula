//! Lowering pass: AST → Tabula IR.
//!
//! Performs name resolution, type checking, slot allocation, and IR emission
//! in a single forward pass.

use std::collections::HashMap;

use tabula_core::ir::{Instruction, Predicate, RowExpr, Slot, ValueExpr};
use tabula_core::schema::{ColumnDef, TableSchema};
use tabula_core::tx::{ParamDef, TxTypeDef, TxTypeId};
use tabula_core::types::{ColId, TableId, Value, ValueType};

use crate::ast::{self, BinOp, Expr, ExprKind, Program, StmtKind, TypeName, UnaryOp};
use crate::error::{CompileError, ErrorKind};
use crate::span::Span;

/// Compilation output: table schemas + tx type definitions.
#[derive(Debug, Clone)]
pub struct CompiledProgram {
    /// Table schemas (ordered by declaration order).
    pub schemas: Vec<TableSchema>,
    /// Transaction type definitions (ordered by declaration order).
    pub tx_types: Vec<TxTypeDef>,
}

/// Lower an AST program to IR.
pub fn lower(program: &Program) -> Result<CompiledProgram, Vec<CompileError>> {
    let mut ctx = LowerCtx::new();
    ctx.collect_schemas(program);
    if !ctx.errors.is_empty() {
        return Err(ctx.errors);
    }
    ctx.lower_transactions(program);
    if ctx.errors.is_empty() {
        Ok(CompiledProgram {
            schemas: ctx.schemas,
            tx_types: ctx.tx_types,
        })
    } else {
        Err(ctx.errors)
    }
}

// ---------------------------------------------------------------------------
// Table & column info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TableInfo {
    id: TableId,
    columns: HashMap<String, ColumnInfo>,
}

#[derive(Debug, Clone, Copy)]
struct ColumnInfo {
    id: ColId,
    ty: ValueType,
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
    errors: Vec<CompileError>,
}

impl LowerCtx {
    fn new() -> Self {
        Self {
            tables: HashMap::new(),
            schemas: Vec::new(),
            tx_types: Vec::new(),
            errors: Vec::new(),
        }
    }

    // --- Phase 1: collect table schemas ---

    fn collect_schemas(&mut self, program: &Program) {
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
                col_defs.push(ColumnDef {
                    id: col_id,
                    name: col.name.clone(),
                    value_type: vt,
                });
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

    fn lower_transactions(&mut self, program: &Program) {
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

// ---------------------------------------------------------------------------
// Per-transaction lowering
// ---------------------------------------------------------------------------

/// A local binding: either a real slot, an alias, or a 2-slot read result.
#[derive(Debug, Clone)]
enum Binding {
    /// Occupies a physical IR slot.
    Slot(Slot, ValueType),
    /// Alias — resolved to a ValueExpr without emitting instructions.
    Alias(ValueExpr, ValueType),
    /// 2-slot cell read result: (val_slot, is_null_slot, column type).
    ReadSlot { val: Slot, is_null: Slot, ty: ValueType },
}

impl Binding {
    fn ty(&self) -> ValueType {
        match self {
            Binding::Slot(_, ty) | Binding::Alias(_, ty) | Binding::ReadSlot { ty, .. } => *ty,
        }
    }

    fn to_value_expr(&self) -> ValueExpr {
        match self {
            Binding::Slot(s, _) => ValueExpr::Slot(*s),
            Binding::Alias(ve, _) => ve.clone(),
            Binding::ReadSlot { val, .. } => ValueExpr::Slot(*val),
        }
    }

    fn to_row_expr(&self) -> RowExpr {
        match self {
            Binding::Slot(s, _) | Binding::ReadSlot { val: s, .. } => RowExpr::Slot(*s),
            Binding::Alias(ve, _) => match ve {
                ValueExpr::Literal(Value::U64(n)) => {
                    RowExpr::Literal(tabula_core::types::RowKey(*n))
                }
                ValueExpr::Slot(s) => RowExpr::Slot(*s),
                ValueExpr::Param(p) => RowExpr::Param(*p),
                _ => RowExpr::Slot(0), // fallback — type checker should prevent this
            },
        }
    }

    fn is_null_slot(&self) -> Option<Slot> {
        match self {
            Binding::ReadSlot { is_null, .. } => Some(*is_null),
            _ => None,
        }
    }
}

struct TxLower<'a> {
    tables: &'a HashMap<String, TableInfo>,
    tx: &'a ast::TxDecl,
    /// param name → (index, type)
    params: HashMap<String, (u16, ValueType)>,
    /// local variable name → binding
    locals: HashMap<String, Binding>,
    /// Next available slot.
    next_slot: Slot,
    /// Emitted IR instructions.
    instructions: Vec<Instruction>,
    errors: Vec<CompileError>,
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

    fn alloc_slot(&mut self) -> Slot {
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

    // --- Statements ---

    fn lower_stmt(&mut self, stmt: &ast::Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, value } => self.lower_let(name, value, stmt.span),
            StmtKind::LetDestructure {
                first,
                second,
                lhs,
                rhs,
            } => self.lower_let_destructure(first, second, lhs, rhs, stmt.span),
            StmtKind::Assign {
                table,
                row,
                col,
                value,
            } => self.lower_assign(table, row, col, value, stmt.span),
            StmtKind::Assert { condition } => self.lower_assert(condition),
            StmtKind::Emit { topic, args } => self.lower_emit(topic, args),
        }
    }

    fn lower_let(&mut self, name: &str, value: &Expr, span: Span) {
        if self.locals.contains_key(name) || self.params.contains_key(name) {
            self.errors.push(CompileError::new(
                ErrorKind::DuplicateBinding,
                span,
                format!("'{}' is already defined", name),
            ));
            return;
        }

        match &value.kind {
            // Cell read → Read instruction (2-slot: val + is_null).
            ExprKind::CellRead { table, row, col } => {
                let Some((table_id, col_info)) = self.resolve_table_col(table, col, value.span)
                else {
                    return;
                };
                let Some(row_expr) = self.lower_row_expr(row) else {
                    return;
                };
                let dst_val = self.alloc_slot();
                let dst_is_null = self.alloc_slot();
                self.instructions.push(Instruction::Read {
                    dst_val,
                    dst_is_null,
                    table: table_id,
                    row: row_expr,
                    col: col_info.id,
                });
                self.locals.insert(
                    name.to_string(),
                    Binding::ReadSlot { val: dst_val, is_null: dst_is_null, ty: col_info.ty },
                );
            }
            // Static table lookup → Lookup instruction.
            ExprKind::StaticRead { table, key, col } => {
                let Some((table_id, col_info)) = self.resolve_table_col(table, col, value.span)
                else {
                    return;
                };
                let Some(key_expr) = self.lower_row_expr(key) else {
                    return;
                };
                let dst = self.alloc_slot();
                self.instructions.push(Instruction::Lookup {
                    dst,
                    static_table: table_id,
                    col: col_info.id,
                    row: key_expr,
                });
                self.locals
                    .insert(name.to_string(), Binding::Slot(dst, col_info.ty));
            }
            // Hash call → Hash instruction.
            ExprKind::Hash(args) => {
                let inputs: Vec<_> = args
                    .iter()
                    .filter_map(|a| self.lower_value_expr(a))
                    .collect();
                if inputs.len() != args.len() {
                    return;
                }
                let dst = self.alloc_slot();
                self.instructions.push(Instruction::Hash { dst, inputs });
                self.locals
                    .insert(name.to_string(), Binding::Slot(dst, ValueType::Bytes32));
            }
            // Select call → Select instruction.
            ExprKind::Select {
                cond,
                if_true,
                if_false,
            } => {
                let Some(cond_ve) = self.lower_value_expr(cond) else {
                    return;
                };
                let Some(true_ve) = self.lower_value_expr(if_true) else {
                    return;
                };
                let Some(false_ve) = self.lower_value_expr(if_false) else {
                    return;
                };
                let ty = self
                    .expr_type(if_true)
                    .or_else(|| self.expr_type(if_false))
                    .unwrap_or(ValueType::U64);
                let dst = self.alloc_slot();
                self.instructions.push(Instruction::Select {
                    dst,
                    cond: cond_ve,
                    if_true: true_ve,
                    if_false: false_ve,
                });
                self.locals.insert(name.to_string(), Binding::Slot(dst, ty));
            }
            // General expression (arithmetic, ident, literal).
            _ => {
                let Some((lowered, ty)) = self.lower_expr_to_slot(value) else {
                    return;
                };
                match lowered {
                    LoweredExpr::Slot(s) => {
                        self.locals.insert(name.to_string(), Binding::Slot(s, ty));
                    }
                    LoweredExpr::ValueExpr(ve, ty) => {
                        // No instruction needed — store as an alias.
                        self.locals.insert(name.to_string(), Binding::Alias(ve, ty));
                    }
                }
            }
        }
    }

    fn lower_let_destructure(
        &mut self,
        first: &str,
        second: &str,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) {
        for name in [first, second] {
            if self.locals.contains_key(name) || self.params.contains_key(name) {
                self.errors.push(CompileError::new(
                    ErrorKind::DuplicateBinding,
                    span,
                    format!("'{}' is already defined", name),
                ));
                return;
            }
        }
        if first == second {
            self.errors.push(CompileError::new(
                ErrorKind::DuplicateBinding,
                span,
                format!("destructuring binds '{}' twice", first),
            ));
            return;
        }

        let Some(lhs_ve) = self.lower_value_expr(lhs) else {
            return;
        };
        let Some(rhs_ve) = self.lower_value_expr(rhs) else {
            return;
        };

        let lhs_ty = self.expr_type(lhs);
        let rhs_ty = self.expr_type(rhs);
        let ty = lhs_ty.or(rhs_ty).unwrap_or(ValueType::U64);

        let dst_q = self.alloc_slot();
        let dst_r = self.alloc_slot();
        self.instructions.push(Instruction::DivMod {
            dst_q,
            dst_r,
            lhs: lhs_ve,
            rhs: rhs_ve,
        });
        self.locals
            .insert(first.to_string(), Binding::Slot(dst_q, ty));
        self.locals
            .insert(second.to_string(), Binding::Slot(dst_r, ty));
    }

    fn lower_assign(&mut self, table: &str, row: &Expr, col: &str, value: &Expr, span: Span) {
        let Some((table_id, col_info)) = self.resolve_table_col(table, col, span) else {
            return;
        };
        let Some(row_expr) = self.lower_row_expr(row) else {
            return;
        };
        // Special case: `table[row].col = null` → write with is_null=true
        if matches!(&value.kind, ExprKind::Null) {
            self.instructions.push(Instruction::Write {
                table: table_id,
                row: row_expr,
                col: col_info.id,
                src_val: ValueExpr::Literal(tabula_core::types::zero_value(col_info.ty)),
                src_is_null: ValueExpr::Literal(Value::Bool(true)),
            });
            return;
        }
        let Some(src) = self.lower_value_expr(value) else {
            return;
        };
        self.instructions.push(Instruction::Write {
            table: table_id,
            row: row_expr,
            col: col_info.id,
            src_val: src,
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
        });
    }

    fn lower_assert(&mut self, condition: &Expr) {
        let Some(predicate) = self.lower_predicate(condition) else {
            return;
        };
        self.instructions.push(Instruction::Assert { predicate });
    }

    fn lower_emit(&mut self, topic: &str, args: &[Expr]) {
        let data: Vec<_> = args
            .iter()
            .filter_map(|a| self.lower_value_expr(a))
            .collect();
        if data.len() != args.len() {
            return;
        }
        self.instructions.push(Instruction::Emit {
            topic: topic.as_bytes().to_vec(),
            data,
        });
    }

    // --- Expression lowering ---

    /// Lower an expression that needs its own slot (for arithmetic results).
    /// Returns (LoweredExpr, inferred type).
    fn lower_expr_to_slot(&mut self, expr: &Expr) -> Option<(LoweredExpr, ValueType)> {
        match &expr.kind {
            ExprKind::BinOp { op, lhs, rhs } if is_arithmetic(*op) => {
                let lhs_ve = self.lower_value_expr(lhs)?;
                let rhs_ve = self.lower_value_expr(rhs)?;
                let ty = self
                    .expr_type(lhs)
                    .or_else(|| self.expr_type(rhs))
                    .unwrap_or(ValueType::U64);
                let dst = self.alloc_slot();
                let instr = match op {
                    BinOp::Add => Instruction::Add {
                        dst,
                        lhs: lhs_ve,
                        rhs: rhs_ve,
                    },
                    BinOp::Sub => Instruction::Sub {
                        dst,
                        lhs: lhs_ve,
                        rhs: rhs_ve,
                    },
                    BinOp::Mul => Instruction::Mul {
                        dst,
                        lhs: lhs_ve,
                        rhs: rhs_ve,
                    },
                    BinOp::Div => {
                        let dst_r = self.alloc_slot(); // dead remainder
                        self.instructions.push(Instruction::DivMod {
                            dst_q: dst,
                            dst_r,
                            lhs: lhs_ve,
                            rhs: rhs_ve,
                        });
                        return Some((LoweredExpr::Slot(dst), ty));
                    }
                    BinOp::Mod => {
                        let dst_q = self.alloc_slot(); // dead quotient — allocate BEFORE dst
                        // Actually, we want dst_r to be our result slot.
                        // Swap: dst_q = dead, dst_r = dst
                        self.instructions.push(Instruction::DivMod {
                            dst_q,
                            dst_r: dst,
                            lhs: lhs_ve,
                            rhs: rhs_ve,
                        });
                        return Some((LoweredExpr::Slot(dst), ty));
                    }
                    _ => unreachable!("non-arithmetic op"),
                };
                self.instructions.push(instr);
                Some((LoweredExpr::Slot(dst), ty))
            }
            ExprKind::UnaryOp {
                op: UnaryOp::Neg,
                operand,
            } => {
                // -x  ⟹  Sub(dst, Literal(0), x)
                let operand_ve = self.lower_value_expr(operand)?;
                let ty = self.expr_type(operand).unwrap_or(ValueType::I64);
                let dst = self.alloc_slot();
                self.instructions.push(Instruction::Sub {
                    dst,
                    lhs: ValueExpr::Literal(tabula_core::types::zero_value(ty)),
                    rhs: operand_ve,
                });
                Some((LoweredExpr::Slot(dst), ty))
            }
            // For non-arithmetic expressions, just return as ValueExpr.
            _ => {
                let ve = self.lower_value_expr(expr)?;
                let ty = self.expr_type(expr).unwrap_or(ValueType::U64);
                Some((LoweredExpr::ValueExpr(ve, ty), ty))
            }
        }
    }

    /// Lower an expression to a ValueExpr (for use as instruction operands).
    fn lower_value_expr(&mut self, expr: &Expr) -> Option<ValueExpr> {
        match &expr.kind {
            ExprKind::IntLit(n) => Some(ValueExpr::Literal(Value::U64(*n))),
            ExprKind::BoolLit(b) => Some(ValueExpr::Literal(Value::Bool(*b))),
            ExprKind::HexLit(b) => Some(ValueExpr::Literal(Value::Bytes32(*b))),
            ExprKind::Null => {
                self.errors.push(CompileError::new(
                    ErrorKind::TypeMismatch,
                    expr.span,
                    "null cannot be used as a value; use in assignments or comparisons",
                ));
                None
            }
            ExprKind::Ident(name) => self.resolve_ident(name, expr.span),
            // Arithmetic ops need a slot.
            ExprKind::BinOp { op, .. } if is_arithmetic(*op) => {
                let (lowered, _) = self.lower_expr_to_slot(expr)?;
                match lowered {
                    LoweredExpr::Slot(s) => Some(ValueExpr::Slot(s)),
                    LoweredExpr::ValueExpr(ve, _) => Some(ve),
                }
            }
            ExprKind::UnaryOp {
                op: UnaryOp::Neg, ..
            } => {
                let (lowered, _) = self.lower_expr_to_slot(expr)?;
                match lowered {
                    LoweredExpr::Slot(s) => Some(ValueExpr::Slot(s)),
                    LoweredExpr::ValueExpr(ve, _) => Some(ve),
                }
            }
            // Cell read as part of an expression (not a let binding) — needs temp slots.
            ExprKind::CellRead { table, row, col } => {
                let (table_id, col_info) = self.resolve_table_col(table, col, expr.span)?;
                let row_expr = self.lower_row_expr(row)?;
                let dst_val = self.alloc_slot();
                let dst_is_null = self.alloc_slot(); // unnamed temp for is_null
                self.instructions.push(Instruction::Read {
                    dst_val,
                    dst_is_null,
                    table: table_id,
                    row: row_expr,
                    col: col_info.id,
                });
                // No local binding — this is a temporary.
                Some(ValueExpr::Slot(dst_val))
            }
            ExprKind::StaticRead { table, key, col } => {
                let (table_id, col_info) = self.resolve_table_col(table, col, expr.span)?;
                let key_expr = self.lower_row_expr(key)?;
                let dst = self.alloc_slot();
                self.instructions.push(Instruction::Lookup {
                    dst,
                    static_table: table_id,
                    col: col_info.id,
                    row: key_expr,
                });
                Some(ValueExpr::Slot(dst))
            }
            ExprKind::Hash(args) => {
                let inputs: Vec<_> = args
                    .iter()
                    .filter_map(|a| self.lower_value_expr(a))
                    .collect();
                if inputs.len() != args.len() {
                    return None;
                }
                let dst = self.alloc_slot();
                self.instructions.push(Instruction::Hash { dst, inputs });
                Some(ValueExpr::Slot(dst))
            }
            ExprKind::Select {
                cond,
                if_true,
                if_false,
            } => {
                let cond_ve = self.lower_value_expr(cond)?;
                let true_ve = self.lower_value_expr(if_true)?;
                let false_ve = self.lower_value_expr(if_false)?;
                let dst = self.alloc_slot();
                self.instructions.push(Instruction::Select {
                    dst,
                    cond: cond_ve,
                    if_true: true_ve,
                    if_false: false_ve,
                });
                Some(ValueExpr::Slot(dst))
            }
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::TypeMismatch,
                    expr.span,
                    format!("expression cannot be used as a value here: {:?}", expr.kind),
                ));
                None
            }
        }
    }

    /// Lower an expression to a RowExpr.
    fn lower_row_expr(&mut self, expr: &Expr) -> Option<RowExpr> {
        match &expr.kind {
            ExprKind::IntLit(n) => Some(RowExpr::Literal(tabula_core::types::RowKey(*n))),
            ExprKind::Ident(name) => {
                if let Some(binding) = self.locals.get(name) {
                    Some(binding.to_row_expr())
                } else if let Some((idx, _)) = self.params.get(name) {
                    Some(RowExpr::Param(*idx))
                } else {
                    self.errors.push(CompileError::new(
                        ErrorKind::UndefinedVariable,
                        expr.span,
                        format!("undefined variable '{}'", name),
                    ));
                    None
                }
            }
            // Arithmetic expression as row key — emit to slot, then use slot.
            _ => {
                let ve = self.lower_value_expr(expr)?;
                match ve {
                    ValueExpr::Literal(Value::U64(n)) => {
                        Some(RowExpr::Literal(tabula_core::types::RowKey(n)))
                    }
                    ValueExpr::Slot(s) => Some(RowExpr::Slot(s)),
                    ValueExpr::Param(p) => Some(RowExpr::Param(p)),
                    _ => {
                        self.errors.push(CompileError::new(
                            ErrorKind::TypeMismatch,
                            expr.span,
                            "expression cannot be used as a row key",
                        ));
                        None
                    }
                }
            }
        }
    }

    /// Lower a comparison/logical expression to a Predicate.
    fn lower_predicate(&mut self, expr: &Expr) -> Option<Predicate> {
        match &expr.kind {
            ExprKind::BinOp { op, lhs, rhs } => match op {
                BinOp::Eq => {
                    // Special case: x == null → check is_null slot is true
                    if matches!(&rhs.kind, ExprKind::Null) {
                        return self.null_check_predicate(lhs, true, expr.span);
                    }
                    if matches!(&lhs.kind, ExprKind::Null) {
                        return self.null_check_predicate(rhs, true, expr.span);
                    }
                    let l = self.lower_value_expr(lhs)?;
                    let r = self.lower_value_expr(rhs)?;
                    Some(Predicate::Eq(l, r))
                }
                BinOp::Neq => {
                    // Special case: x != null → check is_null slot is false (not null)
                    if matches!(&rhs.kind, ExprKind::Null) {
                        return self.null_check_predicate(lhs, false, expr.span);
                    }
                    if matches!(&lhs.kind, ExprKind::Null) {
                        return self.null_check_predicate(rhs, false, expr.span);
                    }
                    // General != → Not(Eq(...))
                    let l = self.lower_value_expr(lhs)?;
                    let r = self.lower_value_expr(rhs)?;
                    Some(Predicate::Not(Box::new(Predicate::Eq(l, r))))
                }
                BinOp::Lt => {
                    let l = self.lower_value_expr(lhs)?;
                    let r = self.lower_value_expr(rhs)?;
                    Some(Predicate::Lt(l, r))
                }
                BinOp::Lte => {
                    let l = self.lower_value_expr(lhs)?;
                    let r = self.lower_value_expr(rhs)?;
                    Some(Predicate::Lte(l, r))
                }
                BinOp::Gt => {
                    let l = self.lower_value_expr(lhs)?;
                    let r = self.lower_value_expr(rhs)?;
                    Some(Predicate::Gt(l, r))
                }
                BinOp::Gte => {
                    let l = self.lower_value_expr(lhs)?;
                    let r = self.lower_value_expr(rhs)?;
                    Some(Predicate::Gte(l, r))
                }
                BinOp::And => {
                    let l = self.lower_predicate(lhs)?;
                    let r = self.lower_predicate(rhs)?;
                    Some(Predicate::And(Box::new(l), Box::new(r)))
                }
                BinOp::Or => {
                    let l = self.lower_predicate(lhs)?;
                    let r = self.lower_predicate(rhs)?;
                    Some(Predicate::Or(Box::new(l), Box::new(r)))
                }
                // Arithmetic ops inside assert — need to evaluate first.
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                    self.errors.push(CompileError::new(
                        ErrorKind::TypeMismatch,
                        expr.span,
                        "arithmetic expression cannot be used as a predicate directly",
                    ));
                    None
                }
            },
            ExprKind::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => {
                let inner = self.lower_predicate(operand)?;
                Some(Predicate::Not(Box::new(inner)))
            }
            ExprKind::BoolLit(true) => {
                // assert true → assert 1 == 1 (always passes)
                Some(Predicate::Eq(
                    ValueExpr::Literal(Value::U64(1)),
                    ValueExpr::Literal(Value::U64(1)),
                ))
            }
            ExprKind::BoolLit(false) => {
                // assert false → assert 0 == 1 (always fails)
                Some(Predicate::Eq(
                    ValueExpr::Literal(Value::U64(0)),
                    ValueExpr::Literal(Value::U64(1)),
                ))
            }
            ExprKind::Ident(name) => {
                // assert some_bool_var → NotNull(var) as a proxy
                // Actually, asserting a boolean variable should check it's true.
                // But the IR doesn't have a "truthiness" predicate.
                // Use: Eq(var, Literal(true))
                let ve = self.resolve_ident(name, expr.span)?;
                Some(Predicate::Eq(ve, ValueExpr::Literal(Value::Bool(true))))
            }
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::TypeMismatch,
                    expr.span,
                    "expression cannot be used as a predicate",
                ));
                None
            }
        }
    }

    // --- Helpers ---

    /// Resolve an identifier to a ValueExpr.
    fn resolve_ident(&mut self, name: &str, span: Span) -> Option<ValueExpr> {
        if let Some(binding) = self.locals.get(name) {
            Some(binding.to_value_expr())
        } else if let Some((idx, _)) = self.params.get(name) {
            Some(ValueExpr::Param(*idx))
        } else {
            self.errors.push(CompileError::new(
                ErrorKind::UndefinedVariable,
                span,
                format!("undefined variable '{}'", name),
            ));
            None
        }
    }

    /// Build a null-check predicate for `expr == null` or `expr != null`.
    /// `is_eq` = true → Eq(is_null, Bool(true)), false → Eq(is_null, Bool(false))
    fn null_check_predicate(
        &mut self,
        expr: &Expr,
        is_eq: bool,
        span: Span,
    ) -> Option<Predicate> {
        // The expression must resolve to a ReadSlot binding (from a cell read).
        if let ExprKind::Ident(name) = &expr.kind
            && let Some(binding) = self.locals.get(name)
            && let Some(is_null_slot) = binding.is_null_slot()
        {
            return Some(Predicate::Eq(
                ValueExpr::Slot(is_null_slot),
                ValueExpr::Literal(Value::Bool(is_eq)),
            ));
        }
        self.errors.push(CompileError::new(
            ErrorKind::TypeMismatch,
            span,
            "null comparison requires a cell-read binding (let x = table[row].col)",
        ));
        None
    }

    fn resolve_table_col(
        &mut self,
        table_name: &str,
        col_name: &str,
        span: Span,
    ) -> Option<(TableId, ColumnInfo)> {
        let Some(table) = self.tables.get(table_name) else {
            self.errors.push(CompileError::new(
                ErrorKind::UndefinedTable,
                span,
                format!("undefined table '{}'", table_name),
            ));
            return None;
        };
        let Some(col) = table.columns.get(col_name) else {
            self.errors.push(CompileError::new(
                ErrorKind::UndefinedColumn,
                span,
                format!("undefined column '{}' in table '{}'", col_name, table_name),
            ));
            return None;
        };
        Some((table.id, *col))
    }

    /// Infer the type of an expression (best-effort).
    fn expr_type(&self, expr: &Expr) -> Option<ValueType> {
        match &expr.kind {
            ExprKind::IntLit(_) => Some(ValueType::U64),
            ExprKind::BoolLit(_) => Some(ValueType::Bool),
            ExprKind::HexLit(_) => Some(ValueType::Bytes32),
            ExprKind::Null => None,
            ExprKind::Ident(name) => {
                if let Some(binding) = self.locals.get(name) {
                    Some(binding.ty())
                } else if let Some((_, ty)) = self.params.get(name) {
                    Some(*ty)
                } else {
                    None
                }
            }
            ExprKind::CellRead { table, col, .. } => self
                .tables
                .get(table)
                .and_then(|t| t.columns.get(col))
                .map(|c| c.ty),
            ExprKind::StaticRead { table, col, .. } => self
                .tables
                .get(table)
                .and_then(|t| t.columns.get(col))
                .map(|c| c.ty),
            ExprKind::Hash(_) => Some(ValueType::Bytes32),
            ExprKind::BinOp { op, lhs, rhs, .. } if is_arithmetic(*op) => {
                self.expr_type(lhs).or_else(|| self.expr_type(rhs))
            }
            ExprKind::UnaryOp {
                op: UnaryOp::Neg,
                operand,
            } => self.expr_type(operand),
            ExprKind::UnaryOp {
                op: UnaryOp::Not, ..
            } => Some(ValueType::Bool),
            ExprKind::Divmod { lhs, rhs } => self.expr_type(lhs).or_else(|| self.expr_type(rhs)),
            ExprKind::Select {
                if_true, if_false, ..
            } => self.expr_type(if_true).or_else(|| self.expr_type(if_false)),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

enum LoweredExpr {
    Slot(Slot),
    ValueExpr(ValueExpr, ValueType),
}

fn ast_type_to_value_type(ty: TypeName) -> ValueType {
    match ty {
        TypeName::U64 => ValueType::U64,
        TypeName::I64 => ValueType::I64,
        TypeName::Bool => ValueType::Bool,
        TypeName::Bytes32 => ValueType::Bytes32,
    }
}

fn is_arithmetic(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;
    fn compile(source: &str) -> CompiledProgram {
        let tokens = lex(source).expect("lex failed");
        let ast = parse(tokens).expect("parse failed");
        lower(&ast).expect("lower failed")
    }

    // --- Schema lowering ---

    #[test]
    fn test_lower_table_schema() {
        let prog = compile("table balances { balance: u64 }");
        assert_eq!(prog.schemas.len(), 1);
        assert_eq!(prog.schemas[0].id, TableId(0));
        assert_eq!(prog.schemas[0].name, "balances");
        assert_eq!(prog.schemas[0].columns[0].id, ColId(0));
        assert_eq!(prog.schemas[0].columns[0].value_type, ValueType::U64);
    }

    #[test]
    fn test_lower_multiple_tables() {
        let prog = compile("table a { x: u64 }\ntable b { y: bool }");
        assert_eq!(prog.schemas.len(), 2);
        assert_eq!(prog.schemas[0].id, TableId(0));
        assert_eq!(prog.schemas[1].id, TableId(1));
    }

    #[test]
    fn test_lower_duplicate_table_error() {
        let tokens = lex("table a { x: u64 }\ntable a { y: bool }").unwrap();
        let ast = parse(tokens).unwrap();
        let err = lower(&ast).unwrap_err();
        assert!(err.iter().any(|e| e.kind == ErrorKind::DuplicateTable));
    }

    // --- Simple tx ---

    #[test]
    fn test_lower_empty_tx() {
        let prog = compile("tx noop() {}");
        assert_eq!(prog.tx_types.len(), 1);
        assert_eq!(prog.tx_types[0].id, TxTypeId(0));
        assert_eq!(prog.tx_types[0].name, "noop");
        assert!(prog.tx_types[0].body.is_empty());
    }

    // --- Read + Write ---

    #[test]
    fn test_lower_read_write() {
        let source = "\
table t { v: u64 }
tx rw(id: u64, val: u64) {
    let x = t[id].v
    t[id].v = val
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert_eq!(body.len(), 2);
        assert!(matches!(
            &body[0],
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table,
                row: RowExpr::Param(0),
                col,
            } if *table == TableId(0) && *col == ColId(0)
        ));
        assert!(matches!(
            &body[1],
            Instruction::Write {
                table,
                row: RowExpr::Param(0),
                col,
                src_val: ValueExpr::Param(1),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            } if *table == TableId(0) && *col == ColId(0)
        ));
    }

    // --- Arithmetic ---

    #[test]
    fn test_lower_arithmetic() {
        let source = "\
table t { v: u64 }
tx add_one(id: u64) {
    let x = t[id].v
    let y = x + 1
    t[id].v = y
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert_eq!(body.len(), 3);
        // Read uses slots 0 (val) and 1 (is_null)
        assert!(matches!(&body[0], Instruction::Read { dst_val: 0, dst_is_null: 1, .. }));
        assert!(matches!(
            &body[1],
            Instruction::Add {
                dst: 2,
                lhs: ValueExpr::Slot(0),
                rhs: ValueExpr::Literal(Value::U64(1)),
            }
        ));
        assert!(matches!(
            &body[2],
            Instruction::Write {
                src_val: ValueExpr::Slot(2),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
                ..
            }
        ));
    }

    // --- Assert ---

    #[test]
    fn test_lower_assert_gte() {
        let source = "\
tx check(x: u64, y: u64) {
    assert x >= y
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            Instruction::Assert {
                predicate: Predicate::Gte(ValueExpr::Param(0), ValueExpr::Param(1))
            }
        ));
    }

    #[test]
    fn test_lower_assert_not_null() {
        let source = "\
table t { v: u64 }
tx check(id: u64) {
    let x = t[id].v
    assert x != null
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        // x != null → Eq(is_null_slot, Bool(false))
        // Read uses slots 0 (val) and 1 (is_null)
        assert!(matches!(
            &body[1],
            Instruction::Assert {
                predicate: Predicate::Eq(
                    ValueExpr::Slot(1),
                    ValueExpr::Literal(Value::Bool(false)),
                )
            }
        ));
    }

    #[test]
    fn test_lower_assert_eq_null() {
        let source = "\
table t { v: u64 }
tx check(id: u64) {
    let x = t[id].v
    assert x == null
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        // x == null → Eq(is_null_slot, Bool(true))
        assert!(matches!(
            &body[1],
            Instruction::Assert {
                predicate: Predicate::Eq(
                    ValueExpr::Slot(1),
                    ValueExpr::Literal(Value::Bool(true)),
                )
            }
        ));
    }

    // --- Hash ---

    #[test]
    fn test_lower_hash() {
        let source = "tx h(a: u64, b: u64) { let digest = hash(a, b) }";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert!(matches!(
            &body[0],
            Instruction::Hash { dst: 0, inputs } if inputs.len() == 2
        ));
    }

    // --- Emit ---

    #[test]
    fn test_lower_emit() {
        let source = "tx e(a: u64) { emit \"test\" (a) }";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert!(matches!(
            &body[0],
            Instruction::Emit { topic, data } if topic == b"test" && data.len() == 1
        ));
    }

    // --- DivMod ---

    #[test]
    fn test_lower_divmod() {
        let source = "tx d(a: u64, b: u64) { let (q, r) = divmod(a, b) }";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert!(matches!(
            &body[0],
            Instruction::DivMod {
                dst_q: 0,
                dst_r: 1,
                lhs: ValueExpr::Param(0),
                rhs: ValueExpr::Param(1),
            }
        ));
    }

    // --- Div and Mod operators ---

    #[test]
    fn test_lower_div_operator() {
        let source = "tx d(a: u64, b: u64) { let q = a / b }";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        // Should emit DivMod with dst_q as the used slot.
        assert!(matches!(
            &body[0],
            Instruction::DivMod {
                dst_q: 0,
                dst_r: 1,
                ..
            }
        ));
    }

    // --- Undefined variable ---

    #[test]
    fn test_lower_undefined_variable() {
        let tokens = lex("tx t() { assert x >= 0 }").unwrap();
        let ast = parse(tokens).unwrap();
        let err = lower(&ast).unwrap_err();
        assert!(err.iter().any(|e| e.kind == ErrorKind::UndefinedVariable));
    }

    // --- Undefined table ---

    #[test]
    fn test_lower_undefined_table() {
        let tokens = lex("tx t(id: u64) { let x = foo[id].bar }").unwrap();
        let ast = parse(tokens).unwrap();
        let err = lower(&ast).unwrap_err();
        assert!(err.iter().any(|e| e.kind == ErrorKind::UndefinedTable));
    }

    // --- Full transfer ---

    #[test]
    fn test_lower_transfer() {
        let source = "\
table balances { balance: u64 }

tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    let new_sender = sender_bal - amount
    let new_recv = recv_bal + amount
    balances[from].balance = new_sender
    balances[to].balance = new_recv
}";
        let prog = compile(source);
        let tx = &prog.tx_types[0];
        assert_eq!(tx.name, "transfer");
        assert_eq!(tx.param_schema.len(), 3);
        assert_eq!(tx.body.len(), 7);

        // Verify exact IR output matches hand-written transfer.
        // Read sender_bal: slots 0 (val), 1 (is_null)
        // Read recv_bal:   slots 2 (val), 3 (is_null)
        // Sub new_sender:  slot 4
        // Add new_recv:    slot 5
        assert_eq!(
            tx.body,
            vec![
                Instruction::Read {
                    dst_val: 0,
                    dst_is_null: 1,
                    table: TableId(0),
                    row: RowExpr::Param(0),
                    col: ColId(0),
                },
                Instruction::Read {
                    dst_val: 2,
                    dst_is_null: 3,
                    table: TableId(0),
                    row: RowExpr::Param(1),
                    col: ColId(0),
                },
                Instruction::Assert {
                    predicate: Predicate::Gte(ValueExpr::Slot(0), ValueExpr::Param(2)),
                },
                Instruction::Sub {
                    dst: 4,
                    lhs: ValueExpr::Slot(0),
                    rhs: ValueExpr::Param(2),
                },
                Instruction::Add {
                    dst: 5,
                    lhs: ValueExpr::Slot(2),
                    rhs: ValueExpr::Param(2),
                },
                Instruction::Write {
                    table: TableId(0),
                    row: RowExpr::Param(0),
                    col: ColId(0),
                    src_val: ValueExpr::Slot(4),
                    src_is_null: ValueExpr::Literal(Value::Bool(false)),
                },
                Instruction::Write {
                    table: TableId(0),
                    row: RowExpr::Param(1),
                    col: ColId(0),
                    src_val: ValueExpr::Slot(5),
                    src_is_null: ValueExpr::Literal(Value::Bool(false)),
                },
            ]
        );
    }

    // --- Alias (let x = param) ---

    #[test]
    fn test_lower_alias_no_instruction() {
        let source = "\
tx t(x: u64, y: u64) {
    let a = x
    let b = y
    assert a >= b
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        // `let a = x` and `let b = y` should NOT emit instructions.
        // Only the assert should be emitted.
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            Instruction::Assert {
                predicate: Predicate::Gte(ValueExpr::Param(0), ValueExpr::Param(1))
            }
        ));
    }

    #[test]
    fn test_lower_alias_bool() {
        let source = "\
tx t(flag: bool) {
    let x = flag
    assert x
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            Instruction::Assert {
                predicate: Predicate::Eq(
                    ValueExpr::Param(0),
                    ValueExpr::Literal(Value::Bool(true))
                )
            }
        ));
    }

    // --- Compound expression in write ---

    #[test]
    fn test_lower_inline_arithmetic_in_write() {
        let source = "\
table t { v: u64 }
tx inc(id: u64, amount: u64) {
    let x = t[id].v
    t[id].v = x + amount
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert_eq!(body.len(), 3);
        // Read: slots 0 (val), 1 (is_null)
        assert!(matches!(&body[0], Instruction::Read { dst_val: 0, dst_is_null: 1, .. }));
        assert!(matches!(&body[1], Instruction::Add { dst: 2, .. }));
        assert!(matches!(
            &body[2],
            Instruction::Write {
                src_val: ValueExpr::Slot(2),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
                ..
            }
        ));
    }

    // --- Select ---

    #[test]
    fn test_lower_select() {
        let source = "\
table t { a: u64, b: u64 }
tx s(id: u64, flag: bool) {
    let x = t[id].a
    let y = t[id].b
    let result = select(flag, x, y)
    t[id].a = result
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert_eq!(body.len(), 4);
        // Read x: slots 0 (val), 1 (is_null)
        // Read y: slots 2 (val), 3 (is_null)
        // Select: slot 4
        assert!(matches!(
            &body[2],
            Instruction::Select {
                dst: 4,
                cond: ValueExpr::Param(1),
                if_true: ValueExpr::Slot(0),
                if_false: ValueExpr::Slot(2),
            }
        ));
    }

    #[test]
    fn test_lower_select_literal_branches() {
        let source = "tx s(flag: bool) { let x = select(flag, 42, 0) }";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            Instruction::Select {
                dst: 0,
                cond: ValueExpr::Param(0),
                if_true: ValueExpr::Literal(Value::U64(42)),
                if_false: ValueExpr::Literal(Value::U64(0)),
            }
        ));
    }

    // --- SSA validation of lowered IR ---

    #[test]
    fn test_lowered_ir_passes_ssa_validation() {
        // Verify that the lowered transfer program passes Program::register() SSA validation.
        use tabula_core::tx::TxTypeId;
        use tabula_executor::program::Program;

        let source = "\
table balances { balance: u64 }

tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    let new_sender = sender_bal - amount
    let new_recv = recv_bal + amount
    balances[from].balance = new_sender
    balances[to].balance = new_recv
}";
        let compiled = compile(source);
        let mut prog = Program::new();
        for schema in &compiled.schemas {
            prog.add_schema(schema.clone());
        }
        for tx_type in &compiled.tx_types {
            prog.register(tx_type.clone())
                .unwrap_or_else(|e| panic!("lowered IR failed SSA validation: {e}"));
        }
        // Verify type info was inferred (slot 0 = val U64, slot 1 = is_null Bool)
        let info = prog.type_info(TxTypeId(0)).unwrap();
        assert_eq!(info.slot_types[0], Some(ValueType::U64));
        assert_eq!(info.slot_types[1], Some(ValueType::Bool));
    }

    #[test]
    fn test_lowered_select_passes_ssa_validation() {
        use tabula_executor::program::Program;

        let source = "\
table t { a: u64, b: u64 }
tx s(id: u64, flag: bool) {
    let x = t[id].a
    let y = t[id].b
    let result = select(flag, x, y)
    t[id].a = result
}";
        let compiled = compile(source);
        let mut prog = Program::new();
        for schema in &compiled.schemas {
            prog.add_schema(schema.clone());
        }
        for tx_type in &compiled.tx_types {
            prog.register(tx_type.clone())
                .unwrap_or_else(|e| panic!("lowered Select IR failed SSA validation: {e}"));
        }
    }

    // --- Logical AND in assert ---

    #[test]
    fn test_lower_assert_and() {
        let source = "\
tx t(x: u64, y: u64) {
    assert x > 0 && y > 0
}";
        let prog = compile(source);
        let body = &prog.tx_types[0].body;
        assert_eq!(body.len(), 1);
        if let Instruction::Assert { predicate } = &body[0] {
            assert!(matches!(predicate, Predicate::And(_, _)));
        } else {
            panic!("expected assert");
        }
    }
}
