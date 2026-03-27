#![allow(clippy::wildcard_imports)]
#![allow(missing_docs)]

use crate::build::{FrontendPrelude, TypeCapabilityKind};
use crate::error::{FrontendError, FrontendErrorKind};
use crate::hir::*;
use std::collections::{BTreeMap, BTreeSet};

mod control;
mod effects;
mod env;
mod expr;
mod items;
mod relations;

use effects::{ensure_type, single_output, value_to_fingerprint};
use env::{LocalEnv, RegionKind};

pub fn verify_hir(
    program: Program,
    prelude: &FrontendPrelude,
) -> Result<VerifiedProgram, FrontendError> {
    VerifyCx::new(&program, prelude).verify()?;
    Ok(VerifiedProgram(program))
}

struct VerifyCx<'a> {
    program: &'a Program,
    prelude: &'a FrontendPrelude,
    capabilities: BTreeMap<CapabilityRefId, &'a CapabilityDescriptor>,
    capability_signatures: Vec<&'a CapabilityDescriptor>,
    context_fields: BTreeMap<ContextFieldId, &'a ContextFieldDecl>,
    tables: BTreeMap<TableId, &'a TableDecl>,
    table_fields: BTreeMap<(TableId, FieldId), &'a StateFieldDecl>,
    consts: BTreeMap<ConstId, &'a ConstDecl>,
    relations: BTreeMap<RelationId, &'a RelationDecl>,
    events: BTreeMap<EventId, &'a EventDecl>,
    callables: BTreeMap<CallableId, &'a CallableDecl>,
    top_level_symbols: BTreeSet<String>,
}

impl<'a> VerifyCx<'a> {
    fn new(program: &'a Program, prelude: &'a FrontendPrelude) -> Self {
        let mut capabilities = BTreeMap::new();
        let mut capability_signatures = Vec::new();
        for use_decl in &program.uses {
            capabilities.insert(use_decl.capability.id, &use_decl.capability);
            capability_signatures.push(&use_decl.capability);
        }

        let mut context_fields = BTreeMap::new();
        if let Some(context) = &program.context {
            for field in &context.fields {
                context_fields.insert(field.id, field);
            }
        }

        let mut tables = BTreeMap::new();
        let mut table_fields = BTreeMap::new();
        if let Some(state) = &program.state {
            for table in &state.tables {
                tables.insert(table.id, table);
                for field in &table.fields {
                    table_fields.insert((table.id, field.id), field);
                }
            }
        }

        let mut consts = BTreeMap::new();
        let mut relations = BTreeMap::new();
        let mut events = BTreeMap::new();
        let mut callables = BTreeMap::new();
        for item in &program.items {
            match item {
                Item::Const(decl) => {
                    consts.insert(decl.id, decl);
                }
                Item::Relation(decl) => {
                    relations.insert(decl.id, decl);
                }
                Item::Event(decl) => {
                    events.insert(decl.id, decl);
                }
                Item::Callable(decl) => {
                    callables.insert(decl.id, decl);
                }
            }
        }

        Self {
            program,
            prelude,
            capabilities,
            capability_signatures,
            context_fields,
            tables,
            table_fields,
            consts,
            relations,
            events,
            callables,
            top_level_symbols: BTreeSet::new(),
        }
    }

    fn verify(mut self) -> Result<(), FrontendError> {
        self.verify_top_level_symbols()?;
        self.verify_context()?;
        self.verify_state()?;
        self.verify_consts()?;
        self.verify_relations()?;
        self.verify_events()?;
        for item in &self.program.items {
            if let Item::Callable(callable) = item {
                self.verify_callable(callable)?;
            }
        }
        Ok(())
    }
}
