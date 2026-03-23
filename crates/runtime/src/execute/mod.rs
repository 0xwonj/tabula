mod envelope;
mod inputs;
mod pipeline;
#[cfg(feature = "prove")]
mod snapshot_view;

pub use envelope::ExecutionEnvelope;
#[cfg(feature = "prove")]
pub(crate) use inputs::ExecutionResources;
pub use inputs::{BatchInput, CompiledBatchInput};
#[cfg(feature = "prove")]
pub(crate) use pipeline::execute_pipeline;
pub use pipeline::{run_batch, run_compiled_batch};
#[cfg(feature = "prove")]
pub(crate) use snapshot_view::SnapshotStateView;
