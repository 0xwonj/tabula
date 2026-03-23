//! Lowering pass: AST → Tabula IR.
//!
//! Performs name resolution, type checking, slot allocation, and IR emission
//! in a single forward pass.

mod expr;
mod resolve;
mod stmt;
use std::collections::{BTreeMap, HashMap};

use tabula_core::{ColId, PortableValue, SchemeId, TableId, TxTypeId, TypeId};
use tabula_ir::{
    Instruction, ParamDef, PrecompileId, PrecompileSignature, Slot, TxTypeDef, ValueExpr,
};
use tabula_profile::{
    HostValueFamily, NullSemantics, SemanticRegistry, TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_U64_ID,
    ZeroValueSpec, builtin_semantic_registry,
};

use crate::ast::{self, ColumnSchemeDecl, TypeName};
use crate::error::{CompileError, ErrorKind};
use crate::span::Span;

/// Lowering output: table schemas + tx type definitions.
#[derive(Debug, Clone)]
pub struct LoweredProgram {
    /// Source-side table schemas (ordered by declaration order).
    pub schemas: Vec<SourceTableSchema>,
    /// Transaction type definitions (ordered by declaration order).
    pub tx_types: Vec<TxTypeDef>,
    /// Non-default column commitment scheme selections from source.
    pub column_schemes: Vec<ColumnSchemeSelection>,
}

/// Lower an AST program to IR.
pub fn lower(program: &ast::Program) -> Result<LoweredProgram, Vec<CompileError>> {
    let registry = builtin_semantic_registry().expect("built-in semantic registry must stay valid");
    lower_with_registry_and_precompiles(program, &registry, &BTreeMap::new())
}

/// Lower an AST program to IR using one explicit semantic registry.
pub fn lower_with_registry(
    program: &ast::Program,
    registry: &SemanticRegistry,
) -> Result<LoweredProgram, Vec<CompileError>> {
    lower_with_registry_and_precompiles(program, registry, &BTreeMap::new())
}

/// Lower an AST program to IR using one explicit semantic registry and precompile signatures.
pub fn lower_with_registry_and_precompiles(
    program: &ast::Program,
    registry: &SemanticRegistry,
    precompiles: &BTreeMap<PrecompileId, PrecompileSignature>,
) -> Result<LoweredProgram, Vec<CompileError>> {
    let mut ctx = LowerCtx::new(registry, precompiles);
    ctx.collect_schemas(program);
    if !ctx.errors.is_empty() {
        return Err(ctx.errors);
    }
    ctx.lower_transactions(program);
    if ctx.errors.is_empty() {
        Ok(LoweredProgram {
            schemas: ctx.schemas,
            tx_types: ctx.tx_types,
            column_schemes: ctx.column_schemes,
        })
    } else {
        Err(ctx.errors)
    }
}

/// Source-side column definition before compiler sealing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceColumnDef {
    /// Column identifier.
    pub id: ColId,
    /// Human-readable name.
    pub name: String,
    /// Source-resolved semantic type selection.
    pub type_id: TypeId,
}

/// Source-side table schema before compiler sealing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTableSchema {
    /// Table identifier.
    pub id: TableId,
    /// Human-readable name.
    pub name: String,
    /// Ordered column definitions.
    pub columns: Vec<SourceColumnDef>,
}

// ---------------------------------------------------------------------------
// Table & column info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(super) struct TableInfo {
    pub(super) id: TableId,
    pub(super) columns: HashMap<String, ColumnInfo>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ColumnInfo {
    pub(super) id: ColId,
    pub(super) type_id: TypeId,
}

// ---------------------------------------------------------------------------
// Lowering context
// ---------------------------------------------------------------------------

struct LowerCtx {
    registry: SemanticRegistry,
    precompiles: BTreeMap<PrecompileId, PrecompileSignature>,
    /// table name → info
    tables: HashMap<String, TableInfo>,
    /// Compiled schemas (output).
    schemas: Vec<SourceTableSchema>,
    /// Compiled tx type defs (output).
    tx_types: Vec<TxTypeDef>,
    /// Non-default column scheme selections (output).
    column_schemes: Vec<ColumnSchemeSelection>,
    errors: Vec<CompileError>,
}

impl LowerCtx {
    fn new(
        registry: &SemanticRegistry,
        precompiles: &BTreeMap<PrecompileId, PrecompileSignature>,
    ) -> Self {
        Self {
            registry: registry.clone(),
            precompiles: precompiles.clone(),
            tables: HashMap::new(),
            schemas: Vec::new(),
            tx_types: Vec::new(),
            column_schemes: Vec::new(),
            errors: Vec::new(),
        }
    }

    // --- Phase 1: collect table schemas ---

    fn collect_schemas(&mut self, program: &ast::Program) {
        for (i, table) in program.tables.iter().enumerate() {
            if self.tables.contains_key(&table.name) {
                self.errors.push(CompileError::new(
                    ErrorKind::DuplicateTable,
                    table.span,
                    format!("duplicate table declaration '{}'", table.name),
                ));
                continue;
            }

            let table_id = TableId(i as u32);
            let mut columns = HashMap::new();
            let mut col_defs = Vec::new();
            let mut seen_cols: HashMap<String, Span> = HashMap::new();

            for (j, col) in table.columns.iter().enumerate() {
                if let Some(prev_span) = seen_cols.get(&col.name) {
                    self.errors.push(CompileError::new(
                        ErrorKind::DuplicateColumn,
                        col.span,
                        format!(
                            "duplicate column '{}' in table '{}' (first at {:?})",
                            col.name, table.name, prev_span
                        ),
                    ));
                    continue;
                }
                seen_cols.insert(col.name.clone(), col.span);

                let col_id = ColId(j as u16);
                let Some(type_id) = self.resolve_type_name(col.ty, col.span) else {
                    continue;
                };
                columns.insert(
                    col.name.clone(),
                    ColumnInfo {
                        id: col_id,
                        type_id,
                    },
                );
                col_defs.push(SourceColumnDef {
                    id: col_id,
                    name: col.name.clone(),
                    type_id,
                });
                if let Some(scheme) = col.scheme {
                    let Some(scheme_id) = self.resolve_scheme_decl(scheme, col.span) else {
                        continue;
                    };
                    if scheme_id != SchemeId::SSMC {
                        self.column_schemes.push(ColumnSchemeSelection {
                            table_id,
                            col_id,
                            scheme_id,
                        });
                    }
                }
            }

            self.tables.insert(
                table.name.clone(),
                TableInfo {
                    id: table_id,
                    columns,
                },
            );
            self.schemas.push(SourceTableSchema {
                id: table_id,
                name: table.name.clone(),
                columns: col_defs,
            });
        }
    }

    // --- Phase 2: lower transactions ---

    fn lower_transactions(&mut self, program: &ast::Program) {
        let mut seen_tx: HashMap<String, Span> = HashMap::new();
        for (i, tx) in program.transactions.iter().enumerate() {
            if let Some(prev_span) = seen_tx.get(&tx.name) {
                self.errors.push(CompileError::new(
                    ErrorKind::DuplicateTx,
                    tx.span,
                    format!(
                        "duplicate tx declaration '{}' (first at {:?})",
                        tx.name, prev_span
                    ),
                ));
                continue;
            }
            seen_tx.insert(tx.name.clone(), tx.span);

            let tx_id = TxTypeId(i as u32);
            match TxLower::new(&self.tables, &self.registry, &self.precompiles, tx).lower(tx_id) {
                Ok(def) => self.tx_types.push(def),
                Err(errs) => self.errors.extend(errs),
            }
        }
    }

    fn resolve_type_name(&mut self, ty: TypeName, span: Span) -> Option<TypeId> {
        let name = match ty {
            TypeName::U64 => "u64",
            TypeName::I64 => "i64",
            TypeName::Bool => "bool",
            TypeName::Bytes32 => "bytes32",
        };
        match self.registry.resolve_type_name(name) {
            Ok(type_id) => Some(type_id),
            Err(err) => {
                self.errors.push(CompileError::new(
                    ErrorKind::TypeMismatch,
                    span,
                    err.to_string(),
                ));
                None
            }
        }
    }

    fn resolve_scheme_decl(&mut self, scheme: ColumnSchemeDecl, span: Span) -> Option<SchemeId> {
        let name = match scheme {
            ColumnSchemeDecl::Ssmc => Some("ssmc"),
            ColumnSchemeDecl::Smt => Some("smt"),
            ColumnSchemeDecl::Numeric(id) => return Some(SchemeId(id)),
        };
        match self
            .registry
            .resolve_scheme_name(name.expect("named built-in scheme"))
        {
            Ok(scheme_id) => Some(scheme_id),
            Err(err) => {
                self.errors.push(CompileError::new(
                    ErrorKind::TypeMismatch,
                    span,
                    err.to_string(),
                ));
                None
            }
        }
    }
}

/// Source-selected non-default commitment scheme for one column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSchemeSelection {
    /// Table identifier.
    pub table_id: TableId,
    /// Column identifier.
    pub col_id: ColId,
    /// Portable scheme identifier.
    pub scheme_id: SchemeId,
}

// ---------------------------------------------------------------------------
// Per-transaction lowering
// ---------------------------------------------------------------------------

/// A local binding: either a real slot, an alias, or a 2-slot read result.
#[derive(Debug, Clone)]
pub(super) enum Binding {
    /// Occupies a physical IR slot.
    Slot(Slot, TypeId),
    /// Alias — resolved to a ValueExpr without emitting instructions.
    Alias(ValueExpr, TypeId),
    /// 2-slot cell read result: (val_slot, is_null_slot, column type).
    ReadSlot {
        val: Slot,
        is_null: Slot,
        ty: TypeId,
    },
}

impl Binding {
    pub(super) fn ty(&self) -> TypeId {
        match self {
            Binding::Slot(_, ty) | Binding::Alias(_, ty) | Binding::ReadSlot { ty, .. } => *ty,
        }
    }

    pub(super) fn to_value_expr(&self) -> ValueExpr {
        match self {
            Binding::Slot(s, _) => ValueExpr::Slot(*s),
            Binding::Alias(ve, _) => ve.clone(),
            Binding::ReadSlot { val, .. } => ValueExpr::Slot(*val),
        }
    }

    pub(super) fn is_null_slot(&self) -> Option<Slot> {
        match self {
            Binding::ReadSlot { is_null, .. } => Some(*is_null),
            _ => None,
        }
    }
}

pub(super) struct TxLower<'a> {
    pub(super) tables: &'a HashMap<String, TableInfo>,
    pub(super) registry: &'a SemanticRegistry,
    pub(super) precompiles: &'a BTreeMap<PrecompileId, PrecompileSignature>,
    pub(super) tx: &'a ast::TxDecl,
    /// param name → (index, type_id)
    pub(super) params: HashMap<String, (u16, TypeId)>,
    /// local variable name → binding
    pub(super) locals: HashMap<String, Binding>,
    /// Next available slot.
    pub(super) next_slot: Slot,
    /// Emitted IR instructions.
    pub(super) instructions: Vec<Instruction>,
    pub(super) errors: Vec<CompileError>,
}

impl<'a> TxLower<'a> {
    fn new(
        tables: &'a HashMap<String, TableInfo>,
        registry: &'a SemanticRegistry,
        precompiles: &'a BTreeMap<PrecompileId, PrecompileSignature>,
        tx: &'a ast::TxDecl,
    ) -> Self {
        Self {
            tables,
            registry,
            precompiles,
            tx,
            params: HashMap::new(),
            locals: HashMap::new(),
            next_slot: 0,
            instructions: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(super) fn alloc_slot(&mut self) -> Slot {
        let s = self.next_slot;
        self.next_slot += 1;
        s
    }

    fn lower(mut self, tx_id: TxTypeId) -> Result<TxTypeDef, Vec<CompileError>> {
        // Build param map.
        let mut param_schema = Vec::new();
        let mut seen_params: HashMap<String, Span> = HashMap::new();
        for (i, p) in self.tx.params.iter().enumerate() {
            if let Some(prev_span) = seen_params.get(&p.name) {
                self.errors.push(CompileError::new(
                    ErrorKind::DuplicateParam,
                    p.span,
                    format!(
                        "duplicate parameter '{}' (first at {:?})",
                        p.name, prev_span
                    ),
                ));
                continue;
            }
            seen_params.insert(p.name.clone(), p.span);

            let Some(type_id) = self.resolve_param_type_name(p.ty, p.span) else {
                continue;
            };
            self.params.insert(p.name.clone(), (i as u16, type_id));
            param_schema.push(ParamDef {
                name: p.name.clone(),
                type_id,
            });
        }

        // Lower each statement.
        for stmt in &self.tx.body {
            self.lower_stmt(stmt);
        }

        if self.errors.is_empty() {
            Ok(TxTypeDef {
                id: tx_id,
                name: self.tx.name.clone(),
                param_schema,
                body: self.instructions,
            })
        } else {
            Err(self.errors)
        }
    }

    fn resolve_param_type_name(&mut self, ty: TypeName, span: Span) -> Option<TypeId> {
        let name = match ty {
            TypeName::U64 => "u64",
            TypeName::I64 => "i64",
            TypeName::Bool => "bool",
            TypeName::Bytes32 => "bytes32",
        };
        match self.registry.resolve_type_name(name) {
            Ok(type_id) => Some(type_id),
            Err(err) => {
                self.errors.push(CompileError::new(
                    ErrorKind::TypeMismatch,
                    span,
                    err.to_string(),
                ));
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(super) enum LoweredExpr {
    Slot(Slot),
    ValueExpr(ValueExpr, TypeId),
}

pub(super) fn synthesize_canonical_zero(
    registry: &SemanticRegistry,
    type_id: TypeId,
    require_nullable: bool,
) -> Result<PortableValue, String> {
    let descriptor = registry
        .catalog()
        .type_descriptor(type_id)
        .map_err(|err| format!("type id {} is not registered: {err}", type_id.0))?;
    if require_nullable && descriptor.null_semantics != NullSemantics::NullableWithCanonicalZero {
        return Err(format!(
            "null assignment requires NullableWithCanonicalZero semantics, but type id {} is {}",
            type_id.0,
            match descriptor.null_semantics {
                NullSemantics::NullableWithCanonicalZero => "nullable with canonical zero",
                NullSemantics::NonNullable => "non-nullable",
            },
        ));
    }

    match (&descriptor.zero_value_spec, &descriptor.host_value_family) {
        (ZeroValueSpec::IntegerZero, HostValueFamily::UnsignedInt { bits: 64 }) => {
            Ok(PortableValue::new(type_id, 0u64.to_le_bytes().to_vec()))
        }
        (ZeroValueSpec::IntegerZero, HostValueFamily::SignedInt { bits: 64 }) => {
            Ok(PortableValue::new(type_id, 0i64.to_le_bytes().to_vec()))
        }
        (ZeroValueSpec::BoolFalse, HostValueFamily::Bool) => {
            Ok(PortableValue::new(type_id, vec![0u8]))
        }
        (ZeroValueSpec::ZeroBytes { len: zero_len }, HostValueFamily::Bytes { len: host_len })
            if zero_len == host_len =>
        {
            Ok(PortableValue::new(
                type_id,
                vec![0u8; usize::from(*zero_len)],
            ))
        }
        (ZeroValueSpec::ZeroBytes { len: zero_len }, HostValueFamily::Bytes { len: host_len }) => {
            Err(format!(
                "type id {} declares zero bytes length {} but host bytes length {}",
                type_id.0, zero_len, host_len
            ))
        }
        (ZeroValueSpec::Opaque { .. }, _) => Err(format!(
            "type id {} uses an opaque zero-value rule that source lowering cannot synthesize",
            type_id.0
        )),
        (_, HostValueFamily::Opaque { .. }) => Err(format!(
            "type id {} uses an opaque host value family that source lowering cannot synthesize",
            type_id.0
        )),
        (zero, host) => Err(format!(
            "type id {} has incompatible zero-value rule {:?} for host family {:?}",
            type_id.0, zero, host
        )),
    }
}

pub(super) fn builtin_u64_literal(value: u64) -> PortableValue {
    PortableValue::new(TYPE_U64_ID, value.to_le_bytes().to_vec())
}

pub(super) fn builtin_bool_literal(value: bool) -> PortableValue {
    PortableValue::new(TYPE_BOOL_ID, vec![u8::from(value)])
}

pub(super) fn builtin_bytes32_literal(value: [u8; 32]) -> PortableValue {
    PortableValue::new(TYPE_BYTES32_ID, value.to_vec())
}

pub(super) fn is_arithmetic(op: crate::ast::BinOp) -> bool {
    use crate::ast::BinOp;
    matches!(
        op,
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
    )
}
