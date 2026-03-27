//! Dedicated proof lane for canonical IR hash semantics.
//!
//! This chip proves the portable byte-level `hash_ir` contract used by runtime
//! execution. It models the exact overwrite-mode Poseidon sponge over KoalaBear
//! bytes and relays the final digest back to the execution lane over a private
//! hash bus.
use p3_field::PrimeField32;
use p3_koala_bear::KoalaBear;

use tabula_commitment::{NativeDigest, PoseidonHasher};
use tabula_core::PortableValue;
use tabula_core::error::TabulaError;
use tabula_core::traits::{DOMAIN_TAG_HASH_IR, Hasher};
use tabula_stark::air::interaction::BusId;
use tabula_stark::chips::ChipId;

/// Witness-store label for canonical IR hash calls.
pub const IR_HASH_WITNESS_LABEL: &str = "ir_hash_calls";

/// Dedicated chip id for the canonical IR hash lane.
pub const IR_HASH_CHIP_ID: ChipId = ChipId(91);

/// Private execution-tier bus used to relay hash digests from execution rows to the IR hash lane.
pub const IR_HASH_BUS: BusId = BusId(100);

pub(super) const IR_HASH_RATE: usize = 8;

/// Witness record for one canonical `hash_ir` instruction evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrHashCall {
    /// Zero-based transaction index.
    pub tx_index: u32,
    /// Zero-based instruction index in the tx body.
    pub instruction_index: u32,
    /// Canonically encoded payload bytes for `hash_ir`.
    pub payload: Vec<u8>,
    /// Final digest as the first eight KoalaBear elements of the terminal sponge state.
    pub digest: [u32; 8],
}

impl IrHashCall {
    /// Build one canonical IR hash witness call from already-portable inputs.
    pub fn from_inputs(
        tx_index: u32,
        instruction_index: u32,
        inputs: &[PortableValue],
    ) -> Result<Self, TabulaError> {
        let payload = encode_ir_hash_payload(inputs);
        let digest_bytes = PoseidonHasher::new().hash(&payload);
        let digest = NativeDigest::from_bytes(&digest_bytes)?.0;
        Ok(Self {
            tx_index,
            instruction_index,
            payload,
            digest: core::array::from_fn(|idx| digest[idx].as_canonical_u32()),
        })
    }
}

/// Canonical byte encoding for `hash_ir`.
#[must_use]
pub fn encode_ir_hash_payload(inputs: &[PortableValue]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(DOMAIN_TAG_HASH_IR);
    bytes.extend_from_slice(&(inputs.len() as u32).to_le_bytes());
    for value in inputs {
        bytes.extend_from_slice(&value.type_id().0.to_le_bytes());
        bytes.extend_from_slice(&(value.payload().len() as u32).to_le_bytes());
        bytes.extend_from_slice(value.payload());
    }
    bytes
}

pub(super) fn payload_to_field_bytes(payload: &[u8]) -> Vec<KoalaBear> {
    payload
        .iter()
        .map(|byte| KoalaBear::new(*byte as u32))
        .collect()
}
