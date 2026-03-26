use std::collections::BTreeMap;

use tabula_ir as ir;
use tabula_lang::hir;

use crate::mir;

mod callable;
mod const_eval;
mod ids;
mod manifest;

#[cfg(test)]
mod tests;

pub(crate) use const_eval::{
    decode_i64, decode_u64, eval_const_expr, invalid, portable_i64, portable_u64, single_output,
    zero_for_type,
};
pub(crate) use ids::{
    lower_context_field_id, lower_event_id, lower_field_id, lower_hash_family,
    lower_proof_visibility, lower_query_policy, lower_table_id, lower_totality,
};
pub use manifest::lower_hir_to_mir;

struct LowerCx<'a> {
    program: &'a hir::Program,
    program_id: ir::ProgramId,
    consts: BTreeMap<hir::ConstId, &'a hir::ConstDecl>,
    relations: BTreeMap<hir::RelationId, &'a hir::RelationDecl>,
    callables: BTreeMap<hir::CallableId, &'a hir::CallableDecl>,
}

struct CallableLowerCx<'a> {
    callable: &'a hir::CallableDecl,
    locals: Vec<mir::LocalDecl>,
    next_local: u32,
    bindings: BTreeMap<hir::BindingId, ir::ValueRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LowerRegionKind {
    Root,
    Nested,
}
