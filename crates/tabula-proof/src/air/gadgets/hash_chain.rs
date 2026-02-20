//! Hash chain input operation: Poseidon permutation input composition.
//!
//! Composes the 16-element Poseidon input for SSMC/Merge hash chain steps.
//! First entry: `[0x00, table_id, col_id, key[3], value[W], 0..]`.
//! Continuation: `[prev_hash_acc[8], key[3], value[W], 0..]`.
//!
//! Used by SSMC and Merge chips.

use p3_air::AirBuilder;
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use super::integer::{MASK_30, U64Limbs};

/// Hash chain Poseidon input (16 field elements).
///
/// Columns: 16.
///
/// Two composition modes:
/// - First entry: `[0x00, table_id, col_id, key[3], value[W], 0..]`
/// - Continuation: `[prev_hash_acc[8], key[3], value[W], 0..]`
#[repr(C)]
#[derive(Clone, Debug)]
pub struct HashChainInput<T> {
    /// Composed 16-element Poseidon permutation input.
    pub perm_input: [T; 16],
}

/// Decompose a u64 key into 3 BabyBear limbs (30+30+4).
fn decompose_key(key: u64) -> [BabyBear; 3] {
    [
        BabyBear::new((key & MASK_30) as u32),
        BabyBear::new(((key >> 30) & MASK_30) as u32),
        BabyBear::new((key >> 60) as u32),
    ]
}

impl HashChainInput<BabyBear> {
    /// Populate first-entry input: `[0x00, table_id, col_id, key[3], value[W], 0..]`.
    pub fn populate_first(&mut self, table_id: u32, col_id: u32, key: u64, value: &[BabyBear]) {
        let key_limbs = decompose_key(key);
        self.perm_input = [BabyBear::ZERO; 16];
        // perm_input[0] = 0x00 (domain tag)
        self.perm_input[1] = BabyBear::new(table_id);
        self.perm_input[2] = BabyBear::new(col_id);
        self.perm_input[3] = key_limbs[0];
        self.perm_input[4] = key_limbs[1];
        self.perm_input[5] = key_limbs[2];
        for (i, v) in value.iter().enumerate() {
            self.perm_input[6 + i] = *v;
        }
    }

    /// Populate continuation input: `[prev_hash_acc[8], key[3], value[W], 0..]`.
    pub fn populate_continuation(
        &mut self,
        prev_hash_acc: &[BabyBear; 8],
        key: u64,
        value: &[BabyBear],
    ) {
        let key_limbs = decompose_key(key);
        self.perm_input = [BabyBear::ZERO; 16];
        self.perm_input[..8].copy_from_slice(prev_hash_acc);
        self.perm_input[8] = key_limbs[0];
        self.perm_input[9] = key_limbs[1];
        self.perm_input[10] = key_limbs[2];
        for (i, v) in value.iter().enumerate() {
            self.perm_input[11 + i] = *v;
        }
    }
}

/// Constrain hash chain input composition (local constraints only).
///
/// - `first_gate`: Expression gating first-entry composition (e.g., `is_real * is_first`).
/// - `cont_gate`: Expression gating continuation composition (e.g., `is_real * !is_first`).
/// - `key`: U64Limbs reference for the key in this row.
/// - `value`: Slice of value column references (length W, where W is the value width).
/// - `table_id`, `col_id`: Identity column references for first-entry composition.
///
/// Does NOT constrain the transition linking (perm_input[0..8] = prev hash_acc).
/// Use [`constrain_hash_chain_transition`] for that.
#[allow(clippy::too_many_arguments)]
pub fn constrain_hash_chain_input<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    hash_chain: &HashChainInput<AB::Var>,
    key: &U64Limbs<AB::Var>,
    value: &[AB::Var],
    table_id: AB::Var,
    col_id: AB::Var,
    first_gate: AB::Expr,
    cont_gate: AB::Expr,
) {
    // Continuation layout: [prev_hash[8], key[3], value[W], padding..] must fit in 16 FE.
    const { assert!(11 + W <= 16) }
    // ── First-entry composition ──
    // perm_input[0] = 0 (domain tag 0x00)
    builder.assert_zero(first_gate.clone() * hash_chain.perm_input[0].clone().into());
    // perm_input[1] = table_id
    builder.assert_zero(
        first_gate.clone() * (hash_chain.perm_input[1].clone().into() - table_id.clone().into()),
    );
    // perm_input[2] = col_id
    builder.assert_zero(
        first_gate.clone() * (hash_chain.perm_input[2].clone().into() - col_id.clone().into()),
    );
    // perm_input[3..6] = key limbs
    builder.assert_zero(
        first_gate.clone() * (hash_chain.perm_input[3].clone().into() - key.limb0.clone().into()),
    );
    builder.assert_zero(
        first_gate.clone() * (hash_chain.perm_input[4].clone().into() - key.limb1.clone().into()),
    );
    builder.assert_zero(
        first_gate.clone() * (hash_chain.perm_input[5].clone().into() - key.limb2.clone().into()),
    );
    // perm_input[6..6+W] = value
    for (i, v) in value.iter().enumerate() {
        builder.assert_zero(
            first_gate.clone() * (hash_chain.perm_input[6 + i].clone().into() - v.clone().into()),
        );
    }
    // perm_input[6+W..16] = 0 (padding)
    for i in (6 + W)..16 {
        builder.assert_zero(first_gate.clone() * hash_chain.perm_input[i].clone().into());
    }

    // ── Continuation composition (local part) ──
    // perm_input[8..11] = key limbs
    builder.assert_zero(
        cont_gate.clone() * (hash_chain.perm_input[8].clone().into() - key.limb0.clone().into()),
    );
    builder.assert_zero(
        cont_gate.clone() * (hash_chain.perm_input[9].clone().into() - key.limb1.clone().into()),
    );
    builder.assert_zero(
        cont_gate.clone() * (hash_chain.perm_input[10].clone().into() - key.limb2.clone().into()),
    );
    // perm_input[11..11+W] = value
    for (i, v) in value.iter().enumerate() {
        builder.assert_zero(
            cont_gate.clone() * (hash_chain.perm_input[11 + i].clone().into() - v.clone().into()),
        );
    }
    // perm_input[11+W..16] = 0 (padding)
    for i in (11 + W)..16 {
        builder.assert_zero(cont_gate.clone() * hash_chain.perm_input[i].clone().into());
    }
}

/// Constrain hash chain transition: continuation rows link to previous hash accumulator.
///
/// When `trans_gate` is active: `next.perm_input[0..8] = local.hash_acc[0..8]`.
///
/// `trans_gate` should be:
/// - SSMC: `both_real * (1 - next.is_first)`
/// - Merge: `both_real * (1 - tc_changed) * next.in_new * (1 - next.is_first_in_new)`
pub fn constrain_hash_chain_transition<AB: AirBuilder>(
    builder: &mut AB,
    next_perm_input: &[AB::Var; 16],
    local_hash_acc: &[AB::Var; 8],
    trans_gate: AB::Expr,
) {
    for j in 0..8 {
        builder.when_transition().assert_zero(
            trans_gate.clone()
                * (next_perm_input[j].clone().into() - local_hash_acc[j].clone().into()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_chain_populate_first() {
        let mut hc = HashChainInput {
            perm_input: [BabyBear::ZERO; 16],
        };
        let value = vec![BabyBear::new(10), BabyBear::new(20), BabyBear::new(30)];
        hc.populate_first(1, 2, 42, &value);
        assert_eq!(hc.perm_input[0], BabyBear::ZERO); // domain tag
        assert_eq!(hc.perm_input[1], BabyBear::new(1)); // table_id
        assert_eq!(hc.perm_input[2], BabyBear::new(2)); // col_id
        assert_eq!(hc.perm_input[3], BabyBear::new(42)); // key limb0
        assert_eq!(hc.perm_input[4], BabyBear::ZERO); // key limb1
        assert_eq!(hc.perm_input[5], BabyBear::ZERO); // key limb2
        assert_eq!(hc.perm_input[6], BabyBear::new(10)); // value[0]
        assert_eq!(hc.perm_input[7], BabyBear::new(20)); // value[1]
        assert_eq!(hc.perm_input[8], BabyBear::new(30)); // value[2]
        // Remaining should be zero padding
        for i in 9..16 {
            assert_eq!(hc.perm_input[i], BabyBear::ZERO);
        }
    }

    #[test]
    fn hash_chain_populate_continuation() {
        let mut hc = HashChainInput {
            perm_input: [BabyBear::ZERO; 16],
        };
        let prev = [
            BabyBear::new(100),
            BabyBear::new(101),
            BabyBear::new(102),
            BabyBear::new(103),
            BabyBear::new(104),
            BabyBear::new(105),
            BabyBear::new(106),
            BabyBear::new(107),
        ];
        let value = vec![BabyBear::new(50), BabyBear::new(60), BabyBear::new(70)];
        hc.populate_continuation(&prev, 99, &value);
        // perm_input[0..8] = prev_hash_acc
        for (i, p) in prev.iter().enumerate() {
            assert_eq!(hc.perm_input[i], *p);
        }
        // perm_input[8..11] = key limbs
        assert_eq!(hc.perm_input[8], BabyBear::new(99));
        assert_eq!(hc.perm_input[9], BabyBear::ZERO);
        assert_eq!(hc.perm_input[10], BabyBear::ZERO);
        // perm_input[11..14] = value
        assert_eq!(hc.perm_input[11], BabyBear::new(50));
        assert_eq!(hc.perm_input[12], BabyBear::new(60));
        assert_eq!(hc.perm_input[13], BabyBear::new(70));
        // Remaining padding
        assert_eq!(hc.perm_input[14], BabyBear::ZERO);
        assert_eq!(hc.perm_input[15], BabyBear::ZERO);
    }

    #[test]
    fn hash_chain_first_matches_ssmc_pattern() {
        // Verify our output matches the existing compose_ssmc_perm_input
        let mut hc = HashChainInput {
            perm_input: [BabyBear::ZERO; 16],
        };
        let table_id = 5u32;
        let col_id = 3u32;
        let key = (1u64 << 30) + 7; // multi-limb key
        let value = vec![BabyBear::new(1), BabyBear::new(2), BabyBear::new(3)];
        hc.populate_first(table_id, col_id, key, &value);

        // Verify key decomposition
        let mask30 = (1u64 << 30) - 1;
        assert_eq!(hc.perm_input[3], BabyBear::new((key & mask30) as u32));
        assert_eq!(
            hc.perm_input[4],
            BabyBear::new(((key >> 30) & mask30) as u32)
        );
        assert_eq!(hc.perm_input[5], BabyBear::new((key >> 60) as u32));
    }
}
