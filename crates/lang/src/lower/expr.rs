//! Expression lowering methods for TxLower.

use super::{
    Binding, CompileError, ErrorKind, Instruction, LoweredExpr, TxLower, Value, ValueExpr,
    ValueType, is_arithmetic,
};

use tabula_ir::{ArithOp, CmpOp, RowExpr};

use crate::ast::{BinOp, Expr, ExprKind, UnaryOp};

impl<'a> TxLower<'a> {
    /// Lower an expression that needs its own slot (for arithmetic results).
    /// Returns (LoweredExpr, inferred type).
    pub(super) fn lower_expr_to_slot(&mut self, expr: &Expr) -> Option<(LoweredExpr, ValueType)> {
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
                    BinOp::Add => Instruction::Arith {
                        dst,
                        op: ArithOp::Add,
                        lhs: lhs_ve,
                        rhs: rhs_ve,
                    },
                    BinOp::Sub => Instruction::Arith {
                        dst,
                        op: ArithOp::Sub,
                        lhs: lhs_ve,
                        rhs: rhs_ve,
                    },
                    BinOp::Mul => Instruction::Arith {
                        dst,
                        op: ArithOp::Mul,
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
                self.instructions.push(Instruction::Arith {
                    dst,
                    op: ArithOp::Sub,
                    lhs: ValueExpr::Literal(tabula_core::zero_value(ty)),
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
    pub(super) fn lower_value_expr(&mut self, expr: &Expr) -> Option<ValueExpr> {
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
    pub(super) fn lower_row_expr(&mut self, expr: &Expr) -> Option<RowExpr> {
        match &expr.kind {
            ExprKind::IntLit(n) => Some(RowExpr::Literal(tabula_core::RowKey(*n))),
            ExprKind::Ident(name) => {
                if let Some(binding) = self.locals.get(name).cloned() {
                    match binding {
                        Binding::Slot(slot, ty) => {
                            if ty != ValueType::U64 {
                                self.errors.push(CompileError::new(
                                    ErrorKind::TypeMismatch,
                                    expr.span,
                                    format!("row key expression must have type u64, found {ty:?}"),
                                ));
                                return None;
                            }
                            Some(RowExpr::Slot(slot))
                        }
                        Binding::ReadSlot { val, ty, .. } => {
                            if ty != ValueType::U64 {
                                self.errors.push(CompileError::new(
                                    ErrorKind::TypeMismatch,
                                    expr.span,
                                    format!("row key expression must have type u64, found {ty:?}"),
                                ));
                                return None;
                            }
                            Some(RowExpr::Slot(val))
                        }
                        Binding::Alias(value_expr, ty) => {
                            if ty != ValueType::U64 {
                                self.errors.push(CompileError::new(
                                    ErrorKind::TypeMismatch,
                                    expr.span,
                                    format!("row key expression must have type u64, found {ty:?}"),
                                ));
                                return None;
                            }
                            match value_expr {
                                ValueExpr::Literal(Value::U64(n)) => {
                                    Some(RowExpr::Literal(tabula_core::RowKey(n)))
                                }
                                ValueExpr::Slot(slot) => Some(RowExpr::Slot(slot)),
                                ValueExpr::Param(param) => Some(RowExpr::Param(param)),
                                ValueExpr::Literal(other) => {
                                    self.errors.push(CompileError::new(
                                        ErrorKind::TypeMismatch,
                                        expr.span,
                                        format!(
                                            "row key expression must have type u64, found {}",
                                            other.type_name()
                                        ),
                                    ));
                                    None
                                }
                            }
                        }
                    }
                } else if let Some((idx, ty)) = self.params.get(name) {
                    if *ty != ValueType::U64 {
                        self.errors.push(CompileError::new(
                            ErrorKind::TypeMismatch,
                            expr.span,
                            format!("row key expression must have type u64, found {ty:?}"),
                        ));
                        return None;
                    }
                    Some(RowExpr::Param(*idx))
                } else {
                    self.errors.push(CompileError::new(
                        ErrorKind::UndefinedVariable,
                        expr.span,
                        format!("undefined variable '{name}'"),
                    ));
                    None
                }
            }
            // Arithmetic expression as row key — emit to slot, then use slot.
            _ => {
                let ve = self.lower_value_expr(expr)?;
                match ve {
                    ValueExpr::Literal(Value::U64(n)) => {
                        Some(RowExpr::Literal(tabula_core::RowKey(n)))
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

    /// Lower a comparison/logical expression to a Bool-typed `ValueExpr`.
    ///
    /// Emits flat `Cmp`, `Not`, `And`, `Or` instructions as needed, returning
    /// a `ValueExpr` that references the Bool result.
    pub(super) fn lower_bool_expr(&mut self, expr: &Expr) -> Option<ValueExpr> {
        match &expr.kind {
            ExprKind::BinOp { op, lhs, rhs } => match op {
                BinOp::Eq => {
                    // Special case: x == null → check is_null slot
                    if matches!(&rhs.kind, ExprKind::Null) {
                        return self.null_check_expr(lhs, true, expr.span);
                    }
                    if matches!(&lhs.kind, ExprKind::Null) {
                        return self.null_check_expr(rhs, true, expr.span);
                    }
                    self.emit_cmp(CmpOp::Eq, lhs, rhs)
                }
                BinOp::Neq => {
                    // Special case: x != null → check is_null slot is false
                    if matches!(&rhs.kind, ExprKind::Null) {
                        return self.null_check_expr(lhs, false, expr.span);
                    }
                    if matches!(&lhs.kind, ExprKind::Null) {
                        return self.null_check_expr(rhs, false, expr.span);
                    }
                    self.emit_cmp(CmpOp::Ne, lhs, rhs)
                }
                BinOp::Lt => self.emit_cmp(CmpOp::Lt, lhs, rhs),
                BinOp::Lte => self.emit_cmp(CmpOp::Lte, lhs, rhs),
                BinOp::Gt => self.emit_cmp(CmpOp::Gt, lhs, rhs),
                BinOp::Gte => self.emit_cmp(CmpOp::Gte, lhs, rhs),
                BinOp::And => {
                    let l = self.lower_bool_expr(lhs)?;
                    let r = self.lower_bool_expr(rhs)?;
                    let dst = self.alloc_slot();
                    self.instructions.push(Instruction::And {
                        dst,
                        lhs: l,
                        rhs: r,
                    });
                    Some(ValueExpr::Slot(dst))
                }
                BinOp::Or => {
                    let l = self.lower_bool_expr(lhs)?;
                    let r = self.lower_bool_expr(rhs)?;
                    let dst = self.alloc_slot();
                    self.instructions.push(Instruction::Or {
                        dst,
                        lhs: l,
                        rhs: r,
                    });
                    Some(ValueExpr::Slot(dst))
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
                let inner = self.lower_bool_expr(operand)?;
                let dst = self.alloc_slot();
                self.instructions.push(Instruction::Not { dst, src: inner });
                Some(ValueExpr::Slot(dst))
            }
            ExprKind::BoolLit(b) => Some(ValueExpr::Literal(Value::Bool(*b))),
            ExprKind::Ident(name) => self.resolve_ident(name, expr.span),
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

    /// Emit a `Cmp` instruction and return a `ValueExpr::Slot` referencing the Bool result.
    fn emit_cmp(&mut self, op: CmpOp, lhs: &Expr, rhs: &Expr) -> Option<ValueExpr> {
        let l = self.lower_value_expr(lhs)?;
        let r = self.lower_value_expr(rhs)?;
        let dst = self.alloc_slot();
        self.instructions.push(Instruction::Cmp {
            dst,
            op,
            lhs: l,
            rhs: r,
        });
        Some(ValueExpr::Slot(dst))
    }
}
