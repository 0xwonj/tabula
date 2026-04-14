use tabula_core::KeyComponentSchema;
use tabula_ir as ir;
use tabula_lang::hir;
use tabula_profile::{TYPE_I64_ID, TYPE_U64_ID};

use super::{
    LowerCx, decode_i64, decode_u64, eval_const_expr, invalid, lower_context_field_id,
    lower_event_id, lower_field_id, lower_proof_visibility, lower_query_policy, lower_table_id,
    lower_totality, portable_i64, portable_u64,
};
use crate::error::CompilerError;
use crate::mir;

pub fn lower_hir_to_mir(
    program: &hir::VerifiedProgram,
    program_id: ir::ProgramId,
) -> Result<mir::Program, CompilerError> {
    LowerCx::new(program.program(), program_id).lower_program()
}

impl<'a> LowerCx<'a> {
    pub(super) fn new(program: &'a hir::Program, program_id: ir::ProgramId) -> Self {
        let consts = program
            .items
            .iter()
            .filter_map(|item| match item {
                hir::Item::Const(decl) => Some((decl.id, decl)),
                _ => None,
            })
            .collect();
        let relations = program
            .items
            .iter()
            .filter_map(|item| match item {
                hir::Item::Relation(decl) => Some((decl.id, decl)),
                _ => None,
            })
            .collect();
        let callables = program
            .items
            .iter()
            .filter_map(|item| match item {
                hir::Item::Callable(decl) => Some((decl.id, decl)),
                _ => None,
            })
            .collect();
        Self {
            program,
            program_id,
            consts,
            relations,
            callables,
        }
    }

    fn lower_program(&self) -> Result<mir::Program, CompilerError> {
        Ok(mir::Program {
            program_id: self.program_id,
            state: self.lower_state(),
            context: self.lower_context(),
            const_pool: self.lower_const_pool()?,
            relation_manifest: self.lower_relation_manifest()?,
            capability_manifest: self.lower_capability_manifest(),
            event_manifest: self.lower_event_manifest(),
            callables: self.lower_callables()?,
        })
    }

    fn lower_state(&self) -> ir::StateSchema {
        let tables = self
            .program
            .state
            .as_ref()
            .map(|state| {
                state
                    .tables
                    .iter()
                    .map(|table| ir::TableSchema {
                        id: lower_table_id(table.id),
                        symbol: table.symbol.clone(),
                        keys: table
                            .keys
                            .iter()
                            .map(|key| KeyComponentSchema {
                                symbol: key.symbol.clone(),
                                ty: key.ty,
                            })
                            .collect(),
                        fields: table
                            .fields
                            .iter()
                            .map(|field| ir::FieldSchema {
                                id: lower_field_id(field.id),
                                symbol: field.symbol.clone(),
                                ty: field.ty,
                            })
                            .collect(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        ir::StateSchema { tables }
    }

    fn lower_context(&self) -> ir::ContextSchema {
        ir::ContextSchema {
            fields: self
                .program
                .context
                .as_ref()
                .map(|context| {
                    context
                        .fields
                        .iter()
                        .map(|field| ir::ContextField {
                            id: lower_context_field_id(field.id),
                            symbol: field.symbol.clone(),
                            ty: field.ty,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    fn lower_const_pool(&self) -> Result<ir::ConstantPool, CompilerError> {
        let entries = self
            .consts
            .values()
            .map(|decl| {
                Ok(ir::ConstantEntry {
                    id: ir::ConstId(decl.id.0),
                    ty: decl.ty,
                    value: eval_const_expr(&decl.value)?,
                })
            })
            .collect::<Result<Vec<_>, CompilerError>>()?;
        Ok(ir::ConstantPool { entries })
    }

    fn lower_relation_manifest(&self) -> Result<ir::RelationManifest, CompilerError> {
        let mut entries = Vec::new();
        for decl in self.relations.values() {
            let descriptor = ir::RelationDescriptor {
                symbol: decl.symbol.clone(),
                inputs: decl.params.iter().map(|param| param.ty).collect(),
                outputs: decl.results.iter().map(|result| result.ty).collect(),
            };
            let binding = match &decl.body {
                hir::RelationBody::Enum { values } => ir::RelationBinding::EnumSet {
                    values: values
                        .iter()
                        .map(eval_const_expr)
                        .collect::<Result<Vec<_>, _>>()?,
                },
                hir::RelationBody::Range { start, end } => {
                    if descriptor.inputs.len() != 1 || !descriptor.outputs.is_empty() {
                        return Err(invalid(
                            "range relation lowering requires one input and no outputs",
                        ));
                    }
                    let start = eval_const_expr(start)?;
                    let end = eval_const_expr(end)?;
                    match descriptor.inputs[0] {
                        TYPE_U64_ID => {
                            let lower = decode_u64(&start)?;
                            let upper = decode_u64(&end)?;
                            ir::RelationBinding::EnumSet {
                                values: (lower..upper).map(portable_u64).collect(),
                            }
                        }
                        TYPE_I64_ID => {
                            let lower = decode_i64(&start)?;
                            let upper = decode_i64(&end)?;
                            ir::RelationBinding::EnumSet {
                                values: (lower..upper).map(portable_i64).collect(),
                            }
                        }
                        _ => {
                            return Err(invalid(
                                "range relation lowering requires u64 or i64 input type",
                            ));
                        }
                    }
                }
                hir::RelationBody::Map { entries } => ir::RelationBinding::Map {
                    rows: entries
                        .iter()
                        .map(|entry| {
                            Ok(ir::RelationRow {
                                inputs: entry.inputs.iter().map(eval_const_expr).collect::<Result<
                                    Vec<_>,
                                    _,
                                >>(
                                )?,
                                outputs: entry
                                    .outputs
                                    .iter()
                                    .map(eval_const_expr)
                                    .collect::<Result<Vec<_>, _>>()?,
                            })
                        })
                        .collect::<Result<Vec<_>, CompilerError>>()?,
                },
                hir::RelationBody::Set { tuples } => ir::RelationBinding::Map {
                    rows: tuples
                        .iter()
                        .map(|tuple: &Vec<hir::ConstExpr>| {
                            Ok(ir::RelationRow {
                                inputs: tuple
                                    .iter()
                                    .map(eval_const_expr)
                                    .collect::<Result<Vec<_>, _>>()?,
                                outputs: vec![],
                            })
                        })
                        .collect::<Result<Vec<_>, CompilerError>>()?,
                },
                hir::RelationBody::Extern => {
                    return Err(invalid(
                        "extern relation lowering is not supported by the V1 canonical manifest",
                    ));
                }
            };
            entries.push(ir::RelationManifestEntry {
                id: ir::RelationId(decl.id.0),
                descriptor,
                binding,
            });
        }
        Ok(ir::RelationManifest { entries })
    }

    fn lower_capability_manifest(&self) -> ir::CapabilityManifest {
        ir::CapabilityManifest {
            entries: self
                .program
                .uses
                .iter()
                .map(|use_decl| ir::CapabilityDescriptor {
                    id: ir::CapabilityId(use_decl.capability.id.0),
                    symbol: use_decl.capability.symbol.clone(),
                    inputs: use_decl.capability.inputs.clone(),
                    outputs: use_decl.capability.outputs.clone(),
                    totality: lower_totality(use_decl.capability.totality),
                    query_policy: lower_query_policy(use_decl.capability.query_policy),
                    proof_visibility: lower_proof_visibility(use_decl.capability.proof_visibility),
                })
                .collect(),
        }
    }

    fn lower_event_manifest(&self) -> ir::EventManifest {
        ir::EventManifest {
            entries: self
                .program
                .items
                .iter()
                .filter_map(|item| match item {
                    hir::Item::Event(decl) => Some(ir::EventDescriptor {
                        id: lower_event_id(decl.id),
                        symbol: decl.symbol.clone(),
                        fields: decl.fields.iter().map(|field| field.ty).collect(),
                    }),
                    _ => None,
                })
                .collect(),
        }
    }

    fn lower_callables(&self) -> Result<Vec<mir::Callable>, CompilerError> {
        self.callables
            .values()
            .copied()
            .map(Self::lower_callable)
            .collect()
    }

    fn lower_callable(callable: &hir::CallableDecl) -> Result<mir::Callable, CompilerError> {
        let params = callable
            .params
            .iter()
            .map(|param| ir::ParamDecl {
                id: ir::ParamId(param.id.0),
                symbol: param.symbol.clone(),
                ty: param.ty,
            })
            .collect::<Vec<_>>();
        let mut body_lower = super::CallableLowerCx::new(callable);
        let body = body_lower.lower_body()?;
        Ok(mir::Callable {
            id: mir::CallableId(callable.id.0),
            symbol: callable.symbol.clone(),
            kind: match callable.kind {
                hir::CallableKind::Function => mir::CallableKind::Function,
                hir::CallableKind::Query => mir::CallableKind::Query,
                hir::CallableKind::Tx => mir::CallableKind::Tx,
            },
            params,
            returns: callable.returns.clone(),
            body,
        })
    }
}
