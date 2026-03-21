//! Adapters from product-owned example bundles into shared test cases.

use tabula_compiler::{TRANSFER_EXAMPLE_TAB_SOURCE, transfer_example_bundle};

use crate::exec::{
    compiled_program_from_artifact, core_batch_from_artifact_batch, initial_cells_from_state,
};
use crate::fixtures::cases::{ArtifactRuntimeCase, CompiledRuntimeCase, TraceCase};

pub fn transfer_example_artifact_case() -> ArtifactRuntimeCase {
    let bundle = transfer_example_bundle().expect("transfer example bundle");
    ArtifactRuntimeCase {
        artifact: bundle.program,
        state: bundle.state,
        batch: bundle.batch,
    }
}

pub fn transfer_example_compiled_case() -> CompiledRuntimeCase {
    let artifact_case = transfer_example_artifact_case();
    CompiledRuntimeCase {
        compiled_program: compiled_program_from_artifact(&artifact_case.artifact),
        state: artifact_case.state,
        batch: artifact_case.batch,
    }
}

pub fn transfer_example_trace_case() -> TraceCase {
    let artifact_case = transfer_example_artifact_case();
    let batch = core_batch_from_artifact_batch(&artifact_case.batch)
        .expect("convert transfer example batch");
    TraceCase {
        source: TRANSFER_EXAMPLE_TAB_SOURCE,
        initial_cells: initial_cells_from_state(&artifact_case.state),
        transactions: batch.transactions,
    }
}

pub fn success_transfer_with_emit_trace_case() -> TraceCase {
    transfer_example_trace_case()
}
