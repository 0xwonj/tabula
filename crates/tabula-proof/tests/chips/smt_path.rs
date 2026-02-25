//! Tests for SmtColPathChip and SmtTablePathChip.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use tabula_commitment::NativeDigest;

use tabula_proof::air::{borrow_cols, borrow_cols_mut};
use tabula_proof::chips::smt_path::air::{
    SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET, SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
    SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET,
};
use tabula_proof::chips::smt_path::air::{SmtColPathChip, SmtTablePathChip};
use tabula_proof::chips::smt_path::columns::{
    SMT_COL_PATH_WIDTH, SMT_TABLE_PATH_WIDTH, SmtPathCols, SmtTablePathCols,
};
use tabula_proof::chips::smt_path::trace::{
    SmtPathWitness, SmtTablePathWitness, generate_smt_col_path_trace, generate_smt_table_path_trace,
};
use tabula_proof::debug::{
    debug_check, debug_check_with_public_values, evaluate_chip, evaluate_chip_with_public_values,
};

// ── Width ──

#[test]
fn smt_col_path_width() {
    assert_eq!(SMT_COL_PATH_WIDTH, 82);
}

#[test]
fn smt_table_path_width() {
    assert_eq!(SMT_TABLE_PATH_WIDTH, 83);
}

// ── Helpers ──

fn make_path_bits(key: u32, depth: usize) -> Vec<bool> {
    (0..depth).map(|i| (key >> i) & 1 == 1).collect()
}

fn zero_siblings(depth: usize) -> Vec<NativeDigest> {
    vec![NativeDigest::ZERO; depth]
}

fn nonzero_digest(seed: u32) -> NativeDigest {
    NativeDigest(core::array::from_fn(|i| BabyBear::new(seed + i as u32)))
}

fn table_path_public_values(trace: &p3_matrix::dense::RowMajorMatrix<BabyBear>) -> Vec<BabyBear> {
    let mut pvs = vec![BabyBear::ZERO; SMT_TABLE_PATH_NUM_PUBLIC_VALUES];
    for i in 0..trace.height() {
        let row = trace.row_slice(i).expect("row exists");
        let cols: &SmtTablePathCols<BabyBear> = borrow_cols(&row);
        if cols.base.is_real == BabyBear::ONE && cols.base.is_root == BabyBear::ONE {
            pvs[SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET..(SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET + 8)]
                .copy_from_slice(&cols.base.old_parent);
            pvs[SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET..(SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET + 8)]
                .copy_from_slice(&cols.base.new_parent);
            break;
        }
    }
    pvs
}

// ── SmtColPathChip ──

#[test]
fn col_path_single_path_depth_4() {
    let witness = SmtPathWitness {
        table_id: 1,
        key: 3, // binary: 11 → bits[0]=1, bits[1]=1, bits[2]=0, bits[3]=0
        old_leaf: nonzero_digest(10),
        new_leaf: nonzero_digest(20),
        siblings: zero_siblings(4),
        path_bits: make_path_bits(3, 4),
    };
    let trace = generate_smt_col_path_trace(&[witness]);
    debug_check(&SmtColPathChip, &trace).expect("single col path should pass");
}

#[test]
fn col_path_two_paths_same_table() {
    let w1 = SmtPathWitness {
        table_id: 1,
        key: 0,
        old_leaf: nonzero_digest(10),
        new_leaf: nonzero_digest(20),
        siblings: zero_siblings(3),
        path_bits: make_path_bits(0, 3),
    };
    let w2 = SmtPathWitness {
        table_id: 1,
        key: 5,
        old_leaf: nonzero_digest(30),
        new_leaf: nonzero_digest(40),
        siblings: zero_siblings(3),
        path_bits: make_path_bits(5, 3),
    };
    let trace = generate_smt_col_path_trace(&[w1, w2]);
    debug_check(&SmtColPathChip, &trace).expect("two col paths should pass");
}

#[test]
fn col_path_different_tables() {
    let w1 = SmtPathWitness {
        table_id: 1,
        key: 0,
        old_leaf: nonzero_digest(10),
        new_leaf: nonzero_digest(20),
        siblings: zero_siblings(3),
        path_bits: make_path_bits(0, 3),
    };
    let w2 = SmtPathWitness {
        table_id: 2,
        key: 1,
        old_leaf: nonzero_digest(30),
        new_leaf: nonzero_digest(40),
        siblings: zero_siblings(3),
        path_bits: make_path_bits(1, 3),
    };
    let trace = generate_smt_col_path_trace(&[w1, w2]);
    debug_check(&SmtColPathChip, &trace).expect("different table paths should pass");
}

#[test]
fn col_path_empty_trace() {
    let trace = generate_smt_col_path_trace(&[]);
    debug_check(&SmtColPathChip, &trace).expect("empty trace should pass");
}

#[test]
fn col_path_records_c15_and_c16_interactions() {
    let witness = SmtPathWitness {
        table_id: 1,
        key: 0,
        old_leaf: nonzero_digest(10),
        new_leaf: nonzero_digest(20),
        siblings: zero_siblings(3),
        path_bits: make_path_bits(0, 3),
    };
    let trace = generate_smt_col_path_trace(&[witness]);
    let record = evaluate_chip("SmtColPath", &SmtColPathChip, &trace).unwrap();

    use tabula_proof::air::interaction::{InteractionDirection, InteractionKind};

    // C15 SmtLeafDigest receives (at leaf level)
    let c15: Vec<_> = record
        .interactions
        .iter()
        .filter(|i| {
            i.kind == InteractionKind::SmtLeafDigest
                && i.direction == InteractionDirection::Receive
                && i.multiplicity != BabyBear::ZERO
        })
        .collect();
    assert_eq!(c15.len(), 1, "should have 1 C15 receive (at leaf)");

    // C16 SmtTableRoot sends (at root level)
    let c16: Vec<_> = record
        .interactions
        .iter()
        .filter(|i| {
            i.kind == InteractionKind::SmtTableRoot
                && i.direction == InteractionDirection::Send
                && i.multiplicity != BabyBear::ZERO
        })
        .collect();
    assert_eq!(c16.len(), 1, "should have 1 C16 send (at root)");

    // C5 PoseidonPerm sends (2 per real row, 3 real rows × 2 = 6)
    let c5: Vec<_> = record
        .interactions
        .iter()
        .filter(|i| {
            i.kind == InteractionKind::PoseidonPermutation
                && i.direction == InteractionDirection::Send
                && i.multiplicity != BabyBear::ZERO
        })
        .collect();
    assert_eq!(c5.len(), 6, "should have 6 C5 sends (3 levels × 2 trees)");
}

// ── SmtTablePathChip ──

#[test]
fn table_path_single_path() {
    let witness = SmtTablePathWitness {
        path: SmtPathWitness {
            table_id: 1,
            key: 1,
            old_leaf: nonzero_digest(10),
            new_leaf: nonzero_digest(20),
            siblings: zero_siblings(4),
            path_bits: make_path_bits(1, 4),
        },
        root_mult: 2, // 2 columns in this table
    };
    let trace = generate_smt_table_path_trace(&[witness]);
    let pvs = table_path_public_values(&trace);
    debug_check_with_public_values(&SmtTablePathChip, &trace, &pvs)
        .expect("single table path should pass");
}

#[test]
fn table_path_invalid_public_root_binding() {
    let witness = SmtTablePathWitness {
        path: SmtPathWitness {
            table_id: 1,
            key: 1,
            old_leaf: nonzero_digest(10),
            new_leaf: nonzero_digest(20),
            siblings: zero_siblings(4),
            path_bits: make_path_bits(1, 4),
        },
        root_mult: 1,
    };
    let trace = generate_smt_table_path_trace(&[witness]);
    let mut pvs = table_path_public_values(&trace);
    pvs[SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET] += BabyBear::ONE;

    debug_check_with_public_values(&SmtTablePathChip, &trace, &pvs)
        .expect_err("tampered old_root public value must fail root binding");
}

#[test]
fn table_path_records_c16_receive() {
    let witness = SmtTablePathWitness {
        path: SmtPathWitness {
            table_id: 1,
            key: 1,
            old_leaf: nonzero_digest(10),
            new_leaf: nonzero_digest(20),
            siblings: zero_siblings(3),
            path_bits: make_path_bits(1, 3),
        },
        root_mult: 3,
    };
    let trace = generate_smt_table_path_trace(&[witness]);
    let pvs = table_path_public_values(&trace);
    let record =
        evaluate_chip_with_public_values("SmtTablePath", &SmtTablePathChip, &trace, &pvs).unwrap();

    use tabula_proof::air::interaction::{InteractionDirection, InteractionKind};

    // C16 receive with multiplicity = is_real * is_leaf * root_mult_witness = 1 * 1 * 3 = 3
    let c16: Vec<_> = record
        .interactions
        .iter()
        .filter(|i| {
            i.kind == InteractionKind::SmtTableRoot
                && i.direction == InteractionDirection::Receive
                && i.multiplicity != BabyBear::ZERO
        })
        .collect();
    assert_eq!(c16.len(), 1, "should have 1 C16 receive (at leaf)");
    assert_eq!(
        c16[0].multiplicity,
        BabyBear::new(3),
        "C16 receive multiplicity should be root_mult_witness=3"
    );
}

#[test]
fn table_path_empty_trace() {
    let trace = generate_smt_table_path_trace(&[]);
    let pvs = vec![BabyBear::ZERO; SMT_TABLE_PATH_NUM_PUBLIC_VALUES];
    debug_check_with_public_values(&SmtTablePathChip, &trace, &pvs)
        .expect("empty table path trace should pass");
}

#[test]
fn col_path_invalid_cannot_disable_root_send_on_last_real_row() {
    let witness = SmtPathWitness {
        table_id: 1,
        key: 1,
        old_leaf: nonzero_digest(10),
        new_leaf: nonzero_digest(20),
        siblings: zero_siblings(1),
        path_bits: vec![true],
    };
    let mut trace = generate_smt_col_path_trace(&[witness]);
    let row = trace.values.get_mut(0..SMT_COL_PATH_WIDTH).unwrap();
    let cols: &mut SmtPathCols<BabyBear> = borrow_cols_mut(row);
    cols.is_root = BabyBear::ZERO;
    cols.next_is_new_path.populate(BabyBear::ZERO);

    debug_check(&SmtColPathChip, &trace).expect_err("last real row without is_root must fail");
}

#[test]
fn table_path_invalid_cannot_disable_leaf_receive() {
    let witness = SmtTablePathWitness {
        path: SmtPathWitness {
            table_id: 1,
            key: 1,
            old_leaf: nonzero_digest(10),
            new_leaf: nonzero_digest(20),
            siblings: zero_siblings(1),
            path_bits: vec![true],
        },
        root_mult: 1,
    };
    let mut trace = generate_smt_table_path_trace(&[witness]);
    let pvs = table_path_public_values(&trace);
    let row = trace.values.get_mut(0..SMT_TABLE_PATH_WIDTH).unwrap();
    let cols: &mut SmtTablePathCols<BabyBear> = borrow_cols_mut(row);
    cols.base.is_leaf = BabyBear::ZERO;
    cols.root_mult_witness = BabyBear::ZERO;

    debug_check_with_public_values(&SmtTablePathChip, &trace, &pvs)
        .expect_err("path start without is_leaf must fail");
}
