mod factory;
mod plan;
#[cfg(feature = "prove")]
mod proof_input;
mod runtime_column;
mod views;

pub mod builtins;

pub(crate) use builtins::default_factories;
pub use builtins::{SmtScheme, SsmcScheme};
pub use factory::ColumnSchemeFactory;
pub use plan::ColumnPlan;
#[cfg(feature = "prove")]
pub use proof_input::ProofInputBuilder;
pub use runtime_column::RuntimeColumn;
pub use views::ColumnViews;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use tabula_core::{ColId, RowKey, SchemeId, TableId, Value, ValueType};
    use tabula_ir::{PropertyQuery, PropertyQueryKind};
    use tabula_machine::SetupError;

    use crate::columns::{ColumnPlan, ColumnSchemeFactory, SmtScheme, SsmcScheme};

    #[test]
    fn ssmc_rejects_unsupported_minimum() {
        let mut required = BTreeSet::new();
        required.insert(PropertyQueryKind::Minimum);

        let err = SsmcScheme::<3>
            .build_column(ColumnPlan {
                table_id: TableId(0),
                col_id: ColId(0),
                scheme_id: SchemeId::SSMC,
                value_type: ValueType::U64,
                receives_commitment: true,
                required_property_query_kinds: required,
            })
            .expect_err("minimum should be unsupported for SSMC");

        match err {
            SetupError::SetupFailed(detail) => {
                assert!(detail.contains("does not support property"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn ssmc_runtime_resolves_successor_and_predecessor() {
        let prepared = SsmcScheme::<3>
            .build_column(ColumnPlan {
                table_id: TableId(0),
                col_id: ColId(0),
                scheme_id: SchemeId::SSMC,
                value_type: ValueType::U64,
                receives_commitment: true,
                required_property_query_kinds: BTreeSet::new(),
            })
            .expect("prepare");

        let state = vec![
            (RowKey(5), Value::U64(50), false),
            (RowKey(10), Value::U64(100), false),
            (RowKey(20), Value::U64(200), false),
        ];

        let succ = prepared
            .runtime()
            .resolve_property(
                &PropertyQuery::Successor { key: RowKey(10) },
                &state,
            )
            .expect("successor");
        assert_eq!(succ.key, Some(RowKey(20)));
        assert_eq!(succ.value, Value::U64(200));

        let pred = prepared
            .runtime()
            .resolve_property(
                &PropertyQuery::Predecessor { key: RowKey(10) },
                &state,
            )
            .expect("predecessor");
        assert_eq!(pred.key, Some(RowKey(5)));
        assert_eq!(pred.value, Value::U64(50));
    }

    #[test]
    fn smt_rejects_unimplemented_proving_support() {
        let err = SmtScheme::<3>
            .build_column(ColumnPlan {
                table_id: TableId(0),
                col_id: ColId(0),
                scheme_id: SchemeId::SMT,
                value_type: ValueType::U64,
                receives_commitment: false,
                required_property_query_kinds: BTreeSet::new(),
            })
            .expect_err("SMT proving support is not implemented");

        match err {
            SetupError::SetupFailed(detail) => {
                assert!(detail.contains("does not implement proving support"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
