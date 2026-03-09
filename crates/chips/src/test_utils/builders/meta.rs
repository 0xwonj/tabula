//! Factory functions for MetaShard test data.

use tabula_commitment::NativeDigest;

use crate::shards::meta::trace::MetaShardRow;

/// Build a touched MetaShard row (non-empty → non-empty).
pub fn ms_touched(com_old: NativeDigest, com_new: NativeDigest) -> MetaShardRow {
    MetaShardRow {
        com_old,
        com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
        empty_read_count: 0,
    }
}

/// Build an untouched MetaShard row (com_new = com_old).
pub fn ms_untouched(com: NativeDigest) -> MetaShardRow {
    MetaShardRow {
        com_old: com,
        com_new: com,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: false,
        empty_read_count: 0,
    }
}

/// Build a MetaShard row for empty→non-empty transition.
pub fn ms_empty_to_nonempty(com_empty: NativeDigest, com_new: NativeDigest) -> MetaShardRow {
    MetaShardRow {
        com_old: com_empty,
        com_new,
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
        empty_read_count: 0,
    }
}

/// Build a MetaShard row for untouched empty column (both empty).
pub fn ms_both_empty(com_empty: NativeDigest) -> MetaShardRow {
    MetaShardRow {
        com_old: com_empty,
        com_new: com_empty,
        is_empty_old: true,
        is_empty_new: true,
        is_touched: false,
        empty_read_count: 0,
    }
}
