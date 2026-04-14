use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::SchemeId;
use tabula_ext::ExtError;
use tabula_ext::scheme::{ColumnBackendFactory, ColumnBackendSetup, MaterializedColumnBackend};

mod smt;
mod ssmc;

pub use smt::SmtScheme;
pub use ssmc::SsmcScheme;

#[derive(Clone, Copy, Debug)]
struct BuiltinSsmcScheme;

impl ColumnBackendFactory for BuiltinSsmcScheme {
    fn scheme_id(&self) -> SchemeId {
        SchemeId::SSMC
    }

    fn name(&self) -> &str {
        "ssmc"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        // Current StateShard hash-chain layout only admits W <= 5
        // because continuation inputs reserve 11 of Poseidon's 16 lanes.
        match setup.encoding_runtime.trace_width() {
            1 => SsmcScheme::<1>.materialize_backend(setup),
            2 => SsmcScheme::<2>.materialize_backend(setup),
            3 => SsmcScheme::<3>.materialize_backend(setup),
            4 => SsmcScheme::<4>.materialize_backend(setup),
            5 => SsmcScheme::<5>.materialize_backend(setup),
            width => Err(ExtError::validation(format!(
                "builtin ssmc backend does not support trace width {width}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BuiltinSmtScheme;

impl ColumnBackendFactory for BuiltinSmtScheme {
    fn scheme_id(&self) -> SchemeId {
        SchemeId::SMT
    }

    fn name(&self) -> &str {
        "smt"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        match setup.encoding_runtime.trace_width() {
            1 => SmtScheme::<1>.materialize_backend(setup),
            2 => SmtScheme::<2>.materialize_backend(setup),
            3 => SmtScheme::<3>.materialize_backend(setup),
            4 => SmtScheme::<4>.materialize_backend(setup),
            5 => SmtScheme::<5>.materialize_backend(setup),
            6 => SmtScheme::<6>.materialize_backend(setup),
            7 => SmtScheme::<7>.materialize_backend(setup),
            8 => SmtScheme::<8>.materialize_backend(setup),
            width => Err(ExtError::validation(format!(
                "builtin smt backend does not support trace width {width}"
            ))),
        }
    }
}

pub(crate) fn default_backend_factories() -> BTreeMap<SchemeId, Arc<dyn ColumnBackendFactory>> {
    let mut schemes: BTreeMap<SchemeId, Arc<dyn ColumnBackendFactory>> = BTreeMap::new();
    schemes.insert(SchemeId::SSMC, Arc::new(BuiltinSsmcScheme));
    schemes.insert(SchemeId::SMT, Arc::new(BuiltinSmtScheme));
    schemes
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use tabula_core::error::TabulaError;
    use tabula_core::{
        ColId, CommittedKey, CommittedKeyLayout, CommittedPropertyQuery, KeyComponentSchema,
        KeyOrderingFamily, PropertyQueryKind, SchemeProfileId, StateColumnContract,
        StateTableContract, TableId, TableKeyContract,
    };
    use tabula_ext::ExtError;
    use tabula_ext::scheme::{ColumnBackendFactory, ColumnBackendSetup};
    use tabula_profile::{
        ColumnProfile, CommitmentRole, ENCODING_BYTES32_ID, ENCODING_U64_ID, ProfileCatalog,
        SCHEME_PROFILE_SMT_ID, SCHEME_PROFILE_SSMC_ID, TYPE_BYTES32_ID, TYPE_U64_ID,
        builtin_catalog,
    };
    use tabula_types::{
        CommittedColumnEntry, EncodingRuntimeRegistry, TableKeyCodec, TypeRuntimeRegistry,
        u64_typed,
    };

    use super::{SmtScheme, SsmcScheme};

    fn test_key_codec(
        encoding_runtimes: &EncodingRuntimeRegistry,
        table: &StateTableContract,
    ) -> Arc<TableKeyCodec> {
        Arc::new(
            TableKeyCodec::from_contract(table.id, &table.key, encoding_runtimes)
                .expect("test key codec"),
        )
    }

    fn committed_u64_key(key_codec: &TableKeyCodec, value: u64) -> CommittedKey {
        key_codec
            .encode_tuple(&[u64_typed(value)])
            .expect("encode committed u64 key")
    }

    fn catalog_with_column(scheme_profile_id: SchemeProfileId) -> ProfileCatalog {
        let mut catalog = builtin_catalog().expect("built-in catalog");
        let ty = catalog.type_descriptor(TYPE_U64_ID).expect("type").clone();
        let encoding = catalog
            .encoding_profile(ENCODING_U64_ID)
            .expect("encoding")
            .clone();
        let scheme = catalog
            .scheme_profile(scheme_profile_id)
            .expect("scheme")
            .clone();
        catalog
            .register_column(
                ColumnProfile::new(
                    tabula_core::ColumnProfileId(0x9001 + scheme_profile_id.0),
                    format!("test_col_{}", scheme_profile_id.0),
                    None,
                    &ty,
                    &encoding,
                    &scheme,
                    CommitmentRole::IncludedInRoot,
                )
                .expect("column profile"),
            )
            .expect("register column");
        catalog
    }

    fn ssmc_setup(
        required_property_query_kinds: BTreeSet<PropertyQueryKind>,
    ) -> ColumnBackendSetup<'static> {
        let catalog = Box::leak(Box::new(catalog_with_column(SCHEME_PROFILE_SSMC_ID)));
        let profile = catalog
            .resolve_column_profile(tabula_core::ColumnProfileId(
                0x9001 + SCHEME_PROFILE_SSMC_ID.0,
            ))
            .expect("resolved profile");
        let type_runtimes = Box::leak(Box::new(TypeRuntimeRegistry::seeded().expect("types")));
        let encoding_runtimes = Box::leak(Box::new(
            EncodingRuntimeRegistry::seeded().expect("encodings"),
        ));
        let column = Box::leak(Box::new(StateColumnContract {
            id: ColId(0),
            name: "value".into(),
            ty: TYPE_U64_ID,
            column_profile_id: profile.column_profile.column_profile_id,
            required_property_queries: required_property_query_kinds,
        }));
        let table = Box::leak(Box::new(StateTableContract {
            id: TableId(0),
            name: "accounts".into(),
            key: TableKeyContract {
                components: vec![KeyComponentSchema {
                    symbol: "id".into(),
                    ty: TYPE_U64_ID,
                }],
                component_encoding_profile_ids: vec![ENCODING_U64_ID],
                ordering_family: KeyOrderingFamily::LexicographicByComponent,
                committed_layout: CommittedKeyLayout {
                    byte_width: 8,
                    fe_width: 3,
                },
            },
            columns: vec![column.clone()],
        }));
        ColumnBackendSetup {
            table,
            column,
            profile,
            type_runtime: type_runtimes.resolve(TYPE_U64_ID).expect("runtime").clone(),
            encoding_runtime: encoding_runtimes
                .resolve(ENCODING_U64_ID)
                .expect("encoding runtime")
                .clone(),
            key_codec: test_key_codec(encoding_runtimes, table),
        }
    }

    fn smt_setup(
        required_property_query_kinds: BTreeSet<PropertyQueryKind>,
    ) -> ColumnBackendSetup<'static> {
        let catalog = Box::leak(Box::new(catalog_with_column(SCHEME_PROFILE_SMT_ID)));
        let profile = catalog
            .resolve_column_profile(tabula_core::ColumnProfileId(
                0x9001 + SCHEME_PROFILE_SMT_ID.0,
            ))
            .expect("resolved profile");
        let type_runtimes = Box::leak(Box::new(TypeRuntimeRegistry::seeded().expect("types")));
        let encoding_runtimes = Box::leak(Box::new(
            EncodingRuntimeRegistry::seeded().expect("encodings"),
        ));
        let column = Box::leak(Box::new(StateColumnContract {
            id: ColId(0),
            name: "value".into(),
            ty: TYPE_U64_ID,
            column_profile_id: profile.column_profile.column_profile_id,
            required_property_queries: required_property_query_kinds,
        }));
        let table = Box::leak(Box::new(StateTableContract {
            id: TableId(0),
            name: "accounts".into(),
            key: TableKeyContract {
                components: vec![KeyComponentSchema {
                    symbol: "id".into(),
                    ty: TYPE_U64_ID,
                }],
                component_encoding_profile_ids: vec![ENCODING_U64_ID],
                ordering_family: KeyOrderingFamily::LexicographicByComponent,
                committed_layout: CommittedKeyLayout {
                    byte_width: 8,
                    fe_width: 3,
                },
            },
            columns: vec![column.clone()],
        }));
        ColumnBackendSetup {
            table,
            column,
            profile,
            type_runtime: type_runtimes.resolve(TYPE_U64_ID).expect("runtime").clone(),
            encoding_runtime: encoding_runtimes
                .resolve(ENCODING_U64_ID)
                .expect("encoding runtime")
                .clone(),
            key_codec: test_key_codec(encoding_runtimes, table),
        }
    }

    fn smt_setup_with_bytes32_key() -> ColumnBackendSetup<'static> {
        let catalog = Box::leak(Box::new(catalog_with_column(SCHEME_PROFILE_SMT_ID)));
        let profile = catalog
            .resolve_column_profile(tabula_core::ColumnProfileId(
                0x9001 + SCHEME_PROFILE_SMT_ID.0,
            ))
            .expect("resolved profile");
        let type_runtimes = Box::leak(Box::new(TypeRuntimeRegistry::seeded().expect("types")));
        let encoding_runtimes = Box::leak(Box::new(
            EncodingRuntimeRegistry::seeded().expect("encodings"),
        ));
        let column = Box::leak(Box::new(StateColumnContract {
            id: ColId(0),
            name: "value".into(),
            ty: TYPE_U64_ID,
            column_profile_id: profile.column_profile.column_profile_id,
            required_property_queries: BTreeSet::new(),
        }));
        let table = Box::leak(Box::new(StateTableContract {
            id: TableId(0),
            name: "accounts".into(),
            key: TableKeyContract {
                components: vec![KeyComponentSchema {
                    symbol: "id".into(),
                    ty: TYPE_BYTES32_ID,
                }],
                component_encoding_profile_ids: vec![ENCODING_BYTES32_ID],
                ordering_family: KeyOrderingFamily::LexicographicByComponent,
                committed_layout: CommittedKeyLayout {
                    byte_width: 32,
                    fe_width: 8,
                },
            },
            columns: vec![column.clone()],
        }));
        ColumnBackendSetup {
            table,
            column,
            profile,
            type_runtime: type_runtimes.resolve(TYPE_U64_ID).expect("runtime").clone(),
            encoding_runtime: encoding_runtimes
                .resolve(ENCODING_U64_ID)
                .expect("encoding runtime")
                .clone(),
            key_codec: test_key_codec(encoding_runtimes, table),
        }
    }

    #[test]
    fn ssmc_rejects_unsupported_minimum() {
        let mut required = BTreeSet::new();
        required.insert(PropertyQueryKind::Minimum);

        let Err(err) = SsmcScheme::<3>.materialize_backend(ssmc_setup(required)) else {
            panic!("minimum should be unsupported for SSMC");
        };

        match err {
            ExtError::Validation { detail } => {
                assert!(detail.contains("does not support property"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn ssmc_runtime_resolves_successor_and_predecessor() {
        let setup = ssmc_setup(BTreeSet::new());
        let key_codec = Arc::clone(&setup.key_codec);
        let prepared = SsmcScheme::<3>.materialize_backend(setup).expect("prepare");

        let state = vec![
            CommittedColumnEntry {
                key: committed_u64_key(key_codec.as_ref(), 5),
                value: u64_typed(50),
                is_null: false,
            },
            CommittedColumnEntry {
                key: committed_u64_key(key_codec.as_ref(), 10),
                value: u64_typed(100),
                is_null: false,
            },
            CommittedColumnEntry {
                key: committed_u64_key(key_codec.as_ref(), 20),
                value: u64_typed(200),
                is_null: false,
            },
        ];

        let succ = prepared
            .runtime_column
            .as_ref()
            .resolve_property(
                &CommittedPropertyQuery::Successor {
                    key: committed_u64_key(key_codec.as_ref(), 10),
                },
                &state,
            )
            .expect("successor");
        assert_eq!(succ.key, Some(committed_u64_key(key_codec.as_ref(), 20)));
        assert_eq!(succ.value, u64_typed(200));

        let pred = prepared
            .runtime_column
            .as_ref()
            .resolve_property(
                &CommittedPropertyQuery::Predecessor {
                    key: committed_u64_key(key_codec.as_ref(), 10),
                },
                &state,
            )
            .expect("predecessor");
        assert_eq!(pred.key, Some(committed_u64_key(key_codec.as_ref(), 5)));
        assert_eq!(pred.value, u64_typed(50));
    }

    #[test]
    fn ssmc_runtime_uses_key_codec_committed_order() {
        let setup = ssmc_setup(BTreeSet::new());
        let key_codec = Arc::clone(&setup.key_codec);
        let prepared = SsmcScheme::<3>.materialize_backend(setup).expect("prepare");

        let small = committed_u64_key(key_codec.as_ref(), 2);
        let large = committed_u64_key(key_codec.as_ref(), 1 << 30);
        let small_payload = key_codec
            .encode_padded_proof_payload(&small)
            .expect("small payload");
        let large_payload = key_codec
            .encode_padded_proof_payload(&large)
            .expect("large payload");
        assert_eq!(
            key_codec
                .compare(&small, &large)
                .expect("committed-key compare"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            key_codec
                .compare_padded_payloads(&small_payload, &large_payload)
                .expect("payload compare"),
            std::cmp::Ordering::Less
        );

        let state = vec![
            CommittedColumnEntry {
                key: large.clone(),
                value: u64_typed(900),
                is_null: false,
            },
            CommittedColumnEntry {
                key: small.clone(),
                value: u64_typed(100),
                is_null: false,
            },
        ];

        let successor = prepared
            .runtime_column
            .as_ref()
            .resolve_property(
                &CommittedPropertyQuery::Successor { key: small.clone() },
                &state,
            )
            .expect("successor");
        assert_eq!(successor.key, Some(large.clone()));
        assert_eq!(successor.value, u64_typed(900));

        let predecessor = prepared
            .runtime_column
            .as_ref()
            .resolve_property(&CommittedPropertyQuery::Predecessor { key: large }, &state)
            .expect("predecessor");
        assert_eq!(predecessor.key, Some(small));
        assert_eq!(predecessor.value, u64_typed(100));
    }

    #[test]
    fn smt_rejects_structural_property_requirements_at_setup() {
        let mut required = BTreeSet::new();
        required.insert(PropertyQueryKind::Successor);

        let Err(err) = SmtScheme::<3>.materialize_backend(smt_setup(required)) else {
            panic!("SMT property requirements should fail closed");
        };

        match err {
            ExtError::Validation { detail } => {
                assert!(detail.contains("does not support property query"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn smt_runtime_rejects_property_resolution() {
        let prepared = SmtScheme::<3>
            .materialize_backend(smt_setup(BTreeSet::new()))
            .expect("prepare");

        assert!(
            prepared
                .runtime_column
                .supported_property_query_kinds()
                .is_empty()
        );

        let err = prepared
            .runtime_column
            .resolve_property(
                &CommittedPropertyQuery::Successor {
                    key: CommittedKey(10u64.to_le_bytes().to_vec()),
                },
                &[CommittedColumnEntry {
                    key: CommittedKey(10u64.to_le_bytes().to_vec()),
                    value: u64_typed(100),
                    is_null: false,
                }],
            )
            .expect_err("SMT runtime should reject structural property queries");

        match err {
            TabulaError::InvalidIr(detail) => {
                assert!(detail.contains("does not implement property query"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn smt_rejects_non_locator_key_payload_contracts() {
        let Err(err) = SmtScheme::<3>.materialize_backend(smt_setup_with_bytes32_key()) else {
            panic!("bytes32 key payloads should be rejected by the built-in SMT locator");
        };

        match err {
            ExtError::Validation { detail } => {
                assert!(detail.contains("u64-decodable key payload locators"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
