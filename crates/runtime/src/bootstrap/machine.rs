use std::sync::Arc;

use tabula_core::RootProfileId;
use tabula_ext::backend::ExecutionBackend;
use tabula_ext::backend::execution::{
    IrHashExecutionBackend, PrecompileTranscriptExecutionBackend,
};
use tabula_ir::{Instruction, Program};
use tabula_machine::backend::extension::ExecutionTierExtension;
use tabula_machine::{MachineBuilder, RootProofBackend, TabulaStarkConfig};

pub(crate) fn supported_root_binding_families(
    root_proof_backend: &Arc<dyn RootProofBackend>,
) -> &[RootProfileId] {
    root_proof_backend.supported_root_binding_families()
}

pub(crate) fn build_machine_builder(
    config: &TabulaStarkConfig,
    root_proof_backend: Arc<dyn RootProofBackend>,
) -> MachineBuilder {
    MachineBuilder::new()
        .with_config(config.clone())
        .with_root_proof_backend_arc(root_proof_backend)
}

pub(crate) fn attach_execution_backend<B>(
    builder: MachineBuilder,
    backend: Arc<B>,
) -> MachineBuilder
where
    B: ExecutionBackend + ?Sized + 'static,
{
    builder.with_backend_execution_extension(SharedExecutionBackend(backend))
}

pub(crate) fn attach_builtin_execution_backends(
    mut builder: MachineBuilder,
    program: &Program,
    has_precompile_manifest: bool,
) -> MachineBuilder {
    if program_uses_ir_hash(program) {
        builder = attach_execution_backend(builder, Arc::new(IrHashExecutionBackend));
    }
    if has_precompile_manifest {
        builder = attach_execution_backend(builder, Arc::new(PrecompileTranscriptExecutionBackend));
    }
    builder
}

struct SharedExecutionBackend<B: ExecutionBackend + ?Sized>(Arc<B>);

impl<B> ExecutionTierExtension for SharedExecutionBackend<B>
where
    B: ExecutionBackend + ?Sized + 'static,
{
    fn name(&self) -> &str {
        self.0.name()
    }

    fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
        self.0.airs()
    }

    fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
        self.0.dyn_chips()
    }

    fn bus_consumers(&self) -> Vec<Box<dyn tabula_stark::trace::column_commitment::BusConsumer>> {
        self.0.bus_consumers()
    }
}

fn program_uses_ir_hash(program: &Program) -> bool {
    program
        .all_types()
        .iter()
        .flat_map(|tx| tx.body.iter())
        .any(|instruction| matches!(instruction, Instruction::Hash { .. }))
}
