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

// ── C10 ReadAccess ───────────────────────────────────────────────────────

/// Extension trait for send/receive on the ReadAccess bus (C10).
///
/// Tuple (7+W elements): `(t, c, key[3], val[W], is_null)`.
pub trait ReadAccessAirBuilder: InteractionAirBuilder {
    /// Send on the ReadAccess bus.
    fn send_read_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    );

    /// Receive on the ReadAccess bus.
    fn receive_read_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    );
}

fn read_access_values<AB: InteractionAirBuilder>(
    t: AB::Expr,
    c: AB::Expr,
    key: &U64Limbs<AB::Var>,
    val: &[AB::Var],
    is_null: AB::Expr,
) -> Vec<AB::Expr> {
    let mut values: Vec<AB::Expr> = vec![
        t,
        c,
        key.limb0.clone().into(),
        key.limb1.clone().into(),
        key.limb2.clone().into(),
    ];
    for v in val {
        values.push(v.clone().into());
    }
    values.push(is_null);
    values
}

impl<AB: InteractionAirBuilder> ReadAccessAirBuilder for AB {
    fn send_read_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: read_access_values::<AB>(t, c, key, val, is_null),
            multiplicity: mult,
            kind: InteractionKind::ReadAccess,
        });
    }

    fn receive_read_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: read_access_values::<AB>(t, c, key, val, is_null),
            multiplicity: mult,
            kind: InteractionKind::ReadAccess,
        });
    }
}

// ── C11 WriteAccess ──────────────────────────────────────────────────────

/// Extension trait for send/receive on the WriteAccess bus (C11).
///
/// Tuple (7+W elements): `(t, c, key[3], val[W], is_null)`.
pub trait WriteAccessAirBuilder: InteractionAirBuilder {
    /// Send on the WriteAccess bus.
    fn send_write_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    );

    /// Receive on the WriteAccess bus.
    fn receive_write_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    );
}

impl<AB: InteractionAirBuilder> WriteAccessAirBuilder for AB {
    fn send_write_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: read_access_values::<AB>(t, c, key, val, is_null),
            multiplicity: mult,
            kind: InteractionKind::WriteAccess,
        });
    }

    fn receive_write_access(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: read_access_values::<AB>(t, c, key, val, is_null),
            multiplicity: mult,
            kind: InteractionKind::WriteAccess,
        });
    }
}

// ── C12 EmptyColRead ─────────────────────────────────────────────────────

/// Extension trait for send/receive on the EmptyColRead bus (C12).
///
/// Tuple (2 elements): `(t, c)`.
pub trait EmptyColReadAirBuilder: InteractionAirBuilder {
    /// Send on the EmptyColRead bus.
    fn send_empty_col_read(&mut self, t: Self::Expr, c: Self::Expr, mult: Self::Expr);

    /// Receive on the EmptyColRead bus.
    fn receive_empty_col_read(&mut self, t: Self::Expr, c: Self::Expr, mult: Self::Expr);
}

impl<AB: InteractionAirBuilder> EmptyColReadAirBuilder for AB {
    fn send_empty_col_read(&mut self, t: Self::Expr, c: Self::Expr, mult: Self::Expr) {
        self.send(AirInteraction {
            values: vec![t, c],
            multiplicity: mult,
            kind: InteractionKind::EmptyColRead,
        });
    }

    fn receive_empty_col_read(&mut self, t: Self::Expr, c: Self::Expr, mult: Self::Expr) {
        self.receive(AirInteraction {
            values: vec![t, c],
            multiplicity: mult,
            kind: InteractionKind::EmptyColRead,
        });
    }
}

// ── C13 BaseStateEntry ────────────────────────────────────────────────────

/// Extension trait for send/receive on the BaseStateEntry bus (C13).
///
/// Tuple (7+W elements): `(t, c, key[3], val[W], is_null)`.
/// Same schema as ReadAccess/WriteAccess — only the bus ID differs.
pub trait BaseStateEntryAirBuilder: InteractionAirBuilder {
    /// Send on the BaseStateEntry bus.
    fn send_base_state_entry(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    );

    /// Receive on the BaseStateEntry bus.
    fn receive_base_state_entry(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    );
}

impl<AB: InteractionAirBuilder> BaseStateEntryAirBuilder for AB {
    fn send_base_state_entry(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: read_access_values::<AB>(t, c, key, val, is_null),
            multiplicity: mult,
            kind: InteractionKind::BaseStateEntry,
        });
    }

    fn receive_base_state_entry(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: read_access_values::<AB>(t, c, key, val, is_null),
            multiplicity: mult,
            kind: InteractionKind::BaseStateEntry,
        });
    }
}

// ── C14 CoalescedWrite ───────────────────────────────────────────────────

/// Extension trait for send/receive on the CoalescedWrite bus (C14).
///
/// Tuple (7+W elements): `(t, c, key[3], val[W], is_null)`.
/// Same schema as ReadAccess/WriteAccess — only the bus ID differs.
pub trait CoalescedWriteAirBuilder: InteractionAirBuilder {
    /// Send on the CoalescedWrite bus.
    fn send_coalesced_write(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    );

    /// Receive on the CoalescedWrite bus.
    fn receive_coalesced_write(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    );
}

impl<AB: InteractionAirBuilder> CoalescedWriteAirBuilder for AB {
    fn send_coalesced_write(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.send(AirInteraction {
            values: read_access_values::<AB>(t, c, key, val, is_null),
            multiplicity: mult,
            kind: InteractionKind::CoalescedWrite,
        });
    }

    fn receive_coalesced_write(
        &mut self,
        t: Self::Expr,
        c: Self::Expr,
        key: &U64Limbs<Self::Var>,
        val: &[Self::Var],
        is_null: Self::Expr,
        mult: Self::Expr,
    ) {
        self.receive(AirInteraction {
            values: read_access_values::<AB>(t, c, key, val, is_null),
            multiplicity: mult,
            kind: InteractionKind::CoalescedWrite,
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

fn key_value_tuple<AB: InteractionAirBuilder>(
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
            values: key_value_tuple::<AB>(t, c, key, value),
            multiplicity: mult,
            kind: InteractionKind::StaticTableLookup,
        });
    }
}
