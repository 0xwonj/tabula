#![allow(missing_docs)]

#[allow(clippy::wildcard_imports)]
use super::*;

#[derive(Clone, Default)]
pub(super) struct LocalEnv {
    pub(super) params: BTreeMap<ParamId, TypeRef>,
    pub(super) param_symbols: BTreeSet<String>,
    pub(super) bindings: BTreeMap<BindingId, TypeRef>,
    pub(super) binding_symbols: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RegionKind {
    Root,
    Nested,
}
