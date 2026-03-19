//! Column state and metadata for the state commitment system.
//!
//! [`ColumnState`] holds the per-column commitment data structure (SSMC or SMT)
//! and provides operations for creating, querying, and updating column state.
//! [`ColumnMeta`] describes a column's commitment transition during a batch.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_koala_bear::default_koalabear_poseidon2_16;
use p3_symmetric::Permutation;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId};

use crate::field::{COL_DATA_SMT_DEPTH, DOMAIN_SMT, DOMAIN_SSMC, NativeDigest, encode_u64_limbs};
use crate::hasher::FieldHasher;
use crate::smt::SparseMerkleTree;
use crate::ssmc::{SsmcEntry, SsmcList};
use crate::ssmc_merge::MergeTrace;

// ── Scheme Tags ──────────────────────────────────────────────────────────────

/// Well-known scheme tag constants for commitment strategies.
///
/// Core schemes use values 0–9. Application-defined schemes should
/// use values >= 100 to avoid collisions.
pub mod scheme_tags {
    /// Small Sparse Map Commitment (hash chain).
    pub const SSMC: u16 = 0;
    /// Sparse Merkle Tree.
    pub const SMT: u16 = 1;
}

const SSMC_MAX_VALUE_FES: usize = 5;

fn build_ssmc_hash_input(
    table: TableId,
    col: ColId,
    key_limbs: &[KoalaBear],
    value: &[KoalaBear],
    prev: Option<&NativeDigest>,
) -> [KoalaBear; 16] {
    let mut input = [KoalaBear::ZERO; 16];
    match prev {
        None => {
            input[0] = KoalaBear::new(DOMAIN_SSMC);
            input[1] = KoalaBear::new(table.0);
            input[2] = KoalaBear::new(col.0 as u32);
            for (i, &limb) in key_limbs.iter().enumerate() {
                input[3 + i] = limb;
            }
            for (i, &v) in value.iter().enumerate() {
                input[6 + i] = v;
            }
        }
        Some(prev_digest) => {
            input[..8].copy_from_slice(&prev_digest.0);
            for (i, &limb) in key_limbs.iter().enumerate() {
                input[8 + i] = limb;
            }
            for (i, &v) in value.iter().enumerate() {
                input[11 + i] = v;
            }
        }
    }
    input
}

fn proof_ssmc_step(input: [KoalaBear; 16]) -> NativeDigest {
    let mut state = input;
    default_koalabear_poseidon2_16().permute_mut(&mut state);
    NativeDigest(core::array::from_fn(|i| state[i]))
}

/// Compute the proof-compatible column commitment for one committed column.
///
/// For SSMC this matches the AIR hash-chain layout used by the state shard.
/// For SMT the native tree root already matches the proof commitment.
pub fn proof_column_commitment<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>>(
    table: TableId,
    col: ColId,
    state: &ColumnState<H>,
) -> Result<NativeDigest, TabulaError> {
    match state {
        ColumnState::Ssmc(list) => {
            if list.table != table || list.col != col {
                return Err(TabulaError::ProofError {
                    phase: "commitment",
                    detail: format!(
                        "SSMC list identity mismatch: expected ({:?},{:?}), got ({:?},{:?})",
                        table, col, list.table, list.col
                    ),
                });
            }

            if list.entries().is_empty() {
                return Ok(proof_ssmc_step(build_ssmc_hash_input(
                    table,
                    col,
                    &[],
                    &[],
                    None,
                )));
            }

            let mut prev = None;
            for entry in list.entries() {
                if entry.value.len() > SSMC_MAX_VALUE_FES {
                    return Err(TabulaError::ProofError {
                        phase: "commitment",
                        detail: format!(
                            "value width {} exceeds SSMC continuation limit (max {SSMC_MAX_VALUE_FES})",
                            entry.value.len()
                        ),
                    });
                }

                let key_limbs = encode_u64_limbs(entry.key.0);
                prev = Some(proof_ssmc_step(build_ssmc_hash_input(
                    table,
                    col,
                    &key_limbs,
                    &entry.value,
                    prev.as_ref(),
                )));
            }

            Ok(prev.expect("non-empty entries must produce a hash"))
        }
        ColumnState::Smt(tree) => Ok(tree.root()),
    }
}

// ── ColumnState ──────────────────────────────────────────────────────────────

/// Per-column state holding the underlying data structure.
///
/// Architecture note: this enum is still closed over built-in layouts. A
/// fully open scheme platform will need a registry-backed replacement shared
/// by commitment materialization, witness generation, and root binding.
#[derive(Clone, Debug)]
pub enum ColumnState<H: FieldHasher> {
    /// SSMC-backed column (small).
    Ssmc(SsmcList),
    /// SMT-backed column (large).
    Smt(SparseMerkleTree<H>),
}

impl<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>> ColumnState<H> {
    /// Create a committed column state from pre-encoded entries.
    ///
    /// `scheme_tag` determines which data structure to use:
    /// - [`scheme_tags::SSMC`] → hash chain ([`SsmcList`])
    /// - [`scheme_tags::SMT`] → sparse Merkle tree
    ///
    /// Entries must be sorted by key (enforced by `SsmcList::from_sorted`).
    pub fn commit(
        hasher: &H,
        table: TableId,
        col: ColId,
        entries: Vec<(RowKey, Vec<KoalaBear>)>,
        scheme_tag: u16,
    ) -> Result<(Self, NativeDigest), TabulaError> {
        match scheme_tag {
            scheme_tags::SSMC => {
                let ssmc_entries: Vec<SsmcEntry> = entries
                    .into_iter()
                    .map(|(key, value)| SsmcEntry { key, value })
                    .collect();
                let list = SsmcList::from_sorted(table, col, ssmc_entries)?;
                let com = list.commit(hasher).0;
                Ok((Self::Ssmc(list), com))
            }
            scheme_tags::SMT => {
                let mut tree =
                    SparseMerkleTree::new(hasher.clone(), COL_DATA_SMT_DEPTH, DOMAIN_SMT);
                for (key, value_fes) in entries {
                    let leaf = hasher.hash(&value_fes);
                    tree.insert(key.0, leaf);
                }
                let root = tree.root();
                Ok((Self::Smt(tree), root))
            }
            _ => Err(TabulaError::ProofError {
                phase: "commitment",
                detail: format!("unknown scheme tag: {scheme_tag}"),
            }),
        }
    }

    /// Get the current commitment digest.
    pub fn commitment(&self, hasher: &H) -> NativeDigest {
        match self {
            Self::Ssmc(list) => list.commit(hasher).0,
            Self::Smt(tree) => tree.root(),
        }
    }

    /// Apply writes to produce a new state.
    ///
    /// Returns `(new_state, new_commitment, merge_trace)`.
    /// Merge trace is produced for SSMC columns; SMT columns return `None`.
    pub fn apply_writes(
        &self,
        hasher: &H,
        table: TableId,
        col: ColId,
        writes: &[(RowKey, Option<Vec<KoalaBear>>)],
    ) -> (Self, NativeDigest, Option<MergeTrace>) {
        match self {
            Self::Ssmc(old_list) => {
                let (new_list, com, trace) =
                    crate::ssmc_merge::merge(old_list, writes, table, col, hasher);
                (Self::Ssmc(new_list), com.0, Some(trace))
            }
            Self::Smt(old_tree) => {
                let mut tree = old_tree.clone();
                for (key, value) in writes {
                    match value {
                        Some(fes) => {
                            let leaf = hasher.hash(fes);
                            tree.insert(key.0, leaf);
                        }
                        None => {
                            tree.remove(key.0);
                        }
                    }
                }
                let root = tree.root();
                (Self::Smt(tree), root, None)
            }
        }
    }

    /// Whether the column has zero entries.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Ssmc(list) => list.is_empty(),
            Self::Smt(tree) => tree.is_empty(),
        }
    }

    /// Scheme tag for this column's commitment strategy.
    pub fn scheme_tag(&self) -> u16 {
        match self {
            Self::Ssmc(_) => scheme_tags::SSMC,
            Self::Smt(_) => scheme_tags::SMT,
        }
    }
}

// ── ColumnMeta ───────────────────────────────────────────────────────────────

/// Metadata for a column's commitment transition during a batch.
///
/// Corresponds to the ColumnMeta table in the proof spec.
#[derive(Clone, Debug)]
pub struct ColumnMeta {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// Scheme tag (see [`scheme_tags`]).
    pub tag: u16,
    /// Commitment before the batch.
    pub com_old: NativeDigest,
    /// Commitment after the batch.
    pub com_new: NativeDigest,
    /// Column was empty before the batch.
    pub is_empty_old: bool,
    /// Column is empty after the batch.
    pub is_empty_new: bool,
    /// Column was modified in this batch.
    pub is_touched: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hasher::MockFieldHasher;

    fn val(n: u32) -> Vec<KoalaBear> {
        vec![KoalaBear::new(n)]
    }

    fn entries(pairs: &[(u64, u32)]) -> Vec<(RowKey, Vec<KoalaBear>)> {
        pairs.iter().map(|&(k, v)| (RowKey(k), val(v))).collect()
    }

    #[test]
    fn proof_column_commitment_matches_ssmc_commitment_for_empty_state() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            vec![],
            scheme_tags::SSMC,
        )
        .unwrap();

        assert!(proof_column_commitment(TableId(1), ColId(0), &state).is_ok());
    }

    #[test]
    fn proof_column_commitment_matches_smt_root() {
        let (state, root) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            scheme_tags::SMT,
        )
        .unwrap();

        assert_eq!(
            proof_column_commitment(TableId(1), ColId(0), &state).unwrap(),
            root
        );
    }

    #[test]
    fn commit_ssmc_creates_ssmc_state() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            scheme_tags::SSMC,
        )
        .unwrap();
        assert_eq!(state.scheme_tag(), scheme_tags::SSMC);
        assert!(!state.is_empty());
    }

    #[test]
    fn commit_smt_creates_smt_state() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            scheme_tags::SMT,
        )
        .unwrap();
        assert_eq!(state.scheme_tag(), scheme_tags::SMT);
        assert!(!state.is_empty());
    }

    #[test]
    fn commit_empty_creates_empty_state() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[]),
            scheme_tags::SSMC,
        )
        .unwrap();
        assert!(state.is_empty());
    }

    #[test]
    fn commit_unknown_tag_errors() {
        let result = ColumnState::commit(&MockFieldHasher, TableId(1), ColId(0), entries(&[]), 99);
        assert!(result.is_err());
    }

    #[test]
    fn apply_writes_ssmc_updates_commitment() {
        let (state, com_old) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1)]),
            scheme_tags::SSMC,
        )
        .unwrap();
        let writes = vec![(RowKey(1), Some(val(2)))];
        let (new_state, com_new, trace) =
            state.apply_writes(&MockFieldHasher, TableId(1), ColId(0), &writes);
        assert_ne!(com_old, com_new);
        assert_eq!(new_state.scheme_tag(), scheme_tags::SSMC);
        assert!(trace.is_some());
    }

    #[test]
    fn apply_writes_smt_updates_commitment() {
        let (state, com_old) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            scheme_tags::SMT,
        )
        .unwrap();
        let writes = vec![(RowKey(2), Some(val(3)))];
        let (new_state, com_new, trace) =
            state.apply_writes(&MockFieldHasher, TableId(1), ColId(0), &writes);
        assert_ne!(com_old, com_new);
        assert_eq!(new_state.scheme_tag(), scheme_tags::SMT);
        assert!(trace.is_none());
    }

    #[test]
    fn apply_writes_delete_removes_entry() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            scheme_tags::SSMC,
        )
        .unwrap();
        let writes = vec![(RowKey(0), None)];
        let (new_state, _, trace) =
            state.apply_writes(&MockFieldHasher, TableId(1), ColId(0), &writes);
        assert!(!new_state.is_empty());
        let trace = trace.unwrap();
        assert_eq!(trace.steps.len(), 2);
        assert!(!trace.steps[0].in_new);
    }

    #[test]
    fn round_trip_ssmc_empty_writes() {
        let (state, com) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            scheme_tags::SSMC,
        )
        .unwrap();
        let (_, com_after, _) = state.apply_writes(&MockFieldHasher, TableId(1), ColId(0), &[]);
        assert_eq!(com, com_after);
    }

    #[test]
    fn round_trip_smt_empty_writes() {
        let (state, com) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            scheme_tags::SMT,
        )
        .unwrap();
        let (_, com_after, _) = state.apply_writes(&MockFieldHasher, TableId(1), ColId(0), &[]);
        assert_eq!(com, com_after);
    }

    #[test]
    fn commitment_matches_commit_output() {
        let (state, com) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 42)]),
            scheme_tags::SSMC,
        )
        .unwrap();
        assert_eq!(state.commitment(&MockFieldHasher), com);
    }
}
