use std::fmt;

use tabula_core::{ColId, TableId};
use tabula_stark::trace::{BusConsumer, DynChip, TraceMap};

use crate::config::TabulaStarkConfig;
use crate::proof::types::ColumnProofTrace;
use crate::setup::registry::ChipRegistry;
use crate::{TabulaProvingKey, TabulaVerifyingKey};

/// Complete configured backend state for a machine instance.
pub struct MachineSetup {
    config: TabulaStarkConfig,
    proof_setups: ProofSetups,
}

impl fmt::Debug for MachineSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MachineSetup")
            .field(
                "exec_chips",
                &self.proof_setups.execution.registry.chip_ids(),
            )
            .field("num_columns", &self.proof_setups.columns.len())
            .finish_non_exhaustive()
    }
}

/// Per-tier chip setup for one proof instance.
pub struct TierSetup {
    /// Chip registry (AIR implementations for proving/verification).
    pub registry: ChipRegistry,
    /// Proving key (keygen info cached from the registry).
    pub proving_key: TabulaProvingKey,
    /// Verifying key (minimal verification metadata).
    pub verifying_key: TabulaVerifyingKey,
    /// Dynamic chips for phase-ordered trace building.
    pub(crate) dyn_chips: Vec<Box<dyn DynChip>>,
    /// Bus consumers for interaction-driven trace building.
    pub(crate) bus_consumers: Vec<Box<dyn BusConsumer>>,
}

impl TierSetup {
    /// Dynamic chips used to build this tier's traces.
    pub fn dyn_chips(&self) -> &[Box<dyn DynChip>] {
        &self.dyn_chips
    }

    /// Bus consumers used during dependent-phase trace collection.
    pub fn bus_consumers(&self) -> &[Box<dyn BusConsumer>] {
        &self.bus_consumers
    }
}

impl MachineSetup {
    pub(crate) fn new(config: TabulaStarkConfig, proof_setups: ProofSetups) -> Self {
        Self {
            config,
            proof_setups,
        }
    }

    /// The STARK configuration used by this backend setup.
    pub fn config(&self) -> &TabulaStarkConfig {
        &self.config
    }

    /// The per-tier proof setups for the C+2 architecture.
    pub fn proof_setups(&self) -> &ProofSetups {
        &self.proof_setups
    }
}

/// Per-tier trace maps for the proof architecture.
#[derive(Clone)]
pub struct ProofTraces {
    /// Execution tier traces.
    pub execution: TraceMap,
    /// Column tier traces bundled with ordered identities.
    pub columns: Vec<ColumnProofTrace>,
    /// Root tier traces.
    pub root: TraceMap,
}

/// All per-tier setups for the proof architecture.
pub struct ProofSetups {
    /// Execution tier setup.
    pub execution: TierSetup,
    /// Column tier setups keyed by `(table, col)`.
    pub columns: Vec<((TableId, ColId), TierSetup)>,
    /// Root tier setup.
    pub root: TierSetup,
}
