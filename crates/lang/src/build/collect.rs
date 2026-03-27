#![allow(missing_docs)]

use std::collections::BTreeSet;

use super::consts::insert_top_name;
#[allow(clippy::wildcard_imports)]
use super::*;
use crate::error::{FrontendError, FrontendErrorKind};

impl<'a> CollectCx<'a> {
    pub(super) fn new(
        program: &'a ast::Program,
        prelude: &'a FrontendPrelude,
    ) -> Result<Self, FrontendError> {
        Ok(Self { program, prelude })
    }

    pub(super) fn collect(&self) -> Result<CollectResult, FrontendError> {
        let mut top_level_names = BTreeSet::new();
        let mut uses = Vec::new();
        let mut context = None;
        for (next_capability_id, use_decl) in self.program.uses.iter().enumerate() {
            let alias = use_decl.path.last().ok_or_else(|| {
                FrontendError::new(
                    FrontendErrorKind::InvalidProgram,
                    use_decl.span,
                    "capability import path cannot be empty",
                )
            })?;
            insert_top_name(&mut top_level_names, alias, use_decl.span)?;
            let capability = self
                .prelude
                .resolve_capability(&use_decl.path, use_decl.span)?;
            uses.push(CollectedUse {
                descriptor: hir::CapabilityDescriptor {
                    id: hir::CapabilityRefId(next_capability_id as u32),
                    symbol: alias.to_string(),
                    path: capability.path.clone(),
                    inputs: capability.inputs.clone(),
                    outputs: capability.outputs.clone(),
                    totality: capability.totality,
                    query_policy: capability.query_policy,
                    proof_visibility: capability.proof_visibility,
                    hash_family: capability.hash_family,
                },
                span: use_decl.span,
            });
        }

        let mut state_tables = Vec::new();
        let mut consts = Vec::new();
        let mut relations = Vec::new();
        let mut events = Vec::new();
        let mut callables = Vec::new();
        let mut next_context_field_id = 0u32;
        let mut next_table_id = 0u32;
        let mut next_const_id = 0u32;
        let mut next_relation_id = 0u32;
        let mut next_event_id = 0u32;
        let mut next_callable_id = 0u32;

        let context_decl_count = self
            .program
            .decls
            .iter()
            .filter(|decl| matches!(decl, ast::TopDecl::Context(_)))
            .count();
        if context_decl_count > 1 {
            return Err(FrontendError::new(
                FrontendErrorKind::InvalidProgram,
                self.program.span,
                "at most one context block is allowed",
            ));
        }

        let state_decl_count = self
            .program
            .decls
            .iter()
            .filter(|decl| matches!(decl, ast::TopDecl::State(_)))
            .count();
        if state_decl_count > 1 {
            return Err(FrontendError::new(
                FrontendErrorKind::InvalidProgram,
                self.program.span,
                "at most one state block is allowed",
            ));
        }

        for decl in &self.program.decls {
            match decl {
                ast::TopDecl::Context(context_decl) => {
                    context = Some(CollectedContext {
                        field_ids: (0..context_decl.fields.len())
                            .map(|_| {
                                let id = hir::ContextFieldId(next_context_field_id);
                                next_context_field_id += 1;
                                id
                            })
                            .collect(),
                    });
                }
                ast::TopDecl::State(state) => {
                    for table in &state.tables {
                        insert_top_name(&mut top_level_names, &table.symbol, table.span)?;
                        state_tables.push(CollectedTable {
                            id: hir::TableId(next_table_id),
                            field_ids: (0..table.fields.len())
                                .map(|index| hir::FieldId(index as u16))
                                .collect(),
                        });
                        next_table_id += 1;
                    }
                }
                ast::TopDecl::Const(decl) => {
                    insert_top_name(&mut top_level_names, &decl.symbol, decl.span)?;
                    consts.push(CollectedConst {
                        id: hir::ConstId(next_const_id),
                    });
                    next_const_id += 1;
                }
                ast::TopDecl::Relation(decl) => {
                    insert_top_name(&mut top_level_names, &decl.symbol, decl.span)?;
                    relations.push(CollectedRelation {
                        id: hir::RelationId(next_relation_id),
                    });
                    next_relation_id += 1;
                }
                ast::TopDecl::Event(decl) => {
                    insert_top_name(&mut top_level_names, &decl.symbol, decl.span)?;
                    events.push(CollectedEvent {
                        id: hir::EventId(next_event_id),
                    });
                    next_event_id += 1;
                }
                ast::TopDecl::Callable(decl) => {
                    insert_top_name(&mut top_level_names, &decl.symbol, decl.span)?;
                    callables.push(CollectedCallable {
                        id: hir::CallableId(next_callable_id),
                        kind: match decl.kind {
                            ast::CallableKind::Function => hir::CallableKind::Function,
                            ast::CallableKind::Query => hir::CallableKind::Query,
                            ast::CallableKind::Tx => hir::CallableKind::Tx,
                        },
                    });
                    next_callable_id += 1;
                }
            }
        }

        Ok(CollectResult {
            uses,
            context,
            state_tables,
            consts,
            relations,
            events,
            callables,
            top_level_names,
        })
    }
}
