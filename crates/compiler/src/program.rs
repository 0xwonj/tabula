//! Canonical in-memory program produced by the compiler.

use std::collections::{BTreeMap, BTreeSet};

use tabula_artifact::{ColumnProofPlan, ProgramArtifact};
use tabula_contract::{ContractCompatibilityPolicy, ContractMetadataEnvelope};
use tabula_core::{ColId, TableId, TableSchema};
use tabula_ir::{PrecompileId, Program, PropertyRequirement, TxTypeDef};

/// In-memory semantic artifact produced by the compiler/registration phase.
#[derive(Debug, Clone)]
pub struct CompiledProgram {
    /// Registered IR program.
    program: Program,
    /// Canonical table schemas consumed during registration.
    table_schemas: Vec<TableSchema>,
    /// Canonical transaction definitions consumed during registration.
    tx_types: Vec<TxTypeDef>,
    /// Capability manifest: precompiles required by the program.
    required_precompile_ids: Vec<PrecompileId>,
    /// Capability manifest: exact structural property requirements required by the program.
    required_property_requirements: Vec<PropertyRequirement>,
    /// Compiler-owned proof plan for all committed columns.
    column_proof_plan: Vec<ColumnProofPlan>,
    /// Canonical metadata envelope for proof compatibility checks.
    metadata_envelope: ContractMetadataEnvelope,
}

impl CompiledProgram {
    /// Create a compiler-owned semantic artifact after invariant checks.
    pub(crate) fn new(
        program: Program,
        table_schemas: Vec<TableSchema>,
        tx_types: Vec<TxTypeDef>,
        required_precompile_ids: Vec<PrecompileId>,
        required_property_requirements: Vec<PropertyRequirement>,
        column_proof_plan: Vec<ColumnProofPlan>,
        metadata_envelope: ContractMetadataEnvelope,
    ) -> Result<Self, String> {
        let compiled = Self {
            program,
            table_schemas,
            tx_types,
            required_precompile_ids,
            required_property_requirements,
            column_proof_plan,
            metadata_envelope,
        };
        compiled.validate_column_proof_plan()?;
        Ok(compiled)
    }

    /// Registered IR program.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Canonical table schemas consumed during registration.
    pub fn table_schemas(&self) -> &[TableSchema] {
        &self.table_schemas
    }

    /// Canonical transaction definitions consumed during registration.
    pub fn tx_types(&self) -> &[TxTypeDef] {
        &self.tx_types
    }

    /// Capability manifest: precompiles required by the program.
    pub fn required_precompile_ids(&self) -> &[PrecompileId] {
        &self.required_precompile_ids
    }

    /// Capability manifest: exact structural property requirements required by the program.
    pub fn required_property_requirements(&self) -> &[PropertyRequirement] {
        &self.required_property_requirements
    }

    /// Compiler-owned proof plan for all committed columns.
    pub fn column_proof_plan(&self) -> &[ColumnProofPlan] {
        &self.column_proof_plan
    }

    /// Canonical metadata envelope for proof compatibility checks.
    pub fn metadata_envelope(&self) -> &ContractMetadataEnvelope {
        &self.metadata_envelope
    }

    /// Build a strict compatibility policy pinned to this program's metadata.
    pub fn compatibility_policy(&self) -> ContractCompatibilityPolicy {
        ContractCompatibilityPolicy {
            expected_profile_hash: self.metadata_envelope.profile_hash,
            expected_contract_schema_version: self.metadata_envelope.contract_schema_version,
            expected_binding_version: self.metadata_envelope.binding_version,
            expected_statement_schema_version: self.metadata_envelope.statement_schema_version,
            expected_verifier_profile_version: self.metadata_envelope.verifier_profile_version,
            expected_semantic_hash_stub: self.metadata_envelope.semantic_hash_stub,
        }
    }

    /// Validate that the proof plan covers each schema column exactly once.
    pub fn validate_column_proof_plan(&self) -> Result<(), String> {
        let expected: BTreeSet<(TableId, ColId)> = self
            .table_schemas
            .iter()
            .flat_map(|schema| {
                schema
                    .columns
                    .iter()
                    .map(move |column| (schema.id, column.id))
            })
            .collect();

        let mut actual = BTreeSet::new();
        for plan in &self.column_proof_plan {
            let key = (plan.table_id, plan.col_id);
            if !actual.insert(key) {
                return Err(format!(
                    "column proof plan contains duplicate entry for table {} col {}",
                    plan.table_id.0, plan.col_id.0,
                ));
            }
        }

        let missing: Vec<_> = expected.difference(&actual).copied().collect();
        if let Some((table_id, col_id)) = missing.first().copied() {
            return Err(format!(
                "column proof plan is missing table {} col {}",
                table_id.0, col_id.0,
            ));
        }

        let extra: Vec<_> = actual.difference(&expected).copied().collect();
        if let Some((table_id, col_id)) = extra.first().copied() {
            return Err(format!(
                "column proof plan references unknown table {} col {}",
                table_id.0, col_id.0,
            ));
        }

        Ok(())
    }

    /// Index the proof plan by `(table_id, col_id)` for runtime lookup.
    pub fn column_proof_plan_by_id(
        &self,
    ) -> Result<BTreeMap<(TableId, ColId), ColumnProofPlan>, String> {
        self.validate_column_proof_plan()?;
        Ok(self
            .column_proof_plan
            .iter()
            .copied()
            .map(|plan| ((plan.table_id, plan.col_id), plan))
            .collect())
    }

    /// Clone into a sealed portable artifact.
    pub fn as_program_artifact(&self) -> ProgramArtifact {
        ProgramArtifact {
            table_schemas: self.table_schemas.clone(),
            tx_types: self.tx_types.clone(),
            required_precompile_ids: self.required_precompile_ids.clone(),
            required_property_requirements: self.required_property_requirements.clone(),
            column_proof_plan: self.column_proof_plan.clone(),
            contract_metadata: self.metadata_envelope.clone(),
        }
    }

    /// Convert into a sealed portable artifact.
    pub fn into_program_artifact(self) -> ProgramArtifact {
        ProgramArtifact {
            table_schemas: self.table_schemas,
            tx_types: self.tx_types,
            required_precompile_ids: self.required_precompile_ids,
            required_property_requirements: self.required_property_requirements,
            column_proof_plan: self.column_proof_plan,
            contract_metadata: self.metadata_envelope,
        }
    }

    /// Backward-compatible alias for older call sites.
    pub fn into_program_file(self) -> ProgramArtifact {
        self.into_program_artifact()
    }
}
