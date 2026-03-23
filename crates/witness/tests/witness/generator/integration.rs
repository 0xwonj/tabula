use std::collections::BTreeMap;

use tabula_commitment::schemes::ssmc::SsmcList;
use tabula_commitment::{
    ColumnRootBinding, FieldHasher, NormalizedVerifierDigest,
    compute_column_root_binding_prefix_digest, compute_state_roots_from_bindings,
};
use tabula_core::{BatchResult, ColId, RootProfileId, RowKey, TableId, TxResult};
use tabula_testing::commitment::MockFieldHasher;
use tabula_types::builtins::encode_seeded_field_elements;
use tabula_types::{u64_portable, u64_typed};

use super::*;

fn build_binding(
    table: TableId,
    col: ColId,
    old_state: &SsmcList,
    writes: &[(RowKey, Option<Vec<<MockFieldHasher as FieldHasher>::F>>)],
    is_touched: bool,
) -> ColumnRootBinding {
    let com_old = old_state.proof_commitment().expect("old commitment");
    let is_empty_old = old_state.is_empty();
    let new_state = if is_touched {
        old_state.apply_writes(writes, &MockFieldHasher).0
    } else {
        old_state.clone()
    };
    let com_new = new_state.proof_commitment().expect("new commitment");
    let column_profile_hash = [table.0 as u8; 32];
    let binding_digest = compute_column_root_binding_prefix_digest(
        &MockFieldHasher,
        table,
        col,
        RootProfileId::SMT_V1,
        &column_profile_hash,
    );

    ColumnRootBinding {
        table,
        col,
        root_binding_family: RootProfileId::SMT_V1,
        column_profile_hash,
        binding_digest,
        old_digest: NormalizedVerifierDigest::new(com_old),
        new_digest: NormalizedVerifierDigest::new(com_new),
        is_empty_old,
        is_empty_new: new_state.is_empty(),
        is_touched,
    }
}

#[test]
fn state_root_deterministic_for_same_metadata() {
    let binding = build_binding(
        t(1),
        c(0),
        &column_state_with(1, 0, &[(1, 10)]).1,
        &[],
        false,
    );

    let roots_1 =
        compute_state_roots_from_bindings(&MockFieldHasher, std::slice::from_ref(&binding))
            .expect("compute roots 1");
    let roots_2 =
        compute_state_roots_from_bindings(&MockFieldHasher, std::slice::from_ref(&binding))
            .expect("compute roots 2");

    assert_eq!(roots_1, roots_2);
}

#[test]
fn state_root_changes_when_column_commitment_changes() {
    let old_state = column_state_with(1, 0, &[(1, 10)]).1;
    let unchanged = build_binding(t(1), c(0), &old_state, &[], false);
    let changed = build_binding(
        t(1),
        c(0),
        &old_state,
        &[(
            r(1),
            Some(encode_seeded_field_elements(&u64_typed(20)).unwrap()),
        )],
        true,
    );

    let roots_old =
        compute_state_roots_from_bindings(&MockFieldHasher, std::slice::from_ref(&unchanged))
            .expect("compute old roots");
    let roots_new =
        compute_state_roots_from_bindings(&MockFieldHasher, std::slice::from_ref(&changed))
            .expect("compute new roots");

    assert_ne!(roots_old.1, roots_new.1);
}

#[test]
fn state_root_handles_empty_to_non_empty_transition() {
    let binding = build_binding(
        t(1),
        c(0),
        &empty_column_state(1, 0).1,
        &[(
            r(1),
            Some(encode_seeded_field_elements(&u64_typed(1)).unwrap()),
        )],
        true,
    );

    let (old_root, new_root) =
        compute_state_roots_from_bindings(&MockFieldHasher, std::slice::from_ref(&binding))
            .expect("compute transition roots");

    assert_ne!(old_root, new_root);
}

#[test]
fn prepared_inputs_and_roots_cover_multiple_columns() {
    let preparer = make_preparer();
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 1), some(u64_portable(10))),
            (ck(1, 1, 1), some(u64_portable(20))),
        ],
        write_set_final: vec![
            (ck(1, 0, 1), some(u64_portable(15))),
            (ck(1, 1, 2), some(u64_portable(25))),
        ],
        txs: vec![TxResult::success(
            vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 1, 1, 20, 2, 0),
                write_event(1, 0, 1, 15, 3, 0),
                write_event(1, 1, 2, 25, 4, 0),
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0, 1])]);
    let profile_catalog = profile_catalog_for_schemas(&schema);
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    let prepared = preparer
        .prepare_execution_inputs(
            &result,
            &schema,
            &profile_catalog,
            &type_runtimes,
            &encoding_runtimes,
            [(t(1), c(0)), (t(1), c(1))].iter(),
        )
        .expect("prepared execution inputs");

    assert_eq!(prepared.columns.len(), 2);

    let states: BTreeMap<_, _> = [
        column_state_with(1, 0, &[(1, 10)]),
        column_state_with(1, 1, &[(1, 20)]),
    ]
    .into();
    let bindings: Vec<_> = states
        .into_iter()
        .map(|((table, col), state)| {
            let writes = prepared
                .column(table, col)
                .expect("prepared column")
                .writes
                .iter()
                .map(|write| {
                    (
                        write.row,
                        write
                            .value
                            .as_ref()
                            .map(encode_seeded_field_elements)
                            .transpose()
                            .expect("encode writes"),
                    )
                })
                .collect::<Vec<_>>();
            build_binding(
                table,
                col,
                &state,
                &writes,
                prepared
                    .column(table, col)
                    .expect("prepared column")
                    .is_touched(),
            )
        })
        .collect();
    let (old_root, new_root) =
        compute_state_roots_from_bindings(&MockFieldHasher, &bindings).expect("compute roots");

    assert_ne!(old_root, new_root);
    assert_eq!(bindings.len(), 2);
}
