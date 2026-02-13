//! Program: holds tx type definitions with type info, resolves `TxTypeId`.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::ir::{Instruction, Predicate, RowExpr, Slot, ValueExpr};
use tabula_core::schema::TableSchema;
use tabula_core::tx::{TxTypeDef, TxTypeId};
use tabula_core::types::{ColId, TableId, Value, ValueType};

/// Inferred type information for a transaction body.
///
/// Computed at registration time from IR + `param_schema`.
/// `slot_types[i]` is `None` when the type cannot be determined statically
/// (e.g. `Read` result without table schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyTypeInfo {
    /// Inferred type per slot. `None` = unknown (e.g. Read result).
    pub slot_types: Vec<Option<ValueType>>,
    /// Parameter types (copied from `param_schema` for convenience).
    pub param_types: Vec<ValueType>,
    /// Highest slot index assigned in the body.
    pub max_slot: Option<Slot>,
}

/// Holds registered transaction type definitions.
#[derive(Debug, Clone)]
pub struct Program {
    types: BTreeMap<TxTypeId, TxTypeDef>,
    type_info: BTreeMap<TxTypeId, BodyTypeInfo>,
    schemas: BTreeMap<TableId, TableSchema>,
}

impl Program {
    /// Create an empty program.
    pub fn new() -> Self {
        Self {
            types: BTreeMap::new(),
            type_info: BTreeMap::new(),
            schemas: BTreeMap::new(),
        }
    }

    /// Register a table schema. Must be called before `register()` so
    /// that type inference can use column type information.
    pub fn add_schema(&mut self, schema: TableSchema) {
        self.schemas.insert(schema.id, schema);
    }

    /// Register a transaction type definition.
    ///
    /// Performs IR validation and type inference. Returns an error if the
    /// body contains out-of-bounds param/slot references or type mismatches.
    pub fn register(&mut self, def: TxTypeDef) -> Result<(), TabulaError> {
        let info = compile_body(&def, &self.schemas)?;
        self.type_info.insert(def.id, info);
        self.types.insert(def.id, def);
        Ok(())
    }

    /// Resolve a `TxTypeId` to its definition.
    pub fn resolve(&self, id: TxTypeId) -> Result<&TxTypeDef, TabulaError> {
        self.types.get(&id).ok_or(TabulaError::TxTypeNotFound(id))
    }

    /// Get the inferred type info for a registered tx type.
    pub fn type_info(&self, id: TxTypeId) -> Option<&BodyTypeInfo> {
        self.type_info.get(&id)
    }

    /// Return all registered type definitions.
    pub fn all_types(&self) -> Vec<&TxTypeDef> {
        self.types.values().collect()
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// IR type inference
// ---------------------------------------------------------------------------

/// Compile a tx body: validate references and infer slot types.
fn compile_body(
    def: &TxTypeDef,
    schemas: &BTreeMap<TableId, TableSchema>,
) -> Result<BodyTypeInfo, TabulaError> {
    let param_count = def.param_schema.len() as u16;
    let param_types: Vec<ValueType> = def.param_schema.iter().map(|p| p.value_type).collect();

    // Track which slots have been assigned and their inferred types.
    let mut slot_types: Vec<Option<ValueType>> = Vec::new();
    let mut max_slot: Option<Slot> = None;

    let assign_slot = |slot: Slot,
                       ty: Option<ValueType>,
                       slot_types: &mut Vec<Option<ValueType>>,
                       max_slot: &mut Option<Slot>| {
        let idx = slot as usize;
        if idx >= slot_types.len() {
            slot_types.resize(idx + 1, None);
        }
        slot_types[idx] = ty;
        *max_slot = Some(max_slot.map_or(slot, |m: Slot| m.max(slot)));
    };

    for (i, instr) in def.body.iter().enumerate() {
        match instr {
            Instruction::Read {
                dst, table, col, row,
            } => {
                check_row_expr(row, param_count, &slot_types, i)?;
                let ty = schema_col_type(schemas, table, col);
                assign_slot(*dst, ty, &mut slot_types, &mut max_slot);
            }

            Instruction::Write {
                table, col, row, src,
            } => {
                check_row_expr(row, param_count, &slot_types, i)?;
                check_value_expr(src, param_count, &slot_types, i)?;
                // Type-check: if schema provides expected type and src type is known, they must match.
                if let Some(expected) = schema_col_type(schemas, table, col)
                    && let Some(actual) = expr_type(src, &param_types, &slot_types)
                    && actual != expected
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: write to table {} col {} expects {expected:?} but got {actual:?}",
                        table.0, col.0
                    )));
                }
            }

            Instruction::Lookup {
                dst,
                static_table,
                col,
                key,
            } => {
                check_row_expr(key, param_count, &slot_types, i)?;
                let ty = schema_col_type(schemas, static_table, col);
                assign_slot(*dst, ty, &mut slot_types, &mut max_slot);
            }

            Instruction::Add { dst, lhs, rhs } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                let ty = infer_numeric_result(lhs, rhs, &param_types, &slot_types, i)?;
                assign_slot(*dst, ty, &mut slot_types, &mut max_slot);
            }

            Instruction::Sub { dst, lhs, rhs } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                let ty = infer_numeric_result(lhs, rhs, &param_types, &slot_types, i)?;
                assign_slot(*dst, ty, &mut slot_types, &mut max_slot);
            }

            Instruction::Mul { dst, lhs, rhs } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                let ty = infer_numeric_result(lhs, rhs, &param_types, &slot_types, i)?;
                assign_slot(*dst, ty, &mut slot_types, &mut max_slot);
            }

            Instruction::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                let ty = infer_numeric_result(lhs, rhs, &param_types, &slot_types, i)?;
                assign_slot(*dst_q, ty, &mut slot_types, &mut max_slot);
                assign_slot(*dst_r, ty, &mut slot_types, &mut max_slot);
            }

            Instruction::Assert { predicate } => {
                check_predicate(predicate, param_count, &slot_types, i)?;
            }

            Instruction::Hash { dst, inputs } => {
                for input in inputs {
                    check_value_expr(input, param_count, &slot_types, i)?;
                }
                assign_slot(
                    *dst,
                    Some(ValueType::Bytes32),
                    &mut slot_types,
                    &mut max_slot,
                );
            }

            Instruction::Emit { data, .. } => {
                for d in data {
                    check_value_expr(d, param_count, &slot_types, i)?;
                }
            }
        }
    }

    Ok(BodyTypeInfo {
        slot_types,
        param_types,
        max_slot,
    })
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn check_row_expr(
    expr: &RowExpr,
    param_count: u16,
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    match expr {
        RowExpr::Literal(_) => Ok(()),
        RowExpr::Param(idx) => check_param_idx(*idx, param_count, instr_idx),
        RowExpr::Slot(idx) => check_slot_defined(*idx, slot_types, instr_idx),
    }
}

fn check_value_expr(
    expr: &ValueExpr,
    param_count: u16,
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    match expr {
        ValueExpr::Literal(_) => Ok(()),
        ValueExpr::Param(idx) => check_param_idx(*idx, param_count, instr_idx),
        ValueExpr::Slot(idx) => check_slot_defined(*idx, slot_types, instr_idx),
    }
}

fn check_predicate(
    pred: &Predicate,
    param_count: u16,
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    match pred {
        Predicate::Eq(l, r)
        | Predicate::Lt(l, r)
        | Predicate::Lte(l, r)
        | Predicate::Gt(l, r)
        | Predicate::Gte(l, r) => {
            check_value_expr(l, param_count, slot_types, instr_idx)?;
            check_value_expr(r, param_count, slot_types, instr_idx)?;
            Ok(())
        }
        Predicate::NotNull(v) => check_value_expr(v, param_count, slot_types, instr_idx),
        Predicate::And(a, b) | Predicate::Or(a, b) => {
            check_predicate(a, param_count, slot_types, instr_idx)?;
            check_predicate(b, param_count, slot_types, instr_idx)?;
            Ok(())
        }
        Predicate::Not(p) => check_predicate(p, param_count, slot_types, instr_idx),
    }
}

fn check_param_idx(idx: u16, param_count: u16, instr_idx: usize) -> Result<(), TabulaError> {
    if idx >= param_count {
        return Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: param index {idx} out of bounds (schema has {param_count} params)"
        )));
    }
    Ok(())
}

fn check_slot_defined(
    idx: Slot,
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    let i = idx as usize;
    if i >= slot_types.len() {
        return Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: slot {idx} read before assignment"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Type inference helpers
// ---------------------------------------------------------------------------

/// Look up the column type from table schemas.
fn schema_col_type(
    schemas: &BTreeMap<TableId, TableSchema>,
    table: &TableId,
    col: &ColId,
) -> Option<ValueType> {
    schemas
        .get(table)
        .and_then(|s| s.columns.iter().find(|c| c.id == *col))
        .map(|c| c.value_type)
}

/// Infer the result type of a binary numeric operation.
///
/// Returns `Some(ty)` when at least one operand has a known type,
/// `None` when both are unknown. Errors if both are known but differ.
fn infer_numeric_result(
    lhs: &ValueExpr,
    rhs: &ValueExpr,
    param_types: &[ValueType],
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<Option<ValueType>, TabulaError> {
    let lt = expr_type(lhs, param_types, slot_types);
    let rt = expr_type(rhs, param_types, slot_types);
    match (lt, rt) {
        (Some(l), Some(r)) if l != r => Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: operand type mismatch: {l:?} vs {r:?}"
        ))),
        (Some(l), _) => Ok(Some(l)),
        (_, Some(r)) => Ok(Some(r)),
        (None, None) => Ok(None),
    }
}

/// Determine the static type of a `ValueExpr`, if known.
fn expr_type(
    expr: &ValueExpr,
    param_types: &[ValueType],
    slot_types: &[Option<ValueType>],
) -> Option<ValueType> {
    match expr {
        ValueExpr::Literal(v) => value_to_type(v),
        ValueExpr::Param(idx) => param_types.get(*idx as usize).copied(),
        ValueExpr::Slot(idx) => slot_types.get(*idx as usize).copied().flatten(),
    }
}

fn value_to_type(v: &Value) -> Option<ValueType> {
    match v {
        Value::U64(_) => Some(ValueType::U64),
        Value::I64(_) => Some(ValueType::I64),
        Value::Bool(_) => Some(ValueType::Bool),
        Value::Bytes32(_) => Some(ValueType::Bytes32),
        Value::Null => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_core::ir::{Instruction, Predicate, RowExpr, ValueExpr};
    use tabula_core::schema::{ColumnDef, TableSchema};
    use tabula_core::tx::{ParamDef, TxTypeDef};

    fn transfer_def() -> TxTypeDef {
        TxTypeDef {
            id: TxTypeId(1),
            name: "transfer".into(),
            param_schema: vec![
                ParamDef {
                    name: "from".into(),
                    value_type: ValueType::U64,
                },
                ParamDef {
                    name: "to".into(),
                    value_type: ValueType::U64,
                },
                ParamDef {
                    name: "amount".into(),
                    value_type: ValueType::U64,
                },
            ],
            body: vec![
                Instruction::Read {
                    dst: 0,
                    table: TableId(1),
                    row: RowExpr::Param(0),
                    col: ColId(0),
                },
                Instruction::Read {
                    dst: 1,
                    table: TableId(1),
                    row: RowExpr::Param(1),
                    col: ColId(0),
                },
                Instruction::Assert {
                    predicate: Predicate::Gte(ValueExpr::Slot(0), ValueExpr::Param(2)),
                },
                Instruction::Sub {
                    dst: 2,
                    lhs: ValueExpr::Slot(0),
                    rhs: ValueExpr::Param(2),
                },
                Instruction::Add {
                    dst: 3,
                    lhs: ValueExpr::Slot(1),
                    rhs: ValueExpr::Param(2),
                },
                Instruction::Write {
                    table: TableId(1),
                    row: RowExpr::Param(0),
                    col: ColId(0),
                    src: ValueExpr::Slot(2),
                },
                Instruction::Write {
                    table: TableId(1),
                    row: RowExpr::Param(1),
                    col: ColId(0),
                    src: ValueExpr::Slot(3),
                },
            ],
        }
    }

    #[test]
    fn test_register_valid_program() {
        let mut prog = Program::new();
        prog.register(transfer_def()).unwrap();
        assert!(prog.resolve(TxTypeId(1)).is_ok());
    }

    #[test]
    fn test_type_info_inferred() {
        let mut prog = Program::new();
        prog.register(transfer_def()).unwrap();
        let info = prog.type_info(TxTypeId(1)).unwrap();

        // Slots 0, 1 = Read results → None (unknown without table schema)
        assert_eq!(info.slot_types[0], None);
        assert_eq!(info.slot_types[1], None);
        // Slot 2 = Sub(Slot(0), Param(2)) → Param(2) is U64 → U64
        assert_eq!(info.slot_types[2], Some(ValueType::U64));
        // Slot 3 = Add(Slot(1), Param(2)) → Param(2) is U64 → U64
        assert_eq!(info.slot_types[3], Some(ValueType::U64));
        assert_eq!(info.max_slot, Some(3));
        assert_eq!(
            info.param_types,
            vec![ValueType::U64, ValueType::U64, ValueType::U64]
        );
    }

    #[test]
    fn test_hash_produces_bytes32_type() {
        let def = TxTypeDef {
            id: TxTypeId(2),
            name: "hash_test".into(),
            param_schema: vec![ParamDef {
                name: "input".into(),
                value_type: ValueType::U64,
            }],
            body: vec![Instruction::Hash {
                dst: 0,
                inputs: vec![ValueExpr::Param(0)],
            }],
        };
        let mut prog = Program::new();
        prog.register(def).unwrap();
        let info = prog.type_info(TxTypeId(2)).unwrap();
        assert_eq!(info.slot_types[0], Some(ValueType::Bytes32));
    }

    #[test]
    fn test_param_out_of_bounds_rejected() {
        let def = TxTypeDef {
            id: TxTypeId(3),
            name: "bad_param".into(),
            param_schema: vec![], // no params
            body: vec![Instruction::Write {
                table: TableId(1),
                row: RowExpr::Param(0), // param 0 doesn't exist
                col: ColId(0),
                src: ValueExpr::Literal(Value::U64(1)),
            }],
        };
        let mut prog = Program::new();
        let err = prog.register(def).unwrap_err();
        assert!(matches!(err, TabulaError::InvalidIr(_)));
    }

    #[test]
    fn test_slot_read_before_assign_rejected() {
        let def = TxTypeDef {
            id: TxTypeId(4),
            name: "bad_slot".into(),
            param_schema: vec![],
            body: vec![Instruction::Add {
                dst: 1,
                lhs: ValueExpr::Slot(0), // slot 0 never assigned
                rhs: ValueExpr::Literal(Value::U64(1)),
            }],
        };
        let mut prog = Program::new();
        let err = prog.register(def).unwrap_err();
        assert!(matches!(err, TabulaError::InvalidIr(_)));
    }

    #[test]
    fn test_empty_body_valid() {
        let def = TxTypeDef {
            id: TxTypeId(5),
            name: "noop".into(),
            param_schema: vec![],
            body: vec![],
        };
        let mut prog = Program::new();
        prog.register(def).unwrap();
        let info = prog.type_info(TxTypeId(5)).unwrap();
        assert!(info.slot_types.is_empty());
        assert_eq!(info.max_slot, None);
    }

    #[test]
    fn test_resolve_missing_type() {
        let prog = Program::new();
        let err = prog.resolve(TxTypeId(99)).unwrap_err();
        assert_eq!(err, TabulaError::TxTypeNotFound(TxTypeId(99)));
    }

    #[test]
    fn test_literal_type_inference() {
        let def = TxTypeDef {
            id: TxTypeId(6),
            name: "literal_add".into(),
            param_schema: vec![],
            body: vec![Instruction::Add {
                dst: 0,
                lhs: ValueExpr::Literal(Value::I64(10)),
                rhs: ValueExpr::Literal(Value::I64(20)),
            }],
        };
        let mut prog = Program::new();
        prog.register(def).unwrap();
        let info = prog.type_info(TxTypeId(6)).unwrap();
        assert_eq!(info.slot_types[0], Some(ValueType::I64));
    }

    #[test]
    fn test_operand_type_mismatch_rejected() {
        let def = TxTypeDef {
            id: TxTypeId(7),
            name: "bad_add".into(),
            param_schema: vec![
                ParamDef {
                    name: "a".into(),
                    value_type: ValueType::I64,
                },
                ParamDef {
                    name: "b".into(),
                    value_type: ValueType::U64,
                },
            ],
            body: vec![Instruction::Add {
                dst: 0,
                lhs: ValueExpr::Param(0), // I64
                rhs: ValueExpr::Param(1), // U64
            }],
        };
        let mut prog = Program::new();
        let err = prog.register(def).unwrap_err();
        assert!(matches!(err, TabulaError::InvalidIr(_)));
    }

    fn balances_schema() -> TableSchema {
        TableSchema {
            id: TableId(1),
            name: "balances".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "balance".into(),
                value_type: ValueType::U64,
            }],
        }
    }

    #[test]
    fn test_schema_infers_read_type() {
        let mut prog = Program::new();
        prog.add_schema(balances_schema());
        prog.register(transfer_def()).unwrap();
        let info = prog.type_info(TxTypeId(1)).unwrap();
        // With schema, Read slots now have inferred types.
        assert_eq!(info.slot_types[0], Some(ValueType::U64));
        assert_eq!(info.slot_types[1], Some(ValueType::U64));
    }

    #[test]
    fn test_schema_write_type_mismatch() {
        let def = TxTypeDef {
            id: TxTypeId(10),
            name: "bad_write".into(),
            param_schema: vec![],
            body: vec![Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(tabula_core::types::RowKey(0)),
                col: ColId(0),
                src: ValueExpr::Literal(Value::Bool(true)), // schema expects U64
            }],
        };
        let mut prog = Program::new();
        prog.add_schema(balances_schema());
        let err = prog.register(def).unwrap_err();
        assert!(matches!(err, TabulaError::InvalidIr(_)));
    }

    #[test]
    fn test_schema_write_unknown_src_accepted() {
        // Read result has known type (U64) from schema, write to same column should pass.
        let def = TxTypeDef {
            id: TxTypeId(11),
            name: "passthrough".into(),
            param_schema: vec![],
            body: vec![
                Instruction::Read {
                    dst: 0,
                    table: TableId(1),
                    row: RowExpr::Literal(tabula_core::types::RowKey(0)),
                    col: ColId(0),
                },
                Instruction::Write {
                    table: TableId(1),
                    row: RowExpr::Literal(tabula_core::types::RowKey(0)),
                    col: ColId(0),
                    src: ValueExpr::Slot(0),
                },
            ],
        };
        let mut prog = Program::new();
        prog.add_schema(balances_schema());
        prog.register(def).unwrap(); // should succeed — both are U64
    }

    #[test]
    fn test_no_schema_backward_compatible() {
        // Without schema, Read slots should still be None (backward compat).
        let mut prog = Program::new();
        prog.register(transfer_def()).unwrap();
        let info = prog.type_info(TxTypeId(1)).unwrap();
        assert_eq!(info.slot_types[0], None);
        assert_eq!(info.slot_types[1], None);
    }

    #[test]
    fn test_lookup_type_from_schema() {
        let schema = TableSchema {
            id: TableId(99),
            name: "config".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "flag".into(),
                value_type: ValueType::Bool,
            }],
        };
        let def = TxTypeDef {
            id: TxTypeId(12),
            name: "lookup_test".into(),
            param_schema: vec![ParamDef {
                name: "key".into(),
                value_type: ValueType::U64,
            }],
            body: vec![Instruction::Lookup {
                dst: 0,
                static_table: TableId(99),
                key: RowExpr::Param(0),
                col: ColId(0),
            }],
        };
        let mut prog = Program::new();
        prog.add_schema(schema);
        prog.register(def).unwrap();
        let info = prog.type_info(TxTypeId(12)).unwrap();
        assert_eq!(info.slot_types[0], Some(ValueType::Bool));
    }
}
