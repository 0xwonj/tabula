//! Static-table artifact schema and canonical root derivation.

use borsh::{BorshDeserialize, BorshSerialize};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use serde::{Deserialize, Serialize};

use crate::format::typed_tuple::TYPED_TUPLE_TRANSCRIPT_RATE;
use tabula_commitment::{FieldHasher, NativeDigest, PoseidonHasher};
use tabula_core::Digest;

const STATIC_TABLE_ROOT_DOMAIN_TAG: u32 = 0x52;

/// One canonical sealed static-table row.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct StaticTableArtifactRow {
    /// Relation identifier.
    pub relation_id: u32,
    /// Canonical input tuple digest.
    pub input_digest: [u32; 8],
    /// Canonical output tuple digest.
    pub output_digest: [u32; 8],
}

/// Compiler-sealed static-table artifact bound into the native proof statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StaticTableArtifact {
    /// Canonically ordered static relation rows.
    pub rows: Vec<StaticTableArtifactRow>,
    /// Transcript-bound root digest over `rows`.
    pub root: Digest,
}

/// Deterministic transcript root over the sealed static relation rows.
#[must_use]
pub fn compute_static_table_artifact_root(rows: &[StaticTableArtifactRow]) -> Digest {
    let mut blocks = Vec::with_capacity(1 + rows.len() * 3);
    let mut header = [KoalaBear::ZERO; TYPED_TUPLE_TRANSCRIPT_RATE];
    header[0] = KoalaBear::new(STATIC_TABLE_ROOT_DOMAIN_TAG);
    header[1] = KoalaBear::new(rows.len() as u32);
    blocks.push(header);

    for row in rows {
        let mut first = [KoalaBear::ZERO; TYPED_TUPLE_TRANSCRIPT_RATE];
        first[0] = KoalaBear::new(row.relation_id);
        for idx in 0..7 {
            first[1 + idx] = KoalaBear::new(row.input_digest[idx]);
        }
        blocks.push(first);

        let mut second = [KoalaBear::ZERO; TYPED_TUPLE_TRANSCRIPT_RATE];
        second[0] = KoalaBear::new(row.input_digest[7]);
        for idx in 0..7 {
            second[1 + idx] = KoalaBear::new(row.output_digest[idx]);
        }
        blocks.push(second);

        let mut third = [KoalaBear::ZERO; TYPED_TUPLE_TRANSCRIPT_RATE];
        third[0] = KoalaBear::new(row.output_digest[7]);
        blocks.push(third);
    }

    let digest = compute_block_chain_digest_from_iter(blocks.iter());
    NativeDigest(digest.map(KoalaBear::new)).to_bytes()
}

/// Deterministic Poseidon block-chain digest over any fixed-width block iterator.
pub(crate) fn compute_block_chain_digest_from_iter<'a>(
    blocks: impl IntoIterator<Item = &'a [KoalaBear; TYPED_TUPLE_TRANSCRIPT_RATE]>,
) -> [u32; 8] {
    let hasher = PoseidonHasher::new();
    let mut prev = NativeDigest::ZERO;
    for block in blocks {
        prev = hasher.compress(&prev, &NativeDigest(*block));
    }
    prev.0.map(|word| word.as_canonical_u32())
}
