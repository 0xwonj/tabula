use std::cmp::Ordering;
use std::sync::Arc;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::execution::NATIVE_MAX_KEY_FES;
use tabula_core::{CommittedKey, KeyOrderingFamily, TableId, TableKeyContract};

use crate::{EncodingRuntime, EncodingRuntimeRegistry, OrderedKeySegmentKind, TypedValue};

/// Native proof-visible committed-key payload width currently supported by the stack.
pub const NATIVE_KEY_PAYLOAD_WIDTH: usize = NATIVE_MAX_KEY_FES as usize;
/// Fixed-width committed-key payload used by the current proof stack.
pub type NativeKeyPayload = [KoalaBear; NATIVE_KEY_PAYLOAD_WIDTH];

/// Zero-filled native committed-key payload.
pub fn zero_key_payload() -> NativeKeyPayload {
    [KoalaBear::ZERO; NATIVE_KEY_PAYLOAD_WIDTH]
}

fn pad_key_payload(payload: &[KoalaBear]) -> Result<NativeKeyPayload, TabulaError> {
    if payload.len() > NATIVE_KEY_PAYLOAD_WIDTH {
        return Err(TabulaError::ProofError {
            phase: "key_payload_padding",
            detail: format!(
                "committed key payload width {} exceeds native ceiling {}",
                payload.len(),
                NATIVE_KEY_PAYLOAD_WIDTH
            ),
        });
    }
    let mut padded = zero_key_payload();
    for (index, limb) in payload.iter().copied().enumerate() {
        padded[index] = limb;
    }
    Ok(padded)
}

#[derive(Clone)]
struct ResolvedKeyCodecComponent {
    fixed_byte_width: usize,
    payload_width: usize,
    ordered_segment_kind: Option<OrderedKeySegmentKind>,
    encoding_runtime: Arc<dyn EncodingRuntime>,
}

/// Runtime-derived proof layout for one logical key component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyComponentPayloadLayout {
    /// Component index in declaration order.
    pub component_index: usize,
    /// Starting payload offset in the exact proof payload.
    pub payload_offset: usize,
    /// Component payload width in field elements.
    pub payload_width: usize,
    /// Ordered proof-comparison family for the segment when available.
    pub ordered_segment_kind: Option<OrderedKeySegmentKind>,
}

/// Executable table-key codec derived from one sealed table-key contract.
#[derive(Clone)]
pub struct TableKeyCodec {
    table_id: TableId,
    contract: TableKeyContract,
    components: Vec<ResolvedKeyCodecComponent>,
}

impl TableKeyCodec {
    /// Resolve one executable table-key codec from the sealed table contract.
    pub fn from_contract(
        table_id: TableId,
        contract: &TableKeyContract,
        encoding_runtimes: &EncodingRuntimeRegistry,
    ) -> Result<Self, TabulaError> {
        if contract.components.len() != contract.component_encoding_profile_ids.len() {
            return Err(TabulaError::InvalidIr(format!(
                "table {} key contract has {} components but {} key encodings",
                table_id.0,
                contract.components.len(),
                contract.component_encoding_profile_ids.len()
            )));
        }
        let mut components = Vec::with_capacity(contract.components.len());
        for (component, encoding_profile_id) in contract
            .components
            .iter()
            .zip(contract.component_encoding_profile_ids.iter().copied())
        {
            let encoding_runtime = encoding_runtimes.resolve(encoding_profile_id)?.clone();
            if encoding_runtime.descriptor().type_id != component.ty {
                return Err(TabulaError::TypeMismatch {
                    expected: format!("type {}", component.ty.0),
                    actual: format!(
                        "encoding {} for type {}",
                        encoding_profile_id.0,
                        encoding_runtime.descriptor().type_id.0
                    ),
                });
            }
            let Some(fixed_byte_width) = encoding_runtime.descriptor().fixed_byte_width else {
                return Err(TabulaError::FieldEncodingError(format!(
                    "table {} key encoding {} does not declare a fixed committed byte width",
                    table_id.0, encoding_profile_id.0
                )));
            };
            components.push(ResolvedKeyCodecComponent {
                fixed_byte_width: fixed_byte_width as usize,
                payload_width: encoding_runtime.trace_width(),
                ordered_segment_kind: encoding_runtime.ordered_key_segment_kind(),
                encoding_runtime,
            });
        }
        Ok(Self {
            table_id,
            contract: contract.clone(),
            components,
        })
    }

    /// Borrow the sealed contract.
    pub fn contract(&self) -> &TableKeyContract {
        &self.contract
    }

    /// Exact committed-key proof payload width for this table.
    pub fn exact_payload_width(&self) -> usize {
        self.contract.committed_layout.fe_width as usize
    }

    /// Runtime-derived payload layout for each key component.
    pub fn component_payload_layouts(&self) -> Vec<KeyComponentPayloadLayout> {
        let mut offset = 0usize;
        self.components
            .iter()
            .enumerate()
            .map(|(component_index, component)| {
                let layout = KeyComponentPayloadLayout {
                    component_index,
                    payload_offset: offset,
                    payload_width: component.payload_width,
                    ordered_segment_kind: component.ordered_segment_kind,
                };
                offset += component.payload_width;
                layout
            })
            .collect()
    }

    /// Encode one logical key tuple into a canonical committed key.
    pub fn encode_tuple(&self, values: &[TypedValue]) -> Result<CommittedKey, TabulaError> {
        if values.len() != self.components.len() {
            return Err(TabulaError::InvalidIr(format!(
                "table {} key expects {} components but received {}",
                self.table_id.0,
                self.components.len(),
                values.len()
            )));
        }
        let mut bytes = Vec::with_capacity(self.contract.committed_layout.byte_width as usize);
        for (value, component) in values.iter().zip(self.components.iter()) {
            let encoded = component.encoding_runtime.encode_committed_bytes(value)?;
            if encoded.len() != component.fixed_byte_width {
                return Err(TabulaError::FieldEncodingError(format!(
                    "table {} key encoding {} produced {} committed bytes, expected {}",
                    self.table_id.0,
                    component.encoding_runtime.encoding_profile_id().0,
                    encoded.len(),
                    component.fixed_byte_width
                )));
            }
            bytes.extend(encoded);
        }
        Ok(CommittedKey(bytes))
    }

    /// Decode one committed key back into logical key components.
    pub fn decode_key(&self, key: &CommittedKey) -> Result<Vec<TypedValue>, TabulaError> {
        if key.0.len() != self.contract.committed_layout.byte_width as usize {
            return Err(TabulaError::FieldEncodingError(format!(
                "table {} committed key has {} bytes, expected {}",
                self.table_id.0,
                key.0.len(),
                self.contract.committed_layout.byte_width
            )));
        }
        let mut values = Vec::with_capacity(self.components.len());
        let mut offset = 0usize;
        for component in &self.components {
            let end = offset + component.fixed_byte_width;
            values.push(
                component
                    .encoding_runtime
                    .decode_committed_bytes(&key.0[offset..end])?,
            );
            offset = end;
        }
        Ok(values)
    }

    /// Encode one committed key into its proof-visible FE payload.
    pub fn encode_proof_payload(&self, key: &CommittedKey) -> Result<Vec<KoalaBear>, TabulaError> {
        let values = self.decode_key(key)?;
        let mut payload = Vec::with_capacity(self.contract.committed_layout.fe_width as usize);
        for (value, component) in values.iter().zip(self.components.iter()) {
            payload.extend(
                component
                    .encoding_runtime
                    .encode_key_payload_elements(value)?,
            );
        }
        Ok(payload)
    }

    /// Encode one committed key into a native-width proof payload.
    pub fn encode_padded_proof_payload(
        &self,
        key: &CommittedKey,
    ) -> Result<NativeKeyPayload, TabulaError> {
        pad_key_payload(&self.encode_proof_payload(key)?)
    }

    /// Compare two committed keys according to the sealed ordering family.
    pub fn compare(&self, lhs: &CommittedKey, rhs: &CommittedKey) -> Result<Ordering, TabulaError> {
        match self.contract.ordering_family {
            KeyOrderingFamily::LexicographicByComponent => Ok(lhs.cmp(rhs)),
            KeyOrderingFamily::Opaque { ref family } => Err(TabulaError::InvalidIr(format!(
                "table {} uses unsupported opaque key ordering family '{}'",
                self.table_id.0, family
            ))),
        }
    }

    /// Compare two proof-visible key payloads according to this table's exact key width.
    ///
    /// This is intended for proof consumers that already operate on canonical
    /// padded payloads produced by this codec and need the same semantic order
    /// without re-decoding committed bytes.
    pub fn compare_padded_payloads(
        &self,
        lhs: &NativeKeyPayload,
        rhs: &NativeKeyPayload,
    ) -> Result<Ordering, TabulaError> {
        match self.contract.ordering_family {
            KeyOrderingFamily::LexicographicByComponent => {
                Ok(lhs[..self.exact_payload_width()].cmp(&rhs[..self.exact_payload_width()]))
            }
            KeyOrderingFamily::Opaque { ref family } => Err(TabulaError::InvalidIr(format!(
                "table {} uses unsupported opaque key ordering family '{}'",
                self.table_id.0, family
            ))),
        }
    }
}
