//! Executor adapter for the next-generation canonical IR.

use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_core::{CommittedCellKey, TypeId};
use tabula_ir as ir;
use tabula_types::{
    ContextValues, RelationEffect, StatePropertyEffect, TypedEventEffect, TypedStateEffect,
    TypedValue, typed_bool,
};

use crate::machine::effects::EffectRecorder;
use crate::machine::frame::LocalFrame;
use crate::machine::ops;
use crate::program::{ResolvedEntry, ResolvedExecutionProgram};
use crate::state::Overlay;
use crate::surface::{CapabilityEffect, ExecContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrapKind {
    Semantic,
    Fatal,
}

#[derive(Debug)]
pub(crate) struct EntryTrap {
    pub(crate) kind: TrapKind,
    pub(crate) op_index: usize,
    pub(crate) error: TabulaError,
}

#[derive(Debug)]
pub(in crate::machine) enum OpFailure {
    Semantic(TabulaError),
    Fatal(TabulaError),
}

impl OpFailure {
    fn semantic(error: TabulaError) -> Self {
        Self::Semantic(error)
    }

    fn fatal(error: TabulaError) -> Self {
        Self::Fatal(error)
    }

    fn at(self, op_index: usize) -> EntryTrap {
        match self {
            Self::Semantic(error) => EntryTrap {
                kind: TrapKind::Semantic,
                op_index,
                error,
            },
            Self::Fatal(error) => EntryTrap {
                kind: TrapKind::Fatal,
                op_index,
                error,
            },
        }
    }
}

pub(in crate::machine) fn semantic<T>(result: Result<T, TabulaError>) -> Result<T, OpFailure> {
    result.map_err(OpFailure::semantic)
}

pub(in crate::machine) fn fatal<T>(result: Result<T, TabulaError>) -> Result<T, OpFailure> {
    result.map_err(OpFailure::fatal)
}
pub(crate) struct EntryExecution {
    pub(crate) returns: Vec<TypedValue>,
    pub(crate) state_effects: Vec<TypedStateEffect>,
    pub(crate) property_effects: Vec<StatePropertyEffect>,
    pub(crate) relation_effects: Vec<RelationEffect>,
    pub(crate) capability_effects: Vec<CapabilityEffect>,
    pub(crate) event_effects: Vec<TypedEventEffect>,
    pub(crate) next_logical_time: u64,
}

pub(crate) struct EntryMachineCore<'a, 'snap, 'exec, S: StateView> {
    pub(in crate::machine) program: &'a ResolvedExecutionProgram,
    pub(in crate::machine) entry: &'a ResolvedEntry,
    params: &'a [TypedValue],
    context: &'a ContextValues,
    pub(in crate::machine) overlay: &'a mut Overlay<'snap, S>,
    pub(in crate::machine) exec: &'exec ExecContext<'exec>,
    locals: LocalFrame,
    pub(in crate::machine) effects: EffectRecorder,
}

impl<'a, 'snap, 'exec, S: StateView> EntryMachineCore<'a, 'snap, 'exec, S> {
    pub(crate) fn new(
        program: &'a ResolvedExecutionProgram,
        entry: &'a ResolvedEntry,
        params: &'a [TypedValue],
        context: &'a ContextValues,
        overlay: &'a mut Overlay<'snap, S>,
        exec: &'exec ExecContext<'exec>,
        start_logical_time: u64,
    ) -> Self {
        Self {
            program,
            entry,
            params,
            context,
            overlay,
            exec,
            locals: LocalFrame::new(entry),
            effects: EffectRecorder::new(start_logical_time),
        }
    }

    pub(crate) fn execute(mut self) -> Result<EntryExecution, EntryTrap> {
        let mut returns = Vec::new();
        for (op_index, op) in self.entry.definition.body.ops.iter().enumerate() {
            match op {
                ir::Op::Return { values } => {
                    returns = self
                        .eval_tuple(values)
                        .map_err(OpFailure::semantic)
                        .map_err(|failure| failure.at(op_index))?;
                }
                _ => self
                    .execute_op(op_index, op)
                    .map_err(|failure| failure.at(op_index))?,
            }
        }

        let (
            state_effects,
            property_effects,
            relation_effects,
            capability_effects,
            event_effects,
            next_logical_time,
        ) = self.effects.into_parts();

        Ok(EntryExecution {
            returns,
            state_effects,
            property_effects,
            relation_effects,
            capability_effects,
            event_effects,
            next_logical_time,
        })
    }

    fn execute_op(&mut self, op_index: usize, op: &ir::Op) -> Result<(), OpFailure> {
        match op {
            ir::Op::Arith { .. }
            | ir::Op::Cmp { .. }
            | ir::Op::Not { .. }
            | ir::Op::And { .. }
            | ir::Op::Or { .. }
            | ir::Op::Select { .. }
            | ir::Op::Hash { .. }
            | ir::Op::DivMod { .. } => ops::scalar::execute(self, op)?,
            ir::Op::ReadState { .. } | ir::Op::WriteState { .. } | ir::Op::DeleteState { .. } => {
                ops::state::execute(self, op_index, op)?;
            }
            ir::Op::ReadStateProperty { .. } => ops::property::execute(self, op_index, op)?,
            ir::Op::Assert { guard, cond } => {
                if semantic(self.guard_active(*guard))? {
                    let cond = semantic(self.eval_value(cond))?;
                    if !semantic(typed_bool(&cond, self.exec.type_runtimes))? {
                        return Err(OpFailure::semantic(TabulaError::AssertionFailed(
                            "assert".into(),
                        )));
                    }
                }
            }
            ir::Op::AssertRelation { .. } | ir::Op::EvalRelation { .. } => {
                ops::relation::execute(self, op_index, op)?;
            }
            ir::Op::CallCapability { .. } => ops::capability::execute(self, op_index, op)?,
            ir::Op::EmitEvent { .. } => ops::event::execute(self, op_index, op)?,
            ir::Op::Return { .. } => {}
        }
        Ok(())
    }

    pub(in crate::machine) fn eval_value(
        &self,
        value: &ir::ValueRef,
    ) -> Result<TypedValue, TabulaError> {
        match value {
            ir::ValueRef::Literal(value) => self.exec.type_runtimes.decode_portable(value),
            ir::ValueRef::Param(id) => self.entry.param_value(*id, self.params),
            ir::ValueRef::Context(id) => {
                self.context.fields.get(id).cloned().ok_or_else(|| {
                    TabulaError::InvalidIr(format!("missing context field {}", id.0))
                })
            }
            ir::ValueRef::Local(id) => self.locals.get(self.entry, *id).cloned(),
            ir::ValueRef::Const(id) => {
                let entry = self.program.const_entry(*id)?;
                self.exec.type_runtimes.decode_portable(&entry.value)
            }
        }
    }

    pub(in crate::machine) fn eval_tuple(
        &self,
        values: &ir::ValueTupleRef,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        values
            .0
            .iter()
            .map(|value| self.eval_value(value))
            .collect()
    }

    pub(in crate::machine) fn eval_tuple_portable(
        &self,
        values: &ir::ValueTupleRef,
    ) -> Result<Vec<tabula_core::PortableValue>, TabulaError> {
        self.eval_tuple(values)?
            .iter()
            .map(|value| self.exec.type_runtimes.encode_typed(value))
            .collect()
    }

    pub(in crate::machine) fn guard_active(
        &self,
        guard: Option<ir::GuardRef>,
    ) -> Result<bool, TabulaError> {
        match guard {
            Some(guard) => typed_bool(
                self.locals.get(self.entry, guard.0)?,
                self.exec.type_runtimes,
            ),
            None => Ok(true),
        }
    }

    pub(in crate::machine) fn assign_local(
        &mut self,
        id: ir::LocalId,
        value: TypedValue,
    ) -> Result<(), TabulaError> {
        self.locals.assign(self.entry, id, value)
    }

    pub(in crate::machine) fn resolve_cell_key(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        key: &ir::ValueTupleRef,
    ) -> Result<CommittedCellKey, TabulaError> {
        let values = self.eval_tuple(key)?;
        self.exec
            .state_runtime
            .encode_cell_key(table, field, &values)
    }

    pub(in crate::machine) fn inactive_default(
        &self,
        ty: TypeId,
    ) -> Result<TypedValue, TabulaError> {
        self.exec.type_runtimes.zero_of(ty)
    }
}
