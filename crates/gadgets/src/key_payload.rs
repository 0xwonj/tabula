//! Fixed-width native key payload gadgets for user-state proof lanes.
//!
//! These gadgets operate on the canonical proof-visible key payload emitted by
//! `TableKeyCodec`. Every payload element is range-checked as a 30-bit field
//! limb, and lexicographic ordering is proven directly over the full padded
//! payload.

use p3_air::AirBuilder;
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;

use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::interaction::{AirInteraction, core_buses};

use super::integer::{IsZero, LimbHalves, constrain_is_zero, constrain_limb_halves};

/// Fixed-width key payload columns plus per-element 30-bit range-check halves.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct KeyPayloadWitness<T, const K: usize> {
    /// Canonical padded key payload.
    pub payload: [T; K],
    /// 15+15 decomposition for each payload element.
    pub halves: [LimbHalves<T>; K],
}

impl<const K: usize> KeyPayloadWitness<KoalaBear, K> {
    /// Populate from a canonical padded payload.
    pub fn populate_payload(&mut self, payload: &[KoalaBear; K]) {
        for (index, limb) in payload.iter().copied().enumerate() {
            self.payload[index] = limb;
            self.halves[index].populate(limb.as_canonical_u32());
        }
    }
}

/// Strict `<` witness for one canonical 30-bit field limb.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct FieldLt30Checked<T> {
    /// 15+15 decomposition of `rhs - lhs - 1`.
    pub diff_halves: LimbHalves<T>,
}

impl FieldLt30Checked<KoalaBear> {
    /// Populate witness columns proving `lhs < rhs` for 30-bit canonical field values.
    pub fn populate(&mut self, lhs: u32, rhs: u32) {
        assert!(lhs < rhs, "FieldLt30Checked requires lhs < rhs");
        let diff = rhs - lhs - 1;
        self.diff_halves.populate(diff);
    }
}

/// Lexicographic strict-order witness for full native key payloads.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct KeyLexOrderWitness<T, const K: usize> {
    /// `IsZero(rhs[i] - lhs[i])` for each payload element.
    pub diff_iz: [IsZero<T>; K],
    /// Prefix-equality flags:
    /// - `prefix_eq[0] = 1`
    /// - `prefix_eq[i] = 1` iff all payload positions `< i` are equal.
    pub prefix_eq: [T; K],
    /// Strict `<` witness for the first differing payload element.
    pub first_diff_lt: FieldLt30Checked<T>,
}

impl<const K: usize> KeyLexOrderWitness<KoalaBear, K> {
    /// Populate witness columns proving `lhs < rhs` in lexicographic payload order.
    pub fn populate(&mut self, lhs: &[KoalaBear; K], rhs: &[KoalaBear; K]) {
        let mut all_prev_equal = true;
        let mut first_diff_index = None;

        for index in 0..K {
            self.diff_iz[index].populate(rhs[index] - lhs[index]);
            self.prefix_eq[index] = if all_prev_equal {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            if all_prev_equal && lhs[index] != rhs[index] {
                first_diff_index = Some(index);
                all_prev_equal = false;
            }
        }

        let Some(first_diff_index) = first_diff_index else {
            panic!("KeyLexOrderWitness requires lexicographically distinct payloads");
        };
        let lhs_val = lhs[first_diff_index].as_canonical_u32();
        let rhs_val = rhs[first_diff_index].as_canonical_u32();
        assert!(
            lhs_val < rhs_val,
            "KeyLexOrderWitness requires lhs < rhs at the first differing payload element"
        );
        self.first_diff_lt.populate(lhs_val, rhs_val);
    }
}

/// Constrain every payload element to match its 30-bit half decomposition.
pub fn constrain_key_payload<AB: AirBuilder, const K: usize>(
    builder: &mut AB,
    key: &KeyPayloadWitness<AB::Var, K>,
) {
    for index in 0..K {
        constrain_limb_halves(builder, key.payload[index].into(), &key.halves[index]);
    }
}

/// Send 15-bit range checks for every payload element half.
#[allow(clippy::needless_pass_by_value)]
pub fn send_key_payload_range_checks<AB: InteractionAirBuilder, const K: usize>(
    builder: &mut AB,
    key: &KeyPayloadWitness<AB::Var, K>,
    mult: AB::Expr,
) {
    let mut send_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    for halves in &key.halves {
        send_rc(halves.lo.into());
        send_rc(halves.hi.into());
    }
}

/// Constrain `rhs - lhs - 1` to be a valid 30-bit gap.
pub fn constrain_field_lt30<AB: AirBuilder>(
    builder: &mut AB,
    lhs: AB::Expr,
    rhs: AB::Expr,
    witness: &FieldLt30Checked<AB::Var>,
) {
    let gap = rhs - lhs - AB::Expr::ONE;
    constrain_limb_halves(builder, gap, &witness.diff_halves);
}

/// Send range checks for a 30-bit strict-order witness.
#[allow(clippy::needless_pass_by_value)]
pub fn send_field_lt30_range_checks<AB: InteractionAirBuilder>(
    builder: &mut AB,
    witness: &FieldLt30Checked<AB::Var>,
    mult: AB::Expr,
) {
    let mut send_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_rc(witness.diff_halves.lo.into());
    send_rc(witness.diff_halves.hi.into());
}

/// Constrain equality flags for a key pair and return the `lhs == rhs` expression.
pub fn constrain_key_equality_flags<AB: AirBuilder, const K: usize>(
    builder: &mut AB,
    lhs: &KeyPayloadWitness<AB::Var, K>,
    rhs: &KeyPayloadWitness<AB::Var, K>,
    diff_iz: &[IsZero<AB::Var>; K],
) -> AB::Expr {
    let mut all_equal = AB::Expr::ONE;
    for (index, diff) in diff_iz.iter().enumerate() {
        constrain_is_zero(
            builder,
            rhs.payload[index].into() - lhs.payload[index].into(),
            diff,
        );
        all_equal *= diff.is_zero.into();
    }
    all_equal
}

/// Constrain two key payloads to be identical.
pub fn constrain_key_equal<AB: AirBuilder, const K: usize>(
    builder: &mut AB,
    lhs: &KeyPayloadWitness<AB::Var, K>,
    rhs: &KeyPayloadWitness<AB::Var, K>,
    gate: &AB::Expr,
) {
    for index in 0..K {
        builder
            .assert_zero((*gate).clone() * (lhs.payload[index].into() - rhs.payload[index].into()));
    }
}

/// Constrain a key payload to be canonical zero.
pub fn constrain_key_zero<AB: AirBuilder, const K: usize>(
    builder: &mut AB,
    key: &KeyPayloadWitness<AB::Var, K>,
    gate: &AB::Expr,
) {
    for limb in key.payload {
        builder.assert_zero((*gate).clone() * limb.into());
    }
}

/// Constrain strict lexicographic order over two full padded payloads.
#[allow(clippy::needless_pass_by_value)]
pub fn constrain_key_lex_lt<AB: InteractionAirBuilder, const K: usize>(
    builder: &mut AB,
    lhs: &KeyPayloadWitness<AB::Var, K>,
    rhs: &KeyPayloadWitness<AB::Var, K>,
    witness: &KeyLexOrderWitness<AB::Var, K>,
    gate: AB::Expr,
) {
    let mut lhs_selected = AB::Expr::ZERO;
    let mut rhs_selected = AB::Expr::ZERO;
    let mut first_diff_sum = AB::Expr::ZERO;

    for index in 0..K {
        constrain_is_zero(
            builder,
            rhs.payload[index].into() - lhs.payload[index].into(),
            &witness.diff_iz[index],
        );
        builder.assert_bool(witness.prefix_eq[index]);

        if index == 0 {
            builder.assert_zero(gate.clone() * (witness.prefix_eq[index].into() - AB::Expr::ONE));
        } else {
            builder.assert_zero(
                gate.clone()
                    * (witness.prefix_eq[index].into()
                        - witness.prefix_eq[index - 1].into()
                            * witness.diff_iz[index - 1].is_zero.into()),
            );
        }

        let is_first_diff = witness.prefix_eq[index].into()
            * (AB::Expr::ONE - witness.diff_iz[index].is_zero.into());
        first_diff_sum += is_first_diff.clone();
        lhs_selected += is_first_diff.clone() * lhs.payload[index].into();
        rhs_selected += is_first_diff * rhs.payload[index].into();
    }

    builder.assert_zero(gate.clone() * (first_diff_sum - AB::Expr::ONE));
    {
        let mut when_lt = builder.when(gate.clone());
        constrain_field_lt30(
            &mut when_lt,
            lhs_selected,
            rhs_selected,
            &witness.first_diff_lt,
        );
    }
    send_field_lt30_range_checks(builder, &witness.first_diff_lt, gate);
}
