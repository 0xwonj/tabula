//! Statement lowering methods for TxLower.

use tabula_ir::PrecompileId;

use super::{
    Binding, CompileError, ErrorKind, Instruction, LoweredExpr, Span, TxLower, ValueExpr, ast,
    builtin_bool_literal, synthesize_canonical_zero,
};
use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};

use crate::ast::{Expr, ExprKind, StmtKind};

impl<'a> TxLower<'a> {
    pub(super) fn lower_stmt(&mut self, stmt: &ast::Stmt) {
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
            StmtKind::Precompile {
                id,
                dst_names,
                inputs,
            } => self.lower_precompile(*id, dst_names, inputs, stmt.span),
        }
    }

    fn lower_let(&mut self, name: &str, value: &Expr, span: Span) {
        if self.locals.contains_key(name) || self.params.contains_key(name) {
            self.errors.push(CompileError::new(
                ErrorKind::DuplicateBinding,
                span,
                format!("'{name}' is already defined"),
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
                    Binding::ReadSlot {
                        val: dst_val,
                        is_null: dst_is_null,
                        ty: col_info.type_id,
                    },
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
                    .insert(name.to_string(), Binding::Slot(dst, col_info.type_id));
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
                    .insert(name.to_string(), Binding::Slot(dst, TYPE_BYTES32_ID));
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
                    .unwrap_or(TYPE_U64_ID);
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
                    format!("'{name}' is already defined"),
                ));
                return;
            }
        }
        if first == second {
            self.errors.push(CompileError::new(
                ErrorKind::DuplicateBinding,
                span,
                format!("destructuring binds '{first}' twice"),
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
        let ty = lhs_ty.or(rhs_ty).unwrap_or(TYPE_U64_ID);

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
            let zero = match synthesize_canonical_zero(self.registry, col_info.type_id, true) {
                Ok(zero) => zero,
                Err(detail) => {
                    self.errors.push(CompileError::new(
                        ErrorKind::TypeMismatch,
                        span,
                        format!(
                            "null assignment requires a descriptor-synthesizable zero: {detail}"
                        ),
                    ));
                    return;
                }
            };
            self.instructions.push(Instruction::Write {
                table: table_id,
                row: row_expr,
                col: col_info.id,
                src_val: ValueExpr::Literal(zero),
                src_is_null: ValueExpr::Literal(builtin_bool_literal(true)),
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
            src_is_null: ValueExpr::Literal(builtin_bool_literal(false)),
        });
    }

    fn lower_assert(&mut self, condition: &Expr) {
        let Some(cond) = self.lower_bool_expr(condition) else {
            return;
        };
        self.instructions.push(Instruction::Assert { cond });
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

    fn lower_precompile(&mut self, id: u16, dst_names: &[String], inputs: &[Expr], span: Span) {
        let precompile_id = PrecompileId(id);
        let Some(signature) = self.precompiles.get(&precompile_id).cloned() else {
            self.errors.push(CompileError::new(
                ErrorKind::UndefinedPrecompile,
                span,
                format!("undefined precompile 0x{id:04x}"),
            ));
            return;
        };

        // Check for duplicate bindings.
        for name in dst_names {
            if self.locals.contains_key(name) || self.params.contains_key(name) {
                self.errors.push(CompileError::new(
                    ErrorKind::DuplicateBinding,
                    span,
                    format!("'{name}' is already defined"),
                ));
                return;
            }
        }

        if inputs.len() != signature.inputs.len() {
            self.errors.push(CompileError::new(
                ErrorKind::TypeMismatch,
                span,
                format!(
                    "precompile 0x{id:04x} expects {} inputs but {} were provided",
                    signature.inputs.len(),
                    inputs.len(),
                ),
            ));
            return;
        }
        if dst_names.len() != signature.outputs.len() {
            self.errors.push(CompileError::new(
                ErrorKind::TypeMismatch,
                span,
                format!(
                    "precompile 0x{id:04x} expects {} outputs but {} bindings were provided",
                    signature.outputs.len(),
                    dst_names.len(),
                ),
            ));
            return;
        }

        // Allocate slots for each destination.
        let dst_slots: Vec<_> = dst_names.iter().map(|_| self.alloc_slot()).collect();

        // Lower input expressions.
        let input_ves: Vec<_> = inputs
            .iter()
            .filter_map(|a| self.lower_value_expr(a))
            .collect();
        if input_ves.len() != inputs.len() {
            return;
        }
        for (expr, expected) in inputs.iter().zip(&signature.inputs) {
            let actual = self.expr_type(expr);
            if actual != Some(expected.type_id) {
                self.errors.push(CompileError::new(
                    ErrorKind::TypeMismatch,
                    expr.span,
                    format!(
                        "precompile 0x{id:04x} input expects type id {} but got {}",
                        expected.type_id.0,
                        actual.map_or("unknown".to_string(), |ty| ty.0.to_string()),
                    ),
                ));
                return;
            }
        }

        self.instructions.push(Instruction::Precompile {
            id: precompile_id,
            dst_slots: dst_slots.clone(),
            inputs: input_ves,
        });

        // Bind destination names to their slots.
        for ((name, slot), output) in dst_names.iter().zip(dst_slots).zip(signature.outputs) {
            self.locals
                .insert(name.clone(), Binding::Slot(slot, output.type_id));
        }
    }
}
