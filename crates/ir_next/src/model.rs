use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::{PortableValue, TypeId};

pub type TypeRef = TypeId;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ProgramId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct EntryId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ParamId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct LocalId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ContextFieldId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ConstId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct TableId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct FieldId(pub u16);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct RelationId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CapabilityId(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct EventId(pub u32);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum EntryKind {
    Query,
    Tx,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ReturnPolicy {
    Unit,
    Explicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Program {
    pub program_id: ProgramId,
    pub state: StateSchema,
    pub context: ContextSchema,
    pub const_pool: ConstantPool,
    pub relation_manifest: RelationManifest,
    pub capability_manifest: CapabilityManifest,
    pub event_manifest: EventManifest,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatedProgram(pub(crate) Program);

impl ValidatedProgram {
    pub fn as_program(&self) -> &Program {
        &self.0
    }

    pub fn into_program(self) -> Program {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StateSchema {
    pub tables: Vec<TableSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TableSchema {
    pub id: TableId,
    pub symbol: String,
    pub key_tys: Vec<TypeRef>,
    pub fields: Vec<FieldSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct FieldSchema {
    pub id: FieldId,
    pub symbol: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ContextSchema {
    pub fields: Vec<ContextField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ContextField {
    pub id: ContextFieldId,
    pub symbol: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ConstantPool {
    pub entries: Vec<ConstantEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ConstantEntry {
    pub id: ConstId,
    pub ty: TypeRef,
    pub value: PortableValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationManifest {
    pub entries: Vec<RelationManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationManifestEntry {
    pub id: RelationId,
    pub descriptor: RelationDescriptor,
    pub binding: RelationBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationDescriptor {
    pub symbol: String,
    pub inputs: Vec<TypeRef>,
    pub outputs: Vec<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum RelationBinding {
    EnumSet { values: Vec<PortableValue> },
    Map { rows: Vec<RelationRow> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationRow {
    pub inputs: Vec<PortableValue>,
    pub outputs: Vec<PortableValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EventManifest {
    pub entries: Vec<EventDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EventDescriptor {
    pub id: EventId,
    pub symbol: String,
    pub fields: Vec<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CapabilityManifest {
    pub entries: Vec<CapabilityDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub symbol: String,
    pub inputs: Vec<TypeRef>,
    pub outputs: Vec<TypeRef>,
    pub totality: CapabilityTotality,
    pub query_policy: CapabilityQueryPolicy,
    pub proof_visibility: CapabilityProofVisibility,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityTotality {
    Total,
    Checked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityQueryPolicy {
    QuerySafe,
    TxOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityProofVisibility {
    Journaled,
    OpaqueRuntimeOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Entry {
    pub id: EntryId,
    pub symbol: String,
    pub kind: EntryKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub return_policy: ReturnPolicy,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ParamDecl {
    pub id: ParamId,
    pub symbol: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct LocalDecl {
    pub id: LocalId,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Body {
    pub locals: Vec<LocalDecl>,
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ValueRef {
    Literal(PortableValue),
    Param(ParamId),
    Context(ContextFieldId),
    Local(LocalId),
    Const(ConstId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ValueTupleRef(pub Vec<ValueRef>);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct GuardRef(pub LocalId);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum HashFamily {
    Poseidon,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum AggregateKind {
    Sum,
    Count,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum StatePropertyQuery {
    Minimum,
    Maximum,
    Successor {
        key: ValueTupleRef,
    },
    Predecessor {
        key: ValueTupleRef,
    },
    NonExistenceRange {
        lower: ValueTupleRef,
        upper: ValueTupleRef,
    },
    Aggregate {
        kind: AggregateKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Arith {
        dst: LocalId,
        op: ArithOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Cmp {
        dst: LocalId,
        op: CmpOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Not {
        dst: LocalId,
        src: ValueRef,
    },
    And {
        dst: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Or {
        dst: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Select {
        dst: LocalId,
        cond: ValueRef,
        if_true: ValueRef,
        if_false: ValueRef,
    },
    Hash {
        dst: LocalId,
        family: HashFamily,
        inputs: ValueTupleRef,
    },
    DivMod {
        guard: Option<GuardRef>,
        dst_q: LocalId,
        dst_r: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    ReadState {
        guard: Option<GuardRef>,
        dst_value: LocalId,
        dst_present: LocalId,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    WriteState {
        guard: Option<GuardRef>,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
        value: ValueRef,
    },
    DeleteState {
        guard: Option<GuardRef>,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    ReadStateProperty {
        guard: Option<GuardRef>,
        dsts: Vec<LocalId>,
        table: TableId,
        field: FieldId,
        query: StatePropertyQuery,
    },
    Assert {
        guard: Option<GuardRef>,
        cond: ValueRef,
    },
    AssertRelation {
        guard: Option<GuardRef>,
        relation: RelationId,
        args: ValueTupleRef,
    },
    EvalRelation {
        guard: Option<GuardRef>,
        relation: RelationId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },
    CallCapability {
        guard: Option<GuardRef>,
        capability: CapabilityId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },
    EmitEvent {
        guard: Option<GuardRef>,
        event: EventId,
        args: ValueTupleRef,
    },
    Return {
        values: ValueTupleRef,
    },
}

impl From<TableId> for tabula_core::TableId {
    fn from(value: TableId) -> Self {
        Self(value.0)
    }
}

impl From<FieldId> for tabula_core::ColId {
    fn from(value: FieldId) -> Self {
        Self(value.0)
    }
}
