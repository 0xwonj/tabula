mod factory;
mod resolved_plan;
mod runtime_column;

pub mod builtins;

#[cfg(feature = "prove")]
pub(crate) use builtins::default_factories;
#[cfg(any(feature = "prove", feature = "verify"))]
pub(crate) use builtins::default_proof_factories;
pub use builtins::{SmtScheme, SsmcScheme};
pub(crate) use factory::ColumnSchemeFactory;
pub(crate) use resolved_plan::ResolvedColumnPlan;
pub(crate) use runtime_column::RuntimeColumn;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use tabula_artifact::SchemeDescriptor;
    use tabula_core::error::TabulaError;
    use tabula_core::{ColId, ColumnLayoutKind, RowKey, SchemeId, TableId, Value, ValueType};
    use tabula_ext::ExtError;
    use tabula_ir::{PropertyQuery, PropertyQueryKind};

    use crate::columns::{ColumnSchemeFactory, ResolvedColumnPlan, SmtScheme, SsmcScheme};

    fn ssmc_plan(required_property_query_kinds: BTreeSet<PropertyQueryKind>) -> ResolvedColumnPlan {
        ResolvedColumnPlan {
            table_id: TableId(0),
            col_id: ColId(0),
            scheme_id: SchemeId::SSMC,
            scheme_descriptor: SchemeDescriptor {
                layout_kind: ColumnLayoutKind::SSMC_V1,
                ..SchemeDescriptor::builtin_ssmc()
            },
            value_type: ValueType::U64,
            receives_commitment: true,
            required_property_query_kinds,
        }
    }

    fn smt_plan(required_property_query_kinds: BTreeSet<PropertyQueryKind>) -> ResolvedColumnPlan {
        ResolvedColumnPlan {
            table_id: TableId(0),
            col_id: ColId(0),
            scheme_id: SchemeId::SMT,
            scheme_descriptor: SchemeDescriptor {
                layout_kind: ColumnLayoutKind::SMT_V1,
                ..SchemeDescriptor::builtin_smt()
            },
            value_type: ValueType::U64,
            receives_commitment: true,
            required_property_query_kinds,
        }
    }

    #[test]
    fn ssmc_rejects_unsupported_minimum() {
        let mut required = BTreeSet::new();
        required.insert(PropertyQueryKind::Minimum);

        let Err(err) = SsmcScheme::<3>.build_runtime_column(&ssmc_plan(required)) else {
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
            .build_runtime_column(&ssmc_plan(BTreeSet::new()))
            .expect("prepare");

        let state = vec![
            (RowKey(5), Value::U64(50), false),
            (RowKey(10), Value::U64(100), false),
            (RowKey(20), Value::U64(200), false),
        ];

        let succ = prepared
            .as_ref()
            .resolve_property(&PropertyQuery::Successor { key: RowKey(10) }, &state)
            .expect("successor");
        assert_eq!(succ.key, Some(RowKey(20)));
        assert_eq!(succ.value, Value::U64(200));

        let pred = prepared
            .as_ref()
            .resolve_property(&PropertyQuery::Predecessor { key: RowKey(10) }, &state)
            .expect("predecessor");
        assert_eq!(pred.key, Some(RowKey(5)));
        assert_eq!(pred.value, Value::U64(50));
    }

    #[test]
    fn smt_rejects_structural_property_requirements_at_setup() {
        let mut required = BTreeSet::new();
        required.insert(PropertyQueryKind::Successor);

        let Err(err) = SmtScheme::<3>.build_runtime_column(&smt_plan(required)) else {
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
            .build_runtime_column(&smt_plan(BTreeSet::new()))
            .expect("prepare");

        assert!(prepared.supported_property_query_kinds().is_empty());

        let err = prepared
            .resolve_property(
                &PropertyQuery::Successor { key: RowKey(10) },
                &[(RowKey(10), Value::U64(100), false)],
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
