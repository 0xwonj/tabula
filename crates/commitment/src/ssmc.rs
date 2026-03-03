//! SSMC: Sorted Sparse Map Commitment for small columns.

use p3_baby_bear::BabyBear;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId};

use crate::field::encode_u64_limbs;
use crate::hasher::FieldHasher;

// ── Types ───────────────────────────────────────────────────────────────────

/// A single entry in an SSMC list.
#[derive(Clone, Debug)]
pub struct SsmcEntry {
    /// The row key.
    pub key: RowKey,
    /// The ComEnc-encoded value (w(T) field elements).
    pub value: Vec<BabyBear>,
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

// ── SsmcList ────────────────────────────────────────────────────────────────

impl SsmcList {
    /// Create an empty SSMC list for the given (table, col).
    pub fn new(table: TableId, col: ColId) -> Self {
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
    pub fn insert(&mut self, key: RowKey, value: Vec<BabyBear>) {
        match self.entries.binary_search_by_key(&key, |e| e.key) {
            Ok(i) => self.entries[i].value = value,
            Err(i) => self.entries.insert(i, SsmcEntry { key, value }),
        }
    }

    /// Remove a key. Returns true if it was present.
    pub fn remove(&mut self, key: RowKey) -> bool {
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
    pub fn commit<H: FieldHasher<F = BabyBear>>(&self, hasher: &H) -> SsmcCommitment<H::Digest> {
        let domain = crate::field::DOMAIN_SSMC;
        let mut input = Vec::new();
        input.push(BabyBear::new(self.table.0));
        input.push(BabyBear::new(self.col.0 as u32));
        input.push(BabyBear::new(self.entries.len() as u32));
        for entry in &self.entries {
            let key_limbs = encode_u64_limbs(entry.key.0);
            input.extend_from_slice(&key_limbs);
            input.extend_from_slice(&entry.value);
        }
        SsmcCommitment(hasher.hash_domain(domain, &input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hasher::MockFieldHasher;
    use p3_baby_bear::BabyBear;

    fn val(n: u32) -> Vec<BabyBear> {
        vec![BabyBear::new(n)]
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
