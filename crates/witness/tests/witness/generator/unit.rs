use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use borsh::{from_slice, to_vec};
use p3_koala_bear::KoalaBear;
use tabula_core::error::TabulaError;
use tabula_core::{AccessEvent, BatchResult, ColId, OpKind, TableId, TxResult};
use tabula_core::{
    ColumnDef, ColumnProfileId, EncodingProfileId, PortableValue, TableSchema, TypeId,
};
use tabula_profile::{
    CanonicalNullEncoding, ColumnProfile, CommitmentRole, ENCODING_U64_ID, EncodingClass,
    EncodingProfile, FieldFamily, GenericIrFamily, HostValueFamily, NullSemantics,
    SCHEME_PROFILE_SSMC_ID, TYPE_U64_ID, TranscriptSerialization, TypeCapabilities, TypeDescriptor,
    ZeroValueSpec, builtin_catalog,
};
use tabula_types::builtins::{decode_seeded_field_elements, encode_seeded_field_elements};
use tabula_types::{
    ArithmeticOp, EncodingRuntime, EncodingRuntimeRegistry, TypeRuntime, TypeRuntimeRegistry,
    TypedValue, bytes32_portable, u64_portable, u64_typed,
};

use super::*;

fn prepared_column(
    prepared: &tabula_witness::PreparedExecutionColumns,
    table: TableId,
    col: ColId,
) -> &tabula_witness::PreparedExecutionColumn {
    prepared.column(table, col).expect("prepared column")
}

fn prepare(
    result: &BatchResult,
    schema: &std::collections::BTreeMap<TableId, tabula_core::TableSchema>,
    planned: &[(TableId, ColId)],
) -> tabula_witness::PreparedExecutionColumns {
    let profile_catalog = profile_catalog_for_schemas(schema);
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    make_preparer()
        .prepare_execution_inputs(
            result,
            schema,
            &profile_catalog,
            &type_runtimes,
            &encoding_runtimes,
            planned.iter(),
        )
        .expect("prepared execution inputs")
}

const CUSTOM_NUMERIC_TYPE_ID: TypeId = TypeId(0x9a01);
const CUSTOM_NUMERIC_ENCODING_ID: EncodingProfileId = EncodingProfileId(0x9a01);
const CUSTOM_NUMERIC_COLUMN_PROFILE_ID: ColumnProfileId = ColumnProfileId(0x9a01);

fn custom_numeric_schema(table: u32, col: u16) -> BTreeMap<TableId, TableSchema> {
    schemas(vec![TableSchema {
        id: t(table),
        name: format!("table_{table}"),
        columns: vec![ColumnDef {
            id: c(col),
            name: format!("col_{col}"),
            column_profile_id: CUSTOM_NUMERIC_COLUMN_PROFILE_ID,
        }],
    }])
}

fn custom_numeric_descriptor() -> TypeDescriptor {
    TypeDescriptor::new(
        CUSTOM_NUMERIC_TYPE_ID,
        "nonce64",
        None,
        HostValueFamily::UnsignedInt { bits: 64 },
        GenericIrFamily::UnsignedInteger,
        TypeCapabilities {
            equality: true,
            ordering: true,
            arithmetic: true,
        },
        ZeroValueSpec::IntegerZero,
        NullSemantics::NullableWithCanonicalZero,
    )
    .expect("custom numeric descriptor")
}

fn custom_numeric_encoding(descriptor: &TypeDescriptor) -> EncodingProfile {
    EncodingProfile::new(
        CUSTOM_NUMERIC_ENCODING_ID,
        "nonce64_kb3",
        None,
        descriptor,
        EncodingClass::FieldElementArray,
        FieldFamily::KoalaBear31,
        3,
        CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
        TranscriptSerialization::FieldElementsWithNullFlag,
        true,
    )
    .expect("custom numeric encoding")
}

fn custom_numeric_catalog(
    schema: &BTreeMap<TableId, TableSchema>,
) -> tabula_profile::ProfileCatalog {
    let mut catalog = builtin_catalog().expect("built-in catalog");
    let descriptor = custom_numeric_descriptor();
    let encoding = custom_numeric_encoding(&descriptor);
    let scheme_profile = catalog
        .scheme_profile(SCHEME_PROFILE_SSMC_ID)
        .cloned()
        .expect("built-in ssmc profile");
    catalog
        .register_type(descriptor.clone())
        .expect("register custom descriptor");
    catalog
        .register_encoding(encoding.clone())
        .expect("register custom encoding");
    for schema in schema.values() {
        for column in &schema.columns {
            let column_profile = ColumnProfile::new(
                column.column_profile_id,
                format!("{}.{}", schema.name, column.name),
                None,
                &descriptor,
                &encoding,
                &scheme_profile,
                CommitmentRole::IncludedInRoot,
            )
            .expect("custom column profile");
            catalog
                .register_column(column_profile)
                .expect("register custom column profile");
        }
    }
    catalog
}

fn custom_numeric_portable(value: u64) -> PortableValue {
    PortableValue::new(
        CUSTOM_NUMERIC_TYPE_ID,
        to_vec(&value).expect("custom numeric portable"),
    )
}

fn decode_custom_numeric(value: &TypedValue) -> Result<u64, TabulaError> {
    if value.type_id() != CUSTOM_NUMERIC_TYPE_ID {
        return Err(TabulaError::TypeMismatch {
            expected: format!("type {}", CUSTOM_NUMERIC_TYPE_ID.0),
            actual: format!("type {}", value.type_id().0),
        });
    }
    from_slice(value.payload()).map_err(|err| {
        TabulaError::BorshEncodingError(format!("custom numeric payload decode failed: {err}"))
    })
}

#[derive(Clone)]
struct CustomNumericTypeRuntime {
    descriptor: TypeDescriptor,
}

impl TypeRuntime for CustomNumericTypeRuntime {
    fn type_id(&self) -> TypeId {
        CUSTOM_NUMERIC_TYPE_ID
    }

    fn descriptor(&self) -> &TypeDescriptor {
        &self.descriptor
    }

    fn zero_typed(&self) -> TypedValue {
        TypedValue::new(CUSTOM_NUMERIC_TYPE_ID, to_vec(&0u64).expect("zero payload"))
    }

    fn encode_portable(&self, value: &TypedValue) -> Result<PortableValue, TabulaError> {
        self.validate(value)?;
        Ok(PortableValue::new(
            CUSTOM_NUMERIC_TYPE_ID,
            value.payload().to_vec(),
        ))
    }

    fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError> {
        if value.type_id() != CUSTOM_NUMERIC_TYPE_ID {
            return Err(TabulaError::TypeMismatch {
                expected: format!("type {}", CUSTOM_NUMERIC_TYPE_ID.0),
                actual: format!("type {}", value.type_id().0),
            });
        }
        let typed = TypedValue::new(CUSTOM_NUMERIC_TYPE_ID, value.payload().to_vec());
        self.validate(&typed)?;
        Ok(typed)
    }

    fn validate(&self, value: &TypedValue) -> Result<(), TabulaError> {
        let _ = decode_custom_numeric(value)?;
        Ok(())
    }

    fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError> {
        Ok(decode_custom_numeric(lhs)? == decode_custom_numeric(rhs)?)
    }

    fn cmp_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<Ordering, TabulaError> {
        Ok(decode_custom_numeric(lhs)?.cmp(&decode_custom_numeric(rhs)?))
    }

    fn apply_arithmetic(
        &self,
        op: ArithmeticOp,
        lhs: &TypedValue,
        rhs: &TypedValue,
    ) -> Result<TypedValue, TabulaError> {
        let lhs = decode_custom_numeric(lhs)?;
        let rhs = decode_custom_numeric(rhs)?;
        let value = match op {
            ArithmeticOp::Add => lhs.checked_add(rhs),
            ArithmeticOp::Sub => lhs.checked_sub(rhs),
            ArithmeticOp::Mul => lhs.checked_mul(rhs),
        }
        .ok_or(TabulaError::ArithmeticOverflow)?;
        Ok(TypedValue::new(
            CUSTOM_NUMERIC_TYPE_ID,
            to_vec(&value).expect("custom numeric payload"),
        ))
    }

    fn divmod(
        &self,
        lhs: &TypedValue,
        rhs: &TypedValue,
    ) -> Result<(TypedValue, TypedValue), TabulaError> {
        let lhs = decode_custom_numeric(lhs)?;
        let rhs = decode_custom_numeric(rhs)?;
        if rhs == 0 {
            return Err(TabulaError::DivisionByZero);
        }
        Ok((
            TypedValue::new(
                CUSTOM_NUMERIC_TYPE_ID,
                to_vec(&(lhs / rhs)).expect("custom quotient payload"),
            ),
            TypedValue::new(
                CUSTOM_NUMERIC_TYPE_ID,
                to_vec(&(lhs % rhs)).expect("custom remainder payload"),
            ),
        ))
    }

    fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError> {
        Ok(format!("{}nonce64", decode_custom_numeric(value)?))
    }
}

#[derive(Clone)]
struct CustomNumericEncodingRuntime {
    descriptor: EncodingProfile,
}

impl EncodingRuntime for CustomNumericEncodingRuntime {
    fn encoding_profile_id(&self) -> EncodingProfileId {
        CUSTOM_NUMERIC_ENCODING_ID
    }

    fn descriptor(&self) -> &EncodingProfile {
        &self.descriptor
    }

    fn encode_field_elements(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        encode_seeded_field_elements(&u64_typed(decode_custom_numeric(value)?))
    }

    fn decode_field_elements(
        &self,
        field_elements: &[KoalaBear],
    ) -> Result<TypedValue, TabulaError> {
        let value = decode_seeded_field_elements(TYPE_U64_ID, field_elements)?;
        Ok(TypedValue::new(
            CUSTOM_NUMERIC_TYPE_ID,
            to_vec(&from_slice::<u64>(value.payload()).expect("builtin u64 payload"))
                .expect("custom numeric payload"),
        ))
    }

    fn encode_transcript_atoms(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        self.encode_field_elements(value)
    }

    fn trace_width(&self) -> usize {
        self.descriptor.width as usize
    }
}

fn custom_numeric_type_runtimes() -> TypeRuntimeRegistry {
    let mut runtimes = seeded_type_runtimes();
    runtimes
        .register(Arc::new(CustomNumericTypeRuntime {
            descriptor: custom_numeric_descriptor(),
        }))
        .expect("register custom type runtime");
    runtimes
}

fn custom_numeric_encoding_runtimes() -> EncodingRuntimeRegistry {
    let mut runtimes = seeded_encoding_runtimes();
    runtimes
        .register(Arc::new(CustomNumericEncodingRuntime {
            descriptor: custom_numeric_encoding(&custom_numeric_descriptor()),
        }))
        .expect("register custom encoding runtime");
    runtimes
}

#[test]
fn init_rows_from_read_set_present() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 10), some(u64_portable(42)))],
        write_set_final: vec![],
        txs: vec![TxResult::success(
            vec![read_event(1, 0, 10, 42, 1, 0)],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let rows = &prepared_column(&prepared, t(1), c(0)).init_cells;
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].is_null);
    assert_eq!(rows[0].key.row, r(10));
}

#[test]
fn init_rows_from_read_set_null_are_canonical_zero() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 5), None)],
        write_set_final: vec![(ck(1, 0, 5), some(u64_portable(99)))],
        txs: vec![TxResult::success(
            vec![
                null_read_event(1, 0, 5, 1, 0),
                write_event(1, 0, 5, 99, 2, 0),
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let rows = &prepared_column(&prepared, t(1), c(0)).init_cells;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_null);
    assert_eq!(rows[0].value, u64_typed(0));
}

#[test]
fn init_rows_are_sorted_by_key() {
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 30), some(u64_portable(3))),
            (ck(1, 0, 10), some(u64_portable(1))),
            (ck(1, 0, 20), some(u64_portable(2))),
        ],
        write_set_final: vec![],
        txs: vec![TxResult::success(
            vec![
                read_event(1, 0, 30, 3, 1, 0),
                read_event(1, 0, 10, 1, 2, 0),
                read_event(1, 0, 20, 2, 3, 0),
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let rows = &prepared_column(&prepared, t(1), c(0)).init_cells;
    assert_eq!(
        rows.iter().map(|row| row.key.row.0).collect::<Vec<_>>(),
        vec![10, 20, 30]
    );
}

#[test]
fn access_rows_preserve_event_order_and_metadata() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), some(u64_portable(10)))],
        write_set_final: vec![(ck(1, 0, 1), some(u64_portable(30)))],
        txs: vec![
            TxResult::success(
                vec![
                    read_event(1, 0, 1, 10, 1, 0),
                    write_event(1, 0, 1, 20, 2, 0),
                ],
                vec![],
            ),
            TxResult::success(
                vec![
                    read_event(1, 0, 1, 20, 3, 1),
                    write_event(1, 0, 1, 30, 4, 1),
                ],
                vec![],
            ),
        ],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let access = &prepared_column(&prepared, t(1), c(0)).access_events;
    assert_eq!(access.len(), 4);
    assert!(!access[0].is_write);
    assert!(access[1].is_write);
    assert_eq!(access[0].tx_index, 0);
    assert_eq!(access[3].tx_index, 1);
    assert_eq!(access[0].time, 1);
    assert_eq!(access[3].time, 4);
}

#[test]
fn null_reads_remain_null_in_access_rows() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 5), None)],
        write_set_final: vec![],
        txs: vec![TxResult::success(
            vec![null_read_event(1, 0, 5, 1, 0)],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let access = &prepared_column(&prepared, t(1), c(0)).access_events;
    assert_eq!(access.len(), 1);
    assert!(access[0].is_null);
}

#[test]
fn writes_are_grouped_and_sorted() {
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![
            (ck(1, 0, 30), some(u64_portable(3))),
            (ck(1, 0, 10), some(u64_portable(1))),
            (ck(1, 0, 20), None),
        ],
        txs: vec![TxResult::success(
            vec![
                write_event(1, 0, 30, 3, 1, 0),
                write_event(1, 0, 10, 1, 2, 0),
                AccessEvent {
                    key: ck(1, 0, 20),
                    op: OpKind::Write,
                    value: u64_portable(0),
                    val_is_null: true,
                    time: 3,
                    effect_ordinal_in_tx: 2,
                },
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let writes = &prepared_column(&prepared, t(1), c(0)).writes;
    assert_eq!(
        writes.iter().map(|write| write.row.0).collect::<Vec<_>>(),
        vec![10, 20, 30]
    );
    assert!(writes[1].value.is_none());
}

#[test]
fn written_columns_include_effective_writes_only() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), some(u64_portable(10)))],
        write_set_final: vec![(ck(1, 1, 2), some(u64_portable(20)))],
        txs: vec![TxResult::success(
            vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 1, 2, 20, 2, 0),
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0, 1, 2])]);
    let prepared = prepare(
        &result,
        &schema,
        &[(t(1), c(0)), (t(1), c(1)), (t(1), c(2))],
    );

    assert!(!prepared_column(&prepared, t(1), c(0)).is_touched());
    assert!(prepared_column(&prepared, t(1), c(1)).is_touched());
    assert!(!prepared_column(&prepared, t(1), c(2)).is_touched());
    assert_eq!(prepared.columns.len(), 3);
    assert_eq!(prepared_column(&prepared, t(1), c(2)).type_id, TYPE_U64_ID);
    assert_eq!(
        prepared_column(&prepared, t(1), c(2)).encoding_profile_id,
        ENCODING_U64_ID
    );
}

#[test]
fn missing_schema_returns_error() {
    let preparer = make_preparer();
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), some(u64_portable(10)))],
        txs: vec![TxResult::success(
            vec![write_event(1, 0, 1, 10, 1, 0)],
            vec![],
        )],
    };
    let schema = schemas(vec![]);
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();

    assert!(
        preparer
            .prepare_execution_inputs(
                &result,
                &schema,
                &profile_catalog_for_schemas(&schema),
                &type_runtimes,
                &encoding_runtimes,
                [(t(1), c(0))].iter(),
            )
            .is_err()
    );
}

#[test]
fn written_column_missing_from_planned_columns_returns_error() {
    let preparer = make_preparer();
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), some(u64_portable(10)))],
        txs: vec![TxResult::success(
            vec![write_event(1, 0, 1, 10, 1, 0)],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    let err = preparer
        .prepare_execution_inputs(
            &result,
            &schema,
            &profile_catalog_for_schemas(&schema),
            &type_runtimes,
            &encoding_runtimes,
            std::iter::empty(),
        )
        .expect_err("missing planned column error");

    assert!(err.to_string().contains("not in planned columns"));
}

#[test]
fn custom_numeric_column_uses_registered_type_and_encoding() {
    let schema = custom_numeric_schema(7, 1);
    let catalog = custom_numeric_catalog(&schema);
    let type_runtimes = custom_numeric_type_runtimes();
    let encoding_runtimes = custom_numeric_encoding_runtimes();
    let result = BatchResult {
        read_set_old: vec![(ck(7, 1, 9), Some(custom_numeric_portable(11)))],
        write_set_final: vec![(ck(7, 1, 9), Some(custom_numeric_portable(17)))],
        txs: vec![TxResult::success(
            vec![
                AccessEvent {
                    key: ck(7, 1, 9),
                    op: OpKind::Read,
                    value: custom_numeric_portable(11),
                    val_is_null: false,
                    time: 1,
                    effect_ordinal_in_tx: 0,
                },
                AccessEvent {
                    key: ck(7, 1, 9),
                    op: OpKind::Write,
                    value: custom_numeric_portable(17),
                    val_is_null: false,
                    time: 2,
                    effect_ordinal_in_tx: 1,
                },
            ],
            vec![],
        )],
    };

    let prepared = make_preparer()
        .prepare_execution_inputs(
            &result,
            &schema,
            &catalog,
            &type_runtimes,
            &encoding_runtimes,
            [(t(7), c(1))].iter(),
        )
        .expect("prepare custom numeric column");

    let column = prepared_column(&prepared, t(7), c(1));
    assert_eq!(column.type_id, CUSTOM_NUMERIC_TYPE_ID);
    assert_eq!(column.encoding_profile_id, CUSTOM_NUMERIC_ENCODING_ID);
    assert_eq!(
        decode_custom_numeric(&column.init_cells[0].value).unwrap(),
        11
    );
    assert_eq!(
        decode_custom_numeric(&column.access_events[0].value).unwrap(),
        11
    );
    assert_eq!(
        decode_custom_numeric(column.writes[0].value.as_ref().unwrap()).unwrap(),
        17
    );
}

#[test]
fn mismatched_portable_type_id_fails_closed() {
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), Some(bytes32_portable([7; 32])))],
        txs: vec![TxResult::success(
            vec![AccessEvent {
                key: ck(1, 0, 1),
                op: OpKind::Write,
                value: bytes32_portable([7; 32]),
                val_is_null: false,
                time: 1,
                effect_ordinal_in_tx: 0,
            }],
            vec![],
        )],
    };

    let err = make_preparer()
        .prepare_execution_inputs(
            &result,
            &schema,
            &profile_catalog_for_schemas(&schema),
            &type_runtimes,
            &encoding_runtimes,
            [(t(1), c(0))].iter(),
        )
        .expect_err("mismatched portable type id must fail");
    assert!(
        err.to_string()
            .contains("does not match sealed column type")
    );
}

#[test]
fn missing_encoding_runtime_fails_closed() {
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = EncodingRuntimeRegistry::new();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(u64_portable(10)))],
        write_set_final: vec![],
        txs: vec![TxResult::success(
            vec![read_event(1, 0, 1, 10, 1, 0)],
            vec![],
        )],
    };

    let err = make_preparer()
        .prepare_execution_inputs(
            &result,
            &schema,
            &profile_catalog_for_schemas(&schema),
            &type_runtimes,
            &encoding_runtimes,
            [(t(1), c(0))].iter(),
        )
        .expect_err("missing encoding runtime must fail");
    assert!(
        err.to_string()
            .contains("references missing encoding runtime")
    );
}
