//! SmtPathChip — AIR constraints for SMT Merkle path verification.
//!
//! Two chip variants share the same core constraint logic:
//! - **SmtColPathChip**: C15 receive at leaf, C16 send at root
//! - **SmtTablePathChip**: C16 receive at leaf (with root_mult_witness), public input at root
//!
//! Constraints:
//! 1. Booleans: is_real, path_bit, is_leaf, is_root
//! 2. is_real prefix (monotonic 1→0)
//! 3. Perm input mux: 16 constraints × 2 trees = 32
//! 4. C5 PoseidonPerm send ×2 per real row
//! 5. Continuity within path: next.old_node = local.old_parent (×8 each tree)
//! 6. Key reconstruction: is_leaf→key_acc=bit,level_power=1; within path accumulate
//! 7. Key binding: is_root → key_acc = bind_key
//! 8. Identity constancy: bind_table_id, bind_key constant within path
//! 9. Path structure: is_leaf at path start, is_root before boundary

use p3_air::{Air, AirBuilder, AirBuilderWithPublicValues, BaseAir, BaseAirWithPublicValues};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::bus::{PoseidonAirBuilder, SmtLeafDigestAirBuilder, SmtTableRootAirBuilder};
use crate::air::columns::borrow_cols;
use crate::air::gadgets::{constrain_is_real_prefix, constrain_is_zero};

use super::columns::{
    DIGEST_WIDTH, SMT_COL_PATH_WIDTH, SMT_TABLE_PATH_WIDTH, SmtPathCols, SmtTablePathCols,
};

/// SmtColPathChip — column-level SMT path verification.
///
/// Receives leaf digests from ColumnMeta (C15), sends table roots (C16).
#[derive(Debug)]
pub struct SmtColPathChip;

/// SmtTablePathChip — table-level SMT path verification.
///
/// Receives table roots from SmtColPathChip (C16), roots verified against public inputs.
#[derive(Debug)]
pub struct SmtTablePathChip;

impl<F> BaseAir<F> for SmtColPathChip {
    fn width(&self) -> usize {
        SMT_COL_PATH_WIDTH
    }
}

impl<F> BaseAir<F> for SmtTablePathChip {
    fn width(&self) -> usize {
        SMT_TABLE_PATH_WIDTH
    }
}

/// Public value offset for the old state root digest (8 field elements).
pub const SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET: usize = 0;
/// Public value offset for the new state root digest (8 field elements).
pub const SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET: usize = SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET + 8;
/// Number of public values consumed by SmtTablePath (old_root + new_root).
pub const SMT_TABLE_PATH_NUM_PUBLIC_VALUES: usize = SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET + 8;

impl<F> BaseAirWithPublicValues<F> for SmtTablePathChip {
    fn num_public_values(&self) -> usize {
        SMT_TABLE_PATH_NUM_PUBLIC_VALUES
    }
}

// ── Core constraint logic (shared) ──

/// Emit shared constraints on SmtPathCols (constraints 1–9).
fn constrain_smt_path_core<AB: InteractionAirBuilder>(
    builder: &mut AB,
    local: &SmtPathCols<AB::Var>,
    next: &SmtPathCols<AB::Var>,
) {
    let is_real: AB::Expr = local.is_real.clone().into();

    // ── 1. Booleans ──
    builder.assert_bool(local.path_bit.clone());
    builder.assert_bool(local.is_leaf.clone());
    builder.assert_bool(local.is_root.clone());

    // ── 2. is_real prefix ──
    constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());

    // First real row must start a path.
    builder
        .when_first_row()
        .assert_zero(is_real.clone() * (AB::Expr::ONE - local.is_leaf.clone().into()));

    // ── 3. Perm input mux ──
    // left[i]  = (1-bit)*node[i] + bit*sib[i]
    // right[i] = bit*node[i] + (1-bit)*sib[i]
    {
        let bit: AB::Expr = local.path_bit.clone().into();
        let not_bit: AB::Expr = AB::Expr::ONE - bit.clone();

        for i in 0..DIGEST_WIDTH {
            // Old tree left = perm_input[i]
            let expected_left_old: AB::Expr = not_bit.clone() * local.old_node[i].clone().into()
                + bit.clone() * local.sibling[i].clone().into();
            builder.assert_zero(
                is_real.clone() * (local.old_perm_input[i].clone().into() - expected_left_old),
            );
            // Old tree right = perm_input[8+i]
            let expected_right_old: AB::Expr = bit.clone() * local.old_node[i].clone().into()
                + not_bit.clone() * local.sibling[i].clone().into();
            builder.assert_zero(
                is_real.clone() * (local.old_perm_input[8 + i].clone().into() - expected_right_old),
            );

            // New tree left = perm_input[i]
            let expected_left_new: AB::Expr = not_bit.clone() * local.new_node[i].clone().into()
                + bit.clone() * local.sibling[i].clone().into();
            builder.assert_zero(
                is_real.clone() * (local.new_perm_input[i].clone().into() - expected_left_new),
            );
            // New tree right = perm_input[8+i]
            let expected_right_new: AB::Expr = bit.clone() * local.new_node[i].clone().into()
                + not_bit.clone() * local.sibling[i].clone().into();
            builder.assert_zero(
                is_real.clone() * (local.new_perm_input[8 + i].clone().into() - expected_right_new),
            );
        }
    }

    // ── 4. C5 PoseidonPerm send ×2 ──
    builder.send_poseidon_perm(&local.old_perm_input, &local.old_parent, is_real.clone());
    builder.send_poseidon_perm(&local.new_perm_input, &local.new_parent, is_real.clone());

    // ── Path boundary detection ──
    // next_is_new_path detects whether the next row starts a new path.
    // We use a synthetic "path_id" that changes at boundaries.
    // For simplicity: at an is_root row, the next row is a new path start.
    // next_is_new_path.is_zero = 1 means "next row is same path" (diff=0).
    // We define: diff = local.is_root (1 at boundary, 0 within path).
    constrain_is_zero(
        builder,
        local.is_root.clone().into(),
        &local.next_is_new_path,
    );

    let within_path: AB::Expr = local.next_is_new_path.is_zero.clone().into();
    let at_boundary: AB::Expr = AB::Expr::ONE - within_path.clone();
    let both_real: AB::Expr = is_real.clone() * next.is_real.clone().into();

    // Within a path, only the first row may have is_leaf=1.
    builder
        .when_transition()
        .assert_zero(both_real.clone() * within_path.clone() * next.is_leaf.clone().into());

    // Last real row before padding must end a path with is_root=1.
    builder.when_transition().assert_zero(
        is_real.clone()
            * (AB::Expr::ONE - next.is_real.clone().into())
            * (AB::Expr::ONE - local.is_root.clone().into()),
    );

    // ── 5. Continuity within path: next.node = local.parent ──
    for i in 0..DIGEST_WIDTH {
        builder.when_transition().assert_zero(
            both_real.clone()
                * within_path.clone()
                * (next.old_node[i].clone().into() - local.old_parent[i].clone().into()),
        );
        builder.when_transition().assert_zero(
            both_real.clone()
                * within_path.clone()
                * (next.new_node[i].clone().into() - local.new_parent[i].clone().into()),
        );
    }

    // ── 6. Key reconstruction ──
    // is_leaf → key_acc = path_bit, level_power = 1
    builder.assert_zero(
        is_real.clone()
            * local.is_leaf.clone().into()
            * (local.key_acc.clone().into() - local.path_bit.clone().into()),
    );
    builder.assert_zero(
        is_real.clone()
            * local.is_leaf.clone().into()
            * (local.level_power.clone().into() - AB::Expr::ONE),
    );

    // Within path: next.key_acc = key_acc + next.path_bit * next.level_power
    //              next.level_power = level_power * 2
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * (next.key_acc.clone().into()
                - local.key_acc.clone().into()
                - next.path_bit.clone().into() * next.level_power.clone().into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * (next.level_power.clone().into() - local.level_power.clone().into() * AB::Expr::TWO),
    );

    // ── 7. Key binding: is_root → key_acc = bind_key ──
    builder.assert_zero(
        is_real.clone()
            * local.is_root.clone().into()
            * (local.key_acc.clone().into() - local.bind_key.clone().into()),
    );

    // ── 8. Identity constancy within path ──
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * (next.bind_table_id.clone().into() - local.bind_table_id.clone().into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * (next.bind_key.clone().into() - local.bind_key.clone().into()),
    );

    // ── 9. Path structure ──
    // At boundary (transition to new path): next row must be is_leaf
    builder.when_transition().assert_zero(
        both_real.clone() * at_boundary * (AB::Expr::ONE - next.is_leaf.clone().into()),
    );

    // is_leaf row must NOT also be is_root (unless path depth = 1, which we disallow
    // for SMT depths ≥ 2). For robustness we allow it — the constraint is:
    // A path must have is_leaf at start and is_root at end, enforced by boundary logic above.
}

// ── SmtColPathChip ──

impl<AB: InteractionAirBuilder> Air<AB> for SmtColPathChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &SmtPathCols<AB::Var> = borrow_cols(&local_row);
        let next: &SmtPathCols<AB::Var> = borrow_cols(&next_row);

        // Core constraints
        constrain_smt_path_core(builder, local, next);

        // C15 SmtLeafDigest receive (at leaf)
        builder.receive_smt_leaf_digest(
            local.bind_table_id.clone().into(),
            local.bind_key.clone().into(),
            &local.old_node,
            &local.new_node,
            local.is_real.clone().into() * local.is_leaf.clone().into(),
        );

        // C16 SmtTableRoot send (at root)
        builder.send_smt_table_root(
            local.bind_table_id.clone().into(),
            &local.old_parent,
            &local.new_parent,
            local.is_real.clone().into() * local.is_root.clone().into(),
        );
    }
}

// ── SmtTablePathChip ──

impl<AB> Air<AB> for SmtTablePathChip
where
    AB: InteractionAirBuilder + AirBuilderWithPublicValues,
{
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &SmtTablePathCols<AB::Var> = borrow_cols(&local_row);
        let next: &SmtTablePathCols<AB::Var> = borrow_cols(&next_row);

        // Core constraints on base columns
        constrain_smt_path_core(builder, &local.base, &next.base);

        // C16 SmtTableRoot receive (at leaf, with multiplicity witness)
        builder.receive_smt_table_root(
            local.base.bind_table_id.clone().into(),
            &local.base.old_node,
            &local.base.new_node,
            local.base.is_real.clone().into()
                * local.base.is_leaf.clone().into()
                * local.root_mult_witness.clone().into(),
        );

        // Root rows are bound to public values (old_root/new_root).
        constrain_state_root_public_values(builder, &local.base);
    }
}

fn constrain_state_root_public_values<AB>(builder: &mut AB, local: &SmtPathCols<AB::Var>)
where
    AB: AirBuilder + AirBuilderWithPublicValues,
{
    let (old_root_pvs, new_root_pvs) = {
        let pvs = builder.public_values();
        debug_assert!(
            pvs.len() >= SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
            "SmtTablePath requires at least {} public values, got {}",
            SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
            pvs.len()
        );
        (
            pvs[SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET
                ..(SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET + DIGEST_WIDTH)]
                .to_vec(),
            pvs[SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET
                ..(SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET + DIGEST_WIDTH)]
                .to_vec(),
        )
    };

    let root_gate: AB::Expr = local.is_real.clone().into() * local.is_root.clone().into();
    for i in 0..DIGEST_WIDTH {
        builder.assert_zero(
            root_gate.clone() * (local.old_parent[i].clone().into() - old_root_pvs[i].into()),
        );
        builder.assert_zero(
            root_gate.clone() * (local.new_parent[i].clone().into() - new_root_pvs[i].into()),
        );
    }
}
