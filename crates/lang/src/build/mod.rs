use std::collections::{BTreeMap, BTreeSet};

use crate::ast;
use crate::hir;
use crate::span::Span;

mod body;
mod collect;
mod consts;
mod expr;
mod items;
mod prelude;

pub use items::{build_hir, compile_to_hir};
pub(crate) use prelude::TypeCapabilityKind;
pub use prelude::{CapabilityPreludeEntry, FrontendPrelude};

#[derive(Debug, Clone)]
struct CollectResult {
    uses: Vec<CollectedUse>,
    context: Option<CollectedContext>,
    state_tables: Vec<CollectedTable>,
    consts: Vec<CollectedConst>,
    relations: Vec<CollectedRelation>,
    events: Vec<CollectedEvent>,
    callables: Vec<CollectedCallable>,
    top_level_names: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct CollectedUse {
    descriptor: hir::CapabilityDescriptor,
    span: Span,
}

#[derive(Debug, Clone)]
struct CollectedTable {
    id: hir::TableId,
    field_ids: Vec<hir::FieldId>,
}

#[derive(Debug, Clone)]
struct CollectedContext {
    field_ids: Vec<hir::ContextFieldId>,
}

#[derive(Debug, Clone)]
struct CollectedConst {
    id: hir::ConstId,
}

#[derive(Debug, Clone)]
struct CollectedRelation {
    id: hir::RelationId,
}

#[derive(Debug, Clone)]
struct CollectedEvent {
    id: hir::EventId,
}

#[derive(Debug, Clone)]
struct CollectedCallable {
    id: hir::CallableId,
    kind: hir::CallableKind,
}

struct CollectCx<'a> {
    program: &'a ast::Program,
    prelude: &'a FrontendPrelude,
}

#[derive(Debug, Clone)]
struct BuiltConstInfo {
    id: hir::ConstId,
    ty: hir::TypeRef,
}

#[derive(Debug, Clone)]
struct BuiltRelationInfo {
    id: hir::RelationId,
    outputs: Vec<hir::TypeRef>,
}

#[derive(Debug, Clone)]
struct BuiltCallableInfo {
    id: hir::CallableId,
    params: Vec<hir::TypeRef>,
    returns: Vec<hir::TypeRef>,
}

#[derive(Debug, Clone)]
struct BuiltContextFieldInfo {
    id: hir::ContextFieldId,
    ty: hir::TypeRef,
}

#[derive(Debug, Clone)]
struct BuiltTableInfo {
    id: hir::TableId,
    fields: BTreeMap<String, BuiltFieldInfo>,
}

#[derive(Debug, Clone)]
struct BuiltFieldInfo {
    id: hir::FieldId,
    ty: hir::TypeRef,
}

#[derive(Debug, Clone)]
struct BuiltEventInfo {
    id: hir::EventId,
}

struct BuildCx<'a> {
    program: ast::Program,
    prelude: &'a FrontendPrelude,
    collected: CollectResult,
    top_level_names: BTreeSet<String>,
    context_infos: BTreeMap<String, BuiltContextFieldInfo>,
    const_infos: BTreeMap<String, BuiltConstInfo>,
    relation_infos: BTreeMap<String, BuiltRelationInfo>,
    event_infos: BTreeMap<String, BuiltEventInfo>,
    callable_infos: BTreeMap<String, BuiltCallableInfo>,
    table_infos: BTreeMap<String, BuiltTableInfo>,
    capability_infos: BTreeMap<String, hir::CapabilityDescriptor>,
}

struct BodyBuildCx<'a> {
    top_level_names: &'a BTreeSet<String>,
    context_fields: &'a BTreeMap<String, BuiltContextFieldInfo>,
    tables: &'a BTreeMap<String, BuiltTableInfo>,
    consts: &'a BTreeMap<String, BuiltConstInfo>,
    relations: &'a BTreeMap<String, BuiltRelationInfo>,
    events: &'a BTreeMap<String, BuiltEventInfo>,
    callables: &'a BTreeMap<String, BuiltCallableInfo>,
    capabilities: &'a BTreeMap<String, hir::CapabilityDescriptor>,
    params: &'a [hir::ParamDecl],
    returns: &'a [hir::TypeRef],
    bindings: BTreeMap<String, BindingInfo>,
    next_binding_id: u32,
}

#[derive(Debug, Clone, Copy)]
struct BindingInfo {
    id: hir::BindingId,
    ty: hir::TypeRef,
}

#[derive(Debug)]
struct TypedExpr {
    expr: hir::Expr,
    ty: Option<hir::TypeRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionTermKind {
    Root,
    Nested,
}
