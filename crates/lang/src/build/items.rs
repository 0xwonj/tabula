#![allow(clippy::wildcard_imports)]

use std::collections::{BTreeMap, BTreeSet};

use super::consts::{body_span, build_literal_value};
use super::*;
use crate::error::{FrontendError, FrontendErrorKind};
use crate::verify::verify_hir;

pub fn build_hir(
    program: ast::Program,
    prelude: &FrontendPrelude,
) -> Result<hir::Program, FrontendError> {
    let collected = CollectCx::new(&program, prelude)?.collect()?;
    BuildCx::new(program, prelude, collected).build()
}

pub fn compile_to_hir(
    source: &str,
    prelude: &FrontendPrelude,
) -> Result<hir::VerifiedProgram, FrontendError> {
    let ast = crate::parse_program(source)?;
    let hir = build_hir(ast, prelude)?;
    verify_hir(hir, prelude)
}

impl<'a> BuildCx<'a> {
    pub(super) fn new(
        program: ast::Program,
        prelude: &'a FrontendPrelude,
        collected: CollectResult,
    ) -> Self {
        Self {
            top_level_names: collected.top_level_names.clone(),
            program,
            prelude,
            collected,
            context_infos: BTreeMap::new(),
            const_infos: BTreeMap::new(),
            relation_infos: BTreeMap::new(),
            event_infos: BTreeMap::new(),
            callable_infos: BTreeMap::new(),
            table_infos: BTreeMap::new(),
            capability_infos: BTreeMap::new(),
        }
    }

    fn build(mut self) -> Result<hir::Program, FrontendError> {
        let uses = self
            .collected
            .uses
            .iter()
            .map(|use_decl| {
                self.capability_infos.insert(
                    use_decl.descriptor.symbol.clone(),
                    use_decl.descriptor.clone(),
                );
                hir::UseDecl {
                    capability: use_decl.descriptor.clone(),
                    span: use_decl.span,
                }
            })
            .collect::<Vec<_>>();

        let context = self.build_context()?;
        let state = self.build_state()?;
        let const_items = self.build_consts()?;
        let relation_items = self.build_relations()?;
        let event_items = self.build_events()?;
        self.build_callable_signatures()?;
        let callable_items = self.build_callables()?;

        let mut items = Vec::new();
        items.extend(const_items.into_iter().map(hir::Item::Const));
        items.extend(relation_items.into_iter().map(hir::Item::Relation));
        items.extend(event_items.into_iter().map(hir::Item::Event));
        items.extend(callable_items.into_iter().map(hir::Item::Callable));

        Ok(hir::Program {
            symbol: self.program.symbol,
            uses,
            context,
            state,
            items,
            span: self.program.span,
        })
    }

    fn build_context(&mut self) -> Result<Option<hir::ContextDecl>, FrontendError> {
        let context_ast = self.program.decls.iter().find_map(|decl| match decl {
            ast::TopDecl::Context(context) => Some(context),
            _ => None,
        });
        let Some(context_ast) = context_ast else {
            return Ok(None);
        };
        let collected = self
            .collected
            .context
            .as_ref()
            .expect("context IDs collected");
        let mut fields = Vec::new();
        let mut seen_field_names = BTreeSet::new();
        for (index, field_ast) in context_ast.fields.iter().enumerate() {
            if !seen_field_names.insert(field_ast.symbol.clone()) {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    field_ast.span,
                    format!("duplicate context field {}", field_ast.symbol),
                ));
            }
            let ty = self.resolve_type_expr(&field_ast.ty)?;
            let field = hir::ContextFieldDecl {
                id: collected.field_ids[index],
                symbol: field_ast.symbol.clone(),
                ty,
                span: field_ast.span,
            };
            self.context_infos.insert(
                field.symbol.clone(),
                BuiltContextFieldInfo { id: field.id, ty },
            );
            fields.push(field);
        }
        Ok(Some(hir::ContextDecl {
            fields,
            span: context_ast.span,
        }))
    }

    fn build_state(&mut self) -> Result<Option<hir::StateDecl>, FrontendError> {
        let state_ast = self.program.decls.iter().find_map(|decl| match decl {
            ast::TopDecl::State(state) => Some(state),
            _ => None,
        });
        let Some(state_ast) = state_ast else {
            return Ok(None);
        };
        let mut tables = Vec::new();
        for (index, table_ast) in state_ast.tables.iter().enumerate() {
            let collected = &self.collected.state_tables[index];
            let mut keys = Vec::new();
            let mut seen_key_names = BTreeSet::new();
            for (param_index, key_ast) in table_ast.keys.iter().enumerate() {
                if !seen_key_names.insert(key_ast.symbol.clone()) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        key_ast.span,
                        format!("duplicate key field {}", key_ast.symbol),
                    ));
                }
                keys.push(hir::ParamDecl {
                    id: hir::ParamId(param_index as u32),
                    symbol: key_ast.symbol.clone(),
                    ty: self.resolve_type_expr(&key_ast.ty)?,
                    span: key_ast.span,
                });
            }
            let mut fields = Vec::new();
            let mut field_infos = BTreeMap::new();
            let mut seen_field_names = BTreeSet::new();
            for (field_index, field_ast) in table_ast.fields.iter().enumerate() {
                if !seen_field_names.insert(field_ast.symbol.clone()) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        field_ast.span,
                        format!("duplicate state field {}", field_ast.symbol),
                    ));
                }
                let ty = self.resolve_type_expr(&field_ast.ty)?;
                let scheme = field_ast
                    .scheme
                    .as_ref()
                    .map(|path| {
                        self.prelude
                            .resolve_scheme(&path.as_string(), path.span)
                            .map(|(id, symbol)| hir::SchemeRef { id, symbol })
                    })
                    .transpose()?;
                let field_decl = hir::StateFieldDecl {
                    id: collected.field_ids[field_index],
                    symbol: field_ast.symbol.clone(),
                    ty,
                    scheme,
                    span: field_ast.span,
                };
                field_infos.insert(
                    field_decl.symbol.clone(),
                    BuiltFieldInfo {
                        id: field_decl.id,
                        ty: field_decl.ty,
                    },
                );
                fields.push(field_decl);
            }
            let table = hir::TableDecl {
                id: collected.id,
                symbol: table_ast.symbol.clone(),
                keys: keys.clone(),
                fields,
                span: table_ast.span,
            };
            self.table_infos.insert(
                table.symbol.clone(),
                BuiltTableInfo {
                    id: table.id,
                    fields: field_infos,
                },
            );
            tables.push(table);
        }
        Ok(Some(hir::StateDecl {
            tables,
            span: state_ast.span,
        }))
    }

    fn build_consts(&mut self) -> Result<Vec<hir::ConstDecl>, FrontendError> {
        let mut built = Vec::new();
        let ast_consts = self.program.decls.iter().filter_map(|decl| match decl {
            ast::TopDecl::Const(const_decl) => Some(const_decl),
            _ => None,
        });
        for (index, const_ast) in ast_consts.enumerate() {
            let collected = &self.collected.consts[index];
            let ty = self.resolve_type_expr(&const_ast.ty)?;
            let value = self.build_const_expr(&const_ast.value, Some(ty))?;
            self.const_infos.insert(
                const_ast.symbol.clone(),
                BuiltConstInfo {
                    id: collected.id,
                    ty,
                },
            );
            built.push(hir::ConstDecl {
                id: collected.id,
                symbol: const_ast.symbol.clone(),
                ty,
                value,
                span: const_ast.span,
            });
        }
        Ok(built)
    }

    fn build_relations(&mut self) -> Result<Vec<hir::RelationDecl>, FrontendError> {
        let mut built = Vec::new();
        let ast_relations = self.program.decls.iter().filter_map(|decl| match decl {
            ast::TopDecl::Relation(relation) => Some(relation),
            _ => None,
        });
        for (index, relation_ast) in ast_relations.enumerate() {
            let collected = &self.collected.relations[index];
            let params = relation_ast
                .params
                .iter()
                .enumerate()
                .map(|(param_index, param)| {
                    Ok(hir::ParamDecl {
                        id: hir::ParamId(param_index as u32),
                        symbol: param.symbol.clone(),
                        ty: self.resolve_type_expr(&param.ty)?,
                        span: param.span,
                    })
                })
                .collect::<Result<Vec<_>, FrontendError>>()?;
            let results = relation_ast
                .results
                .iter()
                .map(|result| {
                    Ok(hir::ResultDecl {
                        symbol: result.symbol.clone(),
                        ty: self.resolve_type_expr(&result.ty)?,
                        span: result.span,
                    })
                })
                .collect::<Result<Vec<_>, FrontendError>>()?;
            let input_tys = params.iter().map(|param| param.ty).collect::<Vec<_>>();
            let output_tys = results.iter().map(|result| result.ty).collect::<Vec<_>>();
            let body = self.build_relation_body(&relation_ast.body, &input_tys, &output_tys)?;
            self.relation_infos.insert(
                relation_ast.symbol.clone(),
                BuiltRelationInfo {
                    id: collected.id,
                    outputs: output_tys,
                },
            );
            built.push(hir::RelationDecl {
                id: collected.id,
                symbol: relation_ast.symbol.clone(),
                params,
                results,
                body,
                span: relation_ast.span,
            });
        }
        Ok(built)
    }

    fn build_events(&mut self) -> Result<Vec<hir::EventDecl>, FrontendError> {
        let mut built = Vec::new();
        let ast_events = self.program.decls.iter().filter_map(|decl| match decl {
            ast::TopDecl::Event(event) => Some(event),
            _ => None,
        });
        for (index, event_ast) in ast_events.enumerate() {
            let collected = &self.collected.events[index];
            let fields = event_ast
                .fields
                .iter()
                .enumerate()
                .map(|(field_index, field)| {
                    Ok(hir::ParamDecl {
                        id: hir::ParamId(field_index as u32),
                        symbol: field.symbol.clone(),
                        ty: self.resolve_type_expr(&field.ty)?,
                        span: field.span,
                    })
                })
                .collect::<Result<Vec<_>, FrontendError>>()?;
            self.event_infos.insert(
                event_ast.symbol.clone(),
                BuiltEventInfo { id: collected.id },
            );
            built.push(hir::EventDecl {
                id: collected.id,
                symbol: event_ast.symbol.clone(),
                fields,
                span: event_ast.span,
            });
        }
        Ok(built)
    }

    fn build_callable_signatures(&mut self) -> Result<(), FrontendError> {
        let ast_callables = self.program.decls.iter().filter_map(|decl| match decl {
            ast::TopDecl::Callable(callable) => Some(callable),
            _ => None,
        });
        for (index, callable_ast) in ast_callables.enumerate() {
            let collected = &self.collected.callables[index];
            let params = callable_ast
                .params
                .iter()
                .map(|param| self.resolve_type_expr(&param.ty))
                .collect::<Result<Vec<_>, _>>()?;
            let returns = callable_ast
                .returns
                .iter()
                .map(|ty| self.resolve_type_expr(ty))
                .collect::<Result<Vec<_>, _>>()?;
            self.callable_infos.insert(
                callable_ast.symbol.clone(),
                BuiltCallableInfo {
                    id: collected.id,
                    params,
                    returns,
                },
            );
        }
        Ok(())
    }

    fn build_callables(&mut self) -> Result<Vec<hir::CallableDecl>, FrontendError> {
        let mut built = Vec::new();
        let ast_callables = self.program.decls.iter().filter_map(|decl| match decl {
            ast::TopDecl::Callable(callable) => Some(callable),
            _ => None,
        });
        for (index, callable_ast) in ast_callables.enumerate() {
            let collected = &self.collected.callables[index];
            let signature = self
                .callable_infos
                .get(&callable_ast.symbol)
                .expect("callable signature collected")
                .clone();
            let params = callable_ast
                .params
                .iter()
                .enumerate()
                .map(|(param_index, param)| hir::ParamDecl {
                    id: hir::ParamId(param_index as u32),
                    symbol: param.symbol.clone(),
                    ty: signature.params[param_index],
                    span: param.span,
                })
                .collect::<Vec<_>>();
            let body = BodyBuildCx::new(
                &self.top_level_names,
                &self.context_infos,
                &self.table_infos,
                &self.const_infos,
                &self.relation_infos,
                &self.event_infos,
                &self.callable_infos,
                &self.capability_infos,
                &params,
                &signature.returns,
            )
            .build_body(&callable_ast.body)?;
            built.push(hir::CallableDecl {
                id: collected.id,
                symbol: callable_ast.symbol.clone(),
                kind: collected.kind,
                params,
                returns: signature.returns,
                body,
                span: callable_ast.span,
            });
        }
        Ok(built)
    }

    fn build_relation_body(
        &self,
        body: &ast::RelationBody,
        input_tys: &[hir::TypeRef],
        output_tys: &[hir::TypeRef],
    ) -> Result<hir::RelationBody, FrontendError> {
        match body {
            ast::RelationBody::Enum { values, .. } => {
                if input_tys.len() != 1 || !output_tys.is_empty() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        body_span(body),
                        "enum relations require exactly one input and no outputs",
                    ));
                }
                Ok(hir::RelationBody::Enum {
                    values: values
                        .iter()
                        .map(|value| self.build_const_expr(value, Some(input_tys[0])))
                        .collect::<Result<Vec<_>, _>>()?,
                })
            }
            ast::RelationBody::Range { start, end, .. } => {
                if input_tys.len() != 1 || !output_tys.is_empty() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        body_span(body),
                        "range relations require exactly one input and no outputs",
                    ));
                }
                Ok(hir::RelationBody::Range {
                    start: self.build_const_expr(start, Some(input_tys[0]))?,
                    end: self.build_const_expr(end, Some(input_tys[0]))?,
                })
            }
            ast::RelationBody::Map { entries, .. } => Ok(hir::RelationBody::Map {
                entries: entries
                    .iter()
                    .map(|entry| {
                        if entry.inputs.len() != input_tys.len()
                            || entry.outputs.len() != output_tys.len()
                        {
                            return Err(FrontendError::new(
                                FrontendErrorKind::TypeMismatch,
                                entry.span,
                                "relation map entry arity mismatch",
                            ));
                        }
                        Ok(hir::RelationMapEntry {
                            inputs: entry
                                .inputs
                                .iter()
                                .zip(input_tys)
                                .map(|(value, ty)| self.build_const_expr(value, Some(*ty)))
                                .collect::<Result<Vec<_>, _>>()?,
                            outputs: entry
                                .outputs
                                .iter()
                                .zip(output_tys)
                                .map(|(value, ty)| self.build_const_expr(value, Some(*ty)))
                                .collect::<Result<Vec<_>, _>>()?,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            ast::RelationBody::Set { tuples, .. } => {
                if !output_tys.is_empty() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        body_span(body),
                        "set relations require no outputs",
                    ));
                }
                Ok(hir::RelationBody::Set {
                    tuples: tuples
                        .iter()
                        .map(|tuple| {
                            if tuple.len() != input_tys.len() {
                                return Err(FrontendError::new(
                                    FrontendErrorKind::TypeMismatch,
                                    body_span(body),
                                    "relation set tuple arity mismatch",
                                ));
                            }
                            tuple
                                .iter()
                                .zip(input_tys)
                                .map(|(value, ty)| self.build_const_expr(value, Some(*ty)))
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                })
            }
            ast::RelationBody::Extern { .. } => Ok(hir::RelationBody::Extern),
        }
    }

    fn build_const_expr(
        &self,
        expr: &ast::Expr,
        expected_ty: Option<hir::TypeRef>,
    ) -> Result<hir::ConstExpr, FrontendError> {
        match expr {
            ast::Expr::Literal(literal) => Ok(hir::ConstExpr::Literal(build_literal_value(
                &literal.kind,
                expected_ty,
                literal.span,
            )?)),
            ast::Expr::Unary(unary) => {
                let expr = self.build_const_expr(&unary.expr, expected_ty)?;
                Ok(hir::ConstExpr::Unary {
                    op: super::consts::convert_unary_op(unary.op),
                    expr: Box::new(expr),
                })
            }
            ast::Expr::Binary(binary) => Ok(hir::ConstExpr::Binary {
                op: super::consts::convert_binary_op(binary.op),
                lhs: Box::new(self.build_const_expr(&binary.lhs, expected_ty)?),
                rhs: Box::new(self.build_const_expr(&binary.rhs, expected_ty)?),
            }),
            _ => Err(FrontendError::new(
                FrontendErrorKind::InvalidConstExpr,
                expr.span(),
                "invalid const expression in the current rewritten frontend",
            )),
        }
    }

    fn resolve_type_expr(&self, ty: &ast::TypeExpr) -> Result<hir::TypeRef, FrontendError> {
        self.prelude
            .resolve_type(&ty.path.as_string(), ty.path.span)
    }
}
