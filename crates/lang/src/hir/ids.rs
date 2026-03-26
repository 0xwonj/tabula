use tabula_core::TypeId;

pub type TypeRef = TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelationId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallableId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParamId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityRefId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextFieldId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashFamily {
    Poseidon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityTotality {
    Total,
    Checked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityQueryPolicy {
    QuerySafe,
    TxOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityProofVisibility {
    Journaled,
    OpaqueRuntimeOnly,
}
