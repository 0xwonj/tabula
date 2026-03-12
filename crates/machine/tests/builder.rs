//! Tests for the MachineBuilder API and ChipExtension trait.

use tabula_commitment::scheme_tags;
use tabula_core::{ColId, RowKey, TableId};
use tabula_machine::prelude::*;
use tabula_machine::{ColumnSetupConfig, MachineBuilder, SetupError, SmtRootProof, TabulaMachine};
use tabula_stark::chips::core_chips;

// ── Builder basics ────────────────────────────────────────────────────────────

#[test]
fn builder_creates_valid_machine() {
    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    let machine = TabulaMachine::builder()
        .with_columns(col_configs)
        .build()
        .expect("builder should create a valid machine");

    let setups = machine.setups();
    assert_eq!(setups.execution.registry.chip_ids().len(), 4);
    assert_eq!(setups.columns.len(), 1);
    assert_eq!(setups.root.registry.chip_ids().len(), 4);
}

#[test]
fn builder_with_config() {
    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    let config = tabula_machine::default_config();
    let machine = TabulaMachine::builder()
        .with_columns(col_configs)
        .with_config(config)
        .build()
        .expect("builder with config");

    assert_eq!(machine.setups().execution.registry.chip_ids().len(), 4);
}

#[test]
fn builder_with_custom_root_proof() {
    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    let machine = TabulaMachine::builder()
        .with_columns(col_configs)
        .with_root_proof(SmtRootProof)
        .build()
        .expect("builder with custom root proof");

    let root_ids = machine.setups().root.registry.chip_ids();
    assert!(root_ids.contains(&core_chips::SMT_COL_PATH));
    assert!(root_ids.contains(&core_chips::SMT_TABLE_PATH));
}

#[test]
fn builder_no_columns() {
    let machine = TabulaMachine::builder()
        .build()
        .expect("builder with no columns");

    assert_eq!(machine.setups().columns.len(), 0);
}

#[test]
fn builder_matches_legacy_new() {
    let col_configs = vec![
        ColumnSetupConfig {
            table_id: TableId(0),
            col_id: ColId(0),
            scheme_tag: scheme_tags::SSMC,
            receives_commitment: true,
        },
        ColumnSetupConfig {
            table_id: TableId(0),
            col_id: ColId(1),
            scheme_tag: scheme_tags::SSMC,
            receives_commitment: true,
        },
    ];

    let legacy = TabulaMachine::new(&col_configs).expect("legacy new");
    let built = TabulaMachine::builder()
        .with_columns(col_configs.clone())
        .build()
        .expect("builder");

    assert_eq!(
        legacy.setups().execution.registry.chip_ids(),
        built.setups().execution.registry.chip_ids()
    );
    assert_eq!(legacy.setups().columns.len(), built.setups().columns.len());
    assert_eq!(
        legacy.setups().root.registry.chip_ids(),
        built.setups().root.registry.chip_ids()
    );
}

// ── ChipExtension ─────────────────────────────────────────────────────────────

/// Minimal extension that registers a single chip (RangeCheckChip alias).
/// Uses a distinct ChipId to avoid collisions with core.
mod test_extension {
    use tabula_core::error::TabulaError;
    use tabula_machine::prelude::*;
    use tabula_stark::trace::trace_map::TraceMap;

    /// A trivial chip with 2 columns and no constraints, for testing registration.
    #[derive(Clone, Debug)]
    pub struct DummyChip;

    pub const DUMMY_CHIP_ID: ChipId = ChipId(200);

    impl ChipSpec for DummyChip {
        fn chip_id(&self) -> ChipId {
            DUMMY_CHIP_ID
        }

        fn chip_name(&self) -> &'static str {
            "DummyChip"
        }

        fn has_interactions(&self) -> bool {
            false
        }
    }

    impl<F> BaseAir<F> for DummyChip {
        fn width(&self) -> usize {
            2
        }
    }

    impl<AB: AirBuilder> Air<AB> for DummyChip {
        fn eval(&self, _builder: &mut AB) {
            // No constraints.
        }
    }

    impl TraceContributor for DummyChip {
        fn phase(&self) -> TracePhase {
            TracePhase::INDEPENDENT
        }

        fn contribute(&self, _store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
            let trace = RowMajorMatrix::new(vec![BabyBear::ZERO; 2], 2);
            map.insert(self.chip_id(), trace);
            Ok(())
        }
    }

    pub struct DummyExtension;

    impl ChipExtension for DummyExtension {
        fn name(&self) -> &str {
            "dummy-extension"
        }

        fn airs(&self) -> Vec<Box<dyn AnyRap>> {
            vec![Box::new(DummyChip)]
        }

        fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
            vec![Box::new(DummyChip)]
        }
    }
}

#[test]
fn builder_with_extension_registers_chip() {
    use test_extension::{DUMMY_CHIP_ID, DummyExtension};

    let machine = TabulaMachine::builder()
        .with_extension(DummyExtension)
        .build()
        .expect("builder with extension");

    let exec_ids = machine.setups().execution.registry.chip_ids();

    // Core chips still present.
    assert!(exec_ids.contains(&core_chips::EXECUTION));
    assert!(exec_ids.contains(&core_chips::STATIC_TABLE));
    assert!(exec_ids.contains(&core_chips::POSEIDON));
    assert!(exec_ids.contains(&core_chips::RANGE_CHECK));

    // Extension chip added.
    assert!(exec_ids.contains(&DUMMY_CHIP_ID));
    assert_eq!(exec_ids.len(), 5); // 4 core + 1 extension
}

#[test]
fn builder_rejects_duplicate_chip_id() {
    use test_extension::DummyExtension;

    // Two identical extensions → duplicate ChipId(200).
    let result = TabulaMachine::builder()
        .with_extension(DummyExtension)
        .with_extension(DummyExtension)
        .build();

    assert!(matches!(result, Err(SetupError::DuplicateChipId(_))));
}

#[test]
fn builder_extension_with_columns() {
    use test_extension::{DUMMY_CHIP_ID, DummyExtension};

    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    let machine = TabulaMachine::builder()
        .with_columns(col_configs)
        .with_extension(DummyExtension)
        .build()
        .expect("builder with extension and columns");

    // Execution tier has extension chip.
    assert!(
        machine
            .setups()
            .execution
            .registry
            .chip_ids()
            .contains(&DUMMY_CHIP_ID)
    );

    // Column tier does NOT have extension chip (extensions are execution-tier only).
    let col_ids = &machine.setups().columns[0].1.registry.chip_ids();
    assert!(!col_ids.contains(&DUMMY_CHIP_ID));
}

// ── Column Schemes ────────────────────────────────────────────────────────────

#[test]
fn builder_ssmc_scheme_default() {
    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    let machine = TabulaMachine::builder()
        .with_columns(col_configs)
        .build()
        .expect("default SSMC scheme");

    // SSMC produces 4 shard chips + 2 bus consumers = 6 total.
    let col_ids = machine.setups().columns[0].1.registry.chip_ids();
    assert_eq!(col_ids.len(), 6);
}

#[test]
fn builder_smt_scheme() {
    use tabula_machine::SmtScheme;

    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SMT,
        receives_commitment: false,
    }];

    let machine = TabulaMachine::builder()
        .with_column_scheme(scheme_tags::SMT, SmtScheme::<3>)
        .with_columns(col_configs)
        .build()
        .expect("SMT scheme");

    // SMT produces 2 shard chips + 2 bus consumers = 4 total.
    let col_ids = machine.setups().columns[0].1.registry.chip_ids();
    assert_eq!(col_ids.len(), 4);
}

#[test]
fn builder_unregistered_scheme_tag_errors() {
    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: 999, // Not registered.
        receives_commitment: true,
    }];

    let result = TabulaMachine::builder().with_columns(col_configs).build();

    assert!(matches!(result, Err(SetupError::SetupFailed(_))));
}

#[test]
fn builder_mixed_schemes() {
    use tabula_machine::SmtScheme;

    let col_configs = vec![
        ColumnSetupConfig {
            table_id: TableId(0),
            col_id: ColId(0),
            scheme_tag: scheme_tags::SSMC,
            receives_commitment: true,
        },
        ColumnSetupConfig {
            table_id: TableId(0),
            col_id: ColId(1),
            scheme_tag: scheme_tags::SMT,
            receives_commitment: false,
        },
    ];

    let machine = TabulaMachine::builder()
        .with_column_scheme(scheme_tags::SMT, SmtScheme::<3>)
        .with_columns(col_configs)
        .build()
        .expect("mixed SSMC + SMT");

    // First column: SSMC = 6 chips.
    assert_eq!(machine.setups().columns[0].1.registry.chip_ids().len(), 6);
    // Second column: SMT = 4 chips.
    assert_eq!(machine.setups().columns[1].1.registry.chip_ids().len(), 4);
}

// ── PropertyOpening ──────────────────────────────────────────────────────────

mod test_property {
    use std::any::Any;

    use p3_baby_bear::BabyBear;
    use p3_field::{PrimeCharacteristicRing, PrimeField32};
    use tabula_commitment::scheme_tags;
    use tabula_core::RowKey;
    use tabula_machine::property::*;

    /// A test witness that always returns a single-element value.
    struct TestWitness {
        value: Vec<BabyBear>,
        null: bool,
    }

    impl PropertyWitness for TestWitness {
        fn value(&self) -> &[BabyBear] {
            &self.value
        }

        fn key(&self) -> Option<RowKey> {
            if self.null {
                None
            } else {
                // Test witness returns value as key for simplicity.
                Some(RowKey::from(
                    self.value
                        .first()
                        .map_or(0, |v| v.as_canonical_u32() as u64),
                ))
            }
        }

        fn is_null(&self) -> bool {
            self.null
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// A test property opening that supports Minimum and Maximum queries.
    pub struct TestMinMaxOpening;

    impl PropertyOpening for TestMinMaxOpening {
        fn name(&self) -> &str {
            "test-min-max"
        }

        fn compatible_scheme_tag(&self) -> u16 {
            scheme_tags::SSMC
        }

        fn supported_queries(&self) -> &[PropertyQueryKind] {
            &[PropertyQueryKind::Minimum, PropertyQueryKind::Maximum]
        }

        fn prove(
            &self,
            _commitment_digest: &[BabyBear],
            query: &PropertyQuery,
            state: &[(RowKey, &[BabyBear], bool)],
        ) -> Result<Box<dyn PropertyWitness>, PropertyError> {
            let supported = self.supported_queries();
            if !supported.contains(&query.kind()) {
                return Err(PropertyError::UnsupportedQuery {
                    kind: query.kind(),
                    supported: supported.to_vec(),
                });
            }

            if state.is_empty() {
                return Ok(Box::new(TestWitness {
                    value: vec![],
                    null: true,
                }));
            }

            let row_key = match query {
                PropertyQuery::Minimum => state.iter().map(|(k, _, _)| *k).min().unwrap(),
                PropertyQuery::Maximum => state.iter().map(|(k, _, _)| *k).max().unwrap(),
                _ => unreachable!(),
            };

            Ok(Box::new(TestWitness {
                value: vec![BabyBear::new(u64::from(row_key) as u32)],
                null: false,
            }))
        }
    }

    /// A test property opening for successor queries on a different commitment.
    pub struct TestSuccessorOpening;

    impl PropertyOpening for TestSuccessorOpening {
        fn name(&self) -> &str {
            "test-successor"
        }

        fn compatible_scheme_tag(&self) -> u16 {
            scheme_tags::SMT
        }

        fn supported_queries(&self) -> &[PropertyQueryKind] {
            &[PropertyQueryKind::Successor]
        }

        fn prove(
            &self,
            _commitment_digest: &[BabyBear],
            query: &PropertyQuery,
            _state: &[(RowKey, &[BabyBear], bool)],
        ) -> Result<Box<dyn PropertyWitness>, PropertyError> {
            let supported = self.supported_queries();
            if !supported.contains(&query.kind()) {
                return Err(PropertyError::UnsupportedQuery {
                    kind: query.kind(),
                    supported: supported.to_vec(),
                });
            }

            Ok(Box::new(TestWitness {
                value: vec![BabyBear::ZERO],
                null: true,
            }))
        }
    }

    /// A test property opening that claims Minimum on SMT (unsupported).
    pub struct SmtMinimumOpening;

    impl PropertyOpening for SmtMinimumOpening {
        fn name(&self) -> &str {
            "smt-minimum"
        }

        fn compatible_scheme_tag(&self) -> u16 {
            scheme_tags::SMT
        }

        fn supported_queries(&self) -> &[PropertyQueryKind] {
            &[PropertyQueryKind::Minimum]
        }

        fn prove(
            &self,
            _commitment_digest: &[BabyBear],
            _query: &PropertyQuery,
            _state: &[(RowKey, &[BabyBear], bool)],
        ) -> Result<Box<dyn PropertyWitness>, PropertyError> {
            unreachable!("should not be called")
        }
    }

    /// A test property opening with unregistered scheme tag.
    pub struct UnregisteredSchemeOpening;

    impl PropertyOpening for UnregisteredSchemeOpening {
        fn name(&self) -> &str {
            "unregistered-scheme"
        }

        fn compatible_scheme_tag(&self) -> u16 {
            9999
        }

        fn supported_queries(&self) -> &[PropertyQueryKind] {
            &[PropertyQueryKind::Minimum]
        }

        fn prove(
            &self,
            _commitment_digest: &[BabyBear],
            _query: &PropertyQuery,
            _state: &[(RowKey, &[BabyBear], bool)],
        ) -> Result<Box<dyn PropertyWitness>, PropertyError> {
            unreachable!("should not be called")
        }
    }
}

#[test]
fn builder_with_property_opening() {
    use test_property::TestMinMaxOpening;

    let machine = TabulaMachine::builder()
        .with_property_opening(TestMinMaxOpening)
        .build()
        .expect("builder with property opening");

    let openings = machine.property_openings();
    assert_eq!(openings.len(), 1);
    assert_eq!(openings[0].name(), "test-min-max");
    assert_eq!(openings[0].compatible_scheme_tag(), scheme_tags::SSMC);
    assert_eq!(
        openings[0].supported_queries(),
        &[
            tabula_machine::PropertyQueryKind::Minimum,
            tabula_machine::PropertyQueryKind::Maximum,
        ]
    );
}

#[test]
fn builder_with_multiple_property_openings() {
    use test_property::{TestMinMaxOpening, TestSuccessorOpening};

    let machine = TabulaMachine::builder()
        .with_property_opening(TestMinMaxOpening)
        .with_property_opening(TestSuccessorOpening)
        .build()
        .expect("builder with multiple property openings");

    let openings = machine.property_openings();
    assert_eq!(openings.len(), 2);
    assert_eq!(openings[0].name(), "test-min-max");
    assert_eq!(openings[1].name(), "test-successor");
}

#[test]
fn property_opening_prove_minimum() {
    use p3_baby_bear::BabyBear;
    use tabula_machine::PropertyQuery;
    use test_property::TestMinMaxOpening;

    let opening = TestMinMaxOpening;

    let v1 = [BabyBear::new(100)];
    let v2 = [BabyBear::new(200)];
    let v3 = [BabyBear::new(50)];
    let state: Vec<(RowKey, &[BabyBear], bool)> = vec![
        (RowKey(10), &v1, false),
        (RowKey(20), &v2, false),
        (RowKey(5), &v3, false),
    ];

    let witness = opening
        .prove(&[], &PropertyQuery::Minimum, &state)
        .expect("prove minimum");

    assert!(!witness.is_null());
    // Minimum row key is RowKey(5) → BabyBear::new(5).
    assert_eq!(witness.value(), &[BabyBear::new(5)]);
}

#[test]
fn property_opening_unsupported_query() {
    use tabula_machine::{PropertyError, PropertyQuery};
    use test_property::TestMinMaxOpening;

    let opening = TestMinMaxOpening;

    let result = opening.prove(&[], &PropertyQuery::Successor { key: RowKey(0) }, &[]);
    assert!(matches!(
        result,
        Err(PropertyError::UnsupportedQuery { .. })
    ));
}

#[test]
fn property_opening_empty_state_returns_null() {
    use tabula_machine::PropertyQuery;
    use test_property::TestMinMaxOpening;

    let opening = TestMinMaxOpening;

    let witness = opening
        .prove(&[], &PropertyQuery::Minimum, &[])
        .expect("prove on empty state");

    assert!(witness.is_null());
    assert!(witness.value().is_empty());
}

#[test]
fn property_query_kind_mapping() {
    use tabula_machine::{AggregateKind, PropertyQuery, PropertyQueryKind};

    assert_eq!(PropertyQuery::Minimum.kind(), PropertyQueryKind::Minimum);
    assert_eq!(PropertyQuery::Maximum.kind(), PropertyQueryKind::Maximum);
    assert_eq!(
        PropertyQuery::Successor { key: RowKey(42) }.kind(),
        PropertyQueryKind::Successor
    );
    assert_eq!(
        PropertyQuery::Predecessor { key: RowKey(42) }.kind(),
        PropertyQueryKind::Predecessor
    );
    assert_eq!(
        PropertyQuery::NonExistenceRange {
            lower: RowKey(0),
            upper: RowKey(100)
        }
        .kind(),
        PropertyQueryKind::NonExistenceRange
    );
    assert_eq!(
        PropertyQuery::Aggregate {
            kind: AggregateKind::Sum
        }
        .kind(),
        PropertyQueryKind::Aggregate
    );
}

// ── Scheme Capability Validation ──────────────────────────────────────────────

#[test]
fn builder_rejects_opening_beyond_scheme_capability() {
    use tabula_machine::SmtScheme;
    use test_property::SmtMinimumOpening;

    // SMT does not support Minimum queries (keys are hashed, ordering lost).
    // Registering a Minimum opening for SMT should fail at build time.
    let result = TabulaMachine::builder()
        .with_columns(vec![ColumnSetupConfig {
            table_id: TableId(1),
            col_id: ColId(0),
            scheme_tag: scheme_tags::SMT,
            receives_commitment: true,
        }])
        .with_column_scheme(scheme_tags::SMT, SmtScheme::<3>)
        .with_property_opening(SmtMinimumOpening)
        .build();

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Minimum"),
        "error should mention the unsupported query kind: {err}"
    );
    assert!(
        err.contains("smt"),
        "error should mention the scheme name: {err}"
    );
}

// ── MachineBuilder Default ────────────────────────────────────────────────────

#[test]
fn machine_builder_default() {
    let builder = MachineBuilder::default();
    let machine = builder.build().expect("default builder");
    assert_eq!(machine.setups().columns.len(), 0);
}

// ── H1: ChipId mismatch detection ──────────────────────────────────────────

mod test_mismatch_extension {
    use tabula_core::error::TabulaError;
    use tabula_machine::prelude::*;
    use tabula_stark::trace::trace_map::TraceMap;

    /// A chip that appears only in AIRs, not DynChips.
    #[derive(Clone, Debug)]
    pub struct AirOnlyChip;

    pub const AIR_ONLY_ID: ChipId = ChipId(201);

    impl ChipSpec for AirOnlyChip {
        fn chip_id(&self) -> ChipId {
            AIR_ONLY_ID
        }
        fn chip_name(&self) -> &'static str {
            "AirOnlyChip"
        }
        fn has_interactions(&self) -> bool {
            false
        }
    }

    impl<F> BaseAir<F> for AirOnlyChip {
        fn width(&self) -> usize {
            1
        }
    }

    impl<AB: AirBuilder> Air<AB> for AirOnlyChip {
        fn eval(&self, _builder: &mut AB) {}
    }

    impl TraceContributor for AirOnlyChip {
        fn phase(&self) -> TracePhase {
            TracePhase::INDEPENDENT
        }
        fn contribute(&self, _store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
            let trace = RowMajorMatrix::new(vec![BabyBear::ZERO; 1], 1);
            map.insert(self.chip_id(), trace);
            Ok(())
        }
    }

    /// A second chip that appears only in DynChips.
    #[derive(Clone, Debug)]
    pub struct DynOnlyChip;

    pub const DYN_ONLY_ID: ChipId = ChipId(202);

    impl ChipSpec for DynOnlyChip {
        fn chip_id(&self) -> ChipId {
            DYN_ONLY_ID
        }
        fn chip_name(&self) -> &'static str {
            "DynOnlyChip"
        }
        fn has_interactions(&self) -> bool {
            false
        }
    }

    impl<F> BaseAir<F> for DynOnlyChip {
        fn width(&self) -> usize {
            1
        }
    }

    impl<AB: AirBuilder> Air<AB> for DynOnlyChip {
        fn eval(&self, _builder: &mut AB) {}
    }

    impl TraceContributor for DynOnlyChip {
        fn phase(&self) -> TracePhase {
            TracePhase::INDEPENDENT
        }
        fn contribute(&self, _store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
            let trace = RowMajorMatrix::new(vec![BabyBear::ZERO; 1], 1);
            map.insert(self.chip_id(), trace);
            Ok(())
        }
    }

    /// Extension with mismatched AIR/DynChip sets — should fail ChipId validation.
    pub struct MismatchedExtension;

    impl ChipExtension for MismatchedExtension {
        fn name(&self) -> &str {
            "mismatched-extension"
        }

        fn airs(&self) -> Vec<Box<dyn AnyRap>> {
            vec![Box::new(AirOnlyChip)]
        }

        fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
            vec![Box::new(DynOnlyChip)]
        }
    }
}

#[test]
fn builder_rejects_mismatched_chip_ids() {
    use test_mismatch_extension::MismatchedExtension;

    let result = TabulaMachine::builder()
        .with_extension(MismatchedExtension)
        .build();

    assert!(
        matches!(result, Err(SetupError::SetupFailed(ref msg)) if msg.contains("ChipId mismatch")),
        "expected ChipId mismatch error, got: {result:?}",
    );
}

// ── L5: Extension bus_consumers registration ──────────────────────────────────

mod test_bus_consumer_extension {
    use p3_baby_bear::BabyBear;
    use tabula_core::error::TabulaError;
    use tabula_machine::prelude::*;
    use tabula_stark::debug::RecordedInteraction;
    use tabula_stark::trace::trace_map::TraceMap;

    #[derive(Clone, Debug)]
    pub struct ConsumerChip;

    pub const CONSUMER_CHIP_ID: ChipId = ChipId(210);
    const CUSTOM_BUS: BusId = BusId(100);

    impl ChipSpec for ConsumerChip {
        fn chip_id(&self) -> ChipId {
            CONSUMER_CHIP_ID
        }
        fn chip_name(&self) -> &'static str {
            "ConsumerChip"
        }
        fn has_interactions(&self) -> bool {
            true
        }
    }

    impl<F> BaseAir<F> for ConsumerChip {
        fn width(&self) -> usize {
            1
        }
    }

    impl<AB: AirBuilder> Air<AB> for ConsumerChip {
        fn eval(&self, _builder: &mut AB) {}
    }

    impl TraceContributor for ConsumerChip {
        fn phase(&self) -> TracePhase {
            TracePhase::DEPENDENT
        }
        fn contribute(&self, _store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
            let trace = RowMajorMatrix::new(vec![BabyBear::ZERO; 1], 1);
            map.insert(self.chip_id(), trace);
            Ok(())
        }
    }

    impl BusConsumer for ConsumerChip {
        fn consumed_buses(&self) -> Vec<BusId> {
            vec![CUSTOM_BUS]
        }
        fn collect(
            &self,
            _interactions: &[RecordedInteraction<BabyBear>],
            _store: &mut WitnessStore,
        ) -> Result<(), TabulaError> {
            Ok(())
        }
    }

    pub struct ConsumerExtension;

    impl ChipExtension for ConsumerExtension {
        fn name(&self) -> &str {
            "consumer-extension"
        }

        fn airs(&self) -> Vec<Box<dyn AnyRap>> {
            vec![Box::new(ConsumerChip)]
        }

        fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
            vec![Box::new(ConsumerChip)]
        }

        fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
            vec![Box::new(ConsumerChip)]
        }
    }
}

#[test]
fn builder_extension_bus_consumers_registered() {
    use test_bus_consumer_extension::{CONSUMER_CHIP_ID, ConsumerExtension};

    let machine = TabulaMachine::builder()
        .with_extension(ConsumerExtension)
        .build()
        .expect("builder with bus consumer extension");

    let exec_ids = machine.setups().execution.registry.chip_ids();

    // Extension chip is registered in the execution tier.
    assert!(exec_ids.contains(&CONSUMER_CHIP_ID));
    // 4 core + 1 extension = 5.
    assert_eq!(exec_ids.len(), 5);
}

// ── L6: Column scheme override ───────────────────────────────────────────────

#[test]
fn builder_column_scheme_override() {
    use tabula_machine::SsmcScheme;

    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    // Override default SSMC<3> with SSMC<1> (different width — Bool encoding).
    let machine = TabulaMachine::builder()
        .with_column_scheme(scheme_tags::SSMC, SsmcScheme::<1>)
        .with_columns(col_configs)
        .build()
        .expect("override SSMC scheme");

    // Should still produce a valid machine (6 chips for SSMC).
    let col_ids = machine.setups().columns[0].1.registry.chip_ids();
    assert_eq!(col_ids.len(), 6);
}

// ── M6: Property opening with unregistered scheme_tag ──────────────────────

#[test]
fn builder_rejects_property_opening_with_unregistered_scheme() {
    use test_property::UnregisteredSchemeOpening;

    // When columns are present, an opening with unregistered scheme_tag should fail.
    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    let result = TabulaMachine::builder()
        .with_columns(col_configs)
        .with_property_opening(UnregisteredSchemeOpening)
        .build();

    assert!(
        matches!(result, Err(SetupError::SetupFailed(ref msg)) if msg.contains("scheme tag")),
        "expected scheme tag error, got: {result:?}",
    );
}

#[test]
fn builder_allows_property_opening_without_columns() {
    use test_property::UnregisteredSchemeOpening;

    // When no columns are present, scheme_tag validation is skipped
    // (property openings are only relevant when columns exist).
    let machine = TabulaMachine::builder()
        .with_property_opening(UnregisteredSchemeOpening)
        .build()
        .expect("property opening without columns should succeed");

    assert_eq!(machine.property_openings().len(), 1);
}

// ── Prelude re-exports ────────────────────────────────────────────────────────

#[test]
fn prelude_exports_core_types() {
    // Verify that the prelude re-exports compile and are usable.
    fn _assert_types() {
        let _chip_id: ChipId = ChipId(0);
        let _bus_id: BusId = BusId(0);
        let _phase: TracePhase = TracePhase::INDEPENDENT;
        let _store: WitnessStore = WitnessStore::new();
        let _alloc = ChipIdAllocator::for_shards();
        let _map: TraceMap = TraceMap::new();
    }
}
