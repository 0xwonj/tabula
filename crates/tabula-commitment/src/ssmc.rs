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

/// Source of a key during merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeSource {
    /// Key only in old list: `(s1=0, s0=1)`.
    OldOnly,
    /// Key only in write set: `(s1=1, s0=0)`.
    WriteOnly,
    /// Key in both old and write set: `(s1=1, s0=1)`.
    Both,
}

/// A single step in the merge trace.
#[derive(Clone, Debug)]
pub struct MergeStep {
    /// The row key.
    pub key: RowKey,
    /// Source classification.
    pub source: MergeSource,
    /// Value from old list (None if write_only).
    pub old_val: Option<Vec<BabyBear>>,
    /// Value from write set (None if old_only or delete).
    pub write_val: Option<Vec<BabyBear>>,
    /// Value in new list (None if deleted).
    pub new_val: Option<Vec<BabyBear>>,
    /// Whether this key appears in the new list.
    pub in_new: bool,
}

/// Complete merge trace from old + writes → new.
#[derive(Clone, Debug)]
pub struct MergeTrace {
    /// One step per unique key across old and write set.
    pub steps: Vec<MergeStep>,
}

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

    /// 3-way merge: old list + write set → new list + commitment + trace.
    ///
    /// `writes` is a sorted list of (key, Option<value>). `None` value = delete.
    pub fn merge<H: FieldHasher<F = BabyBear>>(
        old: &SsmcList,
        writes: &[(RowKey, Option<Vec<BabyBear>>)],
        table: TableId,
        col: ColId,
        hasher: &H,
    ) -> (SsmcList, SsmcCommitment<H::Digest>, MergeTrace) {
        let mut steps = Vec::new();
        let mut new_entries = Vec::new();

        let mut oi = 0;
        let mut wi = 0;

        while oi < old.entries.len() || wi < writes.len() {
            let old_key = old.entries.get(oi).map(|e| e.key);
            let write_key = writes.get(wi).map(|(k, _)| *k);

            match (old_key, write_key) {
                (Some(ok), Some(wk)) if ok < wk => {
                    // Old only: carry to new.
                    let entry = &old.entries[oi];
                    new_entries.push(entry.clone());
                    steps.push(MergeStep {
                        key: ok,
                        source: MergeSource::OldOnly,
                        old_val: Some(entry.value.clone()),
                        write_val: None,
                        new_val: Some(entry.value.clone()),
                        in_new: true,
                    });
                    oi += 1;
                }
                (Some(ok), Some(wk)) if ok > wk => {
                    // Write only: add if not delete.
                    let (_, ref wval) = writes[wi];
                    let in_new = wval.is_some();
                    if let Some(v) = wval {
                        new_entries.push(SsmcEntry {
                            key: wk,
                            value: v.clone(),
                        });
                    }
                    steps.push(MergeStep {
                        key: wk,
                        source: MergeSource::WriteOnly,
                        old_val: None,
                        write_val: wval.clone(),
                        new_val: wval.clone(),
                        in_new,
                    });
                    wi += 1;
                }
                (Some(ok), Some(_wk)) => {
                    // Both: write overwrites old. Delete if write_val is None.
                    let old_entry = &old.entries[oi];
                    let (_, ref wval) = writes[wi];
                    let in_new = wval.is_some();
                    if let Some(v) = wval {
                        new_entries.push(SsmcEntry {
                            key: ok,
                            value: v.clone(),
                        });
                    }
                    steps.push(MergeStep {
                        key: ok,
                        source: MergeSource::Both,
                        old_val: Some(old_entry.value.clone()),
                        write_val: wval.clone(),
                        new_val: wval.clone(),
                        in_new,
                    });
                    oi += 1;
                    wi += 1;
                }
                (Some(_), None) => {
                    // Remaining old entries.
                    let entry = &old.entries[oi];
                    new_entries.push(entry.clone());
                    steps.push(MergeStep {
                        key: entry.key,
                        source: MergeSource::OldOnly,
                        old_val: Some(entry.value.clone()),
                        write_val: None,
                        new_val: Some(entry.value.clone()),
                        in_new: true,
                    });
                    oi += 1;
                }
                (None, Some(_)) => {
                    // Remaining write entries.
                    let (wk, ref wval) = writes[wi];
                    let in_new = wval.is_some();
                    if let Some(v) = wval {
                        new_entries.push(SsmcEntry {
                            key: wk,
                            value: v.clone(),
                        });
                    }
                    steps.push(MergeStep {
                        key: wk,
                        source: MergeSource::WriteOnly,
                        old_val: None,
                        write_val: wval.clone(),
                        new_val: wval.clone(),
                        in_new,
                    });
                    wi += 1;
                }
                (None, None) => unreachable!(),
            }
        }

        let new_list = SsmcList {
            table,
            col,
            entries: new_entries,
        };
        let commitment = new_list.commit(hasher);
        let trace = MergeTrace { steps };
        (new_list, commitment, trace)
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
    fn merge_old_only() {
        let h = MockFieldHasher;
        let old =
            SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 10), entry(1, 20)]).unwrap();
        let writes: Vec<(RowKey, Option<Vec<BabyBear>>)> = vec![];
        let (new_list, _, trace) = SsmcList::merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 2);
        assert_eq!(trace.steps.len(), 2);
        assert!(trace.steps.iter().all(|s| s.source == MergeSource::OldOnly));
    }

    #[test]
    fn merge_write_only() {
        let h = MockFieldHasher;
        let old = SsmcList::new(TableId(1), ColId(0));
        let writes = vec![(RowKey(0), Some(val(10))), (RowKey(1), Some(val(20)))];
        let (new_list, _, trace) = SsmcList::merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 2);
        assert!(
            trace
                .steps
                .iter()
                .all(|s| s.source == MergeSource::WriteOnly)
        );
    }

    #[test]
    fn merge_both_overwrites() {
        let h = MockFieldHasher;
        let old = SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 10)]).unwrap();
        let writes = vec![(RowKey(0), Some(val(99)))];
        let (new_list, _, trace) = SsmcList::merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 1);
        assert_eq!(new_list.entries()[0].value, val(99));
        assert_eq!(trace.steps[0].source, MergeSource::Both);
        assert!(trace.steps[0].in_new);
    }

    #[test]
    fn merge_delete() {
        let h = MockFieldHasher;
        let old = SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 10)]).unwrap();
        let writes: Vec<(RowKey, Option<Vec<BabyBear>>)> = vec![(RowKey(0), None)];
        let (new_list, _, trace) = SsmcList::merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 0);
        assert_eq!(trace.steps[0].source, MergeSource::Both);
        assert!(!trace.steps[0].in_new);
    }

    #[test]
    fn merge_complex_scenario() {
        let h = MockFieldHasher;
        let old = SsmcList::from_sorted(
            TableId(1),
            ColId(0),
            vec![entry(1, 10), entry(3, 30), entry(5, 50)],
        )
        .unwrap();
        let writes = vec![
            (RowKey(2), Some(val(20))), // write_only: new key
            (RowKey(3), Some(val(33))), // both: overwrite
            (RowKey(5), None),          // both: delete
            (RowKey(7), Some(val(70))), // write_only: new key
        ];
        let (new_list, _, trace) = SsmcList::merge(&old, &writes, TableId(1), ColId(0), &h);

        // New list: [1→10, 2→20, 3→33, 7→70] (key 5 deleted)
        assert_eq!(new_list.len(), 4);
        let keys: Vec<u64> = new_list.entries().iter().map(|e| e.key.0).collect();
        assert_eq!(keys, vec![1, 2, 3, 7]);

        // old keys: 1, 3, 5. write keys: 2, 3, 5, 7
        // merged: 1(old), 2(write), 3(both), 5(both), 7(write) = 5 steps
        assert_eq!(trace.steps.len(), 5);
    }

    #[test]
    fn merge_resulting_list_is_sorted() {
        let h = MockFieldHasher;
        let old =
            SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(10, 1), entry(30, 3)]).unwrap();
        let writes = vec![(RowKey(5), Some(val(5))), (RowKey(20), Some(val(2)))];
        let (new_list, _, _) = SsmcList::merge(&old, &writes, TableId(1), ColId(0), &h);
        let keys: Vec<u64> = new_list.entries().iter().map(|e| e.key.0).collect();
        assert_eq!(keys, vec![5, 10, 20, 30]);
    }

    #[test]
    fn merge_empty_old_plus_writes() {
        let h = MockFieldHasher;
        let old = SsmcList::new(TableId(1), ColId(0));
        let writes = vec![(RowKey(0), Some(val(1))), (RowKey(1), Some(val(2)))];
        let (new_list, _, _) = SsmcList::merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 2);
    }

    #[test]
    fn merge_old_plus_empty_writes() {
        let h = MockFieldHasher;
        let old =
            SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 1), entry(1, 2)]).unwrap();
        let writes: Vec<(RowKey, Option<Vec<BabyBear>>)> = vec![];
        let (new_list, c_new, _) = SsmcList::merge(&old, &writes, TableId(1), ColId(0), &h);
        // New list should equal old list.
        assert_eq!(new_list.len(), old.len());
        // Commitments should match.
        assert_eq!(c_new, old.commit(&h));
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
