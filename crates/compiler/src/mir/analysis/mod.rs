use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_ir as ir;

#[allow(clippy::wildcard_imports)]
use super::model::*;
use super::validate::VerifiedProgram;

mod context;
mod effects;
mod failures;
mod policy;
mod summaries;

use context::collect_context_demands_from_op;
use effects::update_effect_summary;
use failures::update_failure_summary;
use policy::update_policy_summary;
use summaries::{CallableSummaries, merge_summaries};

pub(crate) use summaries::{
    AnalyzedProgram, ContextDemandSummary, EffectSummary, FailureSummary, PolicySummary,
    ProgramAnalysis,
};

pub fn analyze_program(program: VerifiedProgram) -> Result<AnalyzedProgram, TabulaError> {
    let analysis = AnalyzeCx::new(program.program()).analyze()?;
    Ok(AnalyzedProgram {
        verified: program,
        analysis,
    })
}

struct AnalyzeCx<'a> {
    program: &'a Program,
    callables: BTreeMap<CallableId, &'a Callable>,
    capabilities: BTreeMap<CapabilityId, &'a ir::CapabilityDescriptor>,
    call_graph: BTreeMap<CallableId, BTreeSet<CallableId>>,
    effect_summaries: BTreeMap<CallableId, EffectSummary>,
    failure_summaries: BTreeMap<CallableId, FailureSummary>,
    policy_summaries: BTreeMap<CallableId, PolicySummary>,
    context_demands: BTreeMap<CallableId, ContextDemandSummary>,
    visiting: BTreeSet<CallableId>,
}

impl<'a> AnalyzeCx<'a> {
    fn new(program: &'a Program) -> Self {
        let callables = program
            .callables
            .iter()
            .map(|callable| (callable.id, callable))
            .collect::<BTreeMap<_, _>>();
        let capabilities = program
            .capability_manifest
            .entries
            .iter()
            .map(|entry| (entry.id, entry))
            .collect::<BTreeMap<_, _>>();
        Self {
            program,
            callables,
            capabilities,
            call_graph: BTreeMap::new(),
            effect_summaries: BTreeMap::new(),
            failure_summaries: BTreeMap::new(),
            policy_summaries: BTreeMap::new(),
            context_demands: BTreeMap::new(),
            visiting: BTreeSet::new(),
        }
    }

    fn analyze(mut self) -> Result<ProgramAnalysis, TabulaError> {
        self.build_call_graph();
        for callable in &self.program.callables {
            let summaries = self.infer_callable_summaries(callable.id)?;
            self.effect_summaries.insert(callable.id, summaries.effect);
            self.failure_summaries
                .insert(callable.id, summaries.failure);
            self.policy_summaries.insert(callable.id, summaries.policy);
            self.context_demands.insert(callable.id, summaries.context);
        }
        for callable in &self.program.callables {
            if callable.kind == CallableKind::Query && !self.query_legal(callable.id)? {
                return Err(TabulaError::InvalidIr(format!(
                    "query callable {} violates read-only or capability policy",
                    callable.symbol
                )));
            }
        }
        Ok(ProgramAnalysis {
            effect_summaries: self.effect_summaries,
            failure_summaries: self.failure_summaries,
            policy_summaries: self.policy_summaries,
            context_demands: self.context_demands,
            call_graph: self.call_graph,
        })
    }

    fn build_call_graph(&mut self) {
        for callable in &self.program.callables {
            let mut edges = BTreeSet::new();
            collect_direct_calls(&callable.body.region, &mut edges);
            self.call_graph.insert(callable.id, edges);
        }
    }

    fn infer_callable_summaries(
        &mut self,
        callable_id: CallableId,
    ) -> Result<CallableSummaries, TabulaError> {
        if let (Some(effect), Some(failure), Some(policy)) = (
            self.effect_summaries.get(&callable_id),
            self.failure_summaries.get(&callable_id),
            self.policy_summaries.get(&callable_id),
        ) {
            return Ok(CallableSummaries {
                effect: *effect,
                failure: *failure,
                policy: *policy,
                context: self
                    .context_demands
                    .get(&callable_id)
                    .cloned()
                    .unwrap_or_default(),
            });
        }
        if !self.visiting.insert(callable_id) {
            return Err(TabulaError::InvalidIr(format!(
                "recursive MIR function cycle detected at callable {}",
                callable_id.0
            )));
        }
        let callable = self.callables.get(&callable_id).ok_or_else(|| {
            TabulaError::InvalidIr(format!("unknown MIR callable {}", callable_id.0))
        })?;
        let summary = self.infer_region_summaries(&callable.body.region)?;
        self.visiting.remove(&callable_id);
        self.effect_summaries.insert(callable_id, summary.effect);
        self.failure_summaries.insert(callable_id, summary.failure);
        self.policy_summaries.insert(callable_id, summary.policy);
        self.context_demands
            .insert(callable_id, summary.context.clone());
        Ok(summary)
    }

    fn infer_region_summaries(
        &mut self,
        region: &Region,
    ) -> Result<CallableSummaries, TabulaError> {
        let mut summaries = CallableSummaries::default();
        for op in &region.ops {
            collect_context_demands_from_op(op, &mut summaries.context);
            update_effect_summary(op, &mut summaries.effect);
            update_failure_summary(op, &self.capabilities, &mut summaries.failure)?;
            update_policy_summary(op, &self.capabilities, &mut summaries.policy)?;
            match op {
                Op::CallFunction { callee, .. } => {
                    let callee_summary = self.infer_callable_summaries(*callee)?;
                    merge_summaries(&mut summaries, callee_summary);
                }
                Op::If {
                    then_region,
                    else_region,
                    ..
                } => {
                    merge_summaries(&mut summaries, self.infer_region_summaries(then_region)?);
                    merge_summaries(&mut summaries, self.infer_region_summaries(else_region)?);
                }
                Op::Match { arms, default, .. } => {
                    for arm in arms {
                        merge_summaries(&mut summaries, self.infer_region_summaries(&arm.region)?);
                    }
                    if let Some(default) = default {
                        merge_summaries(&mut summaries, self.infer_region_summaries(default)?);
                    }
                }
                _ => {}
            }
        }
        context::collect_context_demands_from_tuple(
            region.terminator.values(),
            &mut summaries.context,
        );
        Ok(summaries)
    }

    fn query_legal(&self, callable_id: CallableId) -> Result<bool, TabulaError> {
        let effect = self
            .effect_summaries
            .get(&callable_id)
            .copied()
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!(
                    "missing effect summary for callable {}",
                    callable_id.0
                ))
            })?;
        let policy = self
            .policy_summaries
            .get(&callable_id)
            .copied()
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!(
                    "missing policy summary for callable {}",
                    callable_id.0
                ))
            })?;
        Ok(!effect.world.state_write
            && !effect.world.state_delete
            && !effect.world.emit_event
            && !policy.uses_tx_only_capability)
    }
}

fn collect_direct_calls(region: &Region, edges: &mut BTreeSet<CallableId>) {
    for op in &region.ops {
        match op {
            Op::CallFunction { callee, .. } => {
                edges.insert(*callee);
            }
            Op::If {
                then_region,
                else_region,
                ..
            } => {
                collect_direct_calls(then_region, edges);
                collect_direct_calls(else_region, edges);
            }
            Op::Match { arms, default, .. } => {
                for arm in arms {
                    collect_direct_calls(&arm.region, edges);
                }
                if let Some(default) = default {
                    collect_direct_calls(default, edges);
                }
            }
            _ => {}
        }
    }
}
