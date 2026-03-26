use std::collections::BTreeMap;

use tabula_core::error::TabulaError;

pub(super) fn unique_fields<'a, T, Id: Copy + Ord>(
    values: &'a [T],
    id: impl Fn(&T) -> Id,
    message: &str,
) -> Result<BTreeMap<Id, &'a T>, TabulaError> {
    let mut map = BTreeMap::new();
    for value in values {
        let key = id(value);
        if map.insert(key, value).is_some() {
            return Err(TabulaError::InvalidIr(message.into()));
        }
    }
    Ok(map)
}
