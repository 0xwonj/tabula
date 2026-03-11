//! Name resolution and type inference helpers for TxLower.

use super::{
    ColumnInfo, CompileError, ErrorKind, Instruction, Span, TableId, TxLower, Value, ValueExpr,
    ValueType, is_arithmetic,
};

use crate::ast::{Expr, ExprKind, UnaryOp};

impl<'a> TxLower<'a> {
    /// Resolve an identifier to a ValueExpr.
    pub(super) fn resolve_ident(&mut self, name: &str, span: Span) -> Option<ValueExpr> {
        if let Some(binding) = self.locals.get(name) {
            Some(binding.to_value_expr())
        } else if let Some((idx, _)) = self.params.get(name) {
            Some(ValueExpr::Param(*idx))
        } else {
            self.errors.push(CompileError::new(
                ErrorKind::UndefinedVariable,
                span,
                format!("undefined variable '{name}'"),
            ));
            None
        }
    }

    /// Build a null-check Bool expression for `expr == null` or `expr != null`.
    /// Emits `Cmp { Eq, is_null_slot, Bool(is_eq) }` and returns the result slot.
    pub(super) fn null_check_expr(
        &mut self,
        expr: &Expr,
        is_eq: bool,
        span: Span,
    ) -> Option<ValueExpr> {
        // The expression must resolve to a ReadSlot binding (from a cell read).
        if let ExprKind::Ident(name) = &expr.kind
            && let Some(binding) = self.locals.get(name)
            && let Some(is_null_slot) = binding.is_null_slot()
        {
            let dst = self.alloc_slot();
            self.instructions.push(Instruction::Cmp {
                dst,
                op: tabula_ir::CmpOp::Eq,
                lhs: ValueExpr::Slot(is_null_slot),
                rhs: ValueExpr::Literal(Value::Bool(is_eq)),
            });
            return Some(ValueExpr::Slot(dst));
        }
        self.errors.push(CompileError::new(
            ErrorKind::TypeMismatch,
            span,
            "null comparison requires a cell-read binding (let x = table[row].col)",
        ));
        None
    }

    pub(super) fn resolve_table_col(
        &mut self,
        table_name: &str,
        col_name: &str,
        span: Span,
    ) -> Option<(TableId, ColumnInfo)> {
        let Some(table) = self.tables.get(table_name) else {
            self.errors.push(CompileError::new(
                ErrorKind::UndefinedTable,
                span,
                format!("undefined table '{table_name}'"),
            ));
            return None;
        };
        let Some(col) = table.columns.get(col_name) else {
            self.errors.push(CompileError::new(
                ErrorKind::UndefinedColumn,
                span,
                format!("undefined column '{col_name}' in table '{table_name}'"),
            ));
            return None;
        };
        Some((table.id, *col))
    }

    /// Infer the type of an expression (best-effort).
    pub(super) fn expr_type(&self, expr: &Expr) -> Option<ValueType> {
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
