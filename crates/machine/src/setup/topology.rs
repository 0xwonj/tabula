use std::fmt;

use tabula_core::{ColId, TableId};
use tabula_stark::trace::{BusConsumer, DynChip};

use crate::config::TabulaStarkConfig;
use crate::setup::metadata::{TierProvingMetadata, TierVerificationMetadata};
use crate::setup::registry::ChipRegistry;

/// Complete configured backend topology for a machine instance.
pub(crate) struct MachineTopology {
    config: TabulaStarkConfig,
    proof_topology: ProofTopology,
}

impl fmt::Debug for MachineTopology {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MachineTopology")
            .field(
                "exec_chips",
                &self.proof_topology.execution.registry.chip_ids(),
            )
            .field("num_columns", &self.proof_topology.columns.len())
            .finish_non_exhaustive()
    }
}

/// Per-tier chip topology for one proof instance.
pub(crate) struct TierTopology {
    /// Chip registry (AIR implementations for proving/verification).
    pub(crate) registry: ChipRegistry,
    /// Proving metadata (keygen info cached from the registry).
    pub(crate) proving_metadata: TierProvingMetadata,
    /// Verification metadata (minimal verification-time opening shape info).
    pub(crate) verification_metadata: TierVerificationMetadata,
    /// Dynamic chips for phase-ordered trace building.
    pub(crate) dyn_chips: Vec<Box<dyn DynChip>>,
    /// Bus consumers for interaction-driven trace building.
    pub(crate) bus_consumers: Vec<Box<dyn BusConsumer>>,
}

impl TierTopology {
    /// Dynamic chips used to build this tier's traces.
    pub(crate) fn dyn_chips(&self) -> &[Box<dyn DynChip>] {
        &self.dyn_chips
    }

    /// Bus consumers used during dependent-phase trace collection.
    pub(crate) fn bus_consumers(&self) -> &[Box<dyn BusConsumer>] {
        &self.bus_consumers
    }
}

impl MachineTopology {
    pub(crate) fn new(config: TabulaStarkConfig, proof_topology: ProofTopology) -> Self {
        Self {
            config,
            proof_topology,
        }
    }

    /// The STARK configuration used by this backend topology.
    pub(crate) fn config(&self) -> &TabulaStarkConfig {
        &self.config
    }

    /// The per-tier proof topology for the C+2 architecture.
    pub(crate) fn proof_topology(&self) -> &ProofTopology {
        &self.proof_topology
    }
}

/// All per-tier topology for the proof architecture.
pub(crate) struct ProofTopology {
    /// Execution tier topology.
    pub(crate) execution: TierTopology,
    /// Column tier topology keyed by `(table, col)`.
    pub(crate) columns: Vec<((TableId, ColId), TierTopology)>,
    /// Root tier topology.
    pub(crate) root: TierTopology,
}
