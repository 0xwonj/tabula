//! Thin transaction machine over the Tabula IR instruction set.
//!
//! The interpreter is intentionally journal-first: state mutation goes through
//! the overlay, while semantic effects are recorded through
//! [`TxJournalBuilder`](crate::journal::TxJournalBuilder).

use std::cmp::Ordering;

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, StateView, StaticTableProvider};
use tabula_core::{CellKey, EmittedEvent, OpKind};
use tabula_ir::{Instruction, Slot};
use tabula_types::{
    ArithmeticOp, TypeRuntimeRegistry, TypedValue, bool_typed, bytes32_typed, typed_bool, u64_typed,
};

use crate::journal::{SuccessfulTxExecution, TxJournalBuilder};
use crate::overlay::Overlay;
use crate::precompile::PrecompileRegistry;
use crate::property::{CommittedStateProvider, PropertyQueryRegistry};
use crate::resolve::{resolve_row_expr, resolve_value_expr};
use crate::resolved_program::ResolvedExecutionProgram;

/// Error produced by the interpreter, wrapping the underlying error with
/// the instruction index at which execution failed.
#[derive(Debug, Clone, thiserror::Error)]
#[error("instruction {instruction_index}: {error}")]
pub struct InterpreterError {
    /// The underlying execution error.
    #[source]
    pub error: TabulaError,
    /// Zero-based index of the instruction that failed.
    pub instruction_index: usize,
}

/// Read-only execution context: resources needed by the interpreter
/// that don't change across instructions.
pub struct ExecContext<'a> {
    /// Cryptographic hash function.
    pub hasher: &'a dyn Hasher,
    /// Static (read-only) table lookups.
    pub static_tables: &'a dyn StaticTableProvider,
    /// Runtime type registry used for typed execution.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Canonical resolved execution contract.
    pub execution_program: &'a ResolvedExecutionProgram,
    /// Optional precompile handlers for custom instructions.
    pub precompiles: Option<&'a PrecompileRegistry>,
    /// Optional committed state for PropertyRead instructions.
    pub committed_state: Option<&'a dyn CommittedStateProvider>,
    /// Property query registry for PropertyRead resolution.
    pub property_queries: &'a PropertyQueryRegistry,
}

/// Transaction-local execution machine.
pub(crate) struct TxMachine<'instr, 'params, 'snapshot, 'ctx, 'borrow, S: StateView> {
    instructions: &'instr [Instruction],
    params: &'params [TypedValue],
    overlay: &'borrow mut Overlay<'snapshot, S>,
    ctx: &'ctx ExecContext<'ctx>,
    journal: &'borrow mut TxJournalBuilder,
    slots: Vec<TypedValue>,
}

impl<'instr, 'params, 'snapshot, 'ctx, 'borrow, S: StateView>
    TxMachine<'instr, 'params, 'snapshot, 'ctx, 'borrow, S>
{
    pub(crate) fn new(
        instructions: &'instr [Instruction],
        params: &'params [TypedValue],
        overlay: &'borrow mut Overlay<'snapshot, S>,
        ctx: &'ctx ExecContext<'ctx>,
        journal: &'borrow mut TxJournalBuilder,
    ) -> Self {
        Self {
            instructions,
            params,
            overlay,
            ctx,
            journal,
            slots: Vec::new(),
        }
    }

    pub(crate) fn execute(&mut self) -> Result<(), InterpreterError> {
        for (idx, instr) in self.instructions.iter().enumerate() {
            self.execute_instruction(idx, instr)
                .map_err(|error| InterpreterError {
                    error,
                    instruction_index: idx,
                })?;
        }
        Ok(())
    }

    fn execute_instruction(
        &mut self,
        instruction_index: usize,
        instr: &Instruction,
    ) -> Result<(), TabulaError> {
        match instr {
            Instruction::Read {
                dst_val,
                dst_is_null,
                table,
                col,
                row,
            } => {
                let row_key =
                    resolve_row_expr(row, &self.slots, self.params, self.ctx.type_runtimes)?;
                let key = CellKey {
                    table: *table,
                    col: *col,
                    row: row_key,
                };
                let col_type = self
                    .ctx
                    .execution_program
                    .column_layout(*table, *col)?
                    .type_id;
                let opt = self.overlay.read(&key, col_type)?;
                self.journal
                    .record_access(key, col_type, OpKind::Read, opt.clone());
                match opt {
                    Some(value) => {
                        set_slot(&mut self.slots, *dst_val, value)?;
                        set_slot(&mut self.slots, *dst_is_null, bool_typed(false))?;
                    }
                    None => {
                        set_slot(
                            &mut self.slots,
                            *dst_val,
                            self.ctx.type_runtimes.zero_of(col_type)?,
                        )?;
                        set_slot(&mut self.slots, *dst_is_null, bool_typed(true))?;
                    }
                }
            }

            Instruction::Write {
                table,
                col,
                row,
                src_val,
                src_is_null,
            } => {
                let row_key =
                    resolve_row_expr(row, &self.slots, self.params, self.ctx.type_runtimes)?;
                let value =
                    resolve_value_expr(src_val, &self.slots, self.params, self.ctx.type_runtimes)?;
                let is_null = resolve_value_expr(
                    src_is_null,
                    &self.slots,
                    self.params,
                    self.ctx.type_runtimes,
                )?;
                let key = CellKey {
                    table: *table,
                    col: *col,
                    row: row_key,
                };
                let col_type = self
                    .ctx
                    .execution_program
                    .column_layout(*table, *col)?
                    .type_id;
                let opt = if typed_bool(&is_null, self.ctx.type_runtimes)? {
                    None
                } else {
                    Some(value)
                };
                self.journal
                    .record_access(key, col_type, OpKind::Write, opt.clone());
                self.overlay.write(&key, opt, col_type)?;
            }

            Instruction::Lookup {
                dst,
                static_table,
                col,
                row,
            } => {
                let row_key =
                    resolve_row_expr(row, &self.slots, self.params, self.ctx.type_runtimes)?;
                let value =
                    self.ctx
                        .type_runtimes
                        .decode_portable(&self.ctx.static_tables.lookup(
                            *static_table,
                            row_key,
                            *col,
                        )?)?;
                set_slot(&mut self.slots, *dst, value)?;
            }

            Instruction::Arith { dst, op, lhs, rhs } => {
                let l = resolve_value_expr(lhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                let r = resolve_value_expr(rhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                let runtime = self.ctx.type_runtimes.resolve(l.type_id())?;
                let arithmetic = match op {
                    tabula_ir::ArithOp::Add => ArithmeticOp::Add,
                    tabula_ir::ArithOp::Sub => ArithmeticOp::Sub,
                    tabula_ir::ArithOp::Mul => ArithmeticOp::Mul,
                };
                set_slot(
                    &mut self.slots,
                    *dst,
                    runtime.apply_arithmetic(arithmetic, &l, &r)?,
                )?;
            }

            Instruction::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => {
                let l = resolve_value_expr(lhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                let r = resolve_value_expr(rhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                let runtime = self.ctx.type_runtimes.resolve(l.type_id())?;
                let (q, rem) = runtime.divmod(&l, &r)?;
                set_slot(&mut self.slots, *dst_q, q)?;
                set_slot(&mut self.slots, *dst_r, rem)?;
            }

            Instruction::Cmp { dst, op, lhs, rhs } => {
                let l = resolve_value_expr(lhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                let r = resolve_value_expr(rhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                let runtime = self.ctx.type_runtimes.resolve(l.type_id())?;
                let result = match op {
                    tabula_ir::CmpOp::Eq => runtime.eq_value(&l, &r)?,
                    tabula_ir::CmpOp::Ne => !runtime.eq_value(&l, &r)?,
                    tabula_ir::CmpOp::Lt => runtime.cmp_value(&l, &r)? == Ordering::Less,
                    tabula_ir::CmpOp::Lte => runtime.cmp_value(&l, &r)? != Ordering::Greater,
                    tabula_ir::CmpOp::Gt => runtime.cmp_value(&l, &r)? == Ordering::Greater,
                    tabula_ir::CmpOp::Gte => runtime.cmp_value(&l, &r)? != Ordering::Less,
                };
                set_slot(&mut self.slots, *dst, bool_typed(result))?;
            }

            Instruction::Not { dst, src } => {
                let value =
                    resolve_value_expr(src, &self.slots, self.params, self.ctx.type_runtimes)?;
                set_slot(
                    &mut self.slots,
                    *dst,
                    bool_typed(!typed_bool(&value, self.ctx.type_runtimes)?),
                )?;
            }

            Instruction::And { dst, lhs, rhs } => {
                let l = resolve_value_expr(lhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                let r = resolve_value_expr(rhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                set_slot(
                    &mut self.slots,
                    *dst,
                    bool_typed(
                        typed_bool(&l, self.ctx.type_runtimes)?
                            && typed_bool(&r, self.ctx.type_runtimes)?,
                    ),
                )?;
            }

            Instruction::Or { dst, lhs, rhs } => {
                let l = resolve_value_expr(lhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                let r = resolve_value_expr(rhs, &self.slots, self.params, self.ctx.type_runtimes)?;
                set_slot(
                    &mut self.slots,
                    *dst,
                    bool_typed(
                        typed_bool(&l, self.ctx.type_runtimes)?
                            || typed_bool(&r, self.ctx.type_runtimes)?,
                    ),
                )?;
            }

            Instruction::Assert { cond } => {
                let value =
                    resolve_value_expr(cond, &self.slots, self.params, self.ctx.type_runtimes)?;
                if !typed_bool(&value, self.ctx.type_runtimes)? {
                    return Err(TabulaError::AssertionFailed(format!("{cond:?}")));
                }
            }

            Instruction::Hash { dst, inputs } => {
                let typed_inputs = inputs
                    .iter()
                    .map(|expr| {
                        resolve_value_expr(expr, &self.slots, self.params, self.ctx.type_runtimes)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let portable_inputs = typed_inputs
                    .iter()
                    .map(|value| self.ctx.type_runtimes.encode_typed(value))
                    .collect::<Result<Vec<_>, _>>()?;
                let digest = self.ctx.hasher.hash_ir(&portable_inputs);
                let digest_typed = bytes32_typed(digest);
                self.journal.record_ir_hash(
                    instruction_index,
                    portable_inputs,
                    self.ctx.type_runtimes.encode_typed(&digest_typed)?,
                );
                set_slot(&mut self.slots, *dst, digest_typed)?;
            }

            Instruction::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => {
                let c = resolve_value_expr(cond, &self.slots, self.params, self.ctx.type_runtimes)?;
                let t =
                    resolve_value_expr(if_true, &self.slots, self.params, self.ctx.type_runtimes)?;
                let f =
                    resolve_value_expr(if_false, &self.slots, self.params, self.ctx.type_runtimes)?;
                let selected = if typed_bool(&c, self.ctx.type_runtimes)? {
                    t
                } else {
                    f
                };
                set_slot(&mut self.slots, *dst, selected)?;
            }

            Instruction::Emit { topic, data } => {
                let values = data
                    .iter()
                    .map(|expr| {
                        resolve_value_expr(expr, &self.slots, self.params, self.ctx.type_runtimes)
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .iter()
                    .map(|value| self.ctx.type_runtimes.encode_typed(value))
                    .collect::<Result<Vec<_>, _>>()?;
                self.journal.record_emitted(EmittedEvent {
                    topic: topic.clone(),
                    data: values,
                });
            }

            Instruction::Precompile {
                id,
                dst_slots,
                inputs,
            } => {
                let registry = self.ctx.precompiles.ok_or_else(|| {
                    TabulaError::InvalidIr(
                        "precompile instruction encountered but no PrecompileRegistry provided"
                            .into(),
                    )
                })?;
                let handler = registry.get(*id)?;
                let signature = handler.signature();
                let args = inputs
                    .iter()
                    .map(|expr| {
                        resolve_value_expr(expr, &self.slots, self.params, self.ctx.type_runtimes)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if args.len() != signature.inputs.len() {
                    return Err(TabulaError::InvalidIr(format!(
                        "precompile 0x{:04x} expects {} inputs but IR provided {}",
                        id.0,
                        signature.inputs.len(),
                        args.len(),
                    )));
                }
                for (arg_idx, (arg, expected)) in args.iter().zip(&signature.inputs).enumerate() {
                    if arg.type_id() != expected.type_id {
                        return Err(TabulaError::InvalidIr(format!(
                            "precompile 0x{:04x} input {} expects type {} but got {}",
                            id.0,
                            arg_idx,
                            expected.type_id.0,
                            arg.type_id().0,
                        )));
                    }
                }
                let results = handler.execute(&args)?;
                if results.len() != signature.outputs.len() {
                    return Err(TabulaError::InvalidIr(format!(
                        "precompile 0x{:04x} returned {} values but signature declares {} outputs",
                        id.0,
                        results.len(),
                        signature.outputs.len(),
                    )));
                }
                if results.len() != dst_slots.len() {
                    return Err(TabulaError::InvalidIr(format!(
                        "precompile 0x{:04x} signature declares {} outputs but IR has {} dst_slots",
                        id.0,
                        results.len(),
                        dst_slots.len(),
                    )));
                }
                for (output_idx, (value, expected)) in
                    results.iter().zip(&signature.outputs).enumerate()
                {
                    if value.type_id() != expected.type_id {
                        return Err(TabulaError::InvalidIr(format!(
                            "precompile 0x{:04x} output {} expects type {} but handler returned {}",
                            id.0,
                            output_idx,
                            expected.type_id.0,
                            value.type_id().0,
                        )));
                    }
                }
                self.journal
                    .record_precompile_call(instruction_index, *id, args, results.clone());
                for (dst, value) in dst_slots.iter().zip(results.into_iter()) {
                    set_slot(&mut self.slots, *dst, value)?;
                }
            }

            Instruction::PropertyRead {
                dst_val,
                dst_key,
                dst_is_null,
                table,
                col,
                query,
            } => {
                let provider = self.ctx.committed_state.ok_or_else(|| {
                    TabulaError::InvalidIr(
                        "PropertyRead encountered but no CommittedStateProvider".into(),
                    )
                })?;
                let result = self
                    .ctx
                    .property_queries
                    .resolve(*table, *col, query, provider)?;
                self.journal
                    .record_property_read(instruction_index, result.clone());
                set_slot(&mut self.slots, *dst_val, result.value)?;
                set_slot(
                    &mut self.slots,
                    *dst_key,
                    u64_typed(result.key.map_or(0, |key| key.0)),
                )?;
                set_slot(&mut self.slots, *dst_is_null, bool_typed(result.is_null))?;
            }
        }
        Ok(())
    }
}

/// Execute a transaction body against an overlay and return the successful tx shard.
///
/// This is a test-oriented thin wrapper around [`TxMachine`]. Production batch
/// execution constructs the journal builder directly in `batch.rs`.
pub(crate) fn execute_with_journal<'instr, 'params, 'snapshot, 'ctx, S: StateView>(
    tx_index: u32,
    instructions: &'instr [Instruction],
    params: &'params [TypedValue],
    overlay: &mut Overlay<'snapshot, S>,
    ctx: &'ctx ExecContext<'ctx>,
    journal: &mut TxJournalBuilder,
) -> Result<(), InterpreterError> {
    let _ = tx_index;
    TxMachine::new(instructions, params, overlay, ctx, journal).execute()
}

/// Execute a transaction body against an overlay.
///
/// This public helper is intended for tests and harnesses that need the
/// interpreter semantics without constructing a full batch executor.
pub fn execute<'instr, 'params, 'snapshot, 'ctx, S: StateView>(
    tx_index: u32,
    instructions: &'instr [Instruction],
    params: &'params [TypedValue],
    overlay: &mut Overlay<'snapshot, S>,
    ctx: &'ctx ExecContext<'ctx>,
) -> Result<(), InterpreterError> {
    let mut journal = TxJournalBuilder::new(tx_index, 0);
    execute_with_journal(tx_index, instructions, params, overlay, ctx, &mut journal)
}

/// Convenience wrapper for tests that need the successful shard directly.
pub fn execute_tx<'instr, 'params, 'snapshot, 'ctx, S: StateView>(
    tx_index: u32,
    instructions: &'instr [Instruction],
    params: &'params [TypedValue],
    overlay: &mut Overlay<'snapshot, S>,
    ctx: &'ctx ExecContext<'ctx>,
) -> Result<SuccessfulTxExecution, InterpreterError> {
    let mut journal = TxJournalBuilder::new(tx_index, 0);
    execute_with_journal(tx_index, instructions, params, overlay, ctx, &mut journal)?;
    Ok(journal.into_success())
}

fn set_slot(slots: &mut Vec<TypedValue>, idx: Slot, value: TypedValue) -> Result<(), TabulaError> {
    let i = idx as usize;
    if i < slots.len() {
        slots[i] = value;
    } else if i == slots.len() {
        slots.push(value);
    } else {
        return Err(TabulaError::InvalidIr(format!(
            "slot gap: index {i}, len {}",
            slots.len()
        )));
    }
    Ok(())
}
