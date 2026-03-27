//! Witness-owned preparation for static canonical relation proving.

use std::collections::{BTreeMap, BTreeSet};

use tabula_contract::StaticTableArtifact;
use tabula_core::Digest;
use tabula_core::error::TabulaError;
use tabula_executor as exec;
use tabula_ir as ir;

use crate::{RelationClaim, RelationClaimKind};

/// Witness-owned relation table row prepared from the compiler-sealed static table artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRelationTableRow {
    /// Relation identifier.
    pub relation_id: u32,
    /// Canonical input digest.
    pub input_digest: [u32; 8],
    /// Canonical output digest.
    pub output_digest: [u32; 8],
    /// Multiplicity on the relation membership bus.
    pub lookup_mult: u32,
}

/// Witness-owned relation proof preparation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRelationProof {
    root: Digest,
    table_rows: Vec<PreparedRelationTableRow>,
}

impl PreparedRelationProof {
    /// Compiler-sealed static table root carried by this prepared artifact.
    #[must_use]
    pub fn root(&self) -> Digest {
        self.root
    }

    /// Prepared logical relation table rows, still backend-neutral.
    #[must_use]
    pub fn table_rows(&self) -> &[PreparedRelationTableRow] {
        &self.table_rows
    }
}

/// Prepare witness-owned relation proof artifacts from a compiler-sealed static-table artifact
/// plus execution-derived relation claims.
pub fn prepare_relation_proof(
    program: &ir::Program,
    artifact: &StaticTableArtifact,
    claims: &[RelationClaim],
) -> Result<PreparedRelationProof, TabulaError> {
    let manifest = program
        .relation_manifest
        .entries
        .iter()
        .map(|entry| (entry.id, entry))
        .collect::<BTreeMap<_, _>>();
    let mut multiplicities = artifact
        .rows
        .iter()
        .map(|row| ((row.relation_id, row.input_digest, row.output_digest), 0u32))
        .collect::<BTreeMap<_, _>>();
    let mut consumed_claim_origins = BTreeSet::new();

    for claim in claims {
        let entry = manifest.get(&claim.relation).ok_or_else(|| {
            TabulaError::InvalidIr(format!("unknown relation ID {}", claim.relation.0))
        })?;
        validate_relation_claim(entry, claim)?;
        let origin = (claim.tx_index, claim.effect_ordinal_in_tx);
        if !consumed_claim_origins.insert(origin) {
            return Err(TabulaError::ProofError {
                phase: "relation_proof_prep",
                detail: format!(
                    "duplicate relation claim origin tx={} ordinal={} (latest op={})",
                    claim.tx_index, claim.effect_ordinal_in_tx, claim.op_index,
                ),
            });
        }
        let key = (claim.relation.0, claim.input_digest, claim.output_digest);
        let Some(mult) = multiplicities.get_mut(&key) else {
            return Err(TabulaError::ProofError {
                phase: "relation_proof_prep",
                detail: format!(
                    "relation {} claim ({:?}, {:?}) was not present in the sealed manifest",
                    entry.descriptor.symbol, claim.input_digest, claim.output_digest,
                ),
            });
        };
        *mult += 1;
    }

    Ok(PreparedRelationProof {
        root: artifact.root,
        table_rows: artifact
            .rows
            .iter()
            .map(|row| PreparedRelationTableRow {
                relation_id: row.relation_id,
                input_digest: row.input_digest,
                output_digest: row.output_digest,
                lookup_mult: multiplicities
                    [&(row.relation_id, row.input_digest, row.output_digest)],
            })
            .collect(),
    })
}

fn validate_relation_claim(
    entry: &ir::RelationManifestEntry,
    claim: &RelationClaim,
) -> Result<(), TabulaError> {
    if claim.inputs.len() != entry.descriptor.inputs.len() {
        return Err(TabulaError::ProofError {
            phase: "relation_proof_prep",
            detail: format!(
                "relation {} input arity mismatch: claim={} descriptor={}",
                entry.descriptor.symbol,
                claim.inputs.len(),
                entry.descriptor.inputs.len(),
            ),
        });
    }
    if claim.outputs.len() != entry.descriptor.outputs.len() {
        return Err(TabulaError::ProofError {
            phase: "relation_proof_prep",
            detail: format!(
                "relation {} output arity mismatch: claim={} descriptor={}",
                entry.descriptor.symbol,
                claim.outputs.len(),
                entry.descriptor.outputs.len(),
            ),
        });
    }
    for (value, expected) in claim.inputs.iter().zip(&entry.descriptor.inputs) {
        if value.type_id() != *expected {
            return Err(TabulaError::ProofError {
                phase: "relation_proof_prep",
                detail: format!(
                    "relation {} input type mismatch: claim={} descriptor={}",
                    entry.descriptor.symbol,
                    value.type_id().0,
                    expected.0,
                ),
            });
        }
    }
    for (value, expected) in claim.outputs.iter().zip(&entry.descriptor.outputs) {
        if value.type_id() != *expected {
            return Err(TabulaError::ProofError {
                phase: "relation_proof_prep",
                detail: format!(
                    "relation {} output type mismatch: claim={} descriptor={}",
                    entry.descriptor.symbol,
                    value.type_id().0,
                    expected.0,
                ),
            });
        }
    }
    match claim.kind {
        RelationClaimKind::Assert if !claim.outputs.is_empty() => Err(TabulaError::ProofError {
            phase: "relation_proof_prep",
            detail: format!(
                "assert relation {} unexpectedly carried outputs",
                entry.descriptor.symbol,
            ),
        }),
        RelationClaimKind::Eval if entry.descriptor.outputs.is_empty() => {
            Err(TabulaError::ProofError {
                phase: "relation_proof_prep",
                detail: format!(
                    "eval relation {} requires output-bearing relation",
                    entry.descriptor.symbol,
                ),
            })
        }
        _ => Ok(()),
    }
}

/// Convert one executor relation effect into a witness-owned claim using already-materialized
/// tuple digests from the relation transcript witness path.
#[must_use]
pub(crate) fn relation_claim_from_effect(
    tx_index: u32,
    effect: &exec::RelationEffect,
    input_digest: [u32; 8],
    output_digest: [u32; 8],
) -> RelationClaim {
    RelationClaim {
        relation: effect.relation,
        kind: match effect.kind {
            exec::RelationEffectKind::Assert => RelationClaimKind::Assert,
            exec::RelationEffectKind::Eval => RelationClaimKind::Eval,
        },
        inputs: effect.inputs.clone(),
        input_digest,
        outputs: effect.outputs.clone(),
        output_digest,
        tx_index,
        effect_ordinal_in_tx: effect.effect_ordinal_in_entry,
        op_index: effect.op_index,
    }
}
