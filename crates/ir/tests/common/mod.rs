use tabula_core::PortableValue;
use tabula_ir::*;
use tabula_profile::TYPE_U64_ID;

pub fn base_program(entry: Entry) -> Program {
    Program {
        program_id: ProgramId(0),
        state: StateSchema {
            tables: vec![TableSchema {
                id: TableId(1),
                symbol: "accounts".into(),
                key_tys: vec![TYPE_U64_ID],
                fields: vec![FieldSchema {
                    id: FieldId(0),
                    symbol: "balance".into(),
                    ty: TYPE_U64_ID,
                }],
            }],
        },
        context: ContextSchema { fields: vec![] },
        const_pool: ConstantPool { entries: vec![] },
        relation_manifest: RelationManifest { entries: vec![] },
        capability_manifest: CapabilityManifest { entries: vec![] },
        event_manifest: EventManifest { entries: vec![] },
        entries: vec![entry],
    }
}

pub fn u64_literal(value: u64) -> ValueRef {
    ValueRef::Literal(PortableValue::new(
        TYPE_U64_ID,
        value.to_le_bytes().to_vec(),
    ))
}
