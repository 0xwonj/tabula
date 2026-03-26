use tabula_core::traits::StateView;
use tabula_ir as ir;
use tabula_types::{bool_typed, bytes32_typed, typed_bool};

use crate::machine::entry::{EntryMachineCore, OpFailure, fatal, semantic};

pub(in crate::machine) fn execute<S: StateView>(
    machine: &mut EntryMachineCore<'_, '_, '_, S>,
    op: &ir::Op,
) -> Result<(), OpFailure> {
    match op {
        ir::Op::Arith { dst, op, lhs, rhs } => {
            let lhs = semantic(machine.eval_value(lhs))?;
            let rhs = semantic(machine.eval_value(rhs))?;
            let runtime = semantic(machine.exec.type_runtimes.resolve(lhs.type_id()))?;
            let value = semantic(runtime.apply_arithmetic(map_arith(*op), &lhs, &rhs))?;
            fatal(machine.assign_local(*dst, value))?;
        }
        ir::Op::Cmp { dst, op, lhs, rhs } => {
            let lhs = semantic(machine.eval_value(lhs))?;
            let rhs = semantic(machine.eval_value(rhs))?;
            let runtime = semantic(machine.exec.type_runtimes.resolve(lhs.type_id()))?;
            let result = match op {
                ir::CmpOp::Eq => semantic(runtime.eq_value(&lhs, &rhs))?,
                ir::CmpOp::Ne => !semantic(runtime.eq_value(&lhs, &rhs))?,
                ir::CmpOp::Lt => {
                    semantic(runtime.cmp_value(&lhs, &rhs))? == std::cmp::Ordering::Less
                }
                ir::CmpOp::Lte => {
                    semantic(runtime.cmp_value(&lhs, &rhs))? != std::cmp::Ordering::Greater
                }
                ir::CmpOp::Gt => {
                    semantic(runtime.cmp_value(&lhs, &rhs))? == std::cmp::Ordering::Greater
                }
                ir::CmpOp::Gte => {
                    semantic(runtime.cmp_value(&lhs, &rhs))? != std::cmp::Ordering::Less
                }
            };
            fatal(machine.assign_local(*dst, bool_typed(result)))?;
        }
        ir::Op::Not { dst, src } => {
            let src = semantic(machine.eval_value(src))?;
            fatal(machine.assign_local(
                *dst,
                bool_typed(!semantic(typed_bool(&src, machine.exec.type_runtimes))?),
            ))?;
        }
        ir::Op::And { dst, lhs, rhs } => {
            let lhs = semantic(machine.eval_value(lhs))?;
            let rhs = semantic(machine.eval_value(rhs))?;
            fatal(machine.assign_local(
                *dst,
                bool_typed(
                    semantic(typed_bool(&lhs, machine.exec.type_runtimes))?
                        && semantic(typed_bool(&rhs, machine.exec.type_runtimes))?,
                ),
            ))?;
        }
        ir::Op::Or { dst, lhs, rhs } => {
            let lhs = semantic(machine.eval_value(lhs))?;
            let rhs = semantic(machine.eval_value(rhs))?;
            fatal(machine.assign_local(
                *dst,
                bool_typed(
                    semantic(typed_bool(&lhs, machine.exec.type_runtimes))?
                        || semantic(typed_bool(&rhs, machine.exec.type_runtimes))?,
                ),
            ))?;
        }
        ir::Op::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            let cond = semantic(machine.eval_value(cond))?;
            let selected = if semantic(typed_bool(&cond, machine.exec.type_runtimes))? {
                semantic(machine.eval_value(if_true))?
            } else {
                semantic(machine.eval_value(if_false))?
            };
            fatal(machine.assign_local(*dst, selected))?;
        }
        ir::Op::Hash {
            dst,
            family: ir::HashFamily::Poseidon,
            inputs,
        } => {
            let portable_inputs = semantic(machine.eval_tuple_portable(inputs))?;
            fatal(machine.assign_local(
                *dst,
                bytes32_typed(machine.exec.hasher.hash_ir(&portable_inputs)),
            ))?;
        }
        ir::Op::DivMod {
            guard,
            dst_q,
            dst_r,
            lhs,
            rhs,
        } => {
            let lhs = semantic(machine.eval_value(lhs))?;
            if !semantic(machine.guard_active(*guard))? {
                let zero = semantic(machine.inactive_default(lhs.type_id()))?;
                fatal(machine.assign_local(*dst_q, zero.clone()))?;
                fatal(machine.assign_local(*dst_r, zero))?;
            } else {
                let rhs = semantic(machine.eval_value(rhs))?;
                let runtime = semantic(machine.exec.type_runtimes.resolve(lhs.type_id()))?;
                let (q, r) = semantic(runtime.divmod(&lhs, &rhs))?;
                fatal(machine.assign_local(*dst_q, q))?;
                fatal(machine.assign_local(*dst_r, r))?;
            }
        }
        _ => unreachable!("scalar::execute only handles scalar ops"),
    }
    Ok(())
}

fn map_arith(op: ir::ArithOp) -> tabula_types::ArithmeticOp {
    match op {
        ir::ArithOp::Add => tabula_types::ArithmeticOp::Add,
        ir::ArithOp::Sub => tabula_types::ArithmeticOp::Sub,
        ir::ArithOp::Mul => tabula_types::ArithmeticOp::Mul,
    }
}
