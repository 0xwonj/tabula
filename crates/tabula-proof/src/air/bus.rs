//! Domain-specific builder extension traits for LogUp bus interactions.
//!
//! Each trait encodes the tuple schema for one bus, providing typed
//! `send_*` / `receive_*` methods that replace manual `AirInteraction`
//! construction. Blanket impls on [`InteractionAirBuilder`] mean
//! [`DebugConstraintBuilder`] automatically supports all traits.
//!
//! These traits reduce bus wiring errors by encoding the schema once:
//! the tuple width, field order, and `InteractionKind` are all fixed
//! by the trait method signature.

use crate::air::builder::InteractionAirBuilder;
use crate::air::gadgets::U64Limbs;
use crate::air::interaction::{AirInteraction, InteractionKind};

// ── C8 RangeCheck ─────────────────────────────────────────────────────────

/// Extension trait for sending/receiving on the RangeCheck bus (C8).
///
/// Tuple: `(value)` — single field element in `[0, 2^16)`.
pub trait RangeCheckAirBuilder: InteractionAirBuilder {
    /// Send a single value to the RangeCheck bus.
    fn send_range_check(&mut self, val: Self::Expr, mult: Self::Expr);
}

impl<AB: InteractionAirBuilder> RangeCheckAirBuilder for AB {
    fn send_range_check(&mut self, val: Self::Expr, mult: Self::Expr) {
        self.send(AirInteraction {
            values: vec![val],
            multiplicity: mult,
            kind: InteractionKind::RangeCheck,
        });
    }
}

// ── C1 Memory ─────────────────────────────────────────────────────────────

/// Extension trait for send/receive on the Memory bus (C1).
///
/// Tuple (13 elements): `(t, c, r[3], tau[3], is_write, val[W], val_is_null)`.
#[allow(clippy::too_many_arguments)]
pub trait MemoryAirBuilder: InteractionAirBuilder {
    /// Send a memory access to the Memory bus.
    fn send_memory_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        r: &U64Limbs<Self::Var>,
        tau: &U64Limbs<Self::Var>,
        is_write: Self::Expr,
        val: &[Self::Var],
        val_is_null: Self::Expr,
        mult: Self::Expr,
    );

    /// Receive a memory access from the Memory bus.
    fn receive_memory_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        r: &U64Limbs<Self::Var>,
        tau: &U64Limbs<Self::Var>,
        is_write: Self::Expr,
        val: &[Self::Var],
        val_is_null: Self::Expr,
        mult: Self::Expr,
    );
}

fn memory_values<AB: InteractionAirBuilder>(
    t: AB::Expr,
    c: AB::Expr,
    r: &U64Limbs<AB::Var>,
    tau: &U64Limbs<AB::Var>,
    is_write: AB::Expr,
    val: &[AB::Var],
    val_is_null: AB::Expr,
) -> Vec<AB::Expr> {
    let mut values: Vec<AB::Expr> = vec![
        t,
        c,
        r.limb0.clone().into(),
        r.limb1.clone().into(),
        r.limb2.clone().into(),
        tau.limb0.clone().into(),
        tau.limb1.clone().into(),
        tau.limb2.clone().into(),
        is_write,
    ];
    for v in val {
        values.push(v.clone().into());
    }
    values.push(val_is_null);
    values
}

#[allow(clippy::too_many_arguments)]
impl<AB: InteractionAirBuilder> MemoryAirBuilder for AB {
    fn send_memory_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        r: &U64Limbs<Self::Var>,
        tau: &U64Limbs<Self::Var>,
        is_write: Self::Expr,
        val: &[Self::Var],
        val_is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: memory_values::<AB>(t, c, r, tau, is_write, val, val_is_null),
            multiplicity: mult,
            kind: InteractionKind::Memory,
        });
    }

    fn receive_memory_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        r: &U64Limbs<Self::Var>,
        tau: &U64Limbs<Self::Var>,
        is_write: Self::Expr,
        val: &[Self::Var],
        val_is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: memory_values::<AB>(t, c, r, tau, is_write, val, val_is_null),
            multiplicity: mult,
            kind: InteractionKind::Memory,
        });
    }
}

// ── C5 PoseidonPermutation ────────────────────────────────────────────────

/// Extension trait for send/receive on the PoseidonPermutation bus (C5).
///
/// Tuple (24 elements): `(input[16], output[8])`.
pub trait PoseidonAirBuilder: InteractionAirBuilder {
    /// Send a Poseidon permutation request.
    fn send_poseidon_perm(
        &mut self,
        input: &[Self::Var; 16],
        output: &[Self::Var; 8],
        mult: Self::Expr,
    );

    /// Receive a Poseidon permutation result.
    fn receive_poseidon_perm(
        &mut self,
        input: &[Self::Var; 16],
        output: &[Self::Var; 8],
        mult: Self::Expr,
    );
}

fn poseidon_values<AB: InteractionAirBuilder>(
    input: &[AB::Var; 16],
    output: &[AB::Var; 8],
) -> Vec<AB::Expr> {
    let mut values: Vec<AB::Expr> = Vec::with_capacity(24);
    for v in input {
        values.push(v.clone().into());
    }
    for v in output {
        values.push(v.clone().into());
    }
    values
}

impl<AB: InteractionAirBuilder> PoseidonAirBuilder for AB {
    fn send_poseidon_perm(
        &mut self,
        input: &[Self::Var; 16],
        output: &[Self::Var; 8],
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: poseidon_values::<AB>(input, output),
            multiplicity: mult,
            kind: InteractionKind::PoseidonPermutation,
        });
    }

    fn receive_poseidon_perm(
        &mut self,
        input: &[Self::Var; 16],
        output: &[Self::Var; 8],
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: poseidon_values::<AB>(input, output),
            multiplicity: mult,
            kind: InteractionKind::PoseidonPermutation,
        });
    }
}

// ── C6 CommitmentVerification ─────────────────────────────────────────────

/// Extension trait for send/receive on the CommitmentVerification bus (C6).
///
/// Tuple (12 elements): `(t, c, comm_type, is_touched, digest[8])`.
pub trait CommitmentAirBuilder: InteractionAirBuilder {
    /// Send a commitment to the CommitmentVerification bus.
    fn send_commitment(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        comm_type: Self::Expr,
        is_touched: Self::Expr,
        digest: &[Self::Var; 8],
        mult: Self::Expr,
    );

    /// Receive a commitment from the CommitmentVerification bus.
    fn receive_commitment(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        comm_type: Self::Expr,
        is_touched: Self::Expr,
        digest: &[Self::Var; 8],
        mult: Self::Expr,
    );
}

fn commitment_values<AB: InteractionAirBuilder>(
    t: AB::Expr,
    c: AB::Expr,
    comm_type: AB::Expr,
    is_touched: AB::Expr,
    digest: &[AB::Var; 8],
) -> Vec<AB::Expr> {
    let mut values: Vec<AB::Expr> = vec![t, c, comm_type, is_touched];
    for v in digest {
        values.push(v.clone().into());
    }
    values
}

impl<AB: InteractionAirBuilder> CommitmentAirBuilder for AB {
    fn send_commitment(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        comm_type: Self::Expr,
        is_touched: Self::Expr,
        digest: &[Self::Var; 8],
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: commitment_values::<AB>(t, c, comm_type, is_touched, digest),
            multiplicity: mult,
            kind: InteractionKind::CommitmentVerification,
        });
    }

    fn receive_commitment(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        comm_type: Self::Expr,
        is_touched: Self::Expr,
        digest: &[Self::Var; 8],
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: commitment_values::<AB>(t, c, comm_type, is_touched, digest),
            multiplicity: mult,
            kind: InteractionKind::CommitmentVerification,
        });
    }
}

// ── C2 SsmcMembership ─────────────────────────────────────────────────────

/// Extension trait for send/receive on the SsmcMembership bus (C2).
///
/// Tuple (5+W elements): `(t, c, key[3], value[W])`.
pub trait SsmcMembershipAirBuilder: InteractionAirBuilder {
    /// Send on the SsmcMembership bus.
    fn send_ssmc_membership(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    );

    /// Receive on the SsmcMembership bus.
    fn receive_ssmc_membership(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    );
}

fn ssmc_membership_values<AB: InteractionAirBuilder>(
    t: AB::Expr,
    c: AB::Expr,
    key: &U64Limbs<AB::Var>,
    value: &[AB::Var],
) -> Vec<AB::Expr> {
    let mut values: Vec<AB::Expr> = vec![
        t,
        c,
        key.limb0.clone().into(),
        key.limb1.clone().into(),
        key.limb2.clone().into(),
    ];
    for v in value {
        values.push(v.clone().into());
    }
    values
}

impl<AB: InteractionAirBuilder> SsmcMembershipAirBuilder for AB {
    fn send_ssmc_membership(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: ssmc_membership_values::<AB>(t, c, key, value),
            multiplicity: mult,
            kind: InteractionKind::SsmcMembership,
        });
    }

    fn receive_ssmc_membership(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: ssmc_membership_values::<AB>(t, c, key, value),
            multiplicity: mult,
            kind: InteractionKind::SsmcMembership,
        });
    }
}

// ── C3/C4 Merge buses ─────────────────────────────────────────────────────

/// Extension trait for send/receive on the MergeOldList (C3) and MergeWriteSet (C4) buses.
pub trait MergeAirBuilder: InteractionAirBuilder {
    /// Send on the MergeOldList bus (C3).
    ///
    /// Tuple (5+W): `(t, c, key[3], old_val[W])`.
    fn send_merge_old_list(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    );

    /// Receive on the MergeOldList bus (C3).
    fn receive_merge_old_list(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    );

    /// Send on the MergeWriteSet bus (C4).
    ///
    /// Tuple (6+W): `(t, c, key[3], write_val[W], is_delete)`.
    fn send_merge_write_set(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        is_delete: Self::Expr,
        mult: Self::Expr,
    );

    /// Receive on the MergeWriteSet bus (C4).
    fn receive_merge_write_set(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        is_delete: Self::Expr,
        mult: Self::Expr,
    );
}

impl<AB: InteractionAirBuilder> MergeAirBuilder for AB {
    fn send_merge_old_list(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: ssmc_membership_values::<AB>(t, c, key, value), // same schema as C2
            multiplicity: mult,
            kind: InteractionKind::MergeOldList,
        });
    }

    fn receive_merge_old_list(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: ssmc_membership_values::<AB>(t, c, key, value),
            multiplicity: mult,
            kind: InteractionKind::MergeOldList,
        });
    }

    fn send_merge_write_set(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        is_delete: Self::Expr,
        mult: Self::Expr,
    ) {
        let mut values = ssmc_membership_values::<AB>(t, c, key, value);
        values.push(is_delete);
        self.send(AirInteraction {
            values,
            multiplicity: mult,
            kind: InteractionKind::MergeWriteSet,
        });
    }

    fn receive_merge_write_set(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        is_delete: Self::Expr,
        mult: Self::Expr,
    ) {
        let mut values = ssmc_membership_values::<AB>(t, c, key, value);
        values.push(is_delete);
        self.receive(AirInteraction {
            values,
            multiplicity: mult,
            kind: InteractionKind::MergeWriteSet,
        });
    }
}

// ── C7 SortedMemMeta ──────────────────────────────────────────────────────

/// Extension trait for send/receive on the SortedMemMeta bus (C7).
///
/// Tuple (3 elements): `(t, c, is_empty_old)`.
pub trait SortedMemMetaAirBuilder: InteractionAirBuilder {
    /// Send on the SortedMemMeta bus.
    fn send_sorted_mem_meta(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        is_empty_old: Self::Expr,
        mult: Self::Expr,
    );

    /// Receive on the SortedMemMeta bus.
    fn receive_sorted_mem_meta(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        is_empty_old: Self::Expr,
        mult: Self::Expr,
    );
}

impl<AB: InteractionAirBuilder> SortedMemMetaAirBuilder for AB {
    fn send_sorted_mem_meta(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        is_empty_old: Self::Expr,
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: vec![t, c, is_empty_old],
            multiplicity: mult,
            kind: InteractionKind::SortedMemMeta,
        });
    }

    fn receive_sorted_mem_meta(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        is_empty_old: Self::Expr,
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: vec![t, c, is_empty_old],
            multiplicity: mult,
            kind: InteractionKind::SortedMemMeta,
        });
    }
}

// ── C9 StaticTableLookup ──────────────────────────────────────────────────

/// Extension trait for send/receive on the StaticTableLookup bus (C9).
///
/// Tuple (5+W elements): `(t, c, key[3], val[W])`.
pub trait StaticTableLookupAirBuilder: InteractionAirBuilder {
    /// Send on the StaticTableLookup bus.
    fn send_static_table_lookup(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    );
}

impl<AB: InteractionAirBuilder> StaticTableLookupAirBuilder for AB {
    fn send_static_table_lookup(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        value: &[Self::Var],
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: ssmc_membership_values::<AB>(t, c, key, value), // same schema
            multiplicity: mult,
            kind: InteractionKind::StaticTableLookup,
        });
    }
}
