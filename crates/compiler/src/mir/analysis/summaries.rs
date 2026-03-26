#![allow(clippy::wildcard_imports)]

use std::collections::{BTreeMap, BTreeSet};

use tabula_ir as ir;

use super::super::model::*;
use super::super::validate::VerifiedProgram;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorldEffects {
    pub state_read: bool,
    pub state_write: bool,
    pub state_delete: bool,
    pub emit_event: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProofEffects {
    pub relation_use: bool,
    pub state_property_read: bool,
    pub capability_call: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EffectSummary {
    pub world: WorldEffects,
    pub proof: ProofEffects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FailureSummary {
    pub semantic_may_fail: bool,
    pub host_contract_sensitive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PolicySummary {
    pub uses_builtin_hash: bool,
    pub uses_tx_only_capability: bool,
    pub uses_query_safe_capability: bool,
    pub uses_journaled_capability: bool,
    pub uses_opaque_runtime_capability: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextDemandSummary {
    pub fields: BTreeSet<ir::ContextFieldId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProgramAnalysis {
    pub effect_summaries: BTreeMap<CallableId, EffectSummary>,
    pub failure_summaries: BTreeMap<CallableId, FailureSummary>,
    pub policy_summaries: BTreeMap<CallableId, PolicySummary>,
    pub context_demands: BTreeMap<CallableId, ContextDemandSummary>,
    pub call_graph: BTreeMap<CallableId, BTreeSet<CallableId>>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedProgram {
    pub(super) verified: VerifiedProgram,
    pub(super) analysis: ProgramAnalysis,
}

impl AnalyzedProgram {
    pub fn verified_program(&self) -> &VerifiedProgram {
        &self.verified
    }

    pub fn program(&self) -> &Program {
        self.verified.program()
    }

    pub fn analysis(&self) -> &ProgramAnalysis {
        &self.analysis
    }

    pub fn effect_summary(&self, callable_id: CallableId) -> Option<EffectSummary> {
        self.analysis.effect_summaries.get(&callable_id).copied()
    }

    pub fn failure_summary(&self, callable_id: CallableId) -> Option<FailureSummary> {
        self.analysis.failure_summaries.get(&callable_id).copied()
    }

    pub fn policy_summary(&self, callable_id: CallableId) -> Option<PolicySummary> {
        self.analysis.policy_summaries.get(&callable_id).copied()
    }

    pub fn context_demand_summary(&self, callable_id: CallableId) -> Option<&ContextDemandSummary> {
        self.analysis.context_demands.get(&callable_id)
    }

    pub fn query_legal(&self, callable_id: CallableId) -> Option<bool> {
        let effect = self.effect_summary(callable_id)?;
        let policy = self.policy_summary(callable_id)?;
        Some(
            !effect.world.state_write
                && !effect.world.state_delete
                && !effect.world.emit_event
                && !policy.uses_tx_only_capability,
        )
    }

    pub fn into_parts(self) -> (VerifiedProgram, ProgramAnalysis) {
        (self.verified, self.analysis)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct CallableSummaries {
    pub(super) effect: EffectSummary,
    pub(super) failure: FailureSummary,
    pub(super) policy: PolicySummary,
    pub(super) context: ContextDemandSummary,
}

pub(super) fn merge_summaries(dst: &mut CallableSummaries, src: CallableSummaries) {
    dst.effect.world.state_read |= src.effect.world.state_read;
    dst.effect.world.state_write |= src.effect.world.state_write;
    dst.effect.world.state_delete |= src.effect.world.state_delete;
    dst.effect.world.emit_event |= src.effect.world.emit_event;
    dst.effect.proof.relation_use |= src.effect.proof.relation_use;
    dst.effect.proof.state_property_read |= src.effect.proof.state_property_read;
    dst.effect.proof.capability_call |= src.effect.proof.capability_call;
    dst.failure.semantic_may_fail |= src.failure.semantic_may_fail;
    dst.failure.host_contract_sensitive |= src.failure.host_contract_sensitive;
    dst.policy.uses_builtin_hash |= src.policy.uses_builtin_hash;
    dst.policy.uses_tx_only_capability |= src.policy.uses_tx_only_capability;
    dst.policy.uses_query_safe_capability |= src.policy.uses_query_safe_capability;
    dst.policy.uses_journaled_capability |= src.policy.uses_journaled_capability;
    dst.policy.uses_opaque_runtime_capability |= src.policy.uses_opaque_runtime_capability;
    dst.context.fields.extend(src.context.fields);
}
