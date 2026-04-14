use tabula_core::{CommittedCellKey, CommittedPropertyQuery, TypeId};
use tabula_ir as ir;
use tabula_types::{TypedCommittedPropertyQueryResult, TypedValue};

use crate::surface::{
    CapabilityEffect, RelationEffect, RelationEffectKind, StateEffectKind, StatePropertyEffect,
    TypedEventEffect, TypedStateEffect,
};

type EffectRecorderParts = (
    Vec<TypedStateEffect>,
    Vec<StatePropertyEffect>,
    Vec<RelationEffect>,
    Vec<CapabilityEffect>,
    Vec<TypedEventEffect>,
    u64,
);

pub(crate) struct EffectRecorder {
    logical_time: u64,
    next_effect_ordinal: u32,
    state_effects: Vec<TypedStateEffect>,
    property_effects: Vec<StatePropertyEffect>,
    relation_effects: Vec<RelationEffect>,
    capability_effects: Vec<CapabilityEffect>,
    event_effects: Vec<TypedEventEffect>,
}

impl EffectRecorder {
    pub(crate) fn new(start_logical_time: u64) -> Self {
        Self {
            logical_time: start_logical_time,
            next_effect_ordinal: 0,
            state_effects: Vec::new(),
            property_effects: Vec::new(),
            relation_effects: Vec::new(),
            capability_effects: Vec::new(),
            event_effects: Vec::new(),
        }
    }

    pub(crate) fn record_state(
        &mut self,
        op_index: usize,
        key: CommittedCellKey,
        type_id: TypeId,
        kind: StateEffectKind,
        value: Option<TypedValue>,
    ) {
        let effect_ordinal_in_entry = self.next_effect_ordinal();
        self.state_effects.push(TypedStateEffect {
            key,
            type_id,
            kind,
            value,
            logical_time: self.logical_time,
            op_index,
            effect_ordinal_in_entry,
        });
        self.logical_time += 1;
    }

    pub(crate) fn record_relation(
        &mut self,
        op_index: usize,
        relation: ir::RelationId,
        kind: RelationEffectKind,
        inputs: Vec<TypedValue>,
        outputs: Vec<TypedValue>,
    ) {
        let effect_ordinal_in_entry = self.next_effect_ordinal();
        self.relation_effects.push(RelationEffect {
            relation,
            kind,
            inputs,
            outputs,
            op_index,
            effect_ordinal_in_entry,
        });
    }

    pub(crate) fn record_property(
        &mut self,
        op_index: usize,
        table: ir::TableId,
        field: ir::FieldId,
        query: CommittedPropertyQuery,
        result: TypedCommittedPropertyQueryResult,
    ) {
        let effect_ordinal_in_entry = self.next_effect_ordinal();
        self.property_effects.push(StatePropertyEffect {
            table,
            field,
            query,
            result,
            op_index,
            effect_ordinal_in_entry,
        });
    }

    pub(crate) fn record_capability(
        &mut self,
        op_index: usize,
        capability: ir::CapabilityId,
        inputs: Vec<TypedValue>,
        outputs: Vec<TypedValue>,
    ) {
        let effect_ordinal_in_entry = self.next_effect_ordinal();
        self.capability_effects.push(CapabilityEffect {
            capability,
            inputs,
            outputs,
            op_index,
            effect_ordinal_in_entry,
        });
    }

    pub(crate) fn record_event(
        &mut self,
        op_index: usize,
        event: ir::EventId,
        args: Vec<TypedValue>,
    ) {
        let effect_ordinal_in_entry = self.next_effect_ordinal();
        self.event_effects.push(TypedEventEffect {
            event,
            args,
            op_index,
            effect_ordinal_in_entry,
        });
    }

    pub(crate) fn into_parts(self) -> EffectRecorderParts {
        (
            self.state_effects,
            self.property_effects,
            self.relation_effects,
            self.capability_effects,
            self.event_effects,
            self.logical_time,
        )
    }

    fn next_effect_ordinal(&mut self) -> u32 {
        let current = self.next_effect_ordinal;
        self.next_effect_ordinal += 1;
        current
    }
}

#[cfg(test)]
mod tests {
    use tabula_core::{CommittedCellKey, CommittedKey, TypeId};
    use tabula_ir as ir;
    use tabula_profile::TYPE_U64_ID;
    use tabula_types::u64_typed;

    use super::EffectRecorder;
    use crate::surface::{RelationEffectKind, StateEffectKind};

    #[test]
    fn effect_recorder_shares_ordinals_and_only_state_advances_logical_time() {
        let mut recorder = EffectRecorder::new(11);
        recorder.record_relation(
            0,
            ir::RelationId(1),
            RelationEffectKind::Assert,
            vec![u64_typed(1)],
            vec![],
        );
        recorder.record_state(
            1,
            CommittedCellKey {
                table: tabula_core::TableId(1),
                col: tabula_core::ColId(0),
                key: CommittedKey(vec![9]),
            },
            TypeId(TYPE_U64_ID.0),
            StateEffectKind::Write,
            Some(u64_typed(5)),
        );
        recorder.record_relation(
            2,
            ir::RelationId(2),
            RelationEffectKind::Eval,
            vec![u64_typed(2)],
            vec![u64_typed(3)],
        );

        let (state, _property, relation, _cap, _event, next_time) = recorder.into_parts();
        assert_eq!(relation[0].effect_ordinal_in_entry, 0);
        assert_eq!(state[0].effect_ordinal_in_entry, 1);
        assert_eq!(relation[1].effect_ordinal_in_entry, 2);
        assert_eq!(state[0].logical_time, 11);
        assert_eq!(next_time, 12);
    }
}
