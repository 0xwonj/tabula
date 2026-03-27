use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::PortableValue;
use tabula_ir as ir;

pub type ProgramId = ir::ProgramId;
pub type TypeRef = ir::TypeRef;
pub type ValueRef = ir::ValueRef;
pub type ValueTupleRef = ir::ValueTupleRef;
pub type ParamDecl = ir::ParamDecl;
pub type ParamId = ir::ParamId;
pub type LocalId = ir::LocalId;
pub type TableId = ir::TableId;
pub type FieldId = ir::FieldId;
pub type RelationId = ir::RelationId;
pub type CapabilityId = ir::CapabilityId;
pub type EventId = ir::EventId;
pub type StateSchema = ir::StateSchema;
pub type ContextSchema = ir::ContextSchema;
pub type ConstantPool = ir::ConstantPool;
pub type RelationManifest = ir::RelationManifest;
pub type CapabilityManifest = ir::CapabilityManifest;
pub type EventManifest = ir::EventManifest;
pub type ArithOp = ir::ArithOp;
pub type CmpOp = ir::CmpOp;
pub type HashFamily = ir::HashFamily;
pub type StatePropertyQuery = ir::StatePropertyQuery;
pub type LiteralValue = PortableValue;

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
pub struct CallableId(pub u32);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CallableKind {
    Function,
    Query,
    Tx,
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
    pub callables: Vec<Callable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Callable {
    pub id: CallableId,
    pub symbol: String,
    pub kind: CallableKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Body {
    pub locals: Vec<LocalDecl>,
    pub region: Region,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct LocalDecl {
    pub id: LocalId,
    pub symbol: Option<String>,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Region {
    pub ops: Vec<Op>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Terminator {
    Yield { values: ValueTupleRef },
    Return { values: ValueTupleRef },
}

impl Terminator {
    pub fn values(&self) -> &ValueTupleRef {
        match self {
            Self::Yield { values } | Self::Return { values } => values,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Op {
    BindValue {
        dst: LocalId,
        value: ValueOp,
    },
    DivMod {
        dst_q: LocalId,
        dst_r: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    ReadState {
        dst_value: LocalId,
        dst_present: LocalId,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    WriteState {
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
        value: ValueRef,
    },
    DeleteState {
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    ReadStateProperty {
        dsts: Vec<LocalId>,
        table: TableId,
        field: FieldId,
        query: StatePropertyQuery,
    },
    Assert {
        cond: ValueRef,
    },
    AssertRelation {
        relation: RelationId,
        args: ValueTupleRef,
    },
    EvalRelation {
        relation: RelationId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },
    CallCapability {
        capability: CapabilityId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },
    CallFunction {
        callee: CallableId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },
    EmitEvent {
        event: EventId,
        args: ValueTupleRef,
    },
    If {
        dsts: Vec<LocalId>,
        cond: ValueRef,
        then_region: Region,
        else_region: Region,
    },
    Match {
        dsts: Vec<LocalId>,
        scrutinee: ValueRef,
        arms: Vec<MatchArm>,
        default: Option<Region>,
    },
}

impl Op {
    pub fn dsts(&self) -> Vec<LocalId> {
        match self {
            Self::BindValue { dst, .. } => vec![*dst],
            Self::DivMod { dst_q, dst_r, .. } => vec![*dst_q, *dst_r],
            Self::ReadState {
                dst_value,
                dst_present,
                ..
            } => vec![*dst_value, *dst_present],
            Self::ReadStateProperty { dsts, .. }
            | Self::EvalRelation { dsts, .. }
            | Self::CallCapability { dsts, .. }
            | Self::CallFunction { dsts, .. }
            | Self::If { dsts, .. }
            | Self::Match { dsts, .. } => dsts.clone(),
            Self::WriteState { .. }
            | Self::DeleteState { .. }
            | Self::Assert { .. }
            | Self::AssertRelation { .. }
            | Self::EmitEvent { .. } => Vec::new(),
        }
    }

    pub fn defines_locals(&self) -> Vec<LocalId> {
        self.dsts()
    }

    pub fn nested_regions(&self) -> Vec<&Region> {
        match self {
            Self::If {
                then_region,
                else_region,
                ..
            } => vec![then_region, else_region],
            Self::Match { arms, default, .. } => {
                let mut regions = arms.iter().map(|arm| &arm.region).collect::<Vec<_>>();
                if let Some(default) = default {
                    regions.push(default);
                }
                regions
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ValueOp {
    Arith {
        op: ArithOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Cmp {
        op: CmpOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Not {
        src: ValueRef,
    },
    And {
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Or {
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Select {
        cond: ValueRef,
        if_true: ValueRef,
        if_false: ValueRef,
    },
    Hash {
        family: HashFamily,
        inputs: ValueTupleRef,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub region: Region,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum MatchPattern {
    Literal(LiteralValue),
    Wildcard,
}
