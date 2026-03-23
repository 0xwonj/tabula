//! Test-only precompile extension helpers with semantic proof wiring.

use std::sync::Arc;

use sha2::{Digest as _, Sha256};
use tabula_core::error::TabulaError;
use tabula_ext::backend::WitnessStore;
use tabula_ext::backend::precompile::{
    PrecompileBackendFactory, PrecompileProofContext, PrecompileProofPreparer,
    PrecompileProofSystem, PreparedPrecompileProof, ResolvedPrecompile, ResolvedPrecompileCall,
};
use tabula_ext::backend::prelude::{
    Air, AirInteraction, AnyRap, BaseAir, BusConsumer, ChipId, DynChip, InteractionAirBuilder,
    KoalaBear, PrimeCharacteristicRing, RowMajorMatrix, TraceContributor, TraceMap, TracePhase,
    WindowAccess, borrow_cols, borrow_cols_mut, core_buses, expr_from_u32,
};
use tabula_ext::precompile::PrecompileHandler;
use tabula_ext::{
    ExtError, PrecompileDescriptor, PrecompileId, PrecompileSignature, PrecompileValueProfile,
};
use tabula_profile::{ENCODING_U64_ID, TYPE_U64_ID};
use tabula_types::{TypedValue, u64_portable, u64_typed};

/// Default precompile id used by the testing fixtures.
pub const CONSTANT_ONE_PRECOMPILE_ID: PrecompileId = PrecompileId(0x0001);
/// Default precompile id used by the multi-output testing fixtures.
pub const SEQUENCE_PRECOMPILE_ID: PrecompileId = PrecompileId(0x0002);

const CONSTANT_ONE_PRECOMPILE_VERSION: u16 = 1;
const CONSTANT_ONE_WITNESS_LABEL: &str = "testing_constant_one_precompile_calls";
const SEQUENCE_WITNESS_LABEL: &str = "testing_sequence_precompile_calls";
const CONSTANT_ONE_CHIP_ID_BASE: u16 = 400;
const CONSTANT_ONE_WIDTH: usize = 14;

fn semantic_hash(label: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"tabula.testing.precompile.semantic.v1");
    hasher.update(label.as_bytes());
    hasher.finalize().into()
}

fn u64_profile() -> PrecompileValueProfile {
    PrecompileValueProfile {
        type_id: TYPE_U64_ID,
        encoding_profile_id: ENCODING_U64_ID,
    }
}

/// Canonical descriptor for the "constant one" testing precompile family.
pub fn constant_one_precompile_descriptor(id: PrecompileId) -> PrecompileDescriptor {
    PrecompileDescriptor::new(
        id,
        CONSTANT_ONE_PRECOMPILE_VERSION,
        PrecompileSignature::new(vec![], vec![u64_profile()]),
        semantic_hash("testing.constant_one.semantic"),
    )
}

/// Canonical descriptor for the multi-output "sequence" testing precompile family.
pub fn sequence_precompile_descriptor(id: PrecompileId) -> PrecompileDescriptor {
    PrecompileDescriptor::new(
        id,
        CONSTANT_ONE_PRECOMPILE_VERSION,
        PrecompileSignature::new(
            vec![],
            vec![u64_profile(), u64_profile(), u64_profile(), u64_profile()],
        ),
        semantic_hash("testing.sequence.semantic"),
    )
}

/// Deterministic test precompile: no inputs, always returns `U64(1)`.
#[derive(Clone, Debug)]
pub struct ConstantOnePrecompileHandler {
    descriptor: PrecompileDescriptor,
}

impl ConstantOnePrecompileHandler {
    /// Create a handler for one precompile id.
    pub fn new(id: PrecompileId) -> Self {
        Self {
            descriptor: constant_one_precompile_descriptor(id),
        }
    }
}

impl Default for ConstantOnePrecompileHandler {
    fn default() -> Self {
        Self::new(CONSTANT_ONE_PRECOMPILE_ID)
    }
}

impl PrecompileHandler for ConstantOnePrecompileHandler {
    fn id(&self) -> PrecompileId {
        self.descriptor.precompile_id
    }

    fn signature(&self) -> &PrecompileSignature {
        &self.descriptor.signature
    }

    fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        Ok(vec![u64_typed(1)])
    }
}

/// Deterministic test precompile: no inputs, returns a fixed sequence.
#[derive(Clone, Debug)]
pub struct SequencePrecompileHandler {
    descriptor: PrecompileDescriptor,
}

impl SequencePrecompileHandler {
    /// Create a handler for one precompile id.
    pub fn new(id: PrecompileId) -> Self {
        Self {
            descriptor: sequence_precompile_descriptor(id),
        }
    }
}

impl Default for SequencePrecompileHandler {
    fn default() -> Self {
        Self::new(SEQUENCE_PRECOMPILE_ID)
    }
}

impl PrecompileHandler for SequencePrecompileHandler {
    fn id(&self) -> PrecompileId {
        self.descriptor.precompile_id
    }

    fn signature(&self) -> &PrecompileSignature {
        &self.descriptor.signature
    }

    fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        Ok(sequence_outputs_typed())
    }
}

/// Backend factory for the "constant one" testing precompile family.
#[derive(Clone, Debug)]
pub struct ConstantOnePrecompileBackendFactory {
    descriptor: PrecompileDescriptor,
}

impl ConstantOnePrecompileBackendFactory {
    /// Create a backend factory for one descriptor.
    pub fn new(descriptor: PrecompileDescriptor) -> Self {
        Self { descriptor }
    }
}

impl PrecompileBackendFactory for ConstantOnePrecompileBackendFactory {
    fn name(&self) -> &str {
        "constant_one_precompile"
    }

    fn descriptor(&self) -> &PrecompileDescriptor {
        &self.descriptor
    }

    fn build_system(
        &self,
        resolved: &ResolvedPrecompile,
    ) -> Result<Arc<dyn PrecompileProofSystem>, ExtError> {
        Ok(Arc::new(ConstantOnePrecompileProofSystem::new(
            resolved.descriptor.clone(),
        )))
    }

    fn build_preparer(
        &self,
        resolved: &ResolvedPrecompile,
    ) -> Result<Arc<dyn PrecompileProofPreparer>, ExtError> {
        Ok(Arc::new(ConstantOnePrecompileProofPreparer::new(
            resolved.descriptor.clone(),
        )))
    }

    fn build_handler(
        &self,
        resolved: &ResolvedPrecompile,
    ) -> Result<Arc<dyn tabula_ext::precompile::PrecompileHandler>, ExtError> {
        Ok(Arc::new(ConstantOnePrecompileHandler::new(
            resolved.descriptor.precompile_id,
        )))
    }
}

/// Backend factory for the multi-output testing precompile family.
#[derive(Clone, Debug)]
pub struct SequencePrecompileBackendFactory {
    descriptor: PrecompileDescriptor,
}

impl SequencePrecompileBackendFactory {
    /// Create a backend factory for one descriptor.
    pub fn new(descriptor: PrecompileDescriptor) -> Self {
        Self { descriptor }
    }
}

impl PrecompileBackendFactory for SequencePrecompileBackendFactory {
    fn name(&self) -> &str {
        "sequence_precompile"
    }

    fn descriptor(&self) -> &PrecompileDescriptor {
        &self.descriptor
    }

    fn build_system(
        &self,
        resolved: &ResolvedPrecompile,
    ) -> Result<Arc<dyn PrecompileProofSystem>, ExtError> {
        Ok(Arc::new(FixedOutputsPrecompileProofSystem::new(
            resolved.descriptor.clone(),
            "sequence_precompile",
            sequence_outputs(),
            SEQUENCE_WITNESS_LABEL,
        )))
    }

    fn build_preparer(
        &self,
        resolved: &ResolvedPrecompile,
    ) -> Result<Arc<dyn PrecompileProofPreparer>, ExtError> {
        Ok(Arc::new(FixedOutputsPrecompileProofPreparer::new(
            resolved.descriptor.clone(),
            "sequence_precompile",
            SEQUENCE_WITNESS_LABEL,
        )))
    }

    fn build_handler(
        &self,
        resolved: &ResolvedPrecompile,
    ) -> Result<Arc<dyn tabula_ext::precompile::PrecompileHandler>, ExtError> {
        Ok(Arc::new(SequencePrecompileHandler::new(
            resolved.descriptor.precompile_id,
        )))
    }
}

#[derive(Clone, Debug)]
struct FixedOutputsWitness {
    calls: Vec<ResolvedPrecompileCall>,
}

#[repr(C)]
#[derive(Clone, Debug)]
struct FixedOutputsPrecompileCols<T> {
    is_real: T,
    tx_index: T,
    instruction_index: T,
    precompile_id: T,
    input_count: T,
    output_count: T,
    event_digest: [T; 8],
}

#[derive(Clone, Debug)]
struct FixedOutputsPrecompileChip {
    descriptor: PrecompileDescriptor,
    chip_id: ChipId,
    expected_outputs: Vec<tabula_core::PortableValue>,
    witness_label: &'static str,
}

impl FixedOutputsPrecompileChip {
    fn new(
        descriptor: PrecompileDescriptor,
        expected_outputs: Vec<tabula_core::PortableValue>,
        witness_label: &'static str,
    ) -> Self {
        Self {
            chip_id: ChipId(CONSTANT_ONE_CHIP_ID_BASE + descriptor.precompile_id.0),
            descriptor,
            expected_outputs,
            witness_label,
        }
    }
}

impl tabula_stark::chips::ChipSpec for FixedOutputsPrecompileChip {
    fn chip_id(&self) -> ChipId {
        self.chip_id
    }

    fn chip_name(&self) -> &'static str {
        "FixedOutputsPrecompile"
    }

    fn has_interactions(&self) -> bool {
        true
    }
}

impl<F> BaseAir<F> for FixedOutputsPrecompileChip {
    fn width(&self) -> usize {
        CONSTANT_ONE_WIDTH
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for FixedOutputsPrecompileChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local: &FixedOutputsPrecompileCols<AB::Var> = borrow_cols(main.current_slice());

        let is_real: AB::Expr = local.is_real.into();
        builder.assert_zero(is_real.clone() * (is_real.clone() - AB::Expr::ONE));

        let expected_id = expr_from_u32::<AB>(self.descriptor.precompile_id.0 as u32);
        builder.assert_zero(is_real.clone() * (local.precompile_id.into() - expected_id.clone()));
        builder.assert_zero(is_real.clone() * local.input_count.into());
        builder.assert_zero(
            is_real.clone()
                * (local.output_count.into()
                    - expr_from_u32::<AB>(self.expected_outputs.len() as u32)),
        );

        let mut precompile_values = Vec::with_capacity(13);
        precompile_values.push(local.tx_index.into());
        precompile_values.push(local.instruction_index.into());
        precompile_values.push(expected_id);
        precompile_values.push(local.input_count.into());
        precompile_values.push(local.output_count.into());
        for idx in 0..8 {
            precompile_values.push(local.event_digest[idx].into());
        }
        builder.receive(AirInteraction {
            values: precompile_values,
            multiplicity: is_real,
            bus: core_buses::PRECOMPILE,
        });
    }
}

impl TraceContributor for FixedOutputsPrecompileChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let witness = store.get::<FixedOutputsWitness>(self.witness_label)?;

        let num_real = witness.calls.len();
        let num_rows = (num_real + 1).next_power_of_two().max(2);
        let mut values = vec![KoalaBear::ZERO; num_rows * CONSTANT_ONE_WIDTH];

        for (row_idx, call) in witness.calls.iter().enumerate() {
            let event = &call.event;
            if event.precompile_id != self.descriptor.precompile_id.0 {
                return Err(TabulaError::ProofError {
                    phase: "precompile_trace",
                    detail: format!(
                        "precompile event id 0x{:04x} does not match descriptor 0x{:04x}",
                        event.precompile_id, self.descriptor.precompile_id.0,
                    ),
                });
            }
            if !event.inputs.is_empty() {
                return Err(TabulaError::ProofError {
                    phase: "precompile_trace",
                    detail: format!(
                        "precompile 0x{:04x} expects no inputs",
                        self.descriptor.precompile_id.0,
                    ),
                });
            }
            if event.outputs != self.expected_outputs {
                return Err(TabulaError::ProofError {
                    phase: "precompile_trace",
                    detail: format!(
                        "precompile 0x{:04x} expects outputs {:?}, got {:?}",
                        self.descriptor.precompile_id.0, self.expected_outputs, event.outputs
                    ),
                });
            }

            let offset = row_idx * CONSTANT_ONE_WIDTH;
            let row: &mut FixedOutputsPrecompileCols<KoalaBear> =
                borrow_cols_mut(&mut values[offset..offset + CONSTANT_ONE_WIDTH]);
            row.is_real = KoalaBear::ONE;
            row.tx_index = KoalaBear::new(call.header.tx_index);
            row.instruction_index = KoalaBear::new(call.header.instruction_index);
            row.precompile_id = KoalaBear::new(call.header.precompile_id as u32);
            row.input_count = KoalaBear::new(call.header.input_count);
            row.output_count = KoalaBear::new(call.header.output_count);
            row.event_digest =
                core::array::from_fn(|idx| KoalaBear::new(call.header.event_digest[idx]));
        }

        map.insert(
            self.chip_id,
            RowMajorMatrix::new(values, CONSTANT_ONE_WIDTH),
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct FixedOutputsPrecompileProofSystem {
    descriptor: PrecompileDescriptor,
    name: &'static str,
    expected_outputs: Vec<tabula_core::PortableValue>,
    witness_label: &'static str,
}

impl FixedOutputsPrecompileProofSystem {
    fn new(
        descriptor: PrecompileDescriptor,
        name: &'static str,
        expected_outputs: Vec<tabula_core::PortableValue>,
        witness_label: &'static str,
    ) -> Self {
        Self {
            descriptor,
            name,
            expected_outputs,
            witness_label,
        }
    }

    fn chip(&self) -> FixedOutputsPrecompileChip {
        FixedOutputsPrecompileChip::new(
            self.descriptor.clone(),
            self.expected_outputs.clone(),
            self.witness_label,
        )
    }
}

impl PrecompileProofSystem for FixedOutputsPrecompileProofSystem {
    fn name(&self) -> &str {
        self.name
    }

    fn descriptor(&self) -> PrecompileDescriptor {
        self.descriptor.clone()
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(self.chip())]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![Box::new(self.chip())]
    }

    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        vec![]
    }
}

#[derive(Clone, Debug)]
struct FixedOutputsPrecompileProofPreparer {
    descriptor: PrecompileDescriptor,
    name: &'static str,
    witness_label: &'static str,
}

impl FixedOutputsPrecompileProofPreparer {
    fn new(
        descriptor: PrecompileDescriptor,
        name: &'static str,
        witness_label: &'static str,
    ) -> Self {
        Self {
            descriptor,
            name,
            witness_label,
        }
    }
}

impl PrecompileProofPreparer for FixedOutputsPrecompileProofPreparer {
    fn name(&self) -> &str {
        self.name
    }

    fn descriptor(&self) -> &PrecompileDescriptor {
        &self.descriptor
    }

    fn prepare_precompile(
        &self,
        context: PrecompileProofContext,
    ) -> Result<PreparedPrecompileProof, ExtError> {
        if context.descriptor != self.descriptor {
            return Err(ExtError::proof_preparation(TabulaError::ProofError {
                phase: "precompile_prepare",
                detail: format!(
                    "prepared descriptor mismatch for precompile 0x{:04x}",
                    self.descriptor.precompile_id.0,
                ),
            }));
        }
        let mut store = WitnessStore::new();
        store.put(
            self.witness_label,
            FixedOutputsWitness {
                calls: context.calls,
            },
        );
        Ok(PreparedPrecompileProof { store })
    }
}

#[derive(Clone, Debug)]
struct ConstantOnePrecompileProofSystem(FixedOutputsPrecompileProofSystem);

impl ConstantOnePrecompileProofSystem {
    fn new(descriptor: PrecompileDescriptor) -> Self {
        Self(FixedOutputsPrecompileProofSystem::new(
            descriptor,
            "constant_one_precompile",
            vec![u64_portable(1)],
            CONSTANT_ONE_WITNESS_LABEL,
        ))
    }
}

impl PrecompileProofSystem for ConstantOnePrecompileProofSystem {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn descriptor(&self) -> PrecompileDescriptor {
        self.0.descriptor()
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        self.0.airs()
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        self.0.dyn_chips()
    }

    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        self.0.bus_consumers()
    }
}

#[derive(Clone, Debug)]
struct ConstantOnePrecompileProofPreparer(FixedOutputsPrecompileProofPreparer);

impl ConstantOnePrecompileProofPreparer {
    fn new(descriptor: PrecompileDescriptor) -> Self {
        Self(FixedOutputsPrecompileProofPreparer::new(
            descriptor,
            "constant_one_precompile",
            CONSTANT_ONE_WITNESS_LABEL,
        ))
    }
}

impl PrecompileProofPreparer for ConstantOnePrecompileProofPreparer {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn descriptor(&self) -> &PrecompileDescriptor {
        self.0.descriptor()
    }

    fn prepare_precompile(
        &self,
        context: PrecompileProofContext,
    ) -> Result<PreparedPrecompileProof, ExtError> {
        self.0.prepare_precompile(context)
    }
}

fn sequence_outputs_typed() -> Vec<TypedValue> {
    vec![u64_typed(1), u64_typed(2), u64_typed(3), u64_typed(4)]
}

fn sequence_outputs() -> Vec<tabula_core::PortableValue> {
    vec![
        u64_portable(1),
        u64_portable(2),
        u64_portable(3),
        u64_portable(4),
    ]
}
