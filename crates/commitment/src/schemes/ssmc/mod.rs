//! SSMC: Sorted Sparse Map Commitment for small columns.

mod merge;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_koala_bear::default_koalabear_poseidon2_16;
use p3_symmetric::Permutation;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId};

use crate::primitives::FieldHasher;
use crate::primitives::{DOMAIN_SSMC, NativeDigest, encode_u64_limbs};

// ── Types ───────────────────────────────────────────────────────────────────

/// A single entry in an SSMC list.
#[derive(Clone, Debug)]
pub struct SsmcEntry {
    /// The row key.
    pub key: RowKey,
    /// The ComEnc-encoded value (w(T) field elements).
    pub value: Vec<KoalaBear>,
}

/// A sorted list of (key, value) entries for a single (table, col).
///
/// Invariant: entries are sorted by key, with no duplicate keys.
#[derive(Clone, Debug)]
pub struct SsmcList {
    /// The table this list belongs to.
    pub table: TableId,
    /// The column this list belongs to.
    pub col: ColId,
    /// Sorted entries. Invariant: strictly ascending by key.
    entries: Vec<SsmcEntry>,
}

/// The commitment digest for an SSMC list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SsmcCommitment<D>(pub D);

const SSMC_MAX_VALUE_FES: usize = 5;

fn build_proof_hash_input(
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

fn proof_step(input: [KoalaBear; 16]) -> NativeDigest {
    let mut state = input;
    default_koalabear_poseidon2_16().permute_mut(&mut state);
    NativeDigest(core::array::from_fn(|i| state[i]))
}

fn proof_commitment(
    table: TableId,
    col: ColId,
    list: &SsmcList,
) -> Result<NativeDigest, TabulaError> {
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
        return Ok(proof_step(build_proof_hash_input(
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
        prev = Some(proof_step(build_proof_hash_input(
            table,
            col,
            &key_limbs,
            &entry.value,
            prev.as_ref(),
        )));
    }

    Ok(prev.expect("non-empty entries must produce a hash"))
}

// ── SsmcList ────────────────────────────────────────────────────────────────

impl SsmcList {
    /// Create an empty SSMC list for the given (table, col).
    #[cfg(test)]
    pub(crate) fn new(table: TableId, col: ColId) -> Self {
        Self {
            table,
            col,
            entries: Vec::new(),
        }
    }

    /// Create from pre-sorted entries. Validates sorted + unique keys.
    pub fn from_sorted(
        table: TableId,
        col: ColId,
        entries: Vec<SsmcEntry>,
    ) -> Result<Self, TabulaError> {
        for i in 1..entries.len() {
            if entries[i].key <= entries[i - 1].key {
                return Err(TabulaError::ConsistencyError(format!(
                    "SSMC entries not strictly sorted at index {i}: {:?} >= {:?}",
                    entries[i].key,
                    entries[i - 1].key
                )));
            }
        }
        Ok(Self {
            table,
            col,
            entries,
        })
    }

    /// Insert a single entry, maintaining sort order. Overwrites if key exists.
    #[cfg(test)]
    pub(crate) fn insert(&mut self, key: RowKey, value: Vec<KoalaBear>) {
        match self.entries.binary_search_by_key(&key, |e| e.key) {
            Ok(i) => self.entries[i].value = value,
            Err(i) => self.entries.insert(i, SsmcEntry { key, value }),
        }
    }

    /// Remove a key. Returns true if it was present.
    #[cfg(test)]
    pub(crate) fn remove(&mut self, key: RowKey) -> bool {
        match self.entries.binary_search_by_key(&key, |e| e.key) {
            Ok(i) => {
                self.entries.remove(i);
                true
            }
            Err(_) => false,
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Read-only access to entries.
    pub fn entries(&self) -> &[SsmcEntry] {
        &self.entries
    }

    /// Apply writes to produce a new list and native commitment.
    pub fn apply_writes<H: FieldHasher<F = KoalaBear>>(
        &self,
        writes: &[(RowKey, Option<Vec<KoalaBear>>)],
        hasher: &H,
    ) -> (SsmcList, SsmcCommitment<H::Digest>) {
        merge::merge(self, writes, self.table, self.col, hasher)
    }

    /// Compute the proof-visible commitment for this list.
    pub fn proof_commitment(&self) -> Result<NativeDigest, TabulaError> {
        proof_commitment(self.table, self.col, self)
    }

    /// Create from pre-sorted entries without validation.
    ///
    /// Used internally by the merge algorithm, which guarantees sorted output.
    pub(crate) fn from_entries(table: TableId, col: ColId, entries: Vec<SsmcEntry>) -> Self {
        Self {
            table,
            col,
            entries,
        }
    }

    /// Compute the SSMC commitment.
    ///
    /// Formula: `hash([DOMAIN_SSMC, t, c, n, k_0[0..3], v_0[0..w], k_1[0..3], ...])`
    pub fn commit<H: FieldHasher<F = KoalaBear>>(&self, hasher: &H) -> SsmcCommitment<H::Digest> {
        let mut input = Vec::new();
        input.push(KoalaBear::new(self.table.0));
        input.push(KoalaBear::new(self.col.0 as u32));
        input.push(KoalaBear::new(self.entries.len() as u32));
        for entry in &self.entries {
            let key_limbs = encode_u64_limbs(entry.key.0);
            input.extend_from_slice(&key_limbs);
            input.extend_from_slice(&entry.value);
        }
        SsmcCommitment(hasher.hash_domain(DOMAIN_SSMC, &input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::MockFieldHasher;
    use p3_koala_bear::KoalaBear;

    fn val(n: u32) -> Vec<KoalaBear> {
        vec![KoalaBear::new(n)]
    }

    fn entry(key: u64, n: u32) -> SsmcEntry {
        SsmcEntry {
            key: RowKey(key),
            value: val(n),
        }
    }

    #[test]
    fn empty_list_commitment_deterministic() {
        let h = MockFieldHasher;
        let l1 = SsmcList::new(TableId(1), ColId(0));
        let l2 = SsmcList::new(TableId(1), ColId(0));
        assert_eq!(l1.commit(&h), l2.commit(&h));
    }

    #[test]
    fn single_entry_commit() {
        let h = MockFieldHasher;
        let mut list = SsmcList::new(TableId(1), ColId(0));
        list.insert(RowKey(0), val(42));
        let c1 = list.commit(&h);
        let c2 = list.commit(&h);
        assert_eq!(c1, c2);
    }

    #[test]
    fn multi_entry_commit_deterministic() {
        let h = MockFieldHasher;
        let entries = vec![entry(0, 10), entry(1, 20), entry(2, 30)];
        let l1 = SsmcList::from_sorted(TableId(1), ColId(0), entries.clone()).unwrap();
        let l2 = SsmcList::from_sorted(TableId(1), ColId(0), entries).unwrap();
        assert_eq!(l1.commit(&h), l2.commit(&h));
    }

    #[test]
    fn unsorted_input_rejected() {
        let entries = vec![entry(2, 20), entry(1, 10)];
        assert!(SsmcList::from_sorted(TableId(1), ColId(0), entries).is_err());
    }

    #[test]
    fn duplicate_keys_rejected() {
        let entries = vec![entry(1, 10), entry(1, 20)];
        assert!(SsmcList::from_sorted(TableId(1), ColId(0), entries).is_err());
    }

    #[test]
    fn insert_maintains_order() {
        let mut list = SsmcList::new(TableId(1), ColId(0));
        list.insert(RowKey(5), val(5));
        list.insert(RowKey(1), val(1));
        list.insert(RowKey(3), val(3));
        let keys: Vec<u64> = list.entries().iter().map(|e| e.key.0).collect();
        assert_eq!(keys, vec![1, 3, 5]);
    }

    #[test]
    fn insert_overwrites_existing() {
        let mut list = SsmcList::new(TableId(1), ColId(0));
        list.insert(RowKey(1), val(10));
        list.insert(RowKey(1), val(20));
        assert_eq!(list.len(), 1);
        assert_eq!(list.entries()[0].value, val(20));
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut list = SsmcList::new(TableId(1), ColId(0));
        assert!(!list.remove(RowKey(0)));
    }

    #[test]
    fn different_table_col_different_commitment() {
        let h = MockFieldHasher;
        let l1 = SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 1)]).unwrap();
        let l2 = SsmcList::from_sorted(TableId(2), ColId(0), vec![entry(0, 1)]).unwrap();
        assert_ne!(l1.commit(&h), l2.commit(&h));
    }
}
