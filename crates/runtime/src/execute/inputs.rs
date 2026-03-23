use tabula_artifact::{State, TransactionBatch};
use tabula_compiler::SealedProgram;
use tabula_core::traits::Hasher;
use tabula_executor::precompile::PrecompileRegistry;
use tabula_executor::property::{CommittedStateProvider, PropertyQueryRegistry};
use tabula_ir::Program;
use tabula_types::TypeRuntimeRegistry;

/// Inputs for batch execution (all immutable references).
pub struct BatchInput<'a> {
    /// The IR program to execute.
    pub program: &'a Program,
    /// Pre-execution state.
    pub state: &'a State,
    /// Transaction batch.
    pub batch: &'a TransactionBatch,
    /// Hasher implementation (Blake3Hasher for CLI, PoseidonHasher for STARK).
    pub hasher: &'a dyn Hasher,
    /// Runtime type registry used to decode portable artifact values.
    pub type_runtimes: &'a TypeRuntimeRegistry,
}

/// Inputs for batch execution using a compiler-produced artifact.
pub struct CompiledBatchInput<'a> {
    /// Semantic artifact produced by the compiler/registration phase.
    pub compiled_program: &'a SealedProgram,
    /// Pre-execution state.
    pub state: &'a State,
    /// Transaction batch.
    pub batch: &'a TransactionBatch,
    /// Hasher implementation (Blake3Hasher for CLI, PoseidonHasher for STARK).
    pub hasher: &'a dyn Hasher,
    /// Runtime type registry used to decode portable artifact values.
    pub type_runtimes: &'a TypeRuntimeRegistry,
}

/// Optional runtime-owned resources used during execution.
#[derive(Clone, Copy)]
pub(crate) struct ExecutionResources<'a> {
    pub(crate) precompiles: Option<&'a PrecompileRegistry>,
    pub(crate) committed_state: Option<&'a dyn CommittedStateProvider>,
    pub(crate) property_queries: &'a PropertyQueryRegistry,
}
