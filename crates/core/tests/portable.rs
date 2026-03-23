#![allow(missing_docs)]

use tabula_core::{PortableValue, TypeId};

#[test]
fn borsh_round_trip_portable_value() {
    let value = PortableValue::new(TypeId(7), vec![1, 2, 3, 4, 5]);
    let bytes = borsh::to_vec(&value).unwrap();
    let decoded: PortableValue = borsh::from_slice(&bytes).unwrap();
    assert_eq!(value, decoded);
}

#[test]
fn portable_value_accessors_preserve_type_and_payload() {
    let payload = vec![9, 8, 7, 6];
    let value = PortableValue::new(TypeId(3), payload.clone());

    assert_eq!(value.type_id(), TypeId(3));
    assert_eq!(value.payload(), payload.as_slice());
    assert_eq!(value.into_payload(), payload);
}
