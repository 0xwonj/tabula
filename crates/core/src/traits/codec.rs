//! Value encoding/decoding trait abstraction.

use crate::error::TabulaError;
use crate::{Value, ValueType};

/// Encodes/decodes application-level Values to/from field elements.
pub trait ValueCodec: Send + Sync {
    /// The field element representation.
    type FieldRepr: Clone + Send + Sync;

    /// Encode a Value into field elements.
    fn encode(&self, value: &Value) -> Result<Vec<Self::FieldRepr>, TabulaError>;

    /// Decode field elements back into a Value.
    fn decode(
        &self,
        field_elements: &[Self::FieldRepr],
        target_type: ValueType,
    ) -> Result<Value, TabulaError>;

    /// How many field elements a given type requires.
    fn field_elements_per(&self, value_type: ValueType) -> usize;
}
