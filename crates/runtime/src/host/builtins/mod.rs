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

    use tabula_core::PropertyQueryKind;
    use tabula_core::error::TabulaError;
    use tabula_core::{ColId, RowKey, SchemeProfileId, TableId};
    use tabula_ext::ExtError;
    use tabula_ext::scheme::{ColumnBackendFactory, ColumnBackendSetup};
    use tabula_ir::{StatePropertyQuery as PropertyQuery, ValueRef, ValueTupleRef};
    use tabula_profile::{
        ColumnProfile, CommitmentRole, ENCODING_U64_ID, ProfileCatalog, SCHEME_PROFILE_SMT_ID,
        SCHEME_PROFILE_SSMC_ID, TYPE_U64_ID, builtin_catalog,
    };
    use tabula_types::{
        EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedColumnEntry, u64_portable, u64_typed,
    };

    use super::{SmtScheme, SsmcScheme};

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
        let required = Box::leak(Box::new(required_property_query_kinds));
        let type_runtimes = Box::leak(Box::new(TypeRuntimeRegistry::seeded().expect("types")));
        let encoding_runtimes = Box::leak(Box::new(
            EncodingRuntimeRegistry::seeded().expect("encodings"),
        ));
        ColumnBackendSetup {
            table_id: TableId(0),
            col_id: ColId(0),
            profile,
            type_runtime: type_runtimes.resolve(TYPE_U64_ID).expect("runtime").clone(),
            encoding_runtime: encoding_runtimes
                .resolve(ENCODING_U64_ID)
                .expect("encoding runtime")
                .clone(),
            required_property_query_kinds: required,
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
        let required = Box::leak(Box::new(required_property_query_kinds));
        let type_runtimes = Box::leak(Box::new(TypeRuntimeRegistry::seeded().expect("types")));
        let encoding_runtimes = Box::leak(Box::new(
            EncodingRuntimeRegistry::seeded().expect("encodings"),
        ));
        ColumnBackendSetup {
            table_id: TableId(0),
            col_id: ColId(0),
            profile,
            type_runtime: type_runtimes.resolve(TYPE_U64_ID).expect("runtime").clone(),
            encoding_runtime: encoding_runtimes
                .resolve(ENCODING_U64_ID)
                .expect("encoding runtime")
                .clone(),
            required_property_query_kinds: required,
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
        let prepared = SsmcScheme::<3>
            .materialize_backend(ssmc_setup(BTreeSet::new()))
            .expect("prepare");

        let state = vec![
            TypedColumnEntry {
                row_key: RowKey(5),
                value: u64_typed(50),
                is_null: false,
            },
            TypedColumnEntry {
                row_key: RowKey(10),
                value: u64_typed(100),
                is_null: false,
            },
            TypedColumnEntry {
                row_key: RowKey(20),
                value: u64_typed(200),
                is_null: false,
            },
        ];

        let succ = prepared
            .runtime_column
            .as_ref()
            .resolve_property(
                &PropertyQuery::Successor {
                    key: ValueTupleRef(vec![ValueRef::Literal(u64_portable(10))]),
                },
                &state,
            )
            .expect("successor");
        assert_eq!(succ.key, Some(RowKey(20)));
        assert_eq!(succ.value, u64_typed(200));

        let pred = prepared
            .runtime_column
            .as_ref()
            .resolve_property(
                &PropertyQuery::Predecessor {
                    key: ValueTupleRef(vec![ValueRef::Literal(u64_portable(10))]),
                },
                &state,
            )
            .expect("predecessor");
        assert_eq!(pred.key, Some(RowKey(5)));
        assert_eq!(pred.value, u64_typed(50));
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
                &PropertyQuery::Successor {
                    key: ValueTupleRef(vec![ValueRef::Literal(u64_portable(10))]),
                },
                &[TypedColumnEntry {
                    row_key: RowKey(10),
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
}
