use tabula_chips::capability_transcript::CapabilityTranscriptChip;
use tabula_chips::event_transcript::EventTranscriptChip;
use tabula_chips::ir_hash::{IrHashChip, IrHashKit};
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::public_context_transcript::PublicContextTranscriptChip;
use tabula_chips::relation_table::RelationTableChip;
use tabula_chips::relation_transcript::{RelationTranscriptChip, RelationTranscriptKit};
use tabula_chips::tx_batch_transcript::TxBatchTranscriptChip;
use tabula_stark::trace::{BusConsumer, DynChip};

use tabula_stark::witness_kit::ChipWitnessKit;

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

    /// Witness kits whose rows feed the execution-tier trace.
    ///
    /// Must align with [`Self::airs`]: each AIR that reads rows from a
    /// chip-specific witness-store label should have a matching kit
    /// here. Default-empty for backends whose chips are AIR-only
    /// (e.g. `PoseidonChip`, `RangeCheckChip`) and derive their
    /// witness from the trace itself.
    fn witness_kits(&self) -> Vec<Box<dyn ChipWitnessKit>> {
        Vec::new()
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

    fn witness_kits(&self) -> Vec<Box<dyn ChipWitnessKit>> {
        vec![Box::new(IrHashKit)]
    }
}

/// Built-in execution backend for capability transcript proving.
#[derive(Clone, Copy, Debug, Default)]
pub struct CapabilityTranscriptExecutionBackend;

impl ExecutionBackend for CapabilityTranscriptExecutionBackend {
    fn name(&self) -> &str {
        "capability_transcript"
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(CapabilityTranscriptChip), Box::new(PoseidonChip)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![Box::new(CapabilityTranscriptChip), Box::new(PoseidonChip)]
    }

    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        vec![Box::new(PoseidonChip)]
    }
}

/// Built-in execution backend for static canonical relation proving.
#[derive(Clone, Copy, Debug, Default)]
pub struct RelationExecutionBackend;

impl ExecutionBackend for RelationExecutionBackend {
    fn name(&self) -> &str {
        "relation"
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![
            Box::new(RelationTranscriptChip),
            Box::new(RelationTableChip),
        ]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![
            Box::new(RelationTranscriptChip),
            Box::new(RelationTableChip),
        ]
    }

    fn witness_kits(&self) -> Vec<Box<dyn ChipWitnessKit>> {
        vec![Box::new(RelationTranscriptKit)]
    }
}

/// Built-in execution backend for proved public-statement transcript families.
#[derive(Clone, Copy, Debug, Default)]
pub struct PublicStatementTranscriptExecutionBackend;

impl ExecutionBackend for PublicStatementTranscriptExecutionBackend {
    fn name(&self) -> &str {
        "public_statement_transcript"
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![
            Box::new(PublicContextTranscriptChip),
            Box::new(TxBatchTranscriptChip),
            Box::new(EventTranscriptChip),
            Box::new(PoseidonChip),
        ]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![
            Box::new(PublicContextTranscriptChip),
            Box::new(TxBatchTranscriptChip),
            Box::new(EventTranscriptChip),
            Box::new(PoseidonChip),
        ]
    }

    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        vec![Box::new(PoseidonChip)]
    }
}
