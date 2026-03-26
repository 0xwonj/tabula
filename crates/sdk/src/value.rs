use borsh::BorshDeserialize as _;
use tabula_core::PortableValue;
use tabula_ir as ir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};
use tabula_types::{bool_portable, bytes32_portable, i64_portable, u64_portable};

use crate::error::SdkError;
use crate::schema::ParameterHandle;

pub trait EncodeValue {
    fn encode_for(self, expected: ir::TypeRef) -> Result<PortableValue, SdkError>;
}

pub trait DecodeValue: Sized {
    fn decode_from(value: &PortableValue) -> Result<Self, SdkError>;
}

pub trait EncodeArgs {
    fn encode_args(self, expected: &[ParameterHandle]) -> Result<Vec<PortableValue>, SdkError>;
}

impl EncodeValue for bool {
    fn encode_for(self, expected: ir::TypeRef) -> Result<PortableValue, SdkError> {
        if expected != TYPE_BOOL_ID {
            return Err(SdkError::ValueEncoding {
                detail: format!("expected type {} but received bool", expected.0),
            });
        }
        Ok(bool_portable(self))
    }
}

impl EncodeValue for u64 {
    fn encode_for(self, expected: ir::TypeRef) -> Result<PortableValue, SdkError> {
        if expected != TYPE_U64_ID {
            return Err(SdkError::ValueEncoding {
                detail: format!("expected type {} but received u64", expected.0),
            });
        }
        Ok(u64_portable(self))
    }
}

impl EncodeValue for i64 {
    fn encode_for(self, expected: ir::TypeRef) -> Result<PortableValue, SdkError> {
        if expected != TYPE_I64_ID {
            return Err(SdkError::ValueEncoding {
                detail: format!("expected type {} but received i64", expected.0),
            });
        }
        Ok(i64_portable(self))
    }
}

impl EncodeValue for [u8; 32] {
    fn encode_for(self, expected: ir::TypeRef) -> Result<PortableValue, SdkError> {
        if expected != TYPE_BYTES32_ID {
            return Err(SdkError::ValueEncoding {
                detail: format!("expected type {} but received [u8; 32]", expected.0),
            });
        }
        Ok(bytes32_portable(self))
    }
}

impl EncodeValue for PortableValue {
    fn encode_for(self, expected: ir::TypeRef) -> Result<PortableValue, SdkError> {
        if self.type_id() != expected {
            return Err(SdkError::ValueEncoding {
                detail: format!(
                    "portable value carries type {} but schema expects {}",
                    self.type_id().0,
                    expected.0,
                ),
            });
        }
        Ok(self)
    }
}

impl EncodeValue for &PortableValue {
    fn encode_for(self, expected: ir::TypeRef) -> Result<PortableValue, SdkError> {
        self.clone().encode_for(expected)
    }
}

impl DecodeValue for bool {
    fn decode_from(value: &PortableValue) -> Result<Self, SdkError> {
        if value.type_id() != TYPE_BOOL_ID {
            return Err(SdkError::ValueDecoding {
                detail: format!("expected bool but found type {}", value.type_id().0),
            });
        }
        bool::try_from_slice(value.payload()).map_err(|error| SdkError::ValueDecoding {
            detail: format!("failed to decode bool: {error}"),
        })
    }
}

impl DecodeValue for u64 {
    fn decode_from(value: &PortableValue) -> Result<Self, SdkError> {
        if value.type_id() != TYPE_U64_ID {
            return Err(SdkError::ValueDecoding {
                detail: format!("expected u64 but found type {}", value.type_id().0),
            });
        }
        u64::try_from_slice(value.payload()).map_err(|error| SdkError::ValueDecoding {
            detail: format!("failed to decode u64: {error}"),
        })
    }
}

impl DecodeValue for i64 {
    fn decode_from(value: &PortableValue) -> Result<Self, SdkError> {
        if value.type_id() != TYPE_I64_ID {
            return Err(SdkError::ValueDecoding {
                detail: format!("expected i64 but found type {}", value.type_id().0),
            });
        }
        i64::try_from_slice(value.payload()).map_err(|error| SdkError::ValueDecoding {
            detail: format!("failed to decode i64: {error}"),
        })
    }
}

impl DecodeValue for [u8; 32] {
    fn decode_from(value: &PortableValue) -> Result<Self, SdkError> {
        if value.type_id() != TYPE_BYTES32_ID {
            return Err(SdkError::ValueDecoding {
                detail: format!("expected [u8; 32] but found type {}", value.type_id().0),
            });
        }
        <[u8; 32]>::try_from_slice(value.payload()).map_err(|error| SdkError::ValueDecoding {
            detail: format!("failed to decode [u8; 32]: {error}"),
        })
    }
}

fn encode_values<I, V>(
    expected: &[ParameterHandle],
    values: I,
) -> Result<Vec<PortableValue>, SdkError>
where
    I: IntoIterator<Item = V>,
    V: EncodeValue,
{
    let mut encoded = Vec::with_capacity(expected.len());
    let mut values = values.into_iter();
    for param in expected {
        let Some(value) = values.next() else {
            return Err(SdkError::ValueEncoding {
                detail: format!(
                    "entry expects {} params but fewer were provided",
                    expected.len()
                ),
            });
        };
        encoded.push(value.encode_for(param.ty())?);
    }
    if values.next().is_some() {
        return Err(SdkError::ValueEncoding {
            detail: format!(
                "entry expects {} params but more were provided",
                expected.len()
            ),
        });
    }
    Ok(encoded)
}

impl EncodeArgs for () {
    fn encode_args(self, expected: &[ParameterHandle]) -> Result<Vec<PortableValue>, SdkError> {
        encode_values(expected, std::iter::empty::<PortableValue>())
    }
}

impl<V, const N: usize> EncodeArgs for [V; N]
where
    V: EncodeValue,
{
    fn encode_args(self, expected: &[ParameterHandle]) -> Result<Vec<PortableValue>, SdkError> {
        encode_values(expected, self)
    }
}

impl<V> EncodeArgs for Vec<V>
where
    V: EncodeValue,
{
    fn encode_args(self, expected: &[ParameterHandle]) -> Result<Vec<PortableValue>, SdkError> {
        encode_values(expected, self)
    }
}

impl<V> EncodeArgs for &[V]
where
    V: Clone + EncodeValue,
{
    fn encode_args(self, expected: &[ParameterHandle]) -> Result<Vec<PortableValue>, SdkError> {
        encode_values(expected, self.iter().cloned())
    }
}

fn encode_tuple_value<V: EncodeValue>(
    expected: &[ParameterHandle],
    index: usize,
    value: V,
) -> Result<PortableValue, SdkError> {
    let Some(param) = expected.get(index) else {
        return Err(SdkError::ValueEncoding {
            detail: format!(
                "entry expects {} params but more were provided",
                expected.len()
            ),
        });
    };
    value.encode_for(param.ty())
}

macro_rules! impl_encode_args_for_tuple {
    ($len:expr => $($index:tt : $name:ident),+ $(,)?) => {
        impl<$($name),+> EncodeArgs for ($($name,)+)
        where
            $($name: EncodeValue,)+
        {
            #[allow(non_snake_case)]
            fn encode_args(self, expected: &[ParameterHandle]) -> Result<Vec<PortableValue>, SdkError> {
                if expected.len() != $len {
                    return Err(SdkError::ValueEncoding {
                        detail: format!(
                            "entry expects {} params but {} were provided",
                            expected.len(),
                            $len
                        ),
                    });
                }
                let ($($name,)+) = self;
                Ok(vec![$(encode_tuple_value(expected, $index, $name)?),+])
            }
        }
    };
}

impl_encode_args_for_tuple!(1 => 0: A0);
impl_encode_args_for_tuple!(2 => 0: A0, 1: A1);
impl_encode_args_for_tuple!(3 => 0: A0, 1: A1, 2: A2);
impl_encode_args_for_tuple!(4 => 0: A0, 1: A1, 2: A2, 3: A3);
impl_encode_args_for_tuple!(5 => 0: A0, 1: A1, 2: A2, 3: A3, 4: A4);
impl_encode_args_for_tuple!(6 => 0: A0, 1: A1, 2: A2, 3: A3, 4: A4, 5: A5);
