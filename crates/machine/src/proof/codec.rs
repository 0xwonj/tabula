//! Machine-owned canonical proof byte codec.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use p3_commit::{BatchOpening, ExtensionMmcs};
use p3_field::{BasedVectorSpace, PrimeField32};
use p3_fri::{CommitPhaseProofStep, FriProof, QueryProof};
use p3_koala_bear::KoalaBear;
use p3_merkle_tree::MerkleTreeMmcs;
use tabula_core::{ColId, TableId};
use tabula_stark::air::interaction::BusId;
use tabula_stark::chips::ChipId;
use tabula_stark::rap::ef4::ef4_coeffs;

use crate::backend::pcs::{Blake3FieldCompressor, Blake3FieldHasher};
use crate::config::{EF4, PcsCommitment, PcsOpeningProof};
use crate::input::ColumnSlotKey;
use crate::proof::errors::ProofCodecError;
use crate::proof::model::{
    ChipOpening, ColumnProofEntry, ProofTier, SubProofEnvelope, TabulaProof,
};

type ValMmcs = MerkleTreeMmcs<KoalaBear, KoalaBear, Blake3FieldHasher, Blake3FieldCompressor, 2, 8>;
type FriMmcs = ExtensionMmcs<KoalaBear, EF4, ValMmcs>;
type InputBatchOpening = BatchOpening<KoalaBear, ValMmcs>;
type InputQueryProof = QueryProof<EF4, FriMmcs, Vec<InputBatchOpening>>;
type InputCommitPhaseProofStep = CommitPhaseProofStep<EF4, FriMmcs>;

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct ProofDto {
    execution: SubProofEnvelopeDto,
    columns: Vec<ColumnProofEntryDto>,
    root: SubProofEnvelopeDto,
    binding_digest: [u8; 32],
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct SubProofEnvelopeDto {
    tier: ProofTierDto,
    preprocessed_commitment: Option<MerkleCapDto>,
    main_commitment: MerkleCapDto,
    perm_commitment: Option<MerkleCapDto>,
    quotient_commitment: MerkleCapDto,
    opening_proof: FriProofDto,
    chip_openings: Vec<ChipOpeningDto>,
    exported_cumsums: Vec<(u16, [u32; 4])>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct ColumnProofEntryDto {
    key: ColumnSlotKeyDto,
    proof: SubProofEnvelopeDto,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
enum ProofTierDto {
    Execution,
    Column { key: ColumnSlotKeyDto },
    Root,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct ColumnSlotKeyDto {
    table: u32,
    col: u16,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct ChipOpeningDto {
    chip_id: u16,
    main_local: Vec<[u32; 4]>,
    main_next: Vec<[u32; 4]>,
    perm_local: Vec<[u32; 4]>,
    perm_next: Vec<[u32; 4]>,
    preprocessed_local: Option<Vec<[u32; 4]>>,
    preprocessed_next: Option<Vec<[u32; 4]>>,
    quotient_chunks: Vec<Vec<[u32; 4]>>,
    degree_bits: usize,
    main_width: usize,
    perm_width: usize,
    cumsum_final: [u32; 4],
    log_quotient_chunks: usize,
    public_values: Vec<u32>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct MerkleCapDto {
    roots: Vec<[u32; 8]>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct FriProofDto {
    commit_phase_commits: Vec<MerkleCapDto>,
    commit_pow_witnesses: Vec<u32>,
    query_proofs: Vec<QueryProofDto>,
    final_poly: Vec<[u32; 4]>,
    query_pow_witness: u32,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct QueryProofDto {
    input_proof: Vec<BatchOpeningDto>,
    commit_phase_openings: Vec<CommitPhaseProofStepDto>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct BatchOpeningDto {
    opened_values: Vec<Vec<u32>>,
    opening_proof: Vec<[u32; 8]>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct CommitPhaseProofStepDto {
    log_arity: u8,
    sibling_values: Vec<[u32; 4]>,
    opening_proof: Vec<[u32; 8]>,
}

/// Encode one machine proof into canonical proof bytes.
pub(crate) fn encode_proof_bytes(proof: &TabulaProof) -> Result<Vec<u8>, ProofCodecError> {
    let dto = ProofDto::from_proof(proof);
    borsh::to_vec(&dto).map_err(|error| ProofCodecError::Encode {
        detail: error.to_string(),
    })
}

/// Decode one machine proof from canonical proof bytes.
pub fn decode_proof_bytes(bytes: &[u8]) -> Result<TabulaProof, ProofCodecError> {
    let dto = ProofDto::try_from_slice(bytes).map_err(|error| ProofCodecError::Decode {
        detail: error.to_string(),
    })?;
    dto.into_proof()
}

impl ProofDto {
    fn from_proof(proof: &TabulaProof) -> Self {
        Self {
            execution: SubProofEnvelopeDto::from_subproof(&proof.execution),
            columns: proof
                .columns
                .iter()
                .map(ColumnProofEntryDto::from_entry)
                .collect(),
            root: SubProofEnvelopeDto::from_subproof(&proof.root),
            binding_digest: proof.binding_digest,
        }
    }

    fn into_proof(self) -> Result<TabulaProof, ProofCodecError> {
        Ok(TabulaProof {
            execution: self.execution.into_subproof()?,
            columns: self
                .columns
                .into_iter()
                .map(ColumnProofEntryDto::into_entry)
                .collect::<Result<Vec<_>, _>>()?,
            root: self.root.into_subproof()?,
            binding_digest: self.binding_digest,
        })
    }
}

impl SubProofEnvelopeDto {
    fn from_subproof(subproof: &SubProofEnvelope) -> Self {
        Self {
            tier: ProofTierDto::from_tier(subproof.tier),
            preprocessed_commitment: subproof
                .preprocessed_commitment
                .as_ref()
                .map(MerkleCapDto::from_commitment),
            main_commitment: MerkleCapDto::from_commitment(&subproof.main_commitment),
            perm_commitment: subproof
                .perm_commitment
                .as_ref()
                .map(MerkleCapDto::from_commitment),
            quotient_commitment: MerkleCapDto::from_commitment(&subproof.quotient_commitment),
            opening_proof: FriProofDto::from_opening_proof(&subproof.opening_proof),
            chip_openings: subproof
                .chip_openings
                .iter()
                .map(ChipOpeningDto::from_opening)
                .collect(),
            exported_cumsums: subproof
                .exported_cumsums
                .iter()
                .map(|(bus, cumsum)| (bus.0, encode_ef4(*cumsum)))
                .collect(),
        }
    }

    fn into_subproof(self) -> Result<SubProofEnvelope, ProofCodecError> {
        Ok(SubProofEnvelope {
            tier: self.tier.into_tier(),
            preprocessed_commitment: self
                .preprocessed_commitment
                .map(MerkleCapDto::into_commitment)
                .transpose()?,
            main_commitment: self.main_commitment.into_commitment()?,
            perm_commitment: self
                .perm_commitment
                .map(MerkleCapDto::into_commitment)
                .transpose()?,
            quotient_commitment: self.quotient_commitment.into_commitment()?,
            opening_proof: self.opening_proof.into_opening_proof()?,
            chip_openings: self
                .chip_openings
                .into_iter()
                .map(ChipOpeningDto::into_opening)
                .collect::<Result<Vec<_>, _>>()?,
            exported_cumsums: self
                .exported_cumsums
                .into_iter()
                .map(|(bus, cumsum)| Ok((BusId(bus), decode_ef4(cumsum, "exported_cumsums")?)))
                .collect::<Result<BTreeMap<_, _>, _>>()?,
        })
    }
}

impl ColumnProofEntryDto {
    fn from_entry(entry: &ColumnProofEntry) -> Self {
        Self {
            key: ColumnSlotKeyDto::from_key(entry.key),
            proof: SubProofEnvelopeDto::from_subproof(&entry.proof),
        }
    }

    fn into_entry(self) -> Result<ColumnProofEntry, ProofCodecError> {
        Ok(ColumnProofEntry {
            key: self.key.into_key(),
            proof: self.proof.into_subproof()?,
        })
    }
}

impl ProofTierDto {
    fn from_tier(tier: ProofTier) -> Self {
        match tier {
            ProofTier::Execution => Self::Execution,
            ProofTier::Column { key } => Self::Column {
                key: ColumnSlotKeyDto::from_key(key),
            },
            ProofTier::Root => Self::Root,
        }
    }

    fn into_tier(self) -> ProofTier {
        match self {
            Self::Execution => ProofTier::Execution,
            Self::Column { key } => ProofTier::Column {
                key: key.into_key(),
            },
            Self::Root => ProofTier::Root,
        }
    }
}

impl ColumnSlotKeyDto {
    fn from_key(key: ColumnSlotKey) -> Self {
        Self {
            table: key.table.0,
            col: key.col.0,
        }
    }

    fn into_key(self) -> ColumnSlotKey {
        ColumnSlotKey {
            table: TableId(self.table),
            col: ColId(self.col),
        }
    }
}

impl ChipOpeningDto {
    fn from_opening(opening: &ChipOpening) -> Self {
        Self {
            chip_id: opening.chip_id.0,
            main_local: opening.main_local.iter().copied().map(encode_ef4).collect(),
            main_next: opening.main_next.iter().copied().map(encode_ef4).collect(),
            perm_local: opening.perm_local.iter().copied().map(encode_ef4).collect(),
            perm_next: opening.perm_next.iter().copied().map(encode_ef4).collect(),
            preprocessed_local: opening
                .preprocessed_local
                .as_ref()
                .map(|values| values.iter().copied().map(encode_ef4).collect()),
            preprocessed_next: opening
                .preprocessed_next
                .as_ref()
                .map(|values| values.iter().copied().map(encode_ef4).collect()),
            quotient_chunks: opening
                .quotient_chunks
                .iter()
                .map(|chunk| chunk.iter().copied().map(encode_ef4).collect())
                .collect(),
            degree_bits: opening.degree_bits,
            main_width: opening.main_width,
            perm_width: opening.perm_width,
            cumsum_final: encode_ef4(opening.cumsum_final),
            log_quotient_chunks: opening.log_quotient_chunks,
            public_values: opening
                .public_values
                .iter()
                .copied()
                .map(encode_kb)
                .collect(),
        }
    }

    fn into_opening(self) -> Result<ChipOpening, ProofCodecError> {
        Ok(ChipOpening {
            chip_id: ChipId(self.chip_id),
            main_local: decode_ef4_vec(self.main_local, "main_local")?,
            main_next: decode_ef4_vec(self.main_next, "main_next")?,
            perm_local: decode_ef4_vec(self.perm_local, "perm_local")?,
            perm_next: decode_ef4_vec(self.perm_next, "perm_next")?,
            preprocessed_local: self
                .preprocessed_local
                .map(|values| decode_ef4_vec(values, "preprocessed_local"))
                .transpose()?,
            preprocessed_next: self
                .preprocessed_next
                .map(|values| decode_ef4_vec(values, "preprocessed_next"))
                .transpose()?,
            quotient_chunks: self
                .quotient_chunks
                .into_iter()
                .map(|chunk| decode_ef4_vec(chunk, "quotient_chunks"))
                .collect::<Result<Vec<_>, _>>()?,
            degree_bits: self.degree_bits,
            main_width: self.main_width,
            perm_width: self.perm_width,
            cumsum_final: decode_ef4(self.cumsum_final, "cumsum_final")?,
            log_quotient_chunks: self.log_quotient_chunks,
            public_values: self
                .public_values
                .into_iter()
                .map(|value| decode_kb(value, "public_values"))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl MerkleCapDto {
    fn from_commitment(commitment: &PcsCommitment) -> Self {
        Self {
            roots: commitment
                .roots()
                .iter()
                .map(|root| root.map(encode_kb))
                .collect(),
        }
    }

    fn into_commitment(self) -> Result<PcsCommitment, ProofCodecError> {
        let roots = self
            .roots
            .into_iter()
            .map(|root| {
                root.into_iter()
                    .map(|value| decode_kb(value, "merkle_cap"))
                    .collect::<Result<Vec<_>, _>>()
                    .and_then(|root| {
                        root.try_into().map_err(|_| ProofCodecError::Decode {
                            detail: "merkle cap root must contain exactly 8 field elements"
                                .to_string(),
                        })
                    })
            })
            .collect::<Result<Vec<[KoalaBear; 8]>, _>>()?;
        Ok(PcsCommitment::new(roots))
    }
}

impl FriProofDto {
    fn from_opening_proof(proof: &PcsOpeningProof) -> Self {
        Self {
            commit_phase_commits: proof
                .commit_phase_commits
                .iter()
                .map(MerkleCapDto::from_commitment)
                .collect(),
            commit_pow_witnesses: proof
                .commit_pow_witnesses
                .iter()
                .copied()
                .map(encode_kb)
                .collect(),
            query_proofs: proof
                .query_proofs
                .iter()
                .map(QueryProofDto::from_query_proof)
                .collect(),
            final_poly: proof.final_poly.iter().copied().map(encode_ef4).collect(),
            query_pow_witness: encode_kb(proof.query_pow_witness),
        }
    }

    fn into_opening_proof(self) -> Result<PcsOpeningProof, ProofCodecError> {
        Ok(FriProof {
            commit_phase_commits: self
                .commit_phase_commits
                .into_iter()
                .map(MerkleCapDto::into_commitment)
                .collect::<Result<Vec<_>, _>>()?,
            commit_pow_witnesses: self
                .commit_pow_witnesses
                .into_iter()
                .map(|value| decode_kb(value, "commit_pow_witnesses"))
                .collect::<Result<Vec<_>, _>>()?,
            query_proofs: self
                .query_proofs
                .into_iter()
                .map(QueryProofDto::into_query_proof)
                .collect::<Result<Vec<_>, _>>()?,
            final_poly: self
                .final_poly
                .into_iter()
                .map(|value| decode_ef4(value, "final_poly"))
                .collect::<Result<Vec<_>, _>>()?,
            query_pow_witness: decode_kb(self.query_pow_witness, "query_pow_witness")?,
        })
    }
}

impl QueryProofDto {
    fn from_query_proof(proof: &InputQueryProof) -> Self {
        Self {
            input_proof: proof
                .input_proof
                .iter()
                .map(BatchOpeningDto::from_batch_opening)
                .collect(),
            commit_phase_openings: proof
                .commit_phase_openings
                .iter()
                .map(CommitPhaseProofStepDto::from_commit_phase_step)
                .collect(),
        }
    }

    fn into_query_proof(self) -> Result<InputQueryProof, ProofCodecError> {
        Ok(QueryProof {
            input_proof: self
                .input_proof
                .into_iter()
                .map(BatchOpeningDto::into_batch_opening)
                .collect::<Result<Vec<_>, _>>()?,
            commit_phase_openings: self
                .commit_phase_openings
                .into_iter()
                .map(CommitPhaseProofStepDto::into_commit_phase_step)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl BatchOpeningDto {
    fn from_batch_opening(opening: &InputBatchOpening) -> Self {
        Self {
            opened_values: opening
                .opened_values
                .iter()
                .map(|row| row.iter().copied().map(encode_kb).collect())
                .collect(),
            opening_proof: opening
                .opening_proof
                .iter()
                .map(|sibling| sibling.map(encode_kb))
                .collect(),
        }
    }

    fn into_batch_opening(self) -> Result<InputBatchOpening, ProofCodecError> {
        let opened_values = self
            .opened_values
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| decode_kb(value, "batch_opening.opened_values"))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let opening_proof = self
            .opening_proof
            .into_iter()
            .map(|sibling| {
                sibling
                    .into_iter()
                    .map(|value| decode_kb(value, "batch_opening.opening_proof"))
                    .collect::<Result<Vec<_>, _>>()
                    .and_then(|row| {
                        row.try_into().map_err(|_| ProofCodecError::Decode {
                            detail: "batch opening sibling must contain exactly 8 field elements"
                                .to_string(),
                        })
                    })
            })
            .collect::<Result<Vec<[KoalaBear; 8]>, _>>()?;
        Ok(BatchOpening::new(opened_values, opening_proof))
    }
}

impl CommitPhaseProofStepDto {
    fn from_commit_phase_step(step: &InputCommitPhaseProofStep) -> Self {
        Self {
            log_arity: step.log_arity,
            sibling_values: step
                .sibling_values
                .iter()
                .copied()
                .map(encode_ef4)
                .collect(),
            opening_proof: step
                .opening_proof
                .iter()
                .map(|sibling| sibling.map(encode_kb))
                .collect(),
        }
    }

    fn into_commit_phase_step(self) -> Result<InputCommitPhaseProofStep, ProofCodecError> {
        let opening_proof = self
            .opening_proof
            .into_iter()
            .map(|sibling| {
                sibling
                    .into_iter()
                    .map(|value| decode_kb(value, "commit_phase_opening.opening_proof"))
                    .collect::<Result<Vec<_>, _>>()
                    .and_then(|row| {
                        row.try_into().map_err(|_| ProofCodecError::Decode {
                            detail: "commit phase sibling must contain exactly 8 field elements"
                                .to_string(),
                        })
                    })
            })
            .collect::<Result<Vec<[KoalaBear; 8]>, _>>()?;
        Ok(CommitPhaseProofStep {
            log_arity: self.log_arity,
            sibling_values: self
                .sibling_values
                .into_iter()
                .map(|value| decode_ef4(value, "commit_phase_opening.sibling_values"))
                .collect::<Result<Vec<_>, _>>()?,
            opening_proof,
        })
    }
}

fn encode_kb(value: KoalaBear) -> u32 {
    value.as_canonical_u32()
}

fn decode_kb(value: u32, context: &str) -> Result<KoalaBear, ProofCodecError> {
    if value >= KoalaBear::ORDER_U32 {
        return Err(ProofCodecError::NonCanonicalField {
            context: context.to_string(),
            value,
        });
    }
    Ok(KoalaBear::new(value))
}

fn encode_ef4(value: EF4) -> [u32; 4] {
    ef4_coeffs(value).map(encode_kb)
}

fn decode_ef4(value: [u32; 4], context: &str) -> Result<EF4, ProofCodecError> {
    let coeffs = value
        .into_iter()
        .map(|coefficient| decode_kb(coefficient, context))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EF4::from_basis_coefficients_fn(|index| coeffs[index]))
}

fn decode_ef4_vec(values: Vec<[u32; 4]>, context: &str) -> Result<Vec<EF4>, ProofCodecError> {
    values
        .into_iter()
        .map(|value| decode_ef4(value, context))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use p3_field::PrimeCharacteristicRing;
    use p3_koala_bear::KoalaBear;

    use super::{decode_proof_bytes, encode_proof_bytes};
    use crate::config::PcsCommitment;
    use crate::proof::model::{ProofTier, SubProofEnvelope, TabulaProof};

    fn empty_commitment() -> PcsCommitment {
        PcsCommitment::new(vec![[KoalaBear::ZERO; 8]])
    }

    fn empty_opening_proof() -> crate::config::PcsOpeningProof {
        crate::config::PcsOpeningProof {
            commit_phase_commits: vec![],
            commit_pow_witnesses: vec![],
            query_proofs: vec![],
            final_poly: vec![],
            query_pow_witness: KoalaBear::ZERO,
        }
    }

    fn empty_subproof(tier: ProofTier) -> SubProofEnvelope {
        SubProofEnvelope {
            tier,
            preprocessed_commitment: None,
            main_commitment: empty_commitment(),
            perm_commitment: None,
            quotient_commitment: empty_commitment(),
            opening_proof: empty_opening_proof(),
            chip_openings: vec![],
            exported_cumsums: BTreeMap::new(),
        }
    }

    #[test]
    fn proof_codec_round_trips_minimal_proof() {
        let proof = TabulaProof {
            execution: empty_subproof(ProofTier::Execution),
            columns: vec![],
            root: empty_subproof(ProofTier::Root),
            binding_digest: [7u8; 32],
        };

        let encoded = encode_proof_bytes(&proof).expect("encode proof");
        let decoded = decode_proof_bytes(&encoded).expect("decode proof");
        let reencoded = encode_proof_bytes(&decoded).expect("re-encode proof");

        assert_eq!(encoded, reencoded);
    }

    #[test]
    fn malformed_proof_bytes_are_rejected() {
        let err = decode_proof_bytes(b"not a proof")
            .err()
            .expect("malformed proof bytes");
        assert!(err.to_string().contains("failed to decode proof bytes"));
    }
}
