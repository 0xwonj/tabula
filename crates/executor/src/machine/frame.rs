use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::TypedValue;

use crate::program::ResolvedEntry;

#[derive(Debug, Clone)]
pub(crate) struct LocalFrame {
    values: Vec<Option<TypedValue>>,
}

impl LocalFrame {
    pub(crate) fn new(entry: &ResolvedEntry) -> Self {
        Self {
            values: vec![None; entry.local_count()],
        }
    }

    pub(crate) fn assign(
        &mut self,
        entry: &ResolvedEntry,
        id: ir::LocalId,
        value: TypedValue,
    ) -> Result<(), TabulaError> {
        let slot = entry.local_slot(id)?;
        self.values[slot] = Some(value);
        Ok(())
    }

    pub(crate) fn get(
        &self,
        entry: &ResolvedEntry,
        id: ir::LocalId,
    ) -> Result<&TypedValue, TabulaError> {
        let slot = entry.local_slot(id)?;
        self.values[slot]
            .as_ref()
            .ok_or_else(|| TabulaError::InvalidIr(format!("unassigned local {}", id.0)))
    }
}

#[cfg(test)]
mod tests {
    use tabula_ir as ir;
    use tabula_profile::TYPE_U64_ID;
    use tabula_types::u64_typed;

    use super::LocalFrame;
    use crate::program::ResolvedExecutionProgram;

    fn resolved_entry_program() -> ResolvedExecutionProgram {
        ResolvedExecutionProgram::from_validated_program(
            ir::ValidatedProgram::try_from(ir::Program {
                program_id: ir::ProgramId(1),
                state: ir::StateSchema { tables: vec![] },
                context: ir::ContextSchema { fields: vec![] },
                const_pool: ir::ConstantPool { entries: vec![] },
                relation_manifest: ir::RelationManifest { entries: vec![] },
                capability_manifest: ir::CapabilityManifest { entries: vec![] },
                event_manifest: ir::EventManifest { entries: vec![] },
                entries: vec![ir::Entry {
                    id: ir::EntryId(0),
                    symbol: "entry".into(),
                    kind: ir::EntryKind::Query,
                    params: vec![],
                    returns: vec![],
                    return_policy: ir::ReturnPolicy::Unit,
                    body: ir::Body {
                        locals: vec![
                            ir::LocalDecl {
                                id: ir::LocalId(0),
                                ty: TYPE_U64_ID,
                            },
                            ir::LocalDecl {
                                id: ir::LocalId(1),
                                ty: TYPE_U64_ID,
                            },
                        ],
                        ops: vec![ir::Op::Return {
                            values: ir::ValueTupleRef(vec![]),
                        }],
                    },
                }],
            })
            .expect("valid program"),
        )
        .expect("resolved program")
    }

    #[test]
    fn local_frame_reads_assigned_local() {
        let program = resolved_entry_program();
        let entry = program.entry(ir::EntryId(0)).expect("entry");
        let mut frame = LocalFrame::new(entry);

        frame
            .assign(entry, ir::LocalId(0), u64_typed(7))
            .expect("assign");

        assert_eq!(
            frame.get(entry, ir::LocalId(0)).expect("local"),
            &u64_typed(7)
        );
    }

    #[test]
    fn local_frame_rejects_unassigned_local() {
        let program = resolved_entry_program();
        let entry = program.entry(ir::EntryId(0)).expect("entry");
        let frame = LocalFrame::new(entry);

        let err = frame
            .get(entry, ir::LocalId(1))
            .expect_err("unassigned local");
        assert!(err.to_string().contains("unassigned local"));
    }
}
