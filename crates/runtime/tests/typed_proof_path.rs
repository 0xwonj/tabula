#![cfg(feature = "prove")]
#![allow(missing_docs)]

use std::cmp::Ordering;

use borsh::{from_slice, to_vec};
use p3_koala_bear::KoalaBear;

use tabula_artifact::{State, StateEntry, TransactionBatch, TransactionInput};
use tabula_compiler::{
    CompilerCatalogs, compile_program_source, register_program_definition_with_catalogs,
};
use tabula_core::error::TabulaError;
use tabula_core::{
    ColId, ColumnLayoutKind, EncodingProfileId, PortableValue, SchemeId, TableId, TypeId,
};
use tabula_ir::{PrecompileId, PrecompileSignature, PrecompileValueProfile};
use tabula_profile::{
    CanonicalNullEncoding, ENCODING_BYTES32_ID, EncodingClass, EncodingProfile, FieldFamily,
    GenericIrFamily, HostValueFamily, NullSemantics, SCHEME_PROFILE_SMT_ID, SCHEME_PROFILE_SSMC_ID,
    SemanticRegistry, TYPE_BYTES32_ID, TYPE_U64_ID, TranscriptSerialization, TypeCapabilities,
    TypeDescriptor, ZeroValueSpec, builtin_semantic_registry,
};
use tabula_runtime::{HostEnvironment, HostTypeRuntimes, ProveInput, TabulaRuntime, Verifier};
use tabula_testing::exec::compiled_program_from_artifact;
use tabula_types::builtins::{decode_seeded_field_elements, encode_seeded_field_elements};
use tabula_types::{
    ArithmeticOp, EncodingRuntime, TypeRuntime, TypedValue, bytes32_typed, u64_typed,
};

const DEFAULT_SENDER: &str = "0101010101010101010101010101010101010101010101010101010101010101";

const CUSTOM_NUMERIC_TYPE_ID: TypeId = TypeId(0xb101);
const CUSTOM_NUMERIC_ENCODING_ID: EncodingProfileId = EncodingProfileId(0xb101);
const CUSTOM_OPAQUE_TYPE_ID: TypeId = TypeId(0xb201);
const CUSTOM_OPAQUE_ENCODING_ID: EncodingProfileId = EncodingProfileId(0xb201);

fn numeric_descriptor() -> TypeDescriptor {
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
    .expect("numeric descriptor")
}

fn numeric_encoding(descriptor: &TypeDescriptor) -> EncodingProfile {
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
    .expect("numeric encoding")
}

fn opaque_descriptor() -> TypeDescriptor {
    TypeDescriptor::new(
        CUSTOM_OPAQUE_TYPE_ID,
        "digest32",
        None,
        HostValueFamily::Bytes { len: 32 },
        GenericIrFamily::EqOnly,
        TypeCapabilities {
            equality: true,
            ordering: false,
            arithmetic: false,
        },
        ZeroValueSpec::ZeroBytes { len: 32 },
        NullSemantics::NullableWithCanonicalZero,
    )
    .expect("opaque descriptor")
}

fn opaque_encoding(descriptor: &TypeDescriptor) -> EncodingProfile {
    EncodingProfile::new(
        CUSTOM_OPAQUE_ENCODING_ID,
        "digest32_kb8",
        None,
        descriptor,
        EncodingClass::FieldElementArray,
        FieldFamily::KoalaBear31,
        8,
        CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
        TranscriptSerialization::FieldElementsWithNullFlag,
        false,
    )
    .expect("opaque encoding")
}

fn numeric_registry() -> SemanticRegistry {
    let mut registry = builtin_semantic_registry().expect("built-in semantic registry");
    let descriptor = numeric_descriptor();
    let encoding = numeric_encoding(&descriptor);
    registry
        .register_type_descriptor(descriptor)
        .expect("register numeric descriptor");
    registry
        .register_type_name("nonce64", CUSTOM_NUMERIC_TYPE_ID)
        .expect("register numeric type name");
    registry
        .register_encoding_profile(encoding)
        .expect("register numeric encoding");
    registry
        .register_default_encoding(CUSTOM_NUMERIC_TYPE_ID, CUSTOM_NUMERIC_ENCODING_ID)
        .expect("register numeric default encoding");
    registry
        .register_default_scheme_profile(
            SchemeId::SSMC,
            CUSTOM_NUMERIC_ENCODING_ID,
            SCHEME_PROFILE_SSMC_ID,
        )
        .expect("register numeric ssmc mapping");
    registry.validate().expect("numeric registry");
    registry
}

fn opaque_registry() -> SemanticRegistry {
    let mut registry = builtin_semantic_registry().expect("built-in semantic registry");
    let descriptor = opaque_descriptor();
    let encoding = opaque_encoding(&descriptor);
    registry
        .register_type_descriptor(descriptor)
        .expect("register opaque descriptor");
    registry
        .register_type_name("digest32", CUSTOM_OPAQUE_TYPE_ID)
        .expect("register opaque type name");
    registry
        .register_encoding_profile(encoding)
        .expect("register opaque encoding");
    registry
        .register_default_encoding(CUSTOM_OPAQUE_TYPE_ID, CUSTOM_OPAQUE_ENCODING_ID)
        .expect("register opaque default encoding");
    registry
        .register_default_scheme_profile(
            SchemeId::SMT,
            CUSTOM_OPAQUE_ENCODING_ID,
            SCHEME_PROFILE_SMT_ID,
        )
        .expect("register opaque smt mapping");
    registry.validate().expect("opaque registry");
    registry
}

fn artifact_from_retyped_source(
    source: &str,
    replacement_type_id: TypeId,
    registry: &SemanticRegistry,
) -> tabula_artifact::Artifact {
    let mut definition = compile_program_source(source).expect("compile source");
    for schema in &mut definition.table_schemas {
        for column in &mut schema.columns {
            column.type_id = replacement_type_id;
        }
    }
    for tx in &mut definition.tx_types {
        for param in &mut tx.param_schema {
            param.type_id = replacement_type_id;
        }
    }
    let catalogs = CompilerCatalogs::standard()
        .with_semantic_registry(registry.clone())
        .expect("semantic registry");
    register_program_definition_with_catalogs(&definition, &catalogs)
        .expect("register retyped program")
        .into_artifact()
}

fn numeric_artifact() -> tabula_artifact::Artifact {
    artifact_from_retyped_source(
        "\
table balances {
    amount: u64 @scheme(0)
}

tx bump(delta: u64) {
    let current = balances[0].amount
    balances[0].amount = current + delta
}
",
        CUSTOM_NUMERIC_TYPE_ID,
        &numeric_registry(),
    )
}

fn opaque_artifact() -> tabula_artifact::Artifact {
    artifact_from_retyped_source(
        "\
table roots {
    digest: bytes32 @scheme(1)
}

tx noop() {}
",
        CUSTOM_OPAQUE_TYPE_ID,
        &opaque_registry(),
    )
}

fn numeric_portable(value: u64) -> PortableValue {
    PortableValue::new(
        CUSTOM_NUMERIC_TYPE_ID,
        to_vec(&value).expect("numeric portable"),
    )
}

fn opaque_portable(value: [u8; 32]) -> PortableValue {
    PortableValue::new(
        CUSTOM_OPAQUE_TYPE_ID,
        to_vec(&value).expect("opaque portable"),
    )
}

fn numeric_state(value: u64) -> State {
    State {
        cells: vec![StateEntry {
            table: 0,
            row: 0,
            col: 0,
            value: Some(numeric_portable(value)),
        }],
    }
}

fn opaque_state(value: [u8; 32]) -> State {
    State {
        cells: vec![StateEntry {
            table: 0,
            row: 0,
            col: 0,
            value: Some(opaque_portable(value)),
        }],
    }
}

fn single_tx_batch(portable: PortableValue) -> TransactionBatch {
    TransactionBatch {
        transactions: vec![TransactionInput {
            tx_type: 0,
            params: vec![portable],
            sender: DEFAULT_SENDER.to_string(),
            nonce: 0,
        }],
    }
}

fn no_param_batch() -> TransactionBatch {
    TransactionBatch {
        transactions: vec![TransactionInput {
            tx_type: 0,
            params: vec![],
            sender: DEFAULT_SENDER.to_string(),
            nonce: 0,
        }],
    }
}

fn decode_u64_payload(value: &TypedValue) -> Result<u64, TabulaError> {
    from_slice(value.payload())
        .map_err(|err| TabulaError::BorshEncodingError(format!("u64 payload decode failed: {err}")))
}

fn decode_bytes32_payload(value: &TypedValue) -> Result<[u8; 32], TabulaError> {
    from_slice(value.payload()).map_err(|err| {
        TabulaError::BorshEncodingError(format!("bytes32 payload decode failed: {err}"))
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
        if value.type_id() != CUSTOM_NUMERIC_TYPE_ID {
            return Err(TabulaError::TypeMismatch {
                expected: format!("type {}", CUSTOM_NUMERIC_TYPE_ID.0),
                actual: format!("type {}", value.type_id().0),
            });
        }
        let _ = decode_u64_payload(value)?;
        Ok(())
    }

    fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError> {
        Ok(decode_u64_payload(lhs)? == decode_u64_payload(rhs)?)
    }

    fn cmp_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<Ordering, TabulaError> {
        Ok(decode_u64_payload(lhs)?.cmp(&decode_u64_payload(rhs)?))
    }

    fn apply_arithmetic(
        &self,
        op: ArithmeticOp,
        lhs: &TypedValue,
        rhs: &TypedValue,
    ) -> Result<TypedValue, TabulaError> {
        let lhs = decode_u64_payload(lhs)?;
        let rhs = decode_u64_payload(rhs)?;
        let value = match op {
            ArithmeticOp::Add => lhs.checked_add(rhs),
            ArithmeticOp::Sub => lhs.checked_sub(rhs),
            ArithmeticOp::Mul => lhs.checked_mul(rhs),
        }
        .ok_or(TabulaError::ArithmeticOverflow)?;
        Ok(TypedValue::new(
            CUSTOM_NUMERIC_TYPE_ID,
            to_vec(&value).expect("numeric payload"),
        ))
    }

    fn divmod(
        &self,
        lhs: &TypedValue,
        rhs: &TypedValue,
    ) -> Result<(TypedValue, TypedValue), TabulaError> {
        let lhs = decode_u64_payload(lhs)?;
        let rhs = decode_u64_payload(rhs)?;
        if rhs == 0 {
            return Err(TabulaError::DivisionByZero);
        }
        Ok((
            TypedValue::new(
                CUSTOM_NUMERIC_TYPE_ID,
                to_vec(&(lhs / rhs)).expect("quotient payload"),
            ),
            TypedValue::new(
                CUSTOM_NUMERIC_TYPE_ID,
                to_vec(&(lhs % rhs)).expect("remainder payload"),
            ),
        ))
    }

    fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError> {
        Ok(format!("{}nonce64", decode_u64_payload(value)?))
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
        encode_seeded_field_elements(&u64_typed(decode_u64_payload(value)?))
    }

    fn decode_field_elements(
        &self,
        field_elements: &[KoalaBear],
    ) -> Result<TypedValue, TabulaError> {
        let builtin = decode_seeded_field_elements(TYPE_U64_ID, field_elements)?;
        Ok(TypedValue::new(
            CUSTOM_NUMERIC_TYPE_ID,
            to_vec(&decode_u64_payload(&builtin)?).expect("numeric payload"),
        ))
    }

    fn encode_transcript_atoms(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        self.encode_field_elements(value)
    }

    fn trace_width(&self) -> usize {
        self.descriptor.width as usize
    }
}

#[derive(Clone)]
struct CustomOpaqueTypeRuntime {
    descriptor: TypeDescriptor,
}

impl TypeRuntime for CustomOpaqueTypeRuntime {
    fn type_id(&self) -> TypeId {
        CUSTOM_OPAQUE_TYPE_ID
    }

    fn descriptor(&self) -> &TypeDescriptor {
        &self.descriptor
    }

    fn zero_typed(&self) -> TypedValue {
        TypedValue::new(
            CUSTOM_OPAQUE_TYPE_ID,
            to_vec(&[0u8; 32]).expect("opaque zero payload"),
        )
    }

    fn encode_portable(&self, value: &TypedValue) -> Result<PortableValue, TabulaError> {
        self.validate(value)?;
        Ok(PortableValue::new(
            CUSTOM_OPAQUE_TYPE_ID,
            value.payload().to_vec(),
        ))
    }

    fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError> {
        if value.type_id() != CUSTOM_OPAQUE_TYPE_ID {
            return Err(TabulaError::TypeMismatch {
                expected: format!("type {}", CUSTOM_OPAQUE_TYPE_ID.0),
                actual: format!("type {}", value.type_id().0),
            });
        }
        let typed = TypedValue::new(CUSTOM_OPAQUE_TYPE_ID, value.payload().to_vec());
        self.validate(&typed)?;
        Ok(typed)
    }

    fn validate(&self, value: &TypedValue) -> Result<(), TabulaError> {
        if value.type_id() != CUSTOM_OPAQUE_TYPE_ID {
            return Err(TabulaError::TypeMismatch {
                expected: format!("type {}", CUSTOM_OPAQUE_TYPE_ID.0),
                actual: format!("type {}", value.type_id().0),
            });
        }
        let _ = decode_bytes32_payload(value)?;
        Ok(())
    }

    fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError> {
        Ok(decode_bytes32_payload(lhs)? == decode_bytes32_payload(rhs)?)
    }

    fn cmp_value(&self, _lhs: &TypedValue, _rhs: &TypedValue) -> Result<Ordering, TabulaError> {
        Err(TabulaError::TypeMismatch {
            expected: "ordered type".to_string(),
            actual: self.descriptor.display_name.clone(),
        })
    }

    fn apply_arithmetic(
        &self,
        _op: ArithmeticOp,
        _lhs: &TypedValue,
        _rhs: &TypedValue,
    ) -> Result<TypedValue, TabulaError> {
        Err(TabulaError::TypeMismatch {
            expected: "arithmetic type".to_string(),
            actual: self.descriptor.display_name.clone(),
        })
    }

    fn divmod(
        &self,
        _lhs: &TypedValue,
        _rhs: &TypedValue,
    ) -> Result<(TypedValue, TypedValue), TabulaError> {
        Err(TabulaError::TypeMismatch {
            expected: "divmod-capable type".to_string(),
            actual: self.descriptor.display_name.clone(),
        })
    }

    fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError> {
        let bytes = decode_bytes32_payload(value)?;
        Ok(format!(
            "0x{:02x}{:02x}{:02x}{:02x}..{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[31]
        ))
    }
}

#[derive(Clone)]
struct CustomOpaqueEncodingRuntime {
    descriptor: EncodingProfile,
}

impl EncodingRuntime for CustomOpaqueEncodingRuntime {
    fn encoding_profile_id(&self) -> EncodingProfileId {
        CUSTOM_OPAQUE_ENCODING_ID
    }

    fn descriptor(&self) -> &EncodingProfile {
        &self.descriptor
    }

    fn encode_field_elements(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        encode_seeded_field_elements(&bytes32_typed(decode_bytes32_payload(value)?))
    }

    fn decode_field_elements(
        &self,
        field_elements: &[KoalaBear],
    ) -> Result<TypedValue, TabulaError> {
        let builtin = decode_seeded_field_elements(TYPE_BYTES32_ID, field_elements)?;
        Ok(TypedValue::new(
            CUSTOM_OPAQUE_TYPE_ID,
            to_vec(&decode_bytes32_payload(&builtin)?).expect("opaque payload"),
        ))
    }

    fn encode_transcript_atoms(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        self.encode_field_elements(value)
    }

    fn trace_width(&self) -> usize {
        self.descriptor.width as usize
    }
}

fn host_types_with_custom_numeric() -> HostTypeRuntimes {
    HostTypeRuntimes::standard()
        .with_type_runtime(CustomNumericTypeRuntime {
            descriptor: numeric_descriptor(),
        })
        .expect("register numeric type runtime")
        .with_encoding_runtime(CustomNumericEncodingRuntime {
            descriptor: numeric_encoding(&numeric_descriptor()),
        })
        .expect("register numeric encoding runtime")
}

fn host_types_with_custom_opaque() -> HostTypeRuntimes {
    HostTypeRuntimes::standard()
        .with_type_runtime(CustomOpaqueTypeRuntime {
            descriptor: opaque_descriptor(),
        })
        .expect("register opaque type runtime")
        .with_encoding_runtime(CustomOpaqueEncodingRuntime {
            descriptor: opaque_encoding(&opaque_descriptor()),
        })
        .expect("register opaque encoding runtime")
}

#[test]
fn proof_path_proves_and_verifies_custom_numeric_type_through_ssmc() {
    let artifact = numeric_artifact();
    let compiled = compiled_program_from_artifact(&artifact);
    let resolved = compiled
        .resolve_column_profile(TableId(0), ColId(0))
        .expect("resolve numeric column");
    assert_eq!(resolved.type_descriptor.type_id, CUSTOM_NUMERIC_TYPE_ID);
    assert_eq!(
        resolved.encoding_profile.encoding_profile_id,
        CUSTOM_NUMERIC_ENCODING_ID
    );
    assert_eq!(resolved.proof_layout_family(), ColumnLayoutKind::SSMC_V1);

    let host_environment =
        HostEnvironment::standard().with_type_runtimes(host_types_with_custom_numeric());
    let runtime = TabulaRuntime::builder(compiled)
        .with_host_environment(host_environment.clone())
        .build()
        .expect("runtime");

    let state = numeric_state(7);
    let batch = single_tx_batch(numeric_portable(8));
    let executed = runtime.execute(&state, &batch).expect("execution");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof");

    let verifier = Verifier::builder(artifact)
        .with_host_environment(host_environment)
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("verification");
}

#[test]
fn proof_path_proves_and_verifies_custom_opaque_type_through_smt() {
    let artifact = opaque_artifact();
    let compiled = compiled_program_from_artifact(&artifact);
    let resolved = compiled
        .resolve_column_profile(TableId(0), ColId(0))
        .expect("resolve opaque column");
    assert_eq!(resolved.type_descriptor.type_id, CUSTOM_OPAQUE_TYPE_ID);
    assert_eq!(
        resolved.encoding_profile.encoding_profile_id,
        CUSTOM_OPAQUE_ENCODING_ID
    );
    assert_eq!(resolved.proof_layout_family(), ColumnLayoutKind::SMT_V1);

    let host_environment =
        HostEnvironment::standard().with_type_runtimes(host_types_with_custom_opaque());
    let runtime = TabulaRuntime::builder(compiled)
        .with_host_environment(host_environment.clone())
        .build()
        .expect("runtime");

    let state = opaque_state([1u8; 32]);
    let batch = no_param_batch();
    let executed = runtime.execute(&state, &batch).expect("execution");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof");

    let verifier = Verifier::builder(artifact)
        .with_host_environment(host_environment)
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("verification");
}

#[test]
fn compiler_catalogs_reject_precompile_io_wider_than_execution_width() {
    let descriptor = tabula_artifact::PrecompileDescriptor::new(
        PrecompileId(0x0091),
        1,
        PrecompileSignature::new(
            vec![],
            vec![PrecompileValueProfile {
                type_id: TYPE_BYTES32_ID,
                encoding_profile_id: ENCODING_BYTES32_ID,
            }],
        ),
        [0x91; 32],
    );

    let err = CompilerCatalogs::standard()
        .with_precompile_descriptor(descriptor)
        .expect_err("wide precompile descriptor must fail at catalog registration");
    assert!(
        err.to_string()
            .contains("generic execution lane only supports width 3"),
        "unexpected error: {err}",
    );
}
