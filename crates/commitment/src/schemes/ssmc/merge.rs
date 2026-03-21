//! SSMC 3-way merge algorithm: old list + write set -> new list.

use p3_koala_bear::KoalaBear;

use tabula_core::{ColId, RowKey, TableId};

use crate::primitives::FieldHasher;
use crate::schemes::ssmc::{SsmcCommitment, SsmcEntry, SsmcList};

// ── Merge algorithm ─────────────────────────────────────────────────────────

/// 3-way merge: old list + write set -> new list + commitment.
///
/// `writes` is a sorted list of (key, Option<value>). `None` value = delete.
pub(crate) fn merge<H: FieldHasher<F = KoalaBear>>(
    old: &SsmcList,
    writes: &[(RowKey, Option<Vec<KoalaBear>>)],
    table: TableId,
    col: ColId,
    hasher: &H,
) -> (SsmcList, SsmcCommitment<H::Digest>) {
    let mut new_entries = Vec::new();

    let mut oi = 0;
    let mut wi = 0;

    while oi < old.entries().len() || wi < writes.len() {
        let old_key = old.entries().get(oi).map(|e| e.key);
        let write_key = writes.get(wi).map(|(k, _)| *k);

        match (old_key, write_key) {
            (Some(ok), Some(wk)) if ok < wk => {
                // Old only: carry to new.
                let entry = &old.entries()[oi];
                new_entries.push(entry.clone());
                oi += 1;
            }
            (Some(ok), Some(wk)) if ok > wk => {
                // Write only: add if not delete.
                let (_, ref wval) = writes[wi];
                if let Some(v) = wval {
                    new_entries.push(SsmcEntry {
                        key: wk,
                        value: v.clone(),
                    });
                }
                wi += 1;
            }
            (Some(ok), Some(_wk)) => {
                // Both: write overwrites old. Delete if write_val is None.
                let (_, ref wval) = writes[wi];
                if let Some(v) = wval {
                    new_entries.push(SsmcEntry {
                        key: ok,
                        value: v.clone(),
                    });
                }
                oi += 1;
                wi += 1;
            }
            (Some(_), None) => {
                // Remaining old entries.
                let entry = &old.entries()[oi];
                new_entries.push(entry.clone());
                oi += 1;
            }
            (None, Some(_)) => {
                // Remaining write entries.
                let (wk, ref wval) = writes[wi];
                if let Some(v) = wval {
                    new_entries.push(SsmcEntry {
                        key: wk,
                        value: v.clone(),
                    });
                }
                wi += 1;
            }
            (None, None) => unreachable!(),
        }
    }

    let new_list = SsmcList::from_entries(table, col, new_entries);
    let commitment = new_list.commit(hasher);
    (new_list, commitment)
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
    fn merge_old_only() {
        let h = MockFieldHasher;
        let old =
            SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 10), entry(1, 20)]).unwrap();
        let writes: Vec<(RowKey, Option<Vec<KoalaBear>>)> = vec![];
        let (new_list, _) = merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 2);
        assert_eq!(new_list.entries()[0].value, val(10));
        assert_eq!(new_list.entries()[1].value, val(20));
    }

    #[test]
    fn merge_write_only() {
        let h = MockFieldHasher;
        let old = SsmcList::new(TableId(1), ColId(0));
        let writes = vec![(RowKey(0), Some(val(10))), (RowKey(1), Some(val(20)))];
        let (new_list, _) = merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 2);
        assert_eq!(new_list.entries()[0].key, RowKey(0));
        assert_eq!(new_list.entries()[1].key, RowKey(1));
    }

    #[test]
    fn merge_both_overwrites() {
        let h = MockFieldHasher;
        let old = SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 10)]).unwrap();
        let writes = vec![(RowKey(0), Some(val(99)))];
        let (new_list, _) = merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 1);
        assert_eq!(new_list.entries()[0].value, val(99));
    }

    #[test]
    fn merge_delete() {
        let h = MockFieldHasher;
        let old = SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 10)]).unwrap();
        let writes: Vec<(RowKey, Option<Vec<KoalaBear>>)> = vec![(RowKey(0), None)];
        let (new_list, _) = merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 0);
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
        let (new_list, _) = merge(&old, &writes, TableId(1), ColId(0), &h);

        // New list: [1->10, 2->20, 3->33, 7->70] (key 5 deleted)
        assert_eq!(new_list.len(), 4);
        let keys: Vec<u64> = new_list.entries().iter().map(|e| e.key.0).collect();
        assert_eq!(keys, vec![1, 2, 3, 7]);
    }

    #[test]
    fn merge_resulting_list_is_sorted() {
        let h = MockFieldHasher;
        let old =
            SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(10, 1), entry(30, 3)]).unwrap();
        let writes = vec![(RowKey(5), Some(val(5))), (RowKey(20), Some(val(2)))];
        let (new_list, _) = merge(&old, &writes, TableId(1), ColId(0), &h);
        let keys: Vec<u64> = new_list.entries().iter().map(|e| e.key.0).collect();
        assert_eq!(keys, vec![5, 10, 20, 30]);
    }

    #[test]
    fn merge_empty_old_plus_writes() {
        let h = MockFieldHasher;
        let old = SsmcList::new(TableId(1), ColId(0));
        let writes = vec![(RowKey(0), Some(val(1))), (RowKey(1), Some(val(2)))];
        let (new_list, _) = merge(&old, &writes, TableId(1), ColId(0), &h);
        assert_eq!(new_list.len(), 2);
    }

    #[test]
    fn merge_old_plus_empty_writes() {
        let h = MockFieldHasher;
        let old =
            SsmcList::from_sorted(TableId(1), ColId(0), vec![entry(0, 1), entry(1, 2)]).unwrap();
        let writes: Vec<(RowKey, Option<Vec<KoalaBear>>)> = vec![];
        let (new_list, c_new) = merge(&old, &writes, TableId(1), ColId(0), &h);
        // New list should equal old list.
        assert_eq!(new_list.len(), old.len());
        // Commitments should match.
        assert_eq!(c_new, old.commit(&h));
    }
}
