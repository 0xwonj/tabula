use tabula_chips::ir_hash::IrHashChip;
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::precompile_transcript::PrecompileTranscriptChip;
use tabula_stark::trace::{BusConsumer, DynChip};

use crate::backend::AnyRap;

/// Stable advanced execution-tier backend contract.
pub trait ExecutionBackend: Send + Sync {
    /// Human-readable execution backend name.
    fn name(&self) -> &str;

    /// AIR implementations for proving and verification.
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;

    /// Dynamic chips for trace generation.
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;

    /// Optional dependent bus consumers.
    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        vec![]
    }
}

/// Built-in execution backend for IR hash proving.
#[derive(Clone, Copy, Debug, Default)]
pub struct IrHashExecutionBackend;

impl ExecutionBackend for IrHashExecutionBackend {
    fn name(&self) -> &str {
        "ir_hash"
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(IrHashChip)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![Box::new(IrHashChip)]
    }
}

/// Built-in execution backend for precompile transcript proving.
#[derive(Clone, Copy, Debug, Default)]
pub struct PrecompileTranscriptExecutionBackend;

impl ExecutionBackend for PrecompileTranscriptExecutionBackend {
    fn name(&self) -> &str {
        "precompile_transcript"
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(PrecompileTranscriptChip), Box::new(PoseidonChip)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![Box::new(PrecompileTranscriptChip), Box::new(PoseidonChip)]
    }

    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        vec![Box::new(PoseidonChip)]
    }
}
